//! Phase 5 holdout v1 — independence, preflight, Wilson intervals, official evaluation guards.

use crate::checksum_io::sha256_file;
use crate::starlight_phase5::{load_split_map, Phase5TargetRow, PHASE4_SPLITS};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const HOLDOUT_ID: &str = "phase5_holdout_v1";
pub const OFFICIAL_EVALUATION_FILENAME: &str = "phase5_holdout_v1_official_evaluation.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutExecutionManifest {
    pub schema_version: u32,
    pub execution_id: String,
    pub created_at: String,
    pub software_commit: String,
    pub policy_id: String,
    pub policy_path: String,
    pub policy_checksum: String,
    pub holdout_sources_path: String,
    pub holdout_sources_checksum: String,
    pub holdout_manifest_path: String,
    pub holdout_manifest_checksum: String,
    pub gaiaxpy_version: String,
    pub python_version: String,
    pub adapter_version: String,
    pub schema_version_label: String,
    pub official_evaluation: bool,
    pub evaluation_attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutIndependenceReport {
    pub schema_version: u32,
    pub holdout_id: String,
    pub holdout_source_count: u64,
    pub holdout_cell_count: u64,
    pub phase4_source_overlap_count: u64,
    pub phase4_cell_overlap_count: u64,
    pub duplicate_source_count: u64,
    pub duplicate_cell_count: u64,
    pub passed: bool,
    pub overlapping_source_ids: Vec<u64>,
    pub overlapping_cells: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WilsonInterval {
    pub point_estimate: f64,
    pub wilson_95_low: f64,
    pub wilson_95_high: f64,
    pub n: u64,
    pub success_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoldoutPreflightReport {
    pub schema_version: u32,
    pub passed: bool,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
    pub policy_checksum: String,
    pub sources_checksum: String,
    pub reconstructed_count: u64,
    pub missing_count: u64,
    pub excluded_count: u64,
    pub failed_count: u64,
    pub independence_passed: bool,
}

/// Wilson score interval for a binomial proportion (95% default z=1.96).
pub fn wilson_interval(successes: u64, n: u64) -> WilsonInterval {
    if n == 0 {
        return WilsonInterval {
            point_estimate: 0.0,
            wilson_95_low: 0.0,
            wilson_95_high: 0.0,
            n: 0,
            success_count: 0,
        };
    }
    let n_f = n as f64;
    let p = successes as f64 / n_f;
    let z = 1.96_f64;
    let z2 = z * z;
    let denom = 1.0 + z2 / n_f;
    let center = p + z2 / (2.0 * n_f);
    let margin = z * ((p * (1.0 - p) / n_f + z2 / (4.0 * n_f * n_f)).sqrt());
    WilsonInterval {
        point_estimate: p,
        wilson_95_low: ((center - margin) / denom).clamp(0.0, 1.0),
        wilson_95_high: ((center + margin) / denom).clamp(0.0, 1.0),
        n,
        success_count: successes,
    }
}

pub fn coverage_wilson_intervals(
    comparisons: &[crate::starlight_phase5::OverlapComparison],
    model: &crate::starlight_phase5_uncertainty::OverlapDifferenceUncertaintyModel,
) -> (WilsonInterval, WilsonInterval) {
    let mut cover68 = 0_u64;
    let mut cover95 = 0_u64;
    for row in comparisons {
        let sigma = crate::starlight_phase5_uncertainty::overlap_difference_sigma(row, model);
        let delta = (row.sampled_flux_ph_m2_s - row.reconstructed_flux_ph_m2_s).abs();
        if delta <= sigma {
            cover68 += 1;
        }
        if delta <= 1.96 * sigma {
            cover95 += 1;
        }
    }
    let n = comparisons.len() as u64;
    (wilson_interval(cover68, n), wilson_interval(cover95, n))
}

pub fn verify_holdout_independence(
    missing_flux_root: &Path,
    holdout_targets: &[Phase5TargetRow],
) -> Result<HoldoutIndependenceReport> {
    let phase4_splits = load_split_map(&missing_flux_root.join(PHASE4_SPLITS))?;
    let phase4_source_ids: HashSet<u64> = phase4_splits.keys().copied().collect();
    let phase4_cells: HashSet<u32> = phase4_splits.values().map(|(_, cell)| *cell).collect();

    let mut holdout_sources = HashSet::new();
    let mut holdout_cells = HashSet::new();
    let mut duplicate_sources = 0_u64;
    let mut duplicate_cells = 0_u64;
    let mut overlapping_sources = Vec::new();
    let mut overlapping_cells = Vec::new();

    for target in holdout_targets {
        if !holdout_sources.insert(target.source_id) {
            duplicate_sources += 1;
        }
        if phase4_source_ids.contains(&target.source_id) {
            overlapping_sources.push(target.source_id);
        }
        if !holdout_cells.insert(target.spatial_cell) {
            duplicate_cells += 1;
        }
        if phase4_cells.contains(&target.spatial_cell) {
            overlapping_cells.push(target.spatial_cell);
        }
    }
    overlapping_sources.sort_unstable();
    overlapping_cells.sort_unstable();
    overlapping_cells.dedup();

    let source_overlap = overlapping_sources.len() as u64;
    let cell_overlap = overlapping_cells.len() as u64;
    let passed = source_overlap == 0
        && cell_overlap == 0
        && duplicate_sources == 0
        && duplicate_cells == 0
        && holdout_sources.len() == holdout_targets.len()
        && holdout_cells.len() == holdout_targets.len();

    Ok(HoldoutIndependenceReport {
        schema_version: 1,
        holdout_id: HOLDOUT_ID.to_string(),
        holdout_source_count: holdout_targets.len() as u64,
        holdout_cell_count: holdout_cells.len() as u64,
        phase4_source_overlap_count: source_overlap,
        phase4_cell_overlap_count: cell_overlap,
        duplicate_source_count: duplicate_sources,
        duplicate_cell_count: duplicate_cells,
        passed,
        overlapping_source_ids: overlapping_sources,
        overlapping_cells,
    })
}

pub fn load_execution_manifest(path: &Path) -> Result<HoldoutExecutionManifest> {
    let text = fs::read_to_string(path).context("read execution manifest")?;
    Ok(serde_json::from_str(&text)?)
}

pub fn verify_execution_manifest(manifest: &HoldoutExecutionManifest) -> Result<()> {
    if !manifest.official_evaluation {
        bail!("execution manifest official_evaluation must be true");
    }
    if manifest.evaluation_attempt != 1 {
        bail!(
            "official evaluation requires evaluation_attempt=1, got {}",
            manifest.evaluation_attempt
        );
    }
    let policy_checksum = sha256_file(Path::new(&manifest.policy_path))?;
    if policy_checksum != manifest.policy_checksum {
        bail!("policy checksum mismatch");
    }
    let sources_checksum = sha256_file(Path::new(&manifest.holdout_sources_path))?;
    if sources_checksum != manifest.holdout_sources_checksum {
        bail!("holdout sources checksum mismatch");
    }
    let manifest_checksum = sha256_file(Path::new(&manifest.holdout_manifest_path))?;
    if manifest_checksum != manifest.holdout_manifest_checksum {
        bail!("holdout split manifest checksum mismatch");
    }
    Ok(())
}

pub fn assert_official_evaluation_not_done(holdout_root: &Path) -> Result<()> {
    let path = holdout_root.join(OFFICIAL_EVALUATION_FILENAME);
    if path.is_file() {
        bail!(
            "official evaluation already exists at {}; refusing re-run",
            path.display()
        );
    }
    Ok(())
}

pub fn create_execution_manifest(
    policy_path: &Path,
    sources_path: &Path,
    split_manifest_path: &Path,
    gaiaxpy_env_path: &Path,
    software_commit: &str,
    adapter_version: &str,
) -> Result<HoldoutExecutionManifest> {
    let gaiaxpy_env: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(gaiaxpy_env_path)?)?;
    let policy: crate::starlight_phase5_uncertainty::FrozenValidationPolicyV1 =
        serde_json::from_str(&fs::read_to_string(policy_path)?)?;
    if policy.status != "frozen" {
        bail!("policy must be frozen");
    }
    Ok(HoldoutExecutionManifest {
        schema_version: 1,
        execution_id: format!("{HOLDOUT_ID}-official-001"),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        software_commit: software_commit.to_string(),
        policy_id: policy.policy_id.clone(),
        policy_path: policy_path.display().to_string(),
        policy_checksum: sha256_file(policy_path)?,
        holdout_sources_path: sources_path.display().to_string(),
        holdout_sources_checksum: sha256_file(sources_path)?,
        holdout_manifest_path: split_manifest_path.display().to_string(),
        holdout_manifest_checksum: sha256_file(split_manifest_path)?,
        gaiaxpy_version: gaiaxpy_env["gaiaxpy_version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        python_version: gaiaxpy_env["python_version"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        adapter_version: adapter_version.to_string(),
        schema_version_label: "gaia_xp_continuous_canonical_v1".to_string(),
        official_evaluation: true,
        evaluation_attempt: 1,
    })
}

pub fn count_reconstructed(reconstructed_dir: &Path, target_ids: &HashSet<u64>) -> u64 {
    target_ids
        .iter()
        .filter(|id| reconstructed_dir.join(format!("{id}.csv")).is_file())
        .count() as u64
}

pub fn build_preflight(
    manifest: &HoldoutExecutionManifest,
    independence: &HoldoutIndependenceReport,
    download_report: &crate::starlight_phase5::DownloadStatusReport,
    reconstructed_count: u64,
    requested: u64,
) -> HoldoutPreflightReport {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    if !independence.passed {
        failures.push("spatial independence check failed".to_string());
    }
    if download_report.pending > 0 {
        failures.push(format!("download pending={}", download_report.pending));
    }
    if download_report.downloaded_valid
        + download_report.missing_from_canonical_reference
        + download_report.excluded
        + download_report.nonretryable_error
        + download_report.retryable_error
        != download_report.requested
    {
        failures.push("download reconciliation mismatch".to_string());
    }
    let missing = requested.saturating_sub(reconstructed_count);
    if missing > 0 {
        failures.push(format!("missing reconstructed sources: {missing}"));
    }
    if reconstructed_count != download_report.downloaded_valid {
        warnings.push(format!(
            "reconstructed_count ({reconstructed_count}) != downloaded_valid ({})",
            download_report.downloaded_valid
        ));
    }

    HoldoutPreflightReport {
        schema_version: 1,
        passed: failures.is_empty(),
        failures,
        warnings,
        policy_checksum: manifest.policy_checksum.clone(),
        sources_checksum: manifest.holdout_sources_checksum.clone(),
        reconstructed_count,
        missing_count: missing,
        excluded_count: download_report.excluded + download_report.missing_from_canonical_reference,
        failed_count: download_report.nonretryable_error + download_report.retryable_error,
        independence_passed: independence.passed,
    }
}

pub fn holdout_paths(holdout_root: &Path) -> HashMap<&'static str, PathBuf> {
    HashMap::from([
        (
            "sources",
            holdout_root.join("phase5_holdout_v1_sources.csv"),
        ),
        (
            "split_manifest",
            holdout_root.join("phase5_holdout_v1_split_manifest.json"),
        ),
        (
            "execution_manifest",
            holdout_root.join("phase5_holdout_v1_execution_manifest.json"),
        ),
        (
            "preflight",
            holdout_root.join("phase5_holdout_v1_preflight.json"),
        ),
        (
            "independence",
            holdout_root.join("phase5_holdout_v1_independence.json"),
        ),
        (
            "official_eval",
            holdout_root.join(OFFICIAL_EVALUATION_FILENAME),
        ),
    ])
}

#[cfg(test)]
mod phase5_holdout_tests {
    use super::*;
    use crate::starlight_phase5::{
        DownloadStatusReport, OverlapComparison, Phase5TargetRow, PHASE4_SPLITS,
    };
    use crate::starlight_phase5_uncertainty::{
        compute_overlap_metrics, overlap_difference_sigma, OverlapDifferenceUncertaintyModel,
    };
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nsb-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_phase4_splits(root: &Path, rows: &[(&str, u32, &str)]) {
        let path = root.join(PHASE4_SPLITS);
        fs::create_dir_all(root).unwrap();
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "source_id,spatial_cell,split").unwrap();
        for (sid, cell, split) in rows {
            writeln!(f, "{sid},{cell},{split}").unwrap();
        }
    }

    fn holdout_target(source_id: u64, cell: u32) -> Phase5TargetRow {
        Phase5TargetRow {
            source_id,
            population: "holdout".to_string(),
            split: "holdout".to_string(),
            spatial_cell: cell,
            strata: "g_mag".to_string(),
            phot_g_mean_mag: Some(12.0),
            bp_rp: Some(0.5),
            phot_g_mean_flux_over_error: Some(100.0),
            phot_bp_rp_excess_factor: Some(1.0),
            phot_bp_n_blended_transits: Some(0),
            phot_rp_n_blended_transits: Some(0),
            l: Some(180.0),
            b: Some(0.0),
            duplicated_source: false,
            phot_variable_flag: "not_variable".to_string(),
            in_qso_candidates: false,
            in_galaxy_candidates: false,
        }
    }

    fn download_report(requested: u64, valid: u64, pending: u64) -> DownloadStatusReport {
        DownloadStatusReport {
            schema_version: 1,
            generation_timestamp_utc: String::new(),
            batch_id: "holdout".to_string(),
            requested,
            downloaded_valid: valid,
            downloaded_invalid: 0,
            pending,
            retryable_error: 0,
            nonretryable_error: 0,
            missing_from_response: 0,
            missing_from_canonical_reference: 0,
            excluded: 0,
            overlap_requested: requested,
            overlap_downloaded_valid: valid,
            continuous_only_requested: 0,
            continuous_only_downloaded_valid: 0,
            throughput_sources_per_second: 0.0,
            last_progress_unix_millis: 0,
            stalled: false,
            download_active: false,
        }
    }

    fn test_policy_json() -> String {
        include_str!("../tests/fixtures/phase5_frozen_policy_v1_minimal.json").to_string()
    }

    fn write_test_policy(dir: &Path) -> PathBuf {
        let policy = dir.join("policy.json");
        fs::write(&policy, test_policy_json()).unwrap();
        policy
    }

    fn sample_row(sampled: f64, reconstructed: f64, stat: f64) -> OverlapComparison {
        OverlapComparison {
            source_id: 1,
            split: "holdout".to_string(),
            sampled_flux_ph_m2_s: sampled,
            reconstructed_flux_ph_m2_s: reconstructed,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: stat,
            relative_error: (reconstructed - sampled) / sampled,
            phot_g_mean_mag: None,
            bp_rp: None,
            phot_g_snr: None,
            phot_bp_rp_excess_factor: None,
            l: None,
            b: None,
            g_mag_bin: String::new(),
            colour_bin: String::new(),
            snr_bin: String::new(),
            sky_region: String::new(),
            bp_rp_excess_bin: String::new(),
            crowding_bin: String::new(),
            duplicated_bin: String::new(),
            variable_bin: String::new(),
            qso_galaxy_bin: String::new(),
        }
    }

    #[test]
    fn phase5_holdout_wilson_interval_bounds_are_ordered() {
        let w = wilson_interval(68, 100);
        assert!(w.wilson_95_low <= w.point_estimate);
        assert!(w.point_estimate <= w.wilson_95_high);
        assert_eq!(w.n, 100);
    }

    #[test]
    fn phase5_holdout_wilson_empty_is_zero() {
        let w = wilson_interval(0, 0);
        assert_eq!(w.n, 0);
    }

    #[test]
    fn phase5_holdout_spatial_independence_passes_disjoint_cells() {
        let root = temp_dir("indep-pass");
        write_phase4_splits(&root, &[("100", 42, "train"), ("101", 43, "validation")]);
        let targets = vec![holdout_target(200, 99), holdout_target(201, 100)];
        let report = verify_holdout_independence(&root, &targets).unwrap();
        assert!(report.passed);
        assert_eq!(report.phase4_source_overlap_count, 0);
        assert_eq!(report.phase4_cell_overlap_count, 0);
    }

    #[test]
    fn phase5_holdout_source_overlap_rejection() {
        let root = temp_dir("source-overlap");
        write_phase4_splits(&root, &[("100", 42, "train")]);
        let targets = vec![holdout_target(100, 99)];
        let report = verify_holdout_independence(&root, &targets).unwrap();
        assert!(!report.passed);
        assert_eq!(report.phase4_source_overlap_count, 1);
    }

    #[test]
    fn phase5_holdout_healpix_overlap_rejection() {
        let root = temp_dir("cell-overlap");
        write_phase4_splits(&root, &[("100", 42, "train")]);
        let targets = vec![holdout_target(200, 42)];
        let report = verify_holdout_independence(&root, &targets).unwrap();
        assert!(!report.passed);
        assert_eq!(report.phase4_cell_overlap_count, 1);
    }

    #[test]
    fn phase5_holdout_policy_checksum_mismatch_rejected() {
        let dir = temp_dir("policy-mismatch");
        let policy = write_test_policy(&dir);
        let sources = dir.join("sources.csv");
        fs::write(&sources, "source_id\n1\n").unwrap();
        let manifest = dir.join("split.json");
        fs::write(&manifest, "{}").unwrap();
        let gaia = dir.join("gaia.json");
        fs::write(
            &gaia,
            r#"{"gaiaxpy_version":"2.1.4","python_version":"3.12.3"}"#,
        )
        .unwrap();
        let exec = create_execution_manifest(&policy, &sources, &manifest, &gaia, "deadbeef", "v1")
            .unwrap();
        let mut bad = exec.clone();
        bad.policy_checksum = "0".repeat(64);
        assert!(verify_execution_manifest(&bad).is_err());
    }

    #[test]
    fn phase5_holdout_source_checksum_mismatch_rejected() {
        let dir = temp_dir("source-mismatch");
        let policy = write_test_policy(&dir);
        let sources = dir.join("sources.csv");
        fs::write(&sources, "source_id\n1\n").unwrap();
        let manifest = dir.join("split.json");
        fs::write(&manifest, "{}").unwrap();
        let gaia = dir.join("gaia.json");
        fs::write(
            &gaia,
            r#"{"gaiaxpy_version":"2.1.4","python_version":"3.12.3"}"#,
        )
        .unwrap();
        let exec = create_execution_manifest(&policy, &sources, &manifest, &gaia, "deadbeef", "v1")
            .unwrap();
        let mut bad = exec.clone();
        bad.holdout_sources_checksum = "0".repeat(64);
        assert!(verify_execution_manifest(&bad).is_err());
    }

    #[test]
    fn phase5_holdout_evaluation_attempt_gt1_rejected() {
        let dir = temp_dir("attempt-gt1");
        let policy = write_test_policy(&dir);
        let sources = dir.join("sources.csv");
        fs::write(&sources, "source_id\n1\n").unwrap();
        let manifest = dir.join("split.json");
        fs::write(&manifest, "{}").unwrap();
        let gaia = dir.join("gaia.json");
        fs::write(
            &gaia,
            r#"{"gaiaxpy_version":"2.1.4","python_version":"3.12.3"}"#,
        )
        .unwrap();
        let mut exec =
            create_execution_manifest(&policy, &sources, &manifest, &gaia, "deadbeef", "v1")
                .unwrap();
        exec.evaluation_attempt = 2;
        assert!(verify_execution_manifest(&exec).is_err());
    }

    #[test]
    fn phase5_holdout_reconciliation_mismatch_fails_preflight() {
        let exec = HoldoutExecutionManifest {
            schema_version: 1,
            execution_id: "x".to_string(),
            created_at: String::new(),
            software_commit: String::new(),
            policy_id: "p".to_string(),
            policy_path: String::new(),
            policy_checksum: "abc".to_string(),
            holdout_sources_path: String::new(),
            holdout_sources_checksum: "def".to_string(),
            holdout_manifest_path: String::new(),
            holdout_manifest_checksum: "ghi".to_string(),
            gaiaxpy_version: String::new(),
            python_version: String::new(),
            adapter_version: String::new(),
            schema_version_label: String::new(),
            official_evaluation: true,
            evaluation_attempt: 1,
        };
        let independence = HoldoutIndependenceReport {
            schema_version: 1,
            holdout_id: HOLDOUT_ID.to_string(),
            holdout_source_count: 160,
            holdout_cell_count: 160,
            phase4_source_overlap_count: 0,
            phase4_cell_overlap_count: 0,
            duplicate_source_count: 0,
            duplicate_cell_count: 0,
            passed: true,
            overlapping_source_ids: vec![],
            overlapping_cells: vec![],
        };
        let download = download_report(160, 150, 10);
        let preflight = build_preflight(&exec, &independence, &download, 150, 160);
        assert!(!preflight.passed);
        assert!(preflight.failures.iter().any(|f| f.contains("pending")));
    }

    #[test]
    fn phase5_holdout_missing_reconstructed_fails_preflight() {
        let exec = HoldoutExecutionManifest {
            schema_version: 1,
            execution_id: "x".to_string(),
            created_at: String::new(),
            software_commit: String::new(),
            policy_id: "p".to_string(),
            policy_path: String::new(),
            policy_checksum: "abc".to_string(),
            holdout_sources_path: String::new(),
            holdout_sources_checksum: "def".to_string(),
            holdout_manifest_path: String::new(),
            holdout_manifest_checksum: "ghi".to_string(),
            gaiaxpy_version: String::new(),
            python_version: String::new(),
            adapter_version: String::new(),
            schema_version_label: String::new(),
            official_evaluation: true,
            evaluation_attempt: 1,
        };
        let independence = HoldoutIndependenceReport {
            schema_version: 1,
            holdout_id: HOLDOUT_ID.to_string(),
            holdout_source_count: 160,
            holdout_cell_count: 160,
            phase4_source_overlap_count: 0,
            phase4_cell_overlap_count: 0,
            duplicate_source_count: 0,
            duplicate_cell_count: 0,
            passed: true,
            overlapping_source_ids: vec![],
            overlapping_cells: vec![],
        };
        let download = download_report(160, 160, 0);
        let preflight = build_preflight(&exec, &independence, &download, 150, 160);
        assert!(!preflight.passed);
        assert!(preflight
            .failures
            .iter()
            .any(|f| f.contains("missing reconstructed")));
    }

    fn test_overlap_model() -> OverlapDifferenceUncertaintyModel {
        OverlapDifferenceUncertaintyModel {
            relative_residual_scale: 1.0e-5,
            inflation_factor: 0.59,
            ..Default::default()
        }
    }

    #[test]
    fn phase5_holdout_coverage_uses_difference_uncertainty() {
        let model = test_overlap_model();
        let row = sample_row(1.0e5, 1.0e5 + 0.1, 150.0);
        let sigma = overlap_difference_sigma(&row, &model);
        assert!(sigma < row.statistical_uncertainty_ph_m2_s);
        let metrics = compute_overlap_metrics(&[row], &model);
        let (w68, w95) =
            coverage_wilson_intervals(&[sample_row(1.0e5, 1.0e5 + 0.1, 150.0)], &model);
        assert_eq!(metrics.coverage_68, w68.point_estimate);
        assert_eq!(metrics.coverage_95, w95.point_estimate);
    }

    #[test]
    fn phase5_holdout_absolute_uncertainty_not_used_for_overlap_gates() {
        let model = test_overlap_model();
        let row = sample_row(1.0e5, 1.0e5 + 50.0, 150.0);
        let diff_sigma = overlap_difference_sigma(&row, &model);
        assert!(50.0 > diff_sigma);
        let metrics = compute_overlap_metrics(&[row], &model);
        assert_eq!(metrics.coverage_68, 0.0);
    }

    #[test]
    fn phase5_holdout_official_artefact_immutability() {
        let dir = temp_dir("official-immutable");
        let official = dir.join(OFFICIAL_EVALUATION_FILENAME);
        fs::write(&official, "{}").unwrap();
        assert!(assert_official_evaluation_not_done(&dir).is_err());
    }
}
