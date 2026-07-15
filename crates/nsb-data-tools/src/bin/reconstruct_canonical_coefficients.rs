//! Reconstruct normalized 336–650 nm XP continuous spectra from canonical coefficient CSVs.

use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::WriterBuilder;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_xp::{
    format_series, integrate_photon_flux, XpProduct, NORMALIZED_FLUX_COLUMN,
    NORMALIZED_FLUX_ERROR_COLUMN, NORMALIZED_WAVELENGTH_COLUMN,
};
use nsb_data_tools::gaia_xp_continuous::{parse_datalink_gaiaxpy_csv, PHOTOMETRY_MODEL};
use nsb_data_tools::gaia_xp_continuous_calibrate::GaiaXpContinuousCalibrator;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Pure-Rust XP continuous calibration for canonical coefficient CSVs")]
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
    #[arg(long, hide = true)]
    calibration_fixture: Option<PathBuf>,
    #[arg(long)]
    integrate_only: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.coefficients_dir.is_some() == args.coefficient_file.is_some() {
        bail!("provide exactly one of --coefficients-dir or --coefficient-file");
    }
    fs::create_dir_all(&args.output_dir)?;

    let fixture_path = GaiaXpContinuousCalibrator::resolve_design_fixture_path(
        args.design_fixture
            .as_deref()
            .or(args.calibration_fixture.as_deref()),
        None,
    );
    let fixture_sha256 = sha256_file(&fixture_path)?;
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture_path)?;

    let mut paths = Vec::new();
    if let Some(file) = &args.coefficient_file {
        paths.push(file.clone());
    } else if let Some(dir) = &args.coefficients_dir {
        let mut discovered = fs::read_dir(dir)
            .with_context(|| format!("read coefficient directory {}", dir.display()))?
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
    if paths.is_empty() {
        bail!("no canonical coefficient CSVs were selected");
    }

    let mut entries = Vec::with_capacity(paths.len());
    for coefficient_path in paths {
        entries.push(reconstruct_one(
            &calibrator,
            &coefficient_path,
            &args.output_dir,
            args.integrate_only,
        )?);
    }
    let entry_count = entries.len();

    let manifest = serde_json::json!({
        "schema_version": 2,
        "photometry_model": PHOTOMETRY_MODEL,
        "reconstruction": {
            "implementation": "nsb-data-tools::gaia_xp_continuous_calibrate",
            "backend": "rust_in_process",
            "gaiaxpy_reference_version": calibrator.gaiaxpy_version(),
            "bp_model": calibrator.bp_model(),
            "rp_model": calibrator.rp_model(),
            "truncation": false,
            "design_fixture_path": fixture_path.display().to_string(),
            "design_fixture_sha256": fixture_sha256,
        },
        "integration": {
            "implementation": "nsb-data-tools::gaia_xp::integrate_photon_flux",
            "band_nm": [336.0, 650.0],
            "grid_step_nm": 2.0,
            "uncertainty": "independent_sample_quadrature_with_trapezoid_weights",
        },
        "generation_timestamp_utc": chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        "entries": entries,
    });
    atomic_write_json(&args.manifest, &manifest)?;
    println!(
        "reconstructed {} sources -> {}",
        entry_count,
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
    let source_id = source_id_from_path(coefficient_path)?;
    let output_path = output_dir.join(format!("{source_id}.csv"));
    if !integrate_only && output_path.is_file() {
        return Ok(serde_json::json!({
            "source_id": source_id,
            "status": "skipped_existing",
            "coefficient_path": coefficient_path.display().to_string(),
            "coefficient_sha256": sha256_file(coefficient_path)?,
            "output_path": output_path.display().to_string(),
            "output_sha256": sha256_file(&output_path)?,
        }));
    }

    let bytes = fs::read(coefficient_path)
        .with_context(|| format!("read coefficient file {}", coefficient_path.display()))?;
    let record = parse_datalink_gaiaxpy_csv(&bytes, &source_id)?;
    let product = calibrator.calibrate_record_product(&record)?;
    let integral = integrate_photon_flux(&product)?;
    let uncertainty = integral
        .uncertainty_ph_m2_s
        .context("calibrated product has no integrated uncertainty")?;
    if !integrate_only {
        write_normalized_spectrum_csv(&output_path, &product)?;
    }

    let mut entry = serde_json::json!({
        "source_id": source_id,
        "status": "reconstructed",
        "coefficient_path": coefficient_path.display().to_string(),
        "coefficient_sha256": sha256_file(coefficient_path)?,
        "band_nm": [336.0, 650.0],
        "grid_step_nm": 2.0,
        "samples": product.wavelengths_nm.len(),
        "flux_336_650_ph_m2_s": integral.total_ph_m2_s,
        "statistical_uncertainty_336_650_ph_m2_s": uncertainty,
        "negative_samples": integral.negative_samples,
        "negative_contribution_ratio": integral.negative_contribution_ratio,
    });
    if !integrate_only {
        entry["output_path"] = serde_json::Value::String(output_path.display().to_string());
        entry["output_sha256"] = serde_json::Value::String(sha256_file(&output_path)?);
    }
    Ok(entry)
}

fn write_normalized_spectrum_csv(path: &Path, product: &XpProduct) -> Result<()> {
    let errors = product
        .flux_error_w_m2_nm
        .as_ref()
        .context("calibrated XP continuous product has no spectral uncertainty")?;
    if product.wavelengths_nm.len() != product.flux_w_m2_nm.len()
        || errors.len() != product.wavelengths_nm.len()
    {
        bail!("normalized XP spectrum arrays have inconsistent lengths");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("csv.tmp");
    let mut writer = WriterBuilder::new()
        .from_path(&temporary)
        .with_context(|| format!("create temporary spectrum {}", temporary.display()))?;
    writer.write_record([
        "source_id",
        NORMALIZED_WAVELENGTH_COLUMN,
        NORMALIZED_FLUX_COLUMN,
        NORMALIZED_FLUX_ERROR_COLUMN,
    ])?;
    let wavelengths = format_series(&product.wavelengths_nm, false);
    let flux = format_series(&product.flux_w_m2_nm, true);
    let flux_error = format_series(errors, true);
    writer.write_record([product.source_id.as_str(), &wavelengths, &flux, &flux_error])?;
    writer.flush()?;
    fs::rename(&temporary, path)
        .with_context(|| format!("publish normalized spectrum {}", path.display()))?;
    Ok(())
}

fn source_id_from_path(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .with_context(|| format!("coefficient path has no UTF-8 file stem: {}", path.display()))?;
    let source_id = stem.strip_prefix("xp_source_").unwrap_or(stem);
    source_id
        .parse::<u64>()
        .with_context(|| format!("coefficient filename does not encode a Gaia source_id: {stem}"))?;
    Ok(source_id.to_string())
}

fn atomic_write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write temporary manifest {}", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("publish manifest {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_legacy_xp_source_prefix() {
        assert_eq!(
            source_id_from_path(Path::new("xp_source_123.csv")).unwrap(),
            "123"
        );
        assert_eq!(source_id_from_path(Path::new("456.csv")).unwrap(), "456");
    }

    #[test]
    fn rejects_non_source_filenames() {
        assert!(source_id_from_path(Path::new("coefficients.csv")).is_err());
    }
}
