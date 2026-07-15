//! Frozen GaiaXPy 2.1.4 parity gate for in-process XP continuous calibration.

use nsb_data_tools::gaia_xp::integrate_photon_flux;
use nsb_data_tools::gaia_xp_continuous::PINNED_GAIA_XPY_VERSION;
use nsb_data_tools::gaia_xp_continuous_calibrate::GaiaXpContinuousCalibrator;
use nsb_data_tools::gaia_xp_continuous_canonical::{
    packed_correlation_len, CanonicalXpContinuousRecord, XpContinuousSourceFormat,
    CANONICAL_XP_CONTINUOUS_SCHEMA,
};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleManifest {
    schema_version: u32,
    corpus_id: String,
    generation: OracleGeneration,
    tolerances: OracleTolerances,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleGeneration {
    gaiaxpy_version: String,
    gaiaxpy_distribution_version: String,
    gaiaxpy_package_sha256: String,
    gaiaxpy_hashed_file_count: usize,
    numpy_version: String,
    pandas_version: String,
    python_version: String,
    sampling_nm: Vec<f64>,
    truncation: bool,
    input_kind: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleTolerances {
    spectral_relative: f64,
    spectral_absolute: f64,
    integrated_flux_relative: f64,
    integrated_uncertainty_relative: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleRecord {
    schema_version: u32,
    split: String,
    source_id: String,
    bp_n_relevant_bases: u16,
    rp_n_relevant_bases: u16,
    bp_standard_deviation: f64,
    rp_standard_deviation: f64,
    bp_coefficients: Vec<f64>,
    rp_coefficients: Vec<f64>,
    bp_coefficient_errors: Vec<f64>,
    rp_coefficient_errors: Vec<f64>,
    correlations: String,
    oracle: OracleOutcome,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleOutcome {
    sampling_nm: Vec<f64>,
    flux_w_m2_nm: Vec<f64>,
    flux_error_w_m2_nm: Vec<f64>,
    flux_336_650_ph_m2_s: f64,
    statistical_uncertainty_336_650_ph_m2_s: f64,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gaiaxpy_oracle")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> T {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

#[test]
fn gaia_xp_continuous_calibrate_matches_frozen_gaiaxpy_oracle() {
    let root = fixture_root();
    let manifest: OracleManifest = read_json(&root.join("manifest.json"));
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(
        manifest.corpus_id,
        "gaiaxpy-2.1.4-synthetic-xp-continuous-v1"
    );
    assert_eq!(manifest.generation.gaiaxpy_version, PINNED_GAIA_XPY_VERSION);
    assert_eq!(
        manifest.generation.gaiaxpy_distribution_version,
        PINNED_GAIA_XPY_VERSION
    );
    assert!(!manifest.generation.truncation);
    assert_eq!(manifest.generation.sampling_nm.len(), 158);
    assert_eq!(manifest.generation.gaiaxpy_package_sha256.len(), 64);
    assert!(manifest.generation.gaiaxpy_hashed_file_count > 0);
    assert!(!manifest.generation.numpy_version.is_empty());
    assert!(!manifest.generation.pandas_version.is_empty());
    assert!(!manifest.generation.python_version.is_empty());
    assert!(manifest.generation.input_kind.contains("synthetic"));

    let mut record_paths = fs::read_dir(&root)
        .expect("oracle directory")
        .map(|entry| entry.expect("oracle directory entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("record-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    record_paths.sort();
    assert!(
        record_paths.len() >= 3,
        "oracle must contain representative development and holdout records"
    );

    let design_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json");
    let calibrator =
        GaiaXpContinuousCalibrator::from_design_fixture(&design_path).expect("design fixture");

    let mut development_records = 0;
    let mut holdout_records = 0;
    let mut records_with_negative_samples = 0;
    let mut records_with_relevant_basis_cut = 0;

    for path in record_paths {
        let entry: OracleRecord = read_json(&path);
        assert_eq!(entry.schema_version, 1);
        match entry.split.as_str() {
            "development" => development_records += 1,
            "holdout" => holdout_records += 1,
            other => panic!("unsupported oracle split {other:?}"),
        }
        assert_eq!(entry.correlations, "zero");
        assert_eq!(entry.bp_coefficients.len(), 55);
        assert_eq!(entry.rp_coefficients.len(), 55);
        if entry.bp_n_relevant_bases < 55 || entry.rp_n_relevant_bases < 55 {
            records_with_relevant_basis_cut += 1;
        }

        let correlation_count = packed_correlation_len(55);
        let canonical = CanonicalXpContinuousRecord {
            schema_version: CANONICAL_XP_CONTINUOUS_SCHEMA,
            source_id: entry.source_id.clone(),
            bp_n_parameters: 55,
            rp_n_parameters: 55,
            bp_n_relevant_bases: Some(entry.bp_n_relevant_bases),
            rp_n_relevant_bases: Some(entry.rp_n_relevant_bases),
            bp_standard_deviation: entry.bp_standard_deviation,
            rp_standard_deviation: entry.rp_standard_deviation,
            bp_coefficients: entry.bp_coefficients,
            rp_coefficients: entry.rp_coefficients,
            bp_coefficient_errors: entry.bp_coefficient_errors,
            rp_coefficient_errors: entry.rp_coefficient_errors,
            bp_coefficient_correlations: vec![0.0; correlation_count],
            rp_coefficient_correlations: vec![0.0; correlation_count],
            source_format: XpContinuousSourceFormat::DataLink,
            source_checksum: None,
            quality_flags: Vec::new(),
        };

        let product = calibrator
            .calibrate_record_product(&canonical)
            .unwrap_or_else(|error| {
                panic!("source {} calibration failed: {error}", canonical.source_id)
            });
        assert_slice_close(
            &format!("{} wavelength", canonical.source_id),
            &product.wavelengths_nm,
            &entry.oracle.sampling_nm,
            0.0,
            0.0,
        );
        assert_slice_close(
            &format!("{} spectral flux", canonical.source_id),
            &product.flux_w_m2_nm,
            &entry.oracle.flux_w_m2_nm,
            manifest.tolerances.spectral_relative,
            manifest.tolerances.spectral_absolute,
        );
        let errors = product
            .flux_error_w_m2_nm
            .as_ref()
            .expect("calibrated product must carry spectral uncertainty");
        assert_slice_close(
            &format!("{} spectral uncertainty", canonical.source_id),
            errors,
            &entry.oracle.flux_error_w_m2_nm,
            manifest.tolerances.spectral_relative,
            manifest.tolerances.spectral_absolute,
        );
        if product.flux_w_m2_nm.iter().any(|value| *value < 0.0) {
            records_with_negative_samples += 1;
        }

        let integral = integrate_photon_flux(&product).expect("authoritative Rust integration");
        assert_relative_close(
            &format!("{} integrated flux", canonical.source_id),
            integral.total_ph_m2_s,
            entry.oracle.flux_336_650_ph_m2_s,
            manifest.tolerances.integrated_flux_relative,
        );
        assert_relative_close(
            &format!("{} integrated uncertainty", canonical.source_id),
            integral.uncertainty_ph_m2_s.expect("integral uncertainty"),
            entry.oracle.statistical_uncertainty_336_650_ph_m2_s,
            manifest.tolerances.integrated_uncertainty_relative,
        );

        let summary = calibrator
            .calibrate_record(&canonical)
            .expect("calibrated summary");
        assert_eq!(summary.flux_336_650_ph_m2_s, integral.total_ph_m2_s);
        assert_eq!(
            summary.statistical_uncertainty_336_650_ph_m2_s,
            integral.uncertainty_ph_m2_s.expect("integral uncertainty")
        );
    }

    assert!(development_records > 0, "oracle has no development records");
    assert!(holdout_records > 0, "oracle has no holdout records");
    assert!(
        records_with_negative_samples > 0,
        "oracle must exercise signed spectral samples"
    );
    assert!(
        records_with_relevant_basis_cut > 0,
        "oracle must prove the pinned truncation=false policy"
    );
}

fn assert_slice_close(label: &str, got: &[f64], reference: &[f64], rtol: f64, atol: f64) {
    assert_eq!(got.len(), reference.len(), "{label} length mismatch");
    for (index, (got, reference)) in got.iter().zip(reference).enumerate() {
        let tolerance = atol + rtol * got.abs().max(reference.abs());
        let difference = (got - reference).abs();
        assert!(
            difference <= tolerance,
            "{label} sample {index}: |{got} - {reference}| = {difference} > {tolerance}"
        );
    }
}

fn assert_relative_close(label: &str, got: f64, reference: f64, tolerance: f64) {
    let denominator = got.abs().max(reference.abs()).max(f64::MIN_POSITIVE);
    let relative = (got - reference).abs() / denominator;
    assert!(
        relative <= tolerance,
        "{label}: relative error {relative} > {tolerance} (got={got}, reference={reference})"
    );
}
