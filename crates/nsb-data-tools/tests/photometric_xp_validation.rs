//! Photometric XP-scale validation against the pinned production artifact.

use nsb_data_tools::starlight::photometric::{
    PhotometricCorrection, PhotometricFeatures, PopulationBranch,
};
use serde::Deserialize;
use std::path::PathBuf;

const FIXTURE: &str = include_str!("fixtures/photometric_xp_validation_v1.json");

#[derive(Deserialize)]
struct ValidationFixture {
    production_artifact_sha256: String,
    fixture_artifact_sha256: String,
    cases: Vec<ValidationCase>,
    thresholds: Thresholds,
}

#[derive(Deserialize)]
struct ValidationCase {
    label: String,
    phot_g_mean_mag: f64,
    phot_bp_mean_mag: Option<f64>,
    phot_rp_mean_mag: Option<f64>,
    bp_rp: Option<f64>,
    xp_flux_336_650_ph_m2_s: f64,
    branch: String,
}

#[derive(Deserialize)]
struct Thresholds {
    median_abs_log10_ratio_max: f64,
    p95_abs_log10_ratio_max: f64,
    catastrophic_outlier_fraction_max: f64,
    catastrophic_abs_log10_ratio: f64,
}

#[test]
fn production_photometric_artifact_matches_xp_anchor_validation_set() -> anyhow::Result<()> {
    let fixture: ValidationFixture = serde_json::from_str(FIXTURE)?;
    let fixture_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/photometric_xp_anchored_v1");
    let sha = std::fs::read_to_string(fixture_root.join("artifact.sha256"))?
        .split_whitespace()
        .next()
        .expect("sha256")
        .to_string();
    assert_eq!(sha, fixture.production_artifact_sha256);
    assert_eq!(sha, fixture.fixture_artifact_sha256);

    let correction = PhotometricCorrection::load(&fixture_root.join("artifact.json"), &sha)?;
    let mut log_ratios = Vec::new();
    let mut catastrophic = 0usize;

    for case in &fixture.cases {
        let decision = correction.route_and_evaluate(PhotometricFeatures {
            phot_g_mean_mag: Some(case.phot_g_mean_mag),
            phot_bp_mean_mag: case.phot_bp_mean_mag,
            phot_rp_mean_mag: case.phot_rp_mean_mag,
            bp_rp: case.bp_rp,
            quality_flag: true,
        })?;
        assert_eq!(
            decision.branch,
            expected_branch(&case.branch),
            "{} routed to {:?}, expected {}",
            case.label,
            decision.branch,
            case.branch
        );
        let predicted = decision.flux.expect("flux").flux_336_650_ph_m2_s;
        let ratio = predicted / case.xp_flux_336_650_ph_m2_s;
        let log10_ratio = ratio.log10().abs();
        log_ratios.push(log10_ratio);
        if log10_ratio > fixture.thresholds.catastrophic_abs_log10_ratio {
            catastrophic += 1;
        }
    }

    let mut sorted = log_ratios.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];
    let catastrophic_fraction = catastrophic as f64 / fixture.cases.len() as f64;

    assert!(
        median <= fixture.thresholds.median_abs_log10_ratio_max,
        "median |log10(pred/xp)| {median} exceeds {}",
        fixture.thresholds.median_abs_log10_ratio_max
    );
    assert!(
        p95 <= fixture.thresholds.p95_abs_log10_ratio_max,
        "p95 |log10(pred/xp)| {p95} exceeds {}",
        fixture.thresholds.p95_abs_log10_ratio_max
    );
    assert!(
        catastrophic_fraction <= fixture.thresholds.catastrophic_outlier_fraction_max,
        "catastrophic fraction {catastrophic_fraction}"
    );
    Ok(())
}

fn expected_branch(name: &str) -> PopulationBranch {
    match name {
        "photometric_g_bp_rp" => PopulationBranch::PhotometricGBpRp,
        "photometric_partial" => PopulationBranch::PhotometricPartial,
        "photometric_g_only" => PopulationBranch::PhotometricGOnly,
        other => panic!("unknown branch {other}"),
    }
}
