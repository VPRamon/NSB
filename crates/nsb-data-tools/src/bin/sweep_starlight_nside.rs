use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const NSIDES: [u32; 3] = [64, 128, 256];
const GAIA_XP_MODEL: &str = "gaia_dr3_xp_photon_radiance_330_650nm_v1";

#[derive(Debug, Parser)]
#[command(about = "Run the Gaia starlight release nside sweep")]
struct Args {
    /// Canonical Gaia starlight source CSV.
    #[arg(long)]
    input: PathBuf,
    /// Sweep output directory.
    #[arg(long, default_value = "target/starlight-release/sweep")]
    output_dir: PathBuf,
    /// Independent validation reference JSON.
    #[arg(long)]
    reference: Option<PathBuf>,
    /// Source catalogue checksum for the canonical input.
    #[arg(long)]
    catalog_checksum: String,
    /// Reviewed Gaia derived-product license or redistribution policy.
    #[arg(long)]
    catalog_license: String,
    /// UTC generation timestamp.
    #[arg(long)]
    generation_date_utc: String,
}

#[derive(Debug, Serialize)]
struct NsideSummary {
    nside: u32,
    pixels: u64,
    map_csv_bytes: u64,
    packed_asset_bytes: u64,
    generation_seconds: f64,
    pack_seconds: f64,
    runtime_load_seconds: f64,
    empty_pixels: u64,
    finite_nonnegative_pass: bool,
    plane_pole_pass: bool,
    longitude_wrap_pass: bool,
    independent_comparison_pass: bool,
    production_ready: bool,
}

#[derive(Debug, Serialize)]
struct AggregateSummary {
    recommendation_policy: &'static str,
    recommended_nside: Option<u32>,
    summaries: Vec<NsideSummary>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    if args.reference.is_none() {
        bail!("nside sweep requires --reference; refusing to recommend without independent validation");
    }
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let mut summaries = Vec::new();
    for nside in NSIDES {
        summaries.push(run_one(&args, nside)?);
    }
    let recommended_nside = recommend(&summaries);
    let aggregate = AggregateSummary {
        recommendation_policy: "Prefer nside=128. Use nside=64 if nside=128 is too large or materially slower without validation benefit. Use nside=256 only if validation improves meaningfully and packed size/runtime remain acceptable.",
        recommended_nside,
        summaries,
    };
    write_json(&args.output_dir.join("summary.json"), &aggregate)?;
    if recommended_nside.is_none() {
        bail!("no nside recommendation because at least one validation gate is missing");
    }
    Ok(())
}

fn run_one(args: &Args, nside: u32) -> Result<NsideSummary> {
    let dir = args.output_dir.join(format!("nside{nside}"));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let map = dir.join("starlight_map.csv");
    let diagnostics = dir.join("diagnostics.json");
    let validation = dir.join("validation.json");
    let packed = dir.join("packed-candidate.bin.zst");
    let manifest = dir.join("packed-candidate.manifest.toml");

    let started = Instant::now();
    cargo_tool(
        "build_starlight_map",
        vec![
            "--input".to_string(),
            path_str(&args.input)?.to_string(),
            "--output".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics-output".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--nside".to_string(),
            nside.to_string(),
            "--ordering".to_string(),
            "ring".to_string(),
            "--catalog-name".to_string(),
            "Gaia".to_string(),
            "--catalog-release".to_string(),
            "DR3".to_string(),
            "--catalog-license".to_string(),
            args.catalog_license.clone(),
            "--catalog-checksum".to_string(),
            args.catalog_checksum.clone(),
            "--photometry-model".to_string(),
            GAIA_XP_MODEL.to_string(),
            "--band-min-nm".to_string(),
            "330".to_string(),
            "--band-max-nm".to_string(),
            "650".to_string(),
            "--generation-date-utc".to_string(),
            args.generation_date_utc.clone(),
            "--require-science-diagnostics".to_string(),
        ],
    )?;
    let generation_seconds = started.elapsed().as_secs_f64();

    cargo_tool(
        "validate_starlight_map",
        vec![
            "--input".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--reference".to_string(),
            path_str(args.reference.as_ref().expect("checked by run"))?.to_string(),
            "--output".to_string(),
            path_str(&validation)?.to_string(),
            "--require-independent-comparison".to_string(),
        ],
    )?;

    let pack_started = Instant::now();
    cargo_tool(
        "pack_starlight_asset",
        vec![
            "--input".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--validation".to_string(),
            path_str(&validation)?.to_string(),
            "--output".to_string(),
            path_str(&packed)?.to_string(),
            "--manifest".to_string(),
            path_str(&manifest)?.to_string(),
            "--candidate".to_string(),
        ],
    )?;
    let pack_seconds = pack_started.elapsed().as_secs_f64();

    let diagnostics_json: Value = read_json(&diagnostics)?;
    let validation_json: Value = read_json(&validation)?;
    let summary = NsideSummary {
        nside,
        pixels: 12 * u64::from(nside) * u64::from(nside),
        map_csv_bytes: file_len(&map)?,
        packed_asset_bytes: file_len(&packed)?,
        generation_seconds,
        pack_seconds,
        runtime_load_seconds: 0.0,
        empty_pixels: diagnostics_json["empty_pixels"].as_u64().unwrap_or(0),
        finite_nonnegative_pass: validation_json["finite_nonnegative_pass"]
            .as_bool()
            .unwrap_or(false),
        plane_pole_pass: validation_json["plane_pole_pass"]
            .as_bool()
            .unwrap_or(false),
        longitude_wrap_pass: validation_json["longitude_wrap_pass"]
            .as_bool()
            .unwrap_or(false),
        independent_comparison_pass: validation_json["independent_comparison_pass"]
            .as_bool()
            .unwrap_or(false),
        production_ready: validation_json["production_ready"]
            .as_bool()
            .unwrap_or(false),
    };
    write_json(&dir.join("summary.json"), &summary)?;
    Ok(summary)
}

fn recommend(summaries: &[NsideSummary]) -> Option<u32> {
    if summaries.iter().any(|summary| !summary.production_ready) {
        return None;
    }
    let n128 = summaries.iter().find(|summary| summary.nside == 128)?;
    let n64 = summaries.iter().find(|summary| summary.nside == 64)?;
    let n256 = summaries.iter().find(|summary| summary.nside == 256)?;
    if n128.packed_asset_bytes > 0
        && n64.production_ready
        && n64.packed_asset_bytes * 2 < n128.packed_asset_bytes
    {
        Some(64)
    } else if n256.production_ready
        && n256.empty_pixels < n128.empty_pixels
        && n256.packed_asset_bytes <= n128.packed_asset_bytes * 4
    {
        Some(256)
    } else {
        Some(128)
    }
}

fn cargo_tool(bin: &str, args: Vec<String>) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "--locked",
        "-p",
        "nsb-data-tools",
        "--bin",
        bin,
        "--",
    ]);
    command.args(args);
    let status = command.status().context("failed to spawn cargo")?;
    if !status.success() {
        bail!("sweep subcommand failed with status {status}");
    }
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not UTF-8: {}", path.display()))
}

fn file_len(path: &Path) -> Result<u64> {
    Ok(std::fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len())
}

fn read_json(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let raw = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{raw}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}
