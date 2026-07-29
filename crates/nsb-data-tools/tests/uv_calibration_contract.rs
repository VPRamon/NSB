use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::uv::{
    run_reproducibility_validation, ApplicabilityStatus, EvaluationDecision, OutOfDomainPolicy,
    PartitionManifest, ReproducibilityInputs, UvCalibrationArtifact, UvCorrection,
};
use nsb_data_tools::starlight::xp::{integrate_photon_flux, XpProduct};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ARTIFACT_SHA256: &str = "6880f13ed06577a934b0c388bc9e3ddb84d93be0648a0931cd0d74737d3b2442";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/uv_synthetic_non_production")
}

fn artifact_path() -> PathBuf {
    fixture_root().join("artifact.json")
}

fn artifact() -> UvCalibrationArtifact {
    serde_json::from_slice(&fs::read(artifact_path()).unwrap()).unwrap()
}

fn correction() -> UvCorrection {
    UvCorrection::load(&artifact_path(), ARTIFACT_SHA256).unwrap()
}

fn load_mutated(
    temporary: &tempfile::TempDir,
    artifact: &UvCalibrationArtifact,
) -> anyhow::Result<UvCorrection> {
    let path = temporary.path().join("artifact.json");
    let bytes = serde_json::to_vec_pretty(artifact)?;
    fs::write(&path, &bytes)?;
    UvCorrection::load(&path, &checksum_io::sha256_bytes(&bytes))
}

#[test]
fn valid_artifact_loads_but_is_unmistakably_non_production() {
    let correction = correction();
    assert_eq!(correction.artifact_sha256(), ARTIFACT_SHA256);
    assert!(correction
        .artifact()
        .model_id
        .contains("SYNTHETIC-NON-PRODUCTION"));
    assert!(correction.require_production_status().is_err());
}

#[test]
fn checksum_mismatch_fails_closed() {
    let error = UvCorrection::load(&artifact_path(), &"0".repeat(64))
        .unwrap_err()
        .to_string();
    assert!(error.contains("checksum mismatch"));
}

#[test]
fn wrong_band_units_provenance_and_schema_are_rejected() {
    let mut value = artifact();
    value.correction_band_nm = [301, 336];
    assert!(value.validate().is_err());
    value = artifact();
    value.flux_unit = "W_m-2".to_string();
    assert!(value.validate().is_err());
    value = artifact();
    value.reference_dataset.files[0].sha256 = "bad".to_string();
    assert!(value.validate().is_err());
    value = artifact();
    value.training_command = "TODO".to_string();
    assert!(value.validate().is_err());
    value = artifact();
    value.schema_version += 1;
    assert!(value.validate().is_err());
    let value = artifact();
    let first = serde_json::to_vec(&value).unwrap();
    let round_trip: UvCalibrationArtifact = serde_json::from_slice(&first).unwrap();
    assert_eq!(first, serde_json::to_vec(&round_trip).unwrap());

    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(artifact_path()).unwrap()).unwrap();
    json["unknown_field"] = serde_json::json!(true);
    assert!(serde_json::from_value::<UvCalibrationArtifact>(json).is_err());
}

#[test]
fn non_finite_parameters_bad_covariance_and_negative_uncertainty_are_rejected() {
    let mut value = artifact();
    let nsb_data_tools::starlight::uv::CorrectionModel::Linear { parameters, .. } =
        &mut value.model;
    parameters[0] = f64::NAN;
    assert!(value.validate().is_err());

    value = artifact();
    let nsb_data_tools::starlight::uv::CorrectionModel::Linear { covariance, .. } =
        &mut value.model;
    covariance.pop();
    assert!(value.validate().is_err());

    value = artifact();
    let nsb_data_tools::starlight::uv::CorrectionModel::Linear { covariance, .. } =
        &mut value.model;
    covariance[0][0] = f64::NAN;
    assert!(value.validate().is_err());

    value = artifact();
    value.uncertainty_model.systematic_fraction = -0.1;
    assert!(value.validate().is_err());
}

#[test]
fn evaluation_preserves_in_boundary_and_out_of_domain_status() {
    let correction = correction();
    let evaluate = |x| correction.evaluate(&BTreeMap::from([("x".to_string(), x)]));

    let inside = evaluate(5.0).unwrap();
    assert_eq!(inside.applicability_status, ApplicabilityStatus::InDomain);
    assert_eq!(inside.decision, EvaluationDecision::Applied);
    assert_eq!(inside.flux_300_336_ph_m2_s, Some(20.0));

    let boundary = evaluate(0.0).unwrap();
    assert_eq!(boundary.applicability_status, ApplicabilityStatus::Boundary);
    assert_eq!(boundary.flux_300_336_ph_m2_s, Some(10.0));

    let outside = evaluate(12.0).unwrap();
    assert_eq!(
        outside.applicability_status,
        ApplicabilityStatus::OutOfDomain
    );
    assert_eq!(outside.decision, EvaluationDecision::Clamped);
    assert_eq!(outside.flux_300_336_ph_m2_s, Some(30.0));
    assert!(
        outside.systematic_uncertainty_300_336_ph_m2_s.unwrap()
            > inside.systematic_uncertainty_300_336_ph_m2_s.unwrap()
    );
}

#[test]
fn rejection_policy_never_extrapolates_and_preserves_diagnostics() {
    let temporary = tempfile::tempdir().unwrap();
    let mut value = artifact();
    value.out_of_domain_policy = OutOfDomainPolicy::Reject;
    let correction = load_mutated(&temporary, &value).unwrap();
    let result = correction
        .evaluate(&BTreeMap::from([("x".to_string(), 12.0)]))
        .unwrap();
    assert_eq!(result.decision, EvaluationDecision::Rejected);
    assert_eq!(
        result.applicability_status,
        ApplicabilityStatus::OutOfDomain
    );
    assert!(result.flux_300_336_ph_m2_s.is_none());
}

#[test]
fn band_components_and_uncertainties_are_separate_and_explicitly_combined() {
    let correction = correction();
    let evaluation = correction
        .evaluate(&BTreeMap::from([("x".to_string(), 5.0)]))
        .unwrap();
    let combined = correction
        .combine_with_measured(100.0, 4.0, &evaluation)
        .unwrap();
    assert_eq!(combined.flux_300_336_ph_m2_s, 20.0);
    assert_eq!(combined.flux_336_650_ph_m2_s, 100.0);
    assert_eq!(combined.flux_300_650_ph_m2_s, 120.0);
    let uv_stat = evaluation.statistical_uncertainty_300_336_ph_m2_s.unwrap();
    let expected_stat = (4.0_f64.powi(2) + uv_stat.powi(2) + 2.0 * 0.25 * 4.0 * uv_stat).sqrt();
    assert_eq!(
        combined.statistical_uncertainty_300_650_ph_m2_s,
        expected_stat
    );
    assert_eq!(
        combined.systematic_uncertainty_300_650_ph_m2_s,
        evaluation.systematic_uncertainty_300_336_ph_m2_s.unwrap()
    );
}

#[test]
fn partition_validation_and_materialization_are_deterministic() {
    let bytes = fs::read(fixture_root().join("partitions.json")).unwrap();
    let partitions: PartitionManifest = serde_json::from_slice(&bytes).unwrap();
    let first = serde_json::to_vec_pretty(&partitions.canonicalized().unwrap()).unwrap();
    let second = serde_json::to_vec_pretty(&partitions.canonicalized().unwrap()).unwrap();
    assert_eq!(first, second);

    let mut invalid = partitions;
    invalid.assignments[0].sky_region = invalid.assignments[1].sky_region.clone();
    assert!(invalid.validate().is_err());
}

#[test]
fn reproducibility_report_is_deterministic_and_has_all_required_strata() {
    let temporary = tempfile::tempdir().unwrap();
    let first = temporary.path().join("first.json");
    let second = temporary.path().join("second.json");
    let materialized = temporary.path().join("partitions.json");
    let inputs = |output: &Path| ReproducibilityInputs {
        reference_manifest: fixture_root().join("reference-manifest.json"),
        partition_manifest: fixture_root().join("partitions.json"),
        artifact: artifact_path(),
        artifact_sha256: ARTIFACT_SHA256.to_string(),
        holdout: fixture_root().join("holdout.csv"),
        materialize_partitions: Some(materialized.clone()),
        output: output.to_path_buf(),
    };
    let report = run_reproducibility_validation(&inputs(&first)).unwrap();
    run_reproducibility_validation(&inputs(&second)).unwrap();
    assert_eq!(fs::read(first).unwrap(), fs::read(second).unwrap());
    assert_eq!(report.holdout_rows, 3);
    assert!(!report.by_colour.is_empty());
    assert!(!report.by_magnitude.is_empty());
    assert!(!report.by_extinction_proxy.is_empty());
    assert!(!report.by_quality.is_empty());
    assert!(!report.by_sky_region.is_empty());
    assert_eq!(
        report
            .by_extrapolation_status
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["boundary", "in-domain", "out-of-domain"]
    );
}

#[test]
fn measured_xp_integration_ignores_any_unmeasured_uv_samples() {
    let measured = XpProduct {
        source_id: "synthetic-measured-test".to_string(),
        wavelengths_nm: vec![336.0, 650.0],
        flux_w_m2_nm: vec![1.0, 1.0],
        flux_error_w_m2_nm: None,
    };
    let with_uv_sample = XpProduct {
        source_id: measured.source_id.clone(),
        wavelengths_nm: vec![300.0, 336.0, 650.0],
        flux_w_m2_nm: vec![1.0e30, 1.0, 1.0],
        flux_error_w_m2_nm: None,
    };
    assert_eq!(
        integrate_photon_flux(&measured).unwrap().to_bits(),
        integrate_photon_flux(&with_uv_sample).unwrap().to_bits()
    );
}

#[test]
fn combined_product_without_artifact_fails_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let config = temporary.path().join("config.toml");
    fs::write(
        &config,
        format!(
            "schema_version = 1\n\
             dataset = \"starlight\"\n\
             [workspace]\n\
             root = \"{}\"\n\
             [starlight]\n\
             mode = \"production\"\n\
             product_band = \"combined-300-650\"\n",
            temporary.path().join("workspace").display()
        ),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args([
            "dataset",
            "starlight",
            "build",
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("requires a validated UV correction artifact"));
}
