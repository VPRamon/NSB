//! Reconstruct normalized 336–650 nm XP continuous spectra from canonical coefficient CSVs.

use anyhow::{bail, Result};
use clap::Parser;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_xp::integrate_photon_flux;
use nsb_data_tools::gaia_xp_continuous::{
    parse_datalink_gaiaxpy_csv, write_normalized_spectrum_csv, PHOTOMETRY_MODEL,
    PINNED_GAIA_XPY_VERSION,
};
use nsb_data_tools::gaia_xp_continuous_calibrate::{
    integrate_gaiaxpy_manifest_uncertainty, GaiaXpContinuousCalibrator,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Rust in-process XP continuous calibration for canonical coefficient CSVs")]
struct Args {
    #[arg(long)]
    coefficients_dir: Option<PathBuf>,
    #[arg(long)]
    coefficient_file: Option<PathBuf>,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    design_fixture: Option<PathBuf>,
    #[arg(long)]
    gaiaxpy_environment: Option<PathBuf>,
    #[arg(long)]
    integrate_only: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.coefficients_dir.is_none() && args.coefficient_file.is_none() {
        bail!("either --coefficients-dir or --coefficient-file is required");
    }
    fs::create_dir_all(&args.output_dir)?;
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(
        args.design_fixture.as_deref(),
        args.gaiaxpy_environment.as_deref(),
    );
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;

    let mut paths = Vec::new();
    if let Some(file) = &args.coefficient_file {
        paths.push(file.clone());
    } else if let Some(dir) = &args.coefficients_dir {
        let mut discovered = fs::read_dir(dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("csv"))
            .collect::<Vec<_>>();
        discovered.sort();
        if let Some(limit) = args.limit {
            discovered.truncate(limit);
        }
        paths = discovered;
    }

    let mut entries = Vec::with_capacity(paths.len());
    for coefficient_path in paths {
        let entry = reconstruct_one(
            &calibrator,
            &coefficient_path,
            &args.output_dir,
            args.integrate_only,
        )?;
        entries.push(entry);
    }

    let manifest = serde_json::json!({
        "schema_version": 1,
        "photometry_model": PHOTOMETRY_MODEL,
        "gaiaxpy_version": PINNED_GAIA_XPY_VERSION,
        "reconstruct_backend": "rust",
        "generation_timestamp_utc": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "band_nm": [336.0, 650.0],
        "grid_step_nm": 2.0,
        "entries": entries,
    });
    if let Some(parent) = args.manifest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.manifest,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "reconstructed {} sources -> {}",
        entries.len(),
        args.output_dir.display()
    );
    Ok(())
}

fn reconstruct_one(
    calibrator: &GaiaXpContinuousCalibrator,
    coefficient_path: &Path,
    output_dir: &Path,
    integrate_only: bool,
) -> Result<serde_json::Value> {
    let source_id = coefficient_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();
    let output_path = output_dir.join(format!("{source_id}.csv"));
    if !integrate_only && output_path.is_file() {
        return Ok(serde_json::json!({
            "source_id": source_id,
            "status": "skipped_existing",
            "output_sha256": sha256_file(&output_path)?,
        }));
    }
    let bytes = fs::read(coefficient_path)?;
    let record = parse_datalink_gaiaxpy_csv(&bytes, &source_id)?;
    let product = calibrator.calibrate_record_product(&record)?;
    let integral = integrate_photon_flux(&product)?;
    let uncertainty = integrate_gaiaxpy_manifest_uncertainty(
        &product.wavelengths_nm,
        product.flux_error_w_m2_nm.as_ref().expect("flux errors"),
    )?;
    if !integrate_only {
        write_normalized_spectrum_csv(&output_path, &product)?;
    }
    let mut entry = serde_json::json!({
        "source_id": source_id,
        "status": "reconstructed",
        "band_nm": [336.0, 650.0],
        "grid_step_nm": 2.0,
        "samples": product.wavelengths_nm.len(),
        "flux_336_650_ph_m2_s": integral.total_ph_m2_s,
        "statistical_uncertainty_336_650_ph_m2_s": uncertainty,
    });
    if !integrate_only {
        entry["output_path"] = serde_json::Value::String(output_path.display().to_string());
        entry["coefficient_sha256"] = serde_json::Value::String(sha256_file(coefficient_path)?);
        entry["output_sha256"] = serde_json::Value::String(sha256_file(&output_path)?);
    }
    Ok(entry)
}
