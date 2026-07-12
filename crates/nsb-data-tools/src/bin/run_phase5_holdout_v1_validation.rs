//! Evaluate frozen Phase 5 policy v1 on independent holdout v1 (single shot).

use anyhow::{Context, Result};
use clap::Parser;
use nsb_data_tools::gaia_xp_continuous::{integrate_reconstructed_csv, PHOTOMETRY_MODEL};
use nsb_data_tools::starlight_phase5::{
    bp_rp_excess_bin, colour_bin, crowding_bin, evaluate_gates, g_mag_bin,
    load_canonical_sampled_flux, load_phase5_targets, qso_galaxy_bin, signed_relative_error,
    sky_region, snr_bin, write_sha256sum, GateEvaluation, MetricBundle, OverlapComparison,
    XpContinuousGates, STRATUM_MIN_SAMPLES,
};
use nsb_data_tools::starlight_phase5_uncertainty::{
    compute_overlap_metrics, compute_uncertainty_diagnostics, overlap_difference_sigma,
    FrozenValidationPolicyV1,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    holdout_targets: PathBuf,
    #[arg(long)]
    reconstructed_dir: PathBuf,
    #[arg(long)]
    frozen_policy_json: PathBuf,
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
    reconciliation_json: PathBuf,
    #[arg(long)]
    holdout_root: PathBuf,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

#[derive(Debug, Serialize)]
struct HoldoutValidationReport {
    schema_version: u32,
    holdout_id: String,
    policy_id: String,
    photometry_model: String,
    global: MetricBundle,
    gates: GateEvaluation,
    diagnostics: nsb_data_tools::starlight_phase5_uncertainty::UncertaintyDiagnostics,
    by_stratum: BTreeMap<String, MetricBundle>,
    limitations: Vec<String>,
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    args.canonical_catalogue = expand(args.canonical_catalogue);
    args.holdout_root = expand(args.holdout_root);
    args.holdout_targets = expand(args.holdout_targets);
    args.reconstructed_dir = expand(args.reconstructed_dir);
    args.frozen_policy_json = expand(args.frozen_policy_json);
    args.output_json = expand(args.output_json);
    args.output_md = expand(args.output_md);
    args.predictions_csv = expand(args.predictions_csv);
    args.stratified_csv = expand(args.stratified_csv);
    args.reconciliation_json = expand(args.reconciliation_json);

    let policy: FrozenValidationPolicyV1 =
        serde_json::from_str(&fs::read_to_string(&args.frozen_policy_json)?)
            .context("parse frozen policy v1")?;
    if policy.status != "frozen" {
        anyhow::bail!("policy status must be frozen, got {}", policy.status);
    }
    let overlap_model = policy.uncertainty_model.overlap_difference.clone();

    let targets = load_phase5_targets(&args.holdout_targets)?;
    let source_ids: HashSet<_> = targets.iter().map(|t| t.source_id).collect();
    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &source_ids)?;

    let mut comparisons = Vec::new();
    let mut missing_reconstruction = 0_u64;
    let mut missing_canonical = 0_u64;
    for target in &targets {
        let path = args.reconstructed_dir.join(format!("{}.csv", target.source_id));
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

    let global = compute_overlap_metrics(&comparisons, &overlap_model);
    let gates = evaluate_gates(&global, &policy.gates);
    let diagnostics = compute_uncertainty_diagnostics(&comparisons, &overlap_model);

    let mut by_stratum: BTreeMap<String, Vec<&OverlapComparison>> = BTreeMap::new();
    for row in &comparisons {
        by_stratum
            .entry(row.g_mag_bin.clone())
            .or_default()
            .push(row);
    }
    let by_stratum_metrics = by_stratum
        .into_iter()
        .map(|(label, rows)| {
            let owned: Vec<OverlapComparison> = rows.into_iter().cloned().collect();
            (label, compute_overlap_metrics(&owned, &overlap_model))
        })
        .collect::<BTreeMap<_, _>>();

    let report = HoldoutValidationReport {
        schema_version: 1,
        holdout_id: "phase5_holdout_v1".to_string(),
        policy_id: policy.policy_id.clone(),
        photometry_model: PHOTOMETRY_MODEL.to_string(),
        global: global.clone(),
        gates: gates.clone(),
        diagnostics,
        by_stratum: by_stratum_metrics,
        limitations: vec![
            "single-shot holdout evaluation; policy v1 not retuned on this sample".to_string(),
            format!("missing reconstruction: {missing_reconstruction}"),
            format!("missing canonical sampled flux: {missing_canonical}"),
        ],
    };

    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    fs::write(
        &args.output_md,
        format!(
            "# Phase 5 holdout v1 validation\n\n- policy: {}\n- n={}\n- flux-weighted bias: {:.4}\n- median rel bias: {:.4}\n- p95 abs rel err: {:.4}\n- coverage 68%: {:.3}\n- coverage 95%: {:.3}\n- gates passed: {}\n",
            policy.policy_id,
            global.sample_count,
            global.flux_weighted_integrated_bias,
            global.median_signed_relative_bias,
            global.p95_abs_relative_error,
            global.coverage_68,
            global.coverage_95,
            gates.passed
        ),
    )?;

    let mut writer = csv::WriterBuilder::new().from_path(&args.predictions_csv)?;
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

    let reconciliation = serde_json::json!({
        "schema_version": 1,
        "holdout_id": "phase5_holdout_v1",
        "requested": targets.len(),
        "evaluated": comparisons.len(),
        "missing_reconstruction": missing_reconstruction,
        "missing_canonical": missing_canonical,
        "reconciliation_ok": missing_reconstruction + missing_canonical + comparisons.len() as u64 == targets.len() as u64,
    });
    fs::write(
        &args.reconciliation_json,
        serde_json::to_string_pretty(&reconciliation)? + "\n",
    )?;

    write_sha256sum(
        &args.holdout_root,
        &[
            args.output_json.clone(),
            args.output_md.clone(),
            args.predictions_csv.clone(),
            args.reconciliation_json.clone(),
        ],
    )?;

    println!(
        "holdout v1 validation: n={} gates_passed={}",
        global.sample_count, gates.passed
    );
    let _ = STRATUM_MIN_SAMPLES;
    let _ = XpContinuousGates::default();
    Ok(())
}
