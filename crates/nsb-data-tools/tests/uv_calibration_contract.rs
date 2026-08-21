use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::uv::{
    run_reproducibility_validation, ApplicabilityStatus, CorrectionModel, EvaluationDecision,
    MeasuredBandInput, ModelResponse, OutOfDomainPolicy, PartitionManifest, ReproducibilityInputs,
    UvCalibrationArtifact, UvCorrection, UvEvaluationInput, ValidationMetricKind,
};
use nsb_data_tools::starlight::xp::{integrate_photon_flux, XpProduct};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ARTIFACT_SHA256: &str = "b62b00e454619b0242226f691b9700374f64537cfcafc0d98e84e0720ad7c11b";

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

fn copy_fixture(temporary: &tempfile::TempDir) -> PathBuf {
    let root = temporary.path().join("fixture");
    fs::create_dir(&root).unwrap();
    for entry in fs::read_dir(fixture_root()).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
    }
    root
}

fn reproducibility_inputs(root: &Path, output: &Path) -> ReproducibilityInputs {
    ReproducibilityInputs {
        reference_manifest: root.join("reference-manifest.json"),
        partition_manifest: root.join("partitions.json"),
        artifact: root.join("artifact.json"),
        artifact_sha256: checksum_io::sha256_file(&root.join("artifact.json")).unwrap(),
        holdout: root.join("holdout.csv"),
        materialize_partitions: Some(root.join("materialized-partitions.json")),
        output: output.to_path_buf(),
    }
}

fn validation_error(root: &Path) -> String {
    run_reproducibility_validation(&reproducibility_inputs(root, &root.join("report.json")))
        .unwrap_err()
        .to_string()
}

fn write_reference_sources(root: &Path, contents: &str) {
    let source_path = root.join("reference-sources.csv");
    fs::write(&source_path, contents).unwrap();
    let sha256 = checksum_io::sha256_file(&source_path).unwrap();
    let manifest_path = root.join("reference-manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["source_table_sha256"] = serde_json::json!(sha256);
    let files = manifest["dataset"]["files"].as_array_mut().unwrap();
    files
        .iter_mut()
        .find(|file| file["name"] == "reference-sources.csv")
        .unwrap()["sha256"] = serde_json::json!(sha256);
    fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
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
fn metric_kinds_enforce_signed_nonnegative_and_fraction_domains() {
    let mut value = artifact();
    value.validation_metrics[0].kind = ValidationMetricKind::Bias;
    value.validation_metrics[0].value = -2.0;
    assert!(value.validate().is_ok());

    value.validation_metrics[0].kind = ValidationMetricKind::Rmse;
    assert!(value
        .validate()
        .unwrap_err()
        .to_string()
        .contains("negative"));

    value.validation_metrics[0].kind = ValidationMetricKind::Mae;
    assert!(value
        .validate()
        .unwrap_err()
        .to_string()
        .contains("negative"));

    value.validation_metrics[0].kind = ValidationMetricKind::IntervalCoverage;
    value.validation_metrics[0].value = 1.01;
    assert!(value.validate().unwrap_err().to_string().contains("[0, 1]"));
    value.validation_metrics[0].value = 0.75;
    assert!(value.validate().is_ok());

    value.validation_metrics[0].sample_count = 0;
    assert!(value
        .validate()
        .unwrap_err()
        .to_string()
        .contains("non-zero"));
    value.validation_metrics[0].sample_count = 1;
    value.validation_metrics[0].value = f64::NAN;
    assert!(value.validate().unwrap_err().to_string().contains("finite"));

    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(artifact_path()).unwrap()).unwrap();
    json["validation_metrics"][0]["kind"] = serde_json::json!("invented-score");
    assert!(serde_json::from_value::<UvCalibrationArtifact>(json).is_err());
}

#[test]
fn response_is_required_strict_and_has_a_fixed_log_ratio_denominator() {
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(artifact_path()).unwrap()).unwrap();
    let mut missing = json.clone();
    missing.as_object_mut().unwrap().remove("response");
    assert!(serde_json::from_value::<UvCalibrationArtifact>(missing).is_err());
    let mut unknown = json;
    unknown["response"]["kind"] = serde_json::json!("unknown-response");
    assert!(serde_json::from_value::<UvCalibrationArtifact>(unknown).is_err());

    let mut value = artifact();
    value.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [335, 650],
    };
    assert!(value
        .validate()
        .unwrap_err()
        .to_string()
        .contains("336–650"));
}

#[test]
fn evaluation_preserves_in_boundary_and_out_of_domain_status() {
    let correction = correction();
    let evaluate = |x| {
        let predictors = BTreeMap::from([("x".to_string(), x)]);
        correction.evaluate(UvEvaluationInput {
            predictors: &predictors,
            measured_band: None,
        })
    };

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
        .evaluate(UvEvaluationInput {
            predictors: &BTreeMap::from([("x".to_string(), 12.0)]),
            measured_band: None,
        })
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
        .evaluate(UvEvaluationInput {
            predictors: &BTreeMap::from([("x".to_string(), 5.0)]),
            measured_band: Some(MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            }),
        })
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
fn log_ratio_artifact_rejects_absolute_uncertainty_floors() {
    let temporary = tempfile::tempdir().unwrap();
    let mut value = artifact();
    value.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [336, 650],
    };
    value.uncertainty_model.statistical_floor_ph_m2_s = 1.0;
    let err = load_mutated(&temporary, &value).unwrap_err().to_string();
    assert!(
        err.contains("log-ratio UV artifacts must set absolute"),
        "unexpected error: {err}"
    );
}

#[test]
fn log_ratio_response_uses_typed_measured_context_and_jacobian_covariance() {
    let temporary = tempfile::tempdir().unwrap();
    let mut value = artifact();
    value.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [336, 650],
    };
    let CorrectionModel::Linear {
        parameters,
        covariance,
    } = &mut value.model;
    *parameters = vec![0.1_f64.ln(), 0.0];
    *covariance = vec![vec![0.04, 0.0], vec![0.0, 0.0]];
    value.uncertainty_model.statistical_floor_ph_m2_s = 0.0;
    value.uncertainty_model.systematic_floor_ph_m2_s = 0.0;
    let correction = load_mutated(&temporary, &value).unwrap();
    let predictors = BTreeMap::from([("x".to_string(), 5.0)]);

    let missing = correction
        .evaluate(UvEvaluationInput {
            predictors: &predictors,
            measured_band: None,
        })
        .unwrap_err()
        .to_string();
    assert!(missing.contains("requires measured"));
    for invalid_flux in [0.0, -1.0, f64::NAN] {
        assert!(correction
            .evaluate(UvEvaluationInput {
                predictors: &predictors,
                measured_band: Some(MeasuredBandInput {
                    flux_336_650_ph_m2_s: invalid_flux,
                    statistical_uncertainty_336_650_ph_m2_s: 4.0,
                }),
            })
            .is_err());
    }

    let evaluation = correction
        .evaluate(UvEvaluationInput {
            predictors: &predictors,
            measured_band: Some(MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            }),
        })
        .unwrap();
    assert!((evaluation.flux_300_336_ph_m2_s.unwrap() - 10.0).abs() < 1.0e-12);
    assert!(
        (evaluation
            .statistical_uncertainty_300_336_ph_m2_s
            .unwrap()
            .powi(2)
            - 4.56)
            .abs()
            < 1.0e-12
    );
    assert!(
        (evaluation
            .measured_correction_statistical_covariance_ph2_m4_s2
            .unwrap()
            - 3.6)
            .abs()
            < 1.0e-12
    );
    let combined = correction
        .combine_with_measured(100.0, 4.0, &evaluation)
        .unwrap();
    assert!((combined.flux_300_650_ph_m2_s - 110.0).abs() < 1.0e-12);
    assert!((combined.statistical_uncertainty_300_650_ph_m2_s.powi(2) - 27.76).abs() < 1.0e-12);
    assert!(correction
        .combine_with_measured(101.0, 4.0, &evaluation)
        .unwrap_err()
        .to_string()
        .contains("does not match"));
}

#[test]
fn zero_residual_correlation_preserves_log_ratio_structural_covariance() {
    let temporary = tempfile::tempdir().unwrap();
    let mut value = artifact();
    value.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [336, 650],
    };
    let CorrectionModel::Linear {
        parameters,
        covariance,
    } = &mut value.model;
    *parameters = vec![0.1_f64.ln(), 0.0];
    *covariance = vec![vec![0.04, 0.0], vec![0.0, 0.0]];
    value.uncertainty_model.statistical_floor_ph_m2_s = 0.0;
    value.uncertainty_model.systematic_floor_ph_m2_s = 0.0;
    value
        .uncertainty_model
        .measured_conditional_residual_statistical_correlation = 0.0;
    let correction = load_mutated(&temporary, &value).unwrap();
    let predictors = BTreeMap::from([("x".to_string(), 5.0)]);

    let evaluation = correction
        .evaluate(UvEvaluationInput {
            predictors: &predictors,
            measured_band: Some(MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            }),
        })
        .unwrap();

    assert!(
        (evaluation
            .statistical_uncertainty_300_336_ph_m2_s
            .unwrap()
            .powi(2)
            - 4.16)
            .abs()
            < 1.0e-12
    );
    assert!(
        (evaluation
            .measured_correction_statistical_covariance_ph2_m4_s2
            .unwrap()
            - 1.6)
            .abs()
            < 1.0e-12
    );
    let combined = correction
        .combine_with_measured(100.0, 4.0, &evaluation)
        .unwrap();
    assert!((combined.statistical_uncertainty_300_650_ph_m2_s.powi(2) - 23.36).abs() < 1.0e-12);
}

#[test]
fn log_ratio_response_rejects_exponential_overflow() {
    let temporary = tempfile::tempdir().unwrap();
    let mut value = artifact();
    value.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [336, 650],
    };
    let CorrectionModel::Linear { parameters, .. } = &mut value.model;
    parameters[0] = 1000.0;
    parameters[1] = 0.0;
    value.uncertainty_model.statistical_floor_ph_m2_s = 0.0;
    value.uncertainty_model.systematic_floor_ph_m2_s = 0.0;
    let correction = load_mutated(&temporary, &value).unwrap();
    let predictors = BTreeMap::from([("x".to_string(), 5.0)]);
    assert!(correction
        .evaluate(UvEvaluationInput {
            predictors: &predictors,
            measured_band: Some(MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            }),
        })
        .unwrap_err()
        .to_string()
        .contains("overflow"));
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
fn reference_source_table_must_exactly_cover_partitions_before_materialization() {
    let temporary = tempfile::tempdir().unwrap();
    let root = copy_fixture(&temporary);
    write_reference_sources(
        &root,
        "source_id,sky_region\nboundary,test-sky\ninside,test-sky\noutside,test-sky\ntrain-source,train-sky\nunpartitioned,extra-sky\nvalidation-source,validation-sky\n",
    );
    let error = validation_error(&root);
    assert!(error.contains("source coverage mismatch"));
    assert!(error.contains("unpartitioned"));
    assert!(!root.join("materialized-partitions.json").exists());

    write_reference_sources(
        &root,
        "source_id,sky_region\nboundary,test-sky\ninside,test-sky\noutside,test-sky\ntrain-source,train-sky\n",
    );
    let error = validation_error(&root);
    assert!(error.contains("validation-source"));
    assert!(!root.join("materialized-partitions.json").exists());
}

#[test]
fn reference_source_table_rejects_sky_mismatch_and_duplicate_ids() {
    let temporary = tempfile::tempdir().unwrap();
    let root = copy_fixture(&temporary);
    write_reference_sources(
        &root,
        "source_id,sky_region\nboundary,wrong-sky\ninside,test-sky\noutside,test-sky\ntrain-source,train-sky\nvalidation-source,validation-sky\n",
    );
    let error = validation_error(&root);
    assert!(error.contains("sky mismatch"));
    assert!(error.contains("boundary"));

    write_reference_sources(
        &root,
        "source_id,sky_region\nboundary,test-sky\nboundary,test-sky\ninside,test-sky\noutside,test-sky\ntrain-source,train-sky\nvalidation-source,validation-sky\n",
    );
    assert!(validation_error(&root).contains("repeats source ID boundary"));

    write_reference_sources(
        &root,
        "source_id,sky_region\nTODO,test-sky\nboundary,test-sky\ninside,test-sky\noutside,test-sky\ntrain-source,train-sky\nvalidation-source,validation-sky\n",
    );
    assert!(validation_error(&root).contains("invalid UV reference source table row 2"));
}

#[test]
fn reference_source_table_requires_present_distinct_configured_columns() {
    for (column, expected_error) in [
        ("source_id", "must be distinct"),
        ("missing_sky_column", "has no configured sky-region column"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = copy_fixture(&temporary);
        let manifest_path = root.join("reference-manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest["sky_region_column"] = serde_json::json!(column);
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        assert!(validation_error(&root).contains(expected_error));
    }
}

#[test]
fn holdout_rows_are_restricted_to_the_exact_test_partition() {
    let cases = [
        (
            "train-source,20,100,4,blue,bright,low,good,train-sky,5\n",
            "not the test partition",
        ),
        (
            "validation-source,20,100,4,blue,bright,low,good,validation-sky,5\n",
            "not the test partition",
        ),
        (
            "unknown-source,20,100,4,blue,bright,low,good,test-sky,5\n",
            "is not partitioned",
        ),
    ];
    for (extra_row, expected_error) in cases {
        let temporary = tempfile::tempdir().unwrap();
        let root = copy_fixture(&temporary);
        let mut holdout = fs::read_to_string(root.join("holdout.csv")).unwrap();
        holdout.push_str(extra_row);
        fs::write(root.join("holdout.csv"), holdout).unwrap();
        assert!(validation_error(&root).contains(expected_error));
    }
}

#[test]
fn holdout_rejects_sky_mismatch_duplicate_and_missing_test_sources() {
    let temporary = tempfile::tempdir().unwrap();
    let root = copy_fixture(&temporary);
    let original = fs::read_to_string(root.join("holdout.csv")).unwrap();

    fs::write(
        root.join("holdout.csv"),
        original.replacen(
            "inside,20,100,4,blue,bright,low,good,test-sky,5",
            "inside,20,100,4,blue,bright,low,good,wrong-sky,5",
            1,
        ),
    )
    .unwrap();
    assert!(validation_error(&root).contains("sky region wrong-sky"));

    fs::write(
        root.join("holdout.csv"),
        format!("{original}inside,20,100,4,blue,bright,low,good,test-sky,5\n"),
    )
    .unwrap();
    assert!(validation_error(&root).contains("repeats source_id inside"));

    let without_outside = original
        .lines()
        .filter(|line| !line.starts_with("outside,"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(root.join("holdout.csv"), without_outside).unwrap();
    let error = validation_error(&root);
    assert!(error.contains("does not equal the test partition"));
    assert!(error.contains("outside"));
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
    assert_eq!(report.response, ModelResponse::AbsoluteUvPhotonFlux);
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
