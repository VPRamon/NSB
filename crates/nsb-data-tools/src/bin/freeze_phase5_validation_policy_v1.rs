//! Fit and freeze Phase 5 validation policy v1 using train + validation only.

use anyhow::Result;
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::gaia_xp_continuous::PHOTOMETRY_MODEL;
use nsb_data_tools::starlight_phase5::{
    evaluate_gates, hash_sorted_source_ids, load_phase5_targets, XpContinuousGates,
    CATASTROPHIC_RELATIVE_ERROR,
};
use nsb_data_tools::starlight_phase5_uncertainty::{
    compute_overlap_metrics, compute_uncertainty_diagnostics, fit_difference_inflation,
    fit_relative_residual_scale, AbsolutePhysicalUncertaintyModel, CoverageUncertaintyKind,
    FrozenValidationPolicyV1, OverlapDifferenceUncertaintyModel, UncertaintyModelBundle,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    overlap_targets: PathBuf,
    #[arg(long)]
    predictions_csv: PathBuf,
    #[arg(long)]
    output_policy_json: PathBuf,
    #[arg(long)]
    output_diagnostics_json: PathBuf,
    #[arg(long, default_value = "2.1.4")]
    gaiaxpy_version: String,
    #[arg(long, default_value = "gaia_xp_continuous_canonical_v1")]
    adapter_version: String,
    #[arg(
        long,
        default_value = "phase5-policy-v0-exploratory-no-explicit-uncertainty-model"
    )]
    archived_exploratory_policy: String,
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

#[derive(Debug, Default)]
struct PredictionRow {
    source_id: u64,
    split: String,
    sampled_flux_ph_m2_s: f64,
    reconstructed_flux_ph_m2_s: f64,
    relative_error: f64,
    statistical_uncertainty_ph_m2_s: f64,
}

fn load_predictions(path: &PathBuf) -> Result<Vec<PredictionRow>> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    let mut out = Vec::new();
    for row in reader.records() {
        let row = row?;
        out.push(PredictionRow {
            source_id: row.get(0).unwrap().parse()?,
            split: row.get(1).unwrap().to_string(),
            sampled_flux_ph_m2_s: row.get(2).unwrap().parse()?,
            reconstructed_flux_ph_m2_s: row.get(3).unwrap().parse()?,
            relative_error: row.get(4).unwrap().parse()?,
            statistical_uncertainty_ph_m2_s: row.get(5).unwrap().parse()?,
        });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
struct PolicyFitReport {
    schema_version: u32,
    policy_id: String,
    uncertainty_model: UncertaintyModelBundle,
    train_metrics: nsb_data_tools::starlight_phase5::MetricBundle,
    validation_metrics: nsb_data_tools::starlight_phase5::MetricBundle,
    validation_gates: nsb_data_tools::starlight_phase5::GateEvaluation,
    train_diagnostics: nsb_data_tools::starlight_phase5_uncertainty::UncertaintyDiagnostics,
    validation_diagnostics: nsb_data_tools::starlight_phase5_uncertainty::UncertaintyDiagnostics,
    note: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let overlap_targets = expand(args.overlap_targets);
    let predictions_csv = expand(args.predictions_csv);
    let output_policy_json = expand(args.output_policy_json);
    let output_diagnostics_json = expand(args.output_diagnostics_json);

    let targets = load_phase5_targets(&overlap_targets)?;
    let target_map: std::collections::HashMap<u64, _> =
        targets.iter().map(|t| (t.source_id, t)).collect();
    let predictions = load_predictions(&predictions_csv)?;

    let mut comparisons = Vec::new();
    for pred in predictions {
        let Some(target) = target_map.get(&pred.source_id) else {
            continue;
        };
        comparisons.push(nsb_data_tools::starlight_phase5::OverlapComparison {
            source_id: pred.source_id,
            split: pred.split.clone(),
            sampled_flux_ph_m2_s: pred.sampled_flux_ph_m2_s,
            reconstructed_flux_ph_m2_s: pred.reconstructed_flux_ph_m2_s,
            statistical_uncertainty_ph_m2_s: pred.statistical_uncertainty_ph_m2_s,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: pred.statistical_uncertainty_ph_m2_s,
            relative_error: pred.relative_error,
            phot_g_mean_mag: target.phot_g_mean_mag,
            bp_rp: target.bp_rp,
            phot_g_snr: target.phot_g_mean_flux_over_error,
            phot_bp_rp_excess_factor: target.phot_bp_rp_excess_factor,
            l: target.l,
            b: target.b,
            g_mag_bin: nsb_data_tools::starlight_phase5::g_mag_bin(target.phot_g_mean_mag)
                .to_string(),
            colour_bin: nsb_data_tools::starlight_phase5::colour_bin(target.bp_rp).to_string(),
            snr_bin: nsb_data_tools::starlight_phase5::snr_bin(target.phot_g_mean_flux_over_error)
                .to_string(),
            sky_region: nsb_data_tools::starlight_phase5::sky_region(target.l, target.b)
                .to_string(),
            bp_rp_excess_bin: nsb_data_tools::starlight_phase5::bp_rp_excess_bin(
                target.phot_bp_rp_excess_factor,
            )
            .to_string(),
            crowding_bin: nsb_data_tools::starlight_phase5::crowding_bin(
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
            qso_galaxy_bin: nsb_data_tools::starlight_phase5::qso_galaxy_bin(
                target.in_qso_candidates,
                target.in_galaxy_candidates,
            )
            .to_string(),
        });
    }

    let train: Vec<_> = comparisons
        .iter()
        .filter(|row| row.split == "train")
        .cloned()
        .collect();
    let validation: Vec<_> = comparisons
        .iter()
        .filter(|row| row.split == "validation")
        .cloned()
        .collect();

    let mut overlap_model = OverlapDifferenceUncertaintyModel {
        relative_residual_scale: fit_relative_residual_scale(&train),
        ..Default::default()
    };
    fit_difference_inflation(&validation, &mut overlap_model);

    let uncertainty_model = UncertaintyModelBundle {
        overlap_difference: overlap_model.clone(),
        absolute_physical: AbsolutePhysicalUncertaintyModel::default(),
        coverage_gate_kind: CoverageUncertaintyKind::OverlapDifference,
    };

    let train_metrics = compute_overlap_metrics(&train, &overlap_model);
    let validation_metrics = compute_overlap_metrics(&validation, &overlap_model);
    let validation_gates = evaluate_gates(&validation_metrics, &XpContinuousGates::default());

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

    let policy = FrozenValidationPolicyV1 {
        schema_version: 1,
        policy_id: "phase5_frozen_validation_policy_v1".to_string(),
        status: "frozen".to_string(),
        uncertainty_model: uncertainty_model.clone(),
        quality_filters: vec![
            "skip overlap sources without canonical sampled reference".to_string(),
            "skip sources without normalized reconstruction CSV".to_string(),
        ],
        systematic_floor_ph_m2_s: overlap_model.systematic_floor_ph_m2_s,
        outlier_policy: format!("catastrophic if |relative_error| > {CATASTROPHIC_RELATIVE_ERROR}"),
        fallbacks: vec![
            "missing_from_canonical_sampled_reference: exclude from overlap validation only"
                .to_string(),
        ],
        train_checksum: train_hash,
        validation_checksum: validation_hash,
        software_commit: git_commit(),
        gaiaxpy_version: args.gaiaxpy_version,
        adapter_version: args.adapter_version,
        photometry_model: PHOTOMETRY_MODEL.to_string(),
        integration_band_nm: [336.0, 650.0],
        gates: XpContinuousGates::default(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        archived_exploratory_policy: args.archived_exploratory_policy,
    };

    let report = PolicyFitReport {
        schema_version: 1,
        policy_id: policy.policy_id.clone(),
        uncertainty_model,
        train_metrics,
        validation_metrics: validation_metrics.clone(),
        validation_gates: validation_gates.clone(),
        train_diagnostics: compute_uncertainty_diagnostics(&train, &overlap_model),
        validation_diagnostics: compute_uncertainty_diagnostics(&validation, &overlap_model),
        note: "Policy v1 fit uses train for relative residual scale and validation for inflation. Test split and prior overlap inspection are not used for tuning.".to_string(),
    };

    if let Some(parent) = output_policy_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &output_policy_json,
        serde_json::to_string_pretty(&policy)? + "\n",
    )?;
    fs::write(
        &output_diagnostics_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;

    println!(
        "policy v1 frozen: validation_gates={} inflation={:.3} relative_scale={:.3e}",
        validation_gates.passed,
        overlap_model.inflation_factor,
        overlap_model.relative_residual_scale
    );
    Ok(())
}
