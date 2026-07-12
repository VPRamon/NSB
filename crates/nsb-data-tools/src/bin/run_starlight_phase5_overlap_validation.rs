//! Full Phase 5 overlap validation against canonical XP sampled flux.

use anyhow::Result;
use clap::Parser;
use csv::WriterBuilder;
use nsb_data_tools::gaia_xp_continuous::{integrate_reconstructed_csv, PHOTOMETRY_MODEL};
use nsb_data_tools::starlight_phase5::{
    bp_rp_excess_bin, build_frozen_validation_policy, build_overlap_exclusions, colour_bin,
    compute_metrics, crowding_bin, evaluate_gates, fit_uncertainty_inflation, g_mag_bin,
    hash_sorted_source_ids, load_canonical_sampled_flux, load_phase5_targets,
    load_sampled_catalogue_exclusions, qso_galaxy_bin, signed_relative_error, sky_region, snr_bin,
    write_phase5_exclusions_csv, write_sha256sum, GateEvaluation, MetricBundle, OverlapComparison,
    Phase5TargetRow, XpContinuousGates, CATASTROPHIC_RELATIVE_ERROR, STRATUM_MIN_SAMPLES,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/gaia_dr3_starlight_exclusions.csv"
    )]
    exclusions_csv: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
    #[arg(long)]
    output_md: PathBuf,
    #[arg(long)]
    predictions_csv: PathBuf,
    #[arg(long)]
    stratified_csv: PathBuf,
    #[arg(long)]
    frozen_policy_json: PathBuf,
    #[arg(long)]
    phase5_exclusions_csv: PathBuf,
    #[arg(long)]
    phase5_root: PathBuf,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Serialize)]
struct OverlapValidationReport {
    schema_version: u32,
    photometry_model: String,
    catastrophic_relative_error_threshold: f64,
    uncertainty_inflation_factor: f64,
    inflation_fit_split: String,
    frozen_policy_path: String,
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

fn build_comparisons(
    targets: &[Phase5TargetRow],
    canonical: &HashMap<u64, f64>,
    reconstructed_dir: &Path,
    inflation: f64,
) -> Result<Vec<OverlapComparison>> {
    let mut out = Vec::new();
    for target in targets {
        let path = reconstructed_dir.join(format!("{}.csv", target.source_id));
        if !path.is_file() {
            continue;
        }
        let Some(sampled) = canonical.get(&target.source_id) else {
            continue;
        };
        let sampled = *sampled;
        let (_, integral) = integrate_reconstructed_csv(&path)?;
        let stat = integral.uncertainty_ph_m2_s.unwrap_or(0.0);
        let total = stat * inflation;
        let reconstructed = integral.total_ph_m2_s;
        let relative_error = signed_relative_error(reconstructed, sampled);
        out.push(OverlapComparison {
            source_id: target.source_id,
            split: target.split.clone(),
            sampled_flux_ph_m2_s: sampled,
            reconstructed_flux_ph_m2_s: reconstructed,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: total,
            relative_error,
            phot_g_mean_mag: target.phot_g_mean_mag,
            bp_rp: target.bp_rp,
            phot_g_snr: target.phot_g_mean_flux_over_error,
            phot_bp_rp_excess_factor: target.phot_bp_rp_excess_factor,
            l: target.l,
            b: target.b,
            g_mag_bin: g_mag_bin(target.phot_g_mean_mag).to_string(),
            colour_bin: colour_bin(target.bp_rp).to_string(),
            snr_bin: snr_bin(target.phot_g_mean_flux_over_error).to_string(),
            sky_region: sky_region(target.l, target.b).to_string(),
            bp_rp_excess_bin: bp_rp_excess_bin(target.phot_bp_rp_excess_factor).to_string(),
            crowding_bin: crowding_bin(
                target.phot_bp_n_blended_transits,
                target.phot_rp_n_blended_transits,
            )
            .to_string(),
            duplicated_bin: if target.duplicated_source {
                "duplicated".to_string()
            } else {
                "unique".to_string()
            },
            variable_bin: target.phot_variable_flag.clone(),
            qso_galaxy_bin: qso_galaxy_bin(target.in_qso_candidates, target.in_galaxy_candidates)
                .to_string(),
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

fn write_stratum_block(
    writer: &mut csv::Writer<std::fs::File>,
    kind: &str,
    rows: &[OverlapComparison],
    key: impl Fn(&OverlapComparison) -> &str,
    inflation: f64,
) -> Result<()> {
    for (label, metrics) in metrics_by_key(rows, key, inflation) {
        let evidence = if metrics.sample_count < STRATUM_MIN_SAMPLES {
            "insufficient_sample"
        } else {
            "evaluated"
        };
        writer.write_record([
            kind,
            label.as_str(),
            &metrics.sample_count.to_string(),
            &metrics.mean_signed_relative_bias.to_string(),
            &metrics.median_signed_relative_bias.to_string(),
            &metrics.p95_abs_relative_error.to_string(),
            &metrics.coverage_68.to_string(),
            &metrics.coverage_95.to_string(),
            &metrics.catastrophic_outlier_fraction.to_string(),
            evidence,
        ])?;
    }
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
        "mean_signed_relative_bias",
        "median_signed_relative_bias",
        "p95_abs_relative_error",
        "coverage_68",
        "coverage_95",
        "catastrophic_outlier_fraction",
        "evaluation_status",
    ])?;
    write_stratum_block(
        &mut writer,
        "g_mag_bin",
        rows,
        |row| row.g_mag_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "colour_bin",
        rows,
        |row| row.colour_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "snr_bin",
        rows,
        |row| row.snr_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "bp_rp_excess_bin",
        rows,
        |row| row.bp_rp_excess_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "crowding_bin",
        rows,
        |row| row.crowding_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "sky_region",
        rows,
        |row| row.sky_region.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "duplicated_bin",
        rows,
        |row| row.duplicated_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "variable_bin",
        rows,
        |row| row.variable_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "qso_galaxy_bin",
        rows,
        |row| row.qso_galaxy_bin.as_str(),
        inflation,
    )?;
    write_stratum_block(
        &mut writer,
        "split",
        rows,
        |row| row.split.as_str(),
        inflation,
    )?;
    writer.flush()?;
    Ok(())
}

fn render_md(report: &OverlapValidationReport) -> String {
    format!(
        "# Phase 5 overlap validation\n\n- Model: {}\n- Inflation factor (train fit): {:.3}\n- Catastrophic outlier threshold: {:.0}%\n- Frozen policy: {}\n\n## Global\n- n={}\n- flux-weighted bias: {:.4}\n- median rel bias: {:.4}\n- p95 abs rel err: {:.4}\n- 68% coverage: {:.3}\n- 95% coverage: {:.3}\n\n## Validation gates\n- passed: {}\n\n## Test gates\n- passed: {}\n",
        report.photometry_model,
        report.uncertainty_inflation_factor,
        report.catastrophic_relative_error_threshold * 100.0,
        report.frozen_policy_path,
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
    args.exclusions_csv = expand(args.exclusions_csv);
    args.phase5_root = expand(args.phase5_root);

    let targets = load_phase5_targets(&args.overlap_targets)?;
    let source_ids: HashSet<_> = targets.iter().map(|t| t.source_id).collect();
    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &source_ids)?;
    let canonical_map = canonical.flux_by_source;
    let catalogue_exclusions = load_sampled_catalogue_exclusions(&args.exclusions_csv)?;

    let train_only = build_comparisons(&targets, &canonical_map, &args.reconstructed_dir, 1.0)?;
    let train_rows: Vec<_> = train_only
        .iter()
        .filter(|row| row.split == "train")
        .cloned()
        .collect();
    let inflation = fit_uncertainty_inflation(&train_rows);

    let train_hash = hash_sorted_source_ids(
        &targets
            .iter()
            .filter(|t| t.split == "train")
            .map(|t| t.source_id)
            .collect::<Vec<_>>(),
    );
    let validation_hash = hash_sorted_source_ids(
        &targets
            .iter()
            .filter(|t| t.split == "validation")
            .map(|t| t.source_id)
            .collect::<Vec<_>>(),
    );
    let policy =
        build_frozen_validation_policy(inflation, &git_commit(), &train_hash, &validation_hash);
    if let Some(parent) = args.frozen_policy_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.frozen_policy_json,
        serde_json::to_string_pretty(&policy)? + "\n",
    )?;

    let comparisons =
        build_comparisons(&targets, &canonical_map, &args.reconstructed_dir, inflation)?;

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
        frozen_policy_path: args.frozen_policy_json.display().to_string(),
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

    let inventory_rows: Vec<_> = targets
        .iter()
        .map(
            |target| nsb_data_tools::starlight_phase5::DownloadInventoryRow {
                source_id: target.source_id,
                population: target.population.clone(),
                split: target.split.clone(),
                strata: target.strata.clone(),
                batch_id: "overlap-validation".to_string(),
                requested: true,
                response_present: args
                    .reconstructed_dir
                    .join(format!("{}.csv", target.source_id))
                    .is_file(),
                response_sha256: String::new(),
                parse_status: String::new(),
                classification: String::new(),
                reconstruction_status: String::new(),
                validation_target_available: canonical_map.contains_key(&target.source_id),
                exclusion_reason: String::new(),
                retry_count: 0,
                last_error: String::new(),
            },
        )
        .collect();
    let exclusions = build_overlap_exclusions(&targets, &catalogue_exclusions, &inventory_rows);
    write_phase5_exclusions_csv(&args.phase5_exclusions_csv, &exclusions)?;

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
            args.frozen_policy_json.clone(),
            args.phase5_exclusions_csv.clone(),
        ],
    )?;

    println!(
        "overlap validation: n={} validation_gates={} test_gates={}",
        report.global.sample_count, report.validation_gates.passed, report.test_gates.passed
    );
    Ok(())
}
