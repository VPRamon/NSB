//! Compare GaiaXPy-reconstructed XP continuous integrals against measured XP sampled
//! spectra for the preregistered overlap validation population.

use anyhow::{Context, Result};
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::gaia_xp::integrate_photon_flux;
use nsb_data_tools::gaia_xp::{
    parse_gaia_datalink_array_csv, parse_gaia_datalink_csv, BAND_MAX_NM, BAND_MIN_NM,
};
use nsb_data_tools::gaia_xp_continuous::{
    integrate_reconstructed_csv, PHOTOMETRY_MODEL, PINNED_GAIA_XPY_VERSION,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Validate XP continuous reconstruction against XP sampled overlap sources")]
struct Args {
    /// Deduplicated stratified sample table from Phase 4.
    #[arg(long)]
    sample_sources: PathBuf,
    /// Directory with GaiaXPy normalized reconstructed spectra (`{source_id}.csv`).
    #[arg(long)]
    reconstructed_dir: PathBuf,
    /// Directory with raw Gaia DataLink XP_SAMPLED CSV files.
    #[arg(long)]
    sampled_dir: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
}

#[derive(Debug, Serialize)]
struct SourceComparison {
    source_id: String,
    sampled_ph_m2_s: f64,
    reconstructed_ph_m2_s: f64,
    relative_error: f64,
    phot_g_mean_mag: Option<f64>,
    bp_rp: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    schema_version: u32,
    photometry_model: &'static str,
    gaiaxpy_version: &'static str,
    band_nm: [f64; 2],
    overlap_sources_compared: usize,
    mean_relative_error: f64,
    median_relative_error: f64,
    max_abs_relative_error: f64,
    by_g_mag_bin: BTreeMap<String, f64>,
    by_colour_bin: BTreeMap<String, f64>,
    sources: Vec<SourceComparison>,
    limitations: Vec<String>,
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

fn g_bin(g: Option<f64>) -> &'static str {
    match g {
        Some(value) if value < 10.0 => "g_bright",
        Some(value) if value < 14.0 => "g_intermediate",
        Some(value) if value < 17.0 => "g_faint",
        Some(_) => "g_very_faint",
        None => "g_missing",
    }
}

fn colour_bin(bp_rp: Option<f64>) -> &'static str {
    match bp_rp {
        Some(value) if value < 0.5 => "colour_blue",
        Some(value) if value < 1.5 => "colour_solar",
        Some(value) if value < 2.5 => "colour_red",
        Some(_) => "colour_very_red",
        None => "colour_missing",
    }
}

fn integrate_sampled(path: &Path, source_id: &str) -> Result<f64> {
    let bytes = fs::read(path).with_context(|| format!("read sampled XP {}", path.display()))?;
    let product = parse_gaia_datalink_array_csv(&bytes, source_id)
        .or_else(|_| parse_gaia_datalink_csv(&bytes, source_id))?;
    Ok(integrate_photon_flux(&product)?.total_ph_m2_s)
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        0.5 * (sorted[mid - 1] + sorted[mid])
    } else {
        sorted[mid]
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Run the `validate_xp_continuous_reconstruction` command using process arguments.
pub fn run_cli() -> Result<()> {
    let args = Args::parse();
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(&args.sample_sources)
        .with_context(|| format!("open {}", args.sample_sources.display()))?;
    let headers = reader.headers()?.clone();
    let idx = |name: &str| headers.iter().position(|field| field == name);

    let mut comparisons = Vec::new();
    let mut missing_reconstructed = 0usize;
    let mut missing_sampled = 0usize;

    for record in reader.records() {
        let record = record?;
        let has_continuous = idx("has_xp_continuous")
            .and_then(|i| record.get(i))
            .is_some_and(parse_bool);
        let has_sampled = idx("has_xp_sampled")
            .and_then(|i| record.get(i))
            .is_some_and(parse_bool);
        if !(has_continuous && has_sampled) {
            continue;
        }
        let source_id = idx("source_id")
            .and_then(|i| record.get(i))
            .context("missing source_id")?
            .to_string();
        let reconstructed_path = args.reconstructed_dir.join(format!("{source_id}.csv"));
        let sampled_path = args.sampled_dir.join(format!("{source_id}.csv"));
        if !reconstructed_path.is_file() {
            missing_reconstructed += 1;
            continue;
        }
        if !sampled_path.is_file() {
            missing_sampled += 1;
            continue;
        }
        let (_, reconstructed) = integrate_reconstructed_csv(&reconstructed_path)?;
        let sampled = integrate_sampled(&sampled_path, &source_id)?;
        let reconstructed_ph_m2_s = reconstructed.total_ph_m2_s;
        let relative_error = if sampled.abs() > 0.0 {
            (reconstructed_ph_m2_s - sampled) / sampled
        } else {
            f64::INFINITY
        };
        let phot_g_mean_mag = idx("phot_g_mean_mag")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        let bp_rp = idx("bp_rp")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        comparisons.push(SourceComparison {
            source_id,
            sampled_ph_m2_s: sampled,
            reconstructed_ph_m2_s,
            relative_error,
            phot_g_mean_mag,
            bp_rp,
        });
    }

    let relative_errors: Vec<f64> = comparisons
        .iter()
        .map(|entry| entry.relative_error)
        .collect();
    let mut g_mag_errors: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut colour_errors: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for entry in &comparisons {
        g_mag_errors
            .entry(g_bin(entry.phot_g_mean_mag).to_string())
            .or_default()
            .push(entry.relative_error);
        colour_errors
            .entry(colour_bin(entry.bp_rp).to_string())
            .or_default()
            .push(entry.relative_error);
    }
    let by_g_mag_bin = g_mag_errors
        .into_iter()
        .map(|(key, values)| (key, mean(&values)))
        .collect();
    let by_colour_bin = colour_errors
        .into_iter()
        .map(|(key, values)| (key, mean(&values)))
        .collect();

    let report = ValidationReport {
        schema_version: 1,
        photometry_model: PHOTOMETRY_MODEL,
        gaiaxpy_version: PINNED_GAIA_XPY_VERSION,
        band_nm: [BAND_MIN_NM, BAND_MAX_NM],
        overlap_sources_compared: comparisons.len(),
        mean_relative_error: mean(&relative_errors),
        median_relative_error: median(&relative_errors),
        max_abs_relative_error: relative_errors
            .iter()
            .map(|value| value.abs())
            .fold(0.0, f64::max),
        by_g_mag_bin,
        by_colour_bin,
        sources: comparisons,
        limitations: vec![
            format!("missing reconstructed files skipped: {missing_reconstructed}"),
            format!("missing sampled files skipped: {missing_sampled}"),
        ],
    };

    if let Some(parent) = args.output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!(
        "validated {} overlap sources; median relative error {:.4}",
        report.overlap_sources_compared, report.median_relative_error
    );
    Ok(())
}
