//! Official one-shot Phase 5 holdout v1 validation with frozen policy v1.

use anyhow::{bail, Result};
use clap::Parser;
use csv::WriterBuilder;
use nsb_data_tools::gaia_xp_continuous::{integrate_reconstructed_csv, PHOTOMETRY_MODEL};
use nsb_data_tools::starlight_phase5::{
    bp_rp_excess_bin, colour_bin, crowding_bin, evaluate_gates, g_mag_bin,
    load_canonical_sampled_flux, load_phase5_targets, qso_galaxy_bin, signed_relative_error,
    sky_region, snr_bin, MetricBundle, OverlapComparison, STRATUM_MIN_SAMPLES,
};
use nsb_data_tools::starlight_phase5_holdout::{
    assert_official_evaluation_not_done, coverage_wilson_intervals, load_execution_manifest,
    verify_execution_manifest, WilsonInterval, HOLDOUT_ID, OFFICIAL_EVALUATION_FILENAME,
};
use nsb_data_tools::starlight_phase5_uncertainty::{
    compute_overlap_metrics, compute_uncertainty_diagnostics, overlap_difference_sigma,
    FrozenValidationPolicyV1,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    holdout_root: PathBuf,
    #[arg(long)]
    execution_manifest: PathBuf,
    #[arg(long)]
    preflight_json: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/gaia_dr3_starlight_sources.csv"
    )]
    canonical_catalogue: PathBuf,
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
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[derive(Debug, Serialize)]
struct OfficialEvaluationRecord {
    schema_version: u32,
    execution_id: String,
    evaluation_attempt: u32,
    official: bool,
    started_at: String,
    completed_at: String,
    exit_status: String,
    software_commit: String,
    policy_id: String,
    policy_checksum: String,
    holdout_checksum: String,
    metrics: MetricBundle,
    coverage_68_wilson: WilsonInterval,
    coverage_95_wilson: WilsonInterval,
    gates: nsb_data_tools::starlight_phase5::GateEvaluation,
    gate_failures: Vec<String>,
    nan_count: u64,
    inf_count: u64,
    duplicate_count: u64,
    unexplained_missing_count: u64,
    final_verdict: String,
}

fn write_stratified(
    path: &Path,
    rows: &[OverlapComparison],
    model: &nsb_data_tools::starlight_phase5_uncertainty::OverlapDifferenceUncertaintyModel,
) -> Result<()> {
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
        "evaluation_status",
    ])?;
    let strata = [
        ("g_mag_bin", "g_mag_bin"),
        ("colour_bin", "colour_bin"),
        ("snr_bin", "snr_bin"),
        ("bp_rp_excess_bin", "bp_rp_excess_bin"),
        ("crowding_bin", "crowding_bin"),
        ("sky_region", "sky_region"),
        ("variable_bin", "variable_bin"),
        ("duplicated_bin", "duplicated_bin"),
        ("qso_galaxy_bin", "qso_galaxy_bin"),
    ];
    for (kind, field) in strata {
        let mut groups: BTreeMap<String, Vec<&OverlapComparison>> = BTreeMap::new();
        for row in rows {
            let label = match field {
                "g_mag_bin" => row.g_mag_bin.as_str(),
                "colour_bin" => row.colour_bin.as_str(),
                "snr_bin" => row.snr_bin.as_str(),
                "bp_rp_excess_bin" => row.bp_rp_excess_bin.as_str(),
                "crowding_bin" => row.crowding_bin.as_str(),
                "sky_region" => row.sky_region.as_str(),
                "variable_bin" => row.variable_bin.as_str(),
                "duplicated_bin" => row.duplicated_bin.as_str(),
                "qso_galaxy_bin" => row.qso_galaxy_bin.as_str(),
                _ => "",
            };
            groups.entry(label.to_string()).or_default().push(row);
        }
        for (label, group) in groups {
            let owned: Vec<OverlapComparison> = group.into_iter().cloned().collect();
            let m = compute_overlap_metrics(&owned, model);
            let status = if m.sample_count < STRATUM_MIN_SAMPLES {
                "insufficient_sample"
            } else {
                "evaluated"
            };
            writer.write_record([
                kind,
                label.as_str(),
                &m.sample_count.to_string(),
                &m.mean_signed_relative_bias.to_string(),
                &m.median_signed_relative_bias.to_string(),
                &m.p95_abs_relative_error.to_string(),
                &m.coverage_68.to_string(),
                &m.coverage_95.to_string(),
                status,
            ])?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn write_sha256sum(holdout_root: &Path, files: &[PathBuf]) -> Result<()> {
    let mut lines = Vec::new();
    for path in files {
        if path.is_file() {
            let digest = nsb_data_tools::checksum_io::sha256_file(path)?;
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            lines.push(format!("{digest}\t{name}"));
        }
    }
    lines.sort();
    fs::write(
        holdout_root.join("phase5_holdout_v1.sha256sum"),
        lines.join("\n") + "\n",
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    args.holdout_root = expand(args.holdout_root);
    args.execution_manifest = expand(args.execution_manifest);
    args.preflight_json = expand(args.preflight_json);
    args.canonical_catalogue = expand(args.canonical_catalogue);

    assert_official_evaluation_not_done(&args.holdout_root)?;

    let started_at = now_iso();
    let execution = load_execution_manifest(&args.execution_manifest)?;
    verify_execution_manifest(&execution)?;

    let preflight: nsb_data_tools::starlight_phase5_holdout::HoldoutPreflightReport =
        serde_json::from_str(&fs::read_to_string(&args.preflight_json)?)?;
    if !preflight.passed {
        bail!("preflight failed: {:?}", preflight.failures);
    }

    let policy_path = PathBuf::from(&execution.policy_path);
    let policy: FrozenValidationPolicyV1 =
        serde_json::from_str(&fs::read_to_string(&policy_path)?)?;
    if policy.status != "frozen" {
        bail!("policy not frozen");
    }
    let overlap_model = policy.uncertainty_model.overlap_difference.clone();

    let sources_path = args.holdout_root.join("phase5_holdout_v1_sources.csv");
    let reconstructed_dir = args.holdout_root.join("reconstruction/normalized");
    let targets = load_phase5_targets(&sources_path)?;
    let source_ids: HashSet<_> = targets.iter().map(|t| t.source_id).collect();
    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &source_ids)?;

    let mut comparisons = Vec::new();
    let mut missing_reconstruction = 0_u64;
    let mut missing_canonical = 0_u64;
    let mut nan_count = 0_u64;
    let mut inf_count = 0_u64;

    for target in &targets {
        let path = reconstructed_dir.join(format!("{}.csv", target.source_id));
        if !path.is_file() {
            missing_reconstruction += 1;
            continue;
        }
        let Some(sampled) = canonical.flux_by_source.get(&target.source_id) else {
            missing_canonical += 1;
            continue;
        };
        let (_, integral) = integrate_reconstructed_csv(&path)?;
        let stat = integral.uncertainty_ph_m2_s.unwrap_or(0.0);
        let reconstructed = integral.total_ph_m2_s;
        if !sampled.is_finite() || !reconstructed.is_finite() {
            nan_count += 1;
        }
        if sampled.is_infinite() || reconstructed.is_infinite() {
            inf_count += 1;
        }
        let relative_error = signed_relative_error(reconstructed, *sampled);
        let mut row = OverlapComparison {
            source_id: target.source_id,
            split: target.split.clone(),
            sampled_flux_ph_m2_s: *sampled,
            reconstructed_flux_ph_m2_s: reconstructed,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: 0.0,
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
        };
        row.total_uncertainty_ph_m2_s = overlap_difference_sigma(&row, &overlap_model);
        comparisons.push(row);
    }

    let duplicate_count = {
        let mut seen = HashSet::new();
        comparisons
            .iter()
            .filter(|r| !seen.insert(r.source_id))
            .count() as u64
    };
    let unexplained_missing = missing_reconstruction + missing_canonical;

    if unexplained_missing > 0 || duplicate_count > 0 || nan_count > 0 || inf_count > 0 {
        bail!(
            "data quality gate failed: missing={unexplained_missing} duplicates={duplicate_count} nan={nan_count} inf={inf_count}"
        );
    }

    let global = compute_overlap_metrics(&comparisons, &overlap_model);
    let gates = evaluate_gates(&global, &policy.gates);
    let (coverage_68_wilson, coverage_95_wilson) =
        coverage_wilson_intervals(&comparisons, &overlap_model);
    let diagnostics = compute_uncertainty_diagnostics(&comparisons, &overlap_model);

    let final_verdict = if gates.passed {
        "PHASE 5 SCIENTIFIC VALIDATION PASSED".to_string()
    } else {
        format!("PHASE 5 HOLDOUT V1 FAILED — {}", gates.failures.join("; "))
    };

    let validation_json = args.holdout_root.join("phase5_holdout_v1_validation.json");
    let validation_md = args.holdout_root.join("phase5_holdout_v1_validation.md");
    let predictions_csv = args.holdout_root.join("phase5_holdout_v1_predictions.csv");
    let stratified_csv = args
        .holdout_root
        .join("phase5_holdout_v1_stratified_metrics.csv");
    let reconciliation_json = args
        .holdout_root
        .join("phase5_holdout_v1_reconciliation.json");
    let official_path = args.holdout_root.join(OFFICIAL_EVALUATION_FILENAME);

    let report = serde_json::json!({
        "schema_version": 1,
        "holdout_id": HOLDOUT_ID,
        "policy_id": policy.policy_id,
        "photometry_model": PHOTOMETRY_MODEL,
        "global": global,
        "gates": gates,
        "diagnostics": diagnostics,
        "coverage_68_wilson": coverage_68_wilson,
        "coverage_95_wilson": coverage_95_wilson,
        "coverage_uncertainty_kind": "overlap_difference",
    });
    fs::write(
        &validation_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;

    fs::write(
        &validation_md,
        format!(
            "# Phase 5 holdout v1 official validation\n\n- execution: {}\n- policy: {}\n- n={}\n- flux-weighted bias: {:.6}\n- median rel bias: {:.6}\n- p95 abs rel err: {:.6}\n- coverage 68%: {:.3} (Wilson [{:.3}, {:.3}])\n- coverage 95%: {:.3} (Wilson [{:.3}, {:.3}])\n- gates passed: {}\n- **{}**\n",
            execution.execution_id,
            policy.policy_id,
            global.sample_count,
            global.flux_weighted_integrated_bias,
            global.median_signed_relative_bias,
            global.p95_abs_relative_error,
            global.coverage_68,
            coverage_68_wilson.wilson_95_low,
            coverage_68_wilson.wilson_95_high,
            global.coverage_95,
            coverage_95_wilson.wilson_95_low,
            coverage_95_wilson.wilson_95_high,
            gates.passed,
            final_verdict,
        ),
    )?;

    let mut writer = WriterBuilder::new().from_path(&predictions_csv)?;
    writer.write_record([
        "source_id",
        "split",
        "sampled_flux_ph_m2_s",
        "reconstructed_flux_ph_m2_s",
        "relative_error",
        "statistical_uncertainty_ph_m2_s",
        "difference_uncertainty_ph_m2_s",
    ])?;
    for row in &comparisons {
        writer.write_record([
            row.source_id.to_string(),
            row.split.clone(),
            row.sampled_flux_ph_m2_s.to_string(),
            row.reconstructed_flux_ph_m2_s.to_string(),
            row.relative_error.to_string(),
            row.statistical_uncertainty_ph_m2_s.to_string(),
            overlap_difference_sigma(row, &overlap_model).to_string(),
        ])?;
    }
    writer.flush()?;

    write_stratified(&stratified_csv, &comparisons, &overlap_model)?;

    fs::write(
        &reconciliation_json,
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "holdout_id": HOLDOUT_ID,
            "requested": targets.len(),
            "evaluated": comparisons.len(),
            "reconciliation_ok": comparisons.len() == targets.len(),
        }))? + "\n",
    )?;

    let official = OfficialEvaluationRecord {
        schema_version: 1,
        execution_id: execution.execution_id.clone(),
        evaluation_attempt: 1,
        official: true,
        started_at,
        completed_at: now_iso(),
        exit_status: if gates.passed { "success" } else { "failed" }.to_string(),
        software_commit: git_commit(),
        policy_id: policy.policy_id.clone(),
        policy_checksum: execution.policy_checksum.clone(),
        holdout_checksum: execution.holdout_sources_checksum.clone(),
        metrics: global.clone(),
        coverage_68_wilson,
        coverage_95_wilson,
        gates: gates.clone(),
        gate_failures: gates.failures.clone(),
        nan_count,
        inf_count,
        duplicate_count,
        unexplained_missing_count: unexplained_missing,
        final_verdict: final_verdict.clone(),
    };
    fs::write(
        &official_path,
        serde_json::to_string_pretty(&official)? + "\n",
    )?;

    write_sha256sum(
        &args.holdout_root,
        &[
            validation_json,
            validation_md,
            predictions_csv,
            stratified_csv,
            reconciliation_json,
            official_path,
            args.holdout_root
                .join("phase5_holdout_v1_execution_manifest.json"),
        ],
    )?;

    println!("{final_verdict}");
    if !gates.passed {
        std::process::exit(1);
    }
    Ok(())
}
