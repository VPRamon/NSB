//! Full Phase 5 overlap validation against canonical XP sampled flux.

use anyhow::Result;
use clap::Parser;
use csv::WriterBuilder;
use nsb_data_tools::gaia_xp_continuous::{integrate_reconstructed_csv, PHOTOMETRY_MODEL};
use nsb_data_tools::starlight_phase5::{
    colour_bin, compute_metrics, evaluate_gates, fit_uncertainty_inflation, g_mag_bin,
    load_canonical_sampled_flux, sky_region, snr_bin, write_sha256sum, GateEvaluation,
    MetricBundle, OverlapComparison, XpContinuousGates, CATASTROPHIC_RELATIVE_ERROR,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    overlap_targets: PathBuf,
    #[arg(long)]
    reconstructed_dir: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/gaia_dr3_starlight_sources.csv"
    )]
    canonical_catalogue: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
    #[arg(long)]
    output_md: PathBuf,
    #[arg(long)]
    predictions_csv: PathBuf,
    #[arg(long)]
    stratified_csv: PathBuf,
    #[arg(long)]
    phase5_root: PathBuf,
}

type OverlapTargetRow = (
    u64,
    String,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

#[derive(Debug, Serialize)]
struct OverlapValidationReport {
    schema_version: u32,
    photometry_model: String,
    catastrophic_relative_error_threshold: f64,
    uncertainty_inflation_factor: f64,
    inflation_fit_split: String,
    by_split: BTreeMap<String, MetricBundle>,
    global: MetricBundle,
    validation: MetricBundle,
    test: MetricBundle,
    train: MetricBundle,
    validation_gates: GateEvaluation,
    test_gates: GateEvaluation,
    global_gates: GateEvaluation,
    limitations: Vec<String>,
}

fn load_targets(path: &Path) -> Result<Vec<OverlapTargetRow>> {
    let mut reader = csv::ReaderBuilder::new().from_path(path)?;
    let headers = reader.headers()?.clone();
    let idx = |name: &str| headers.iter().position(|h| h == name);
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let source_id: u64 = record.get(idx("source_id").unwrap()).unwrap().parse()?;
        let split = record.get(idx("split").unwrap()).unwrap().to_string();
        let g = idx("phot_g_mean_mag")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        let bp_rp = idx("bp_rp")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        let snr = idx("phot_g_mean_flux_over_error")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        let l = idx("l")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        let b = idx("b")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse().ok());
        rows.push((source_id, split, g, bp_rp, snr, l, b));
    }
    Ok(rows)
}

fn build_comparisons(
    targets: &[OverlapTargetRow],
    canonical: &HashMap<u64, f64>,
    reconstructed_dir: &Path,
    inflation: f64,
) -> Result<Vec<OverlapComparison>> {
    let mut out = Vec::new();
    for (source_id, split, g, bp_rp, snr, l, b) in targets {
        let path = reconstructed_dir.join(format!("{source_id}.csv"));
        if !path.is_file() {
            continue;
        }
        let Some(sampled) = canonical.get(source_id) else {
            continue;
        };
        let sampled = *sampled;
        let (_, integral) = integrate_reconstructed_csv(&path)?;
        let stat = integral.uncertainty_ph_m2_s.unwrap_or(0.0);
        let total = stat * inflation;
        let reconstructed = integral.total_ph_m2_s;
        let relative_error = if sampled.abs() > 0.0 {
            (reconstructed - sampled) / sampled
        } else {
            f64::INFINITY
        };
        out.push(OverlapComparison {
            source_id: *source_id,
            split: split.clone(),
            sampled_flux_ph_m2_s: sampled,
            reconstructed_flux_ph_m2_s: reconstructed,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: total,
            relative_error,
            phot_g_mean_mag: *g,
            bp_rp: *bp_rp,
            phot_g_snr: *snr,
            phot_bp_rp_excess_factor: None,
            l: *l,
            b: *b,
            g_mag_bin: g_mag_bin(*g).to_string(),
            colour_bin: colour_bin(*bp_rp).to_string(),
            snr_bin: snr_bin(*snr).to_string(),
            sky_region: sky_region(*l, *b).to_string(),
        });
    }
    Ok(out)
}

fn metrics_by_key(
    rows: &[OverlapComparison],
    key: impl Fn(&OverlapComparison) -> &str,
    inflation: f64,
) -> BTreeMap<String, MetricBundle> {
    let mut groups: BTreeMap<String, Vec<&OverlapComparison>> = BTreeMap::new();
    for row in rows {
        groups.entry(key(row).to_string()).or_default().push(row);
    }
    groups
        .into_iter()
        .map(|(label, group)| {
            let owned: Vec<OverlapComparison> = group.into_iter().cloned().collect();
            (label, compute_metrics(&owned, inflation))
        })
        .collect()
}

fn write_predictions(path: &Path, rows: &[OverlapComparison]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = WriterBuilder::new().from_path(path)?;
    writer.write_record([
        "source_id",
        "split",
        "sampled_flux_ph_m2_s",
        "reconstructed_flux_ph_m2_s",
        "relative_error",
        "statistical_uncertainty_ph_m2_s",
        "total_uncertainty_ph_m2_s",
    ])?;
    for row in rows {
        writer.write_record([
            row.source_id.to_string(),
            row.split.clone(),
            row.sampled_flux_ph_m2_s.to_string(),
            row.reconstructed_flux_ph_m2_s.to_string(),
            row.relative_error.to_string(),
            row.statistical_uncertainty_ph_m2_s.to_string(),
            row.total_uncertainty_ph_m2_s.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_stratified(path: &Path, rows: &[OverlapComparison], inflation: f64) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = WriterBuilder::new().from_path(path)?;
    writer.write_record([
        "stratum_type",
        "stratum",
        "sample_count",
        "median_signed_relative_bias",
        "p95_abs_relative_error",
        "coverage_68",
        "coverage_95",
    ])?;
    for (label, metrics) in metrics_by_key(rows, |row| row.g_mag_bin.as_str(), inflation) {
        writer.write_record([
            "g_mag_bin",
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
        ])?;
    }
    for (label, metrics) in metrics_by_key(rows, |row| row.colour_bin.as_str(), inflation) {
        writer.write_record([
            "colour_bin",
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
        ])?;
    }
    for (label, metrics) in metrics_by_key(rows, |row| row.snr_bin.as_str(), inflation) {
        writer.write_record([
            "snr_bin",
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
        ])?;
    }
    for (label, metrics) in metrics_by_key(rows, |row| row.sky_region.as_str(), inflation) {
        writer.write_record([
            "sky_region",
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
        ])?;
    }
    for (label, metrics) in metrics_by_key(rows, |row| row.split.as_str(), inflation) {
        writer.write_record([
            "split",
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn render_md(report: &OverlapValidationReport) -> String {
    format!(
        "# Phase 5 overlap validation\n\n- Model: {}\n- Inflation factor (train fit): {:.3}\n- Catastrophic outlier threshold: {:.0}%\n\n## Global\n- n={}\n- flux-weighted bias: {:.4}\n- median rel bias: {:.4}\n- p95 abs rel err: {:.4}\n- 68% coverage: {:.3}\n- 95% coverage: {:.3}\n\n## Validation gates\n- passed: {}\n\n## Test gates\n- passed: {}\n",
        report.photometry_model,
        report.uncertainty_inflation_factor,
        report.catastrophic_relative_error_threshold * 100.0,
        report.global.sample_count,
        report.global.flux_weighted_integrated_bias,
        report.global.median_signed_relative_bias,
        report.global.p95_abs_relative_error,
        report.global.coverage_68,
        report.global.coverage_95,
        report.validation_gates.passed,
        report.test_gates.passed,
    )
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    args.canonical_catalogue = expand(args.canonical_catalogue);
    args.phase5_root = expand(args.phase5_root);

    let targets = load_targets(&args.overlap_targets)?;
    let source_ids: HashSet<_> = targets.iter().map(|(id, ..)| *id).collect();
    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &source_ids)?;
    let canonical_map = canonical.flux_by_source;

    let mut train_rows = Vec::new();
    let mut comparisons =
        build_comparisons(&targets, &canonical_map, &args.reconstructed_dir, 1.0)?;
    for row in &comparisons {
        if row.split == "train" {
            train_rows.push(row.clone());
        }
    }
    let inflation = fit_uncertainty_inflation(&train_rows);
    comparisons = build_comparisons(&targets, &canonical_map, &args.reconstructed_dir, inflation)?;

    let gates = XpContinuousGates::default();
    let by_split = ["train", "validation", "test"]
        .into_iter()
        .map(|split| {
            let subset: Vec<_> = comparisons
                .iter()
                .filter(|r| r.split == split)
                .cloned()
                .collect();
            (split.to_string(), compute_metrics(&subset, inflation))
        })
        .collect::<BTreeMap<_, _>>();

    let report = OverlapValidationReport {
        schema_version: 1,
        photometry_model: PHOTOMETRY_MODEL.to_string(),
        catastrophic_relative_error_threshold: CATASTROPHIC_RELATIVE_ERROR,
        uncertainty_inflation_factor: inflation,
        inflation_fit_split: "train".to_string(),
        global: compute_metrics(&comparisons, inflation),
        train: by_split.get("train").cloned().unwrap_or_default(),
        validation: by_split.get("validation").cloned().unwrap_or_default(),
        test: by_split.get("test").cloned().unwrap_or_default(),
        by_split: by_split.clone(),
        validation_gates: evaluate_gates(
            by_split
                .get("validation")
                .unwrap_or(&MetricBundle::default()),
            &gates,
        ),
        test_gates: evaluate_gates(
            by_split.get("test").unwrap_or(&MetricBundle::default()),
            &gates,
        ),
        global_gates: evaluate_gates(&compute_metrics(&comparisons, inflation), &gates),
        limitations: vec![
            "sampled target flux from canonical gaia_dr3_starlight_sources.csv".to_string(),
            format!(
                "missing reconstructed spectra skipped: {}",
                targets.len().saturating_sub(comparisons.len())
            ),
            format!(
                "overlap targets absent from canonical catalogue: {}",
                canonical.missing_source_ids.len()
            ),
        ],
    };

    write_predictions(&args.predictions_csv, &comparisons)?;
    write_stratified(&args.stratified_csv, &comparisons, inflation)?;
    if let Some(parent) = args.output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    fs::write(&args.output_md, render_md(&report))?;
    write_sha256sum(
        &args.phase5_root,
        &[
            args.output_json.clone(),
            args.output_md.clone(),
            args.predictions_csv.clone(),
            args.stratified_csv.clone(),
        ],
    )?;

    println!(
        "overlap validation: n={} validation_gates={} test_gates={}",
        report.global.sample_count, report.validation_gates.passed, report.test_gates.passed
    );
    Ok(())
}
