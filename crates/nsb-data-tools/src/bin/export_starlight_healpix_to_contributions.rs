//! Export a runtime HEALPix starlight map into normalized contribution rows for
//! `build_integrated_starlight_product`.
//!
//! Each nonempty pixel becomes one bin-level contribution with `multiplicity=1`
//! so flux conservation round-trips through the integrated builder.

use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::checksum_io;
use serde::Serialize;
use sha2::{Digest, Sha256};
use siderust::checksum::to_hex;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

const CONTRIBUTION_HEADER: &str = "source_or_bin_id,healpix_index,multiplicity,measured_300_650,inferred_300_650,completeness_correction,statistical_uncertainty,systematic_uncertainty,flags_extrapolation,flags_crowding,branch\n";
const FLUX_UNIT_CONVERSION: f64 = 1.0e-13;
const INPUT_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Convert a HEALPix runtime map CSV into integrated-product contribution inputs.
#[derive(Debug, Parser)]
#[command(name = "export_starlight_healpix_to_contributions")]
struct Args {
    /// Runtime HEALPix map CSV (`integrated_ph_cm2_ns_sr` column).
    #[arg(long)]
    input: PathBuf,

    /// Optional build diagnostics JSON (used for coverage metadata).
    #[arg(long)]
    diagnostics: Option<PathBuf>,

    /// HEALPix nside (must match the input map).
    #[arg(long, default_value_t = 256)]
    nside: u32,

    /// Contribution branch label recorded in each row.
    #[arg(long, default_value = "xp_sampled")]
    branch: String,

    /// Output contributions CSV path.
    #[arg(long)]
    output_csv: PathBuf,

    /// Output inputs manifest TOML for `build_integrated_starlight_product`.
    #[arg(long)]
    output_manifest: PathBuf,

    /// Optional partial-coverage metadata JSON.
    #[arg(long)]
    coverage_metadata: Option<PathBuf>,

    /// Stable release identifier recorded in the manifest.
    #[arg(long, default_value = "starlight_partial_integrated_v1")]
    release_id: String,

    /// Model checksum recorded in the manifest.
    #[arg(long)]
    model_checksum: Option<String>,

    /// Relative Poisson-like statistical uncertainty per bin (sqrt scaling).
    #[arg(long, default_value_t = 1.0)]
    statistical_sqrt_scale: f64,

    /// Relative systematic uncertainty fraction of measured flux.
    #[arg(long, default_value_t = 0.05)]
    systematic_fraction: f64,

    /// Skip pixels with radiance below this threshold.
    #[arg(long, default_value_t = 0.0)]
    min_radiance: f64,
}

#[derive(Debug, Serialize)]
struct CoverageMetadata {
    schema_version: u32,
    release_id: String,
    calibration_status: &'static str,
    production_ready: bool,
    effective_measured_band_nm: [u16; 2],
    target_band_nm: [u16; 2],
    measured_fraction: f64,
    continuous_fraction: f64,
    inferred_fraction: f64,
    uv_fraction: f64,
    completeness_fraction: f64,
    pending_branches: Vec<&'static str>,
    limitations: Vec<String>,
    contribution_rows: u64,
    represented_multiplicity: u64,
    sources_used: Option<u64>,
    nside: u32,
    branch: String,
    input_map: String,
    contributions_csv: String,
    contributions_sha256: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !args.nside.is_power_of_two() || args.nside == 0 {
        bail!("nside must be a non-zero power of two");
    }
    let expected_pixels = 12 * args.nside as usize * args.nside as usize;
    let pixel_area_sr = 4.0 * std::f64::consts::PI / expected_pixels as f64;
    let radiance_to_flux = pixel_area_sr / FLUX_UNIT_CONVERSION;

    let sources_used = args
        .diagnostics
        .as_ref()
        .map(read_sources_used)
        .transpose()?;

    fs::create_dir_all(
        args.output_csv
            .parent()
            .context("output_csv has no parent directory")?,
    )?;
    let mut writer = BufWriter::new(
        File::create(&args.output_csv).context("failed to create contributions CSV")?,
    );
    writer.write_all(CONTRIBUTION_HEADER.as_bytes())?;

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_path(&args.input)
        .context("failed to open input map")?;
    let headers = reader.headers()?.clone();
    let index_col = headers
        .iter()
        .position(|name| name == "healpix_index")
        .context("missing healpix_index column")?;
    let radiance_col = headers
        .iter()
        .position(|name| name == "integrated_ph_cm2_ns_sr")
        .context("missing integrated_ph_cm2_ns_sr column")?;

    let mut rows_written = 0_u64;
    let mut multiplicity = 0_u64;
    for record in reader.records() {
        let record = record.context("invalid map CSV row")?;
        let healpix_index: usize = record
            .get(index_col)
            .context("missing healpix_index")?
            .parse()
            .context("invalid healpix_index")?;
        if healpix_index >= expected_pixels {
            bail!(
                "healpix_index {healpix_index} exceeds nside={} range",
                args.nside
            );
        }
        let radiance: f64 = record
            .get(radiance_col)
            .context("missing integrated_ph_cm2_ns_sr")?
            .parse()
            .context("invalid integrated_ph_cm2_ns_sr")?;
        if !radiance.is_finite() || radiance < args.min_radiance {
            continue;
        }
        let measured = radiance * radiance_to_flux;
        if !measured.is_finite() || measured < 0.0 {
            bail!("invalid measured flux at healpix_index {healpix_index}");
        }
        let statistical = (measured.sqrt() * args.statistical_sqrt_scale).max(0.0);
        let systematic = measured * args.systematic_fraction;
        let source_or_bin_id = format!("pix-{healpix_index:06}");
        writeln!(
            writer,
            "{source_or_bin_id},{healpix_index},1,{measured:.17e},0,0,{statistical:.17e},{systematic:.17e},false,false,{}",
            args.branch
        )?;
        rows_written += 1;
        multiplicity += 1;
    }
    writer.flush()?;

    let contributions_sha256 = checksum_io::sha256_file(&args.output_csv)?;
    let model_checksum = args.model_checksum.unwrap_or_else(|| {
        let mut hasher = Sha256::new();
        hasher.update(b"nsb_partial_integrated_xp_sampled_336_650_v1");
        let digest: [u8; 32] = hasher.finalize().into();
        format!("sha256:{}", to_hex(&digest))
    });

    let contributions_name = args
        .output_csv
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("contributions.csv");
    let manifest_body = format!(
        "schema_version = {INPUT_MANIFEST_SCHEMA_VERSION}\nrelease_id = \"{}\"\nmodel_checksum = \"{model_checksum}\"\n\n[[inputs]]\npath = \"{contributions_name}\"\nsha256 = \"{contributions_sha256}\"\nbranch = \"{}\"\n",
        args.release_id,
        args.branch
    );
    if let Some(parent) = args.output_manifest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output_manifest, manifest_body)?;

    if let Some(path) = args.coverage_metadata {
        let metadata = CoverageMetadata {
            schema_version: 1,
            release_id: args.release_id.clone(),
            calibration_status: "candidate",
            production_ready: false,
            effective_measured_band_nm: [336, 650],
            target_band_nm: [300, 650],
            measured_fraction: 1.0,
            continuous_fraction: 0.0,
            inferred_fraction: 0.0,
            uv_fraction: 0.0,
            completeness_fraction: 1.0,
            pending_branches: vec!["xp_continuous", "no_xp", "uv_300_336", "completeness"],
            limitations: vec![
                "XP sampled 336-650 nm only; not a validated 300-650 nm integrated product".into(),
                "UV 300-336 nm correction not applied (uv_fraction=0)".into(),
                "No-XP photometric inference not included".into(),
                "Gaia completeness / selection-function correction not applied beyond unity".into(),
                "Bin-level contributions with multiplicity=1 per occupied HEALPix pixel".into(),
            ],
            contribution_rows: rows_written,
            represented_multiplicity: multiplicity,
            sources_used,
            nside: args.nside,
            branch: args.branch.clone(),
            input_map: args.input.display().to_string(),
            contributions_csv: args.output_csv.display().to_string(),
            contributions_sha256: contributions_sha256.clone(),
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_string_pretty(&metadata)?)?;
    }

    println!(
        "exported {rows_written} contribution rows to {}",
        args.output_csv.display()
    );
    println!("inputs manifest: {}", args.output_manifest.display());
    println!("contributions sha256: {contributions_sha256}");
    Ok(())
}

fn read_sources_used(path: &PathBuf) -> Result<u64> {
    #[derive(serde::Deserialize)]
    struct Diagnostics {
        sources_used: u64,
    }
    let text = fs::read_to_string(path)?;
    let diag: Diagnostics = serde_json::from_str(&text)?;
    Ok(diag.sources_used)
}
