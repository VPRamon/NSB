//! Hand-checkable analytical fixtures for the Starlight uncertainty
//! contract (issue #94).
//!
//! Each test below verifies one closed-form combination rule against a
//! deliberately trivial input so the expected number can be checked by hand
//! (not just re-derived from the same code under test). See
//! `docs/nsb_components/starlight/uncertainty-contract.md` for the full term
//! glossary these fixtures exercise.

use super::*;
use crate::platform::checksum_io;
use crate::starlight::map::accumulator::{
    source_id_to_pixel, PartitionShard, UvCorrectionShardMetadata,
};
use crate::starlight::selection::{
    ColourMarginalisation, CompletenessEntry, FaintTailModel, SelectionArtifact,
    SelectionCorrection, SelectionReferenceDataset, SelectionReferenceFile,
};
use crate::starlight::uv::{
    CalibrationStatus, CombinedBandFlux, CorrectionModel, ModelResponse, SystematicCorrelation,
    UvCorrection,
};
use std::fs;
use tempfile::TempDir;

fn uv_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/uv_synthetic_non_production/artifact.json")
}

fn load_uv_fixture() -> UvCorrection {
    let path = uv_fixture_path();
    let sha256 = checksum_io::sha256_file(&path).unwrap();
    UvCorrection::load(&path, &sha256).unwrap()
}

fn load_uv_correction(artifact: &crate::starlight::uv::UvCalibrationArtifact) -> UvCorrection {
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("artifact.json");
    let bytes = serde_json::to_vec_pretty(artifact).unwrap();
    fs::write(&path, &bytes).unwrap();
    UvCorrection::load(&path, &checksum_io::sha256_bytes(&bytes)).unwrap()
}

fn source_entry(g_mag: Option<f64>, bp_rp: Option<f64>) -> GaiaSourceEntry {
    GaiaSourceEntry {
        phot_g_mean_mag: g_mag,
        phot_bp_mean_mag: None,
        phot_rp_mean_mag: None,
        bp_rp,
        duplicated_source: false,
        in_qso_candidates: false,
        in_galaxy_candidates: false,
        predictors: None,
    }
}

fn fixture_source_id(sequence: u64) -> u64 {
    (12_345_u64 << crate::starlight::healpix::GAIA_SOURCE_ID_HEALPIX_SHIFT) | sequence
}

fn fixture_pixel(source_id: u64, nside: u32) -> u32 {
    source_id_to_pixel(source_id, nside).unwrap()
}

/// Fixture 1: one admitted source with exactly zero statistical and
/// systematic uncertainty leaves the pixel's total uncertainty exactly zero.
#[test]
fn one_source_with_zero_uncertainty_yields_zero_pixel_uncertainty() {
    let mut shard = PartitionShard::new("fixture-one-source", 4).unwrap();
    let source_id = fixture_source_id(1);
    shard.admit(source_id, 10.0, 0.0, 0.0).unwrap();
    let pixel = &shard.pixels[&fixture_pixel(source_id, 4)];
    assert_eq!(pixel.statistical_variance.value().sqrt(), 0.0);
    assert_eq!(pixel.selected_systematic_uncertainty(), 0.0);
}

/// Fixture 2: two independent, equal-uncertainty sources in the same pixel
/// combine in quadrature: `sigma = sqrt(2) * sigma_i`. `PartitionShard::admit`
/// always tags its systematic term
/// [`SystematicCorrelation::IndependentBetweenSources`], so this checks both
/// the statistical and the independent-systematic channel at once.
#[test]
fn two_independent_equal_sources_combine_in_quadrature() {
    let sigma_i = 3.0_f64;
    let mut shard = PartitionShard::new("fixture-two-independent", 4).unwrap();
    let first = fixture_source_id(1);
    let second = fixture_source_id(2);
    shard.admit(first, 10.0, sigma_i, sigma_i).unwrap();
    shard.admit(second, 10.0, sigma_i, sigma_i).unwrap();
    let pixel = &shard.pixels[&fixture_pixel(first, 4)];
    let expected = 2.0_f64.sqrt() * sigma_i;
    assert!((pixel.statistical_variance.value().sqrt() - expected).abs() < 1.0e-12);
    assert!((pixel.selected_systematic_uncertainty() - expected).abs() < 1.0e-12);
}

/// Fixture 3: two sources sharing a fully correlated systematic add
/// linearly: `sigma_sys = 2 * sigma_i`, not `sqrt(2) * sigma_i`. Uses
/// [`PartitionShard::admit_corrected`] because only the corrected
/// (300-650 nm) path can express
/// [`SystematicCorrelation::FullyCorrelatedBetweenSources`].
#[test]
fn two_fully_correlated_sources_combine_linearly() {
    let sigma_i = 4.0_f64;
    let metadata = UvCorrectionShardMetadata {
        model_id: "fixture-correlated".to_string(),
        artifact_sha256: "a".repeat(64),
        calibration_status: CalibrationStatus::Validated,
        response: ModelResponse::AbsoluteUvPhotonFlux,
        measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
        systematic_correlation: SystematicCorrelation::FullyCorrelatedBetweenSources,
    };
    let mut shard = PartitionShard::new_with_policy(
        "fixture-correlated",
        4,
        StarlightProductBand::Combined300To650,
        Some(metadata),
    )
    .unwrap();
    let source = || CombinedBandFlux {
        flux_300_336_ph_m2_s: 0.0,
        flux_336_650_ph_m2_s: 10.0,
        flux_300_650_ph_m2_s: 10.0,
        statistical_uncertainty_300_336_ph_m2_s: 0.0,
        statistical_uncertainty_336_650_ph_m2_s: 0.0,
        statistical_uncertainty_300_650_ph_m2_s: 0.0,
        systematic_uncertainty_300_336_ph_m2_s: sigma_i,
        systematic_uncertainty_300_650_ph_m2_s: sigma_i,
        applicability_status: crate::starlight::uv::ApplicabilityStatus::InDomain,
        decision: crate::starlight::uv::EvaluationDecision::Applied,
        model_id: "fixture-correlated".to_string(),
        artifact_sha256: "a".repeat(64),
        systematic_correlation: SystematicCorrelation::FullyCorrelatedBetweenSources,
    };
    shard
        .admit_corrected(fixture_source_id(1), &source())
        .unwrap();
    shard
        .admit_corrected(fixture_source_id(2), &source())
        .unwrap();
    let pixel = &shard.pixels[&fixture_pixel(fixture_source_id(1), 4)];
    assert_eq!(pixel.systematic_variance.value(), 0.0);
    assert_eq!(
        pixel.systematic_correlated_uncertainty.value(),
        2.0 * sigma_i
    );
    assert_eq!(pixel.selected_systematic_uncertainty(), 2.0 * sigma_i);
}

/// Fixture 4: completeness weighting. Flux and statistical uncertainty scale
/// linearly by the inverse-completeness weight `w`; the selection-driven
/// systematic term is `hypot(photometric_systematic, selection_fraction *
/// weighted_flux)`, exactly as coded in `admit_weighted_source`. This
/// fixture pins `completeness = 0.5` (`w = 2`) and a faint-tail systematic
/// fraction of `0.1`, isolating the selection contribution by setting the
/// photometric systematic to zero.
#[test]
fn completeness_weight_scales_flux_and_derives_systematic_from_selection_fraction() {
    let artifact = SelectionArtifact {
        schema_version: crate::starlight::selection::SELECTION_ARTIFACT_SCHEMA_VERSION,
        model_id: "fixture-selection".to_string(),
        calibration_status: CalibrationStatus::Candidate,
        reference_dataset: SelectionReferenceDataset {
            name: "fixture-selection-dataset".to_string(),
            release: "fixture".to_string(),
            licence: "CC-BY-4.0".to_string(),
            doi: "10.0000/fixture".to_string(),
            files: vec![SelectionReferenceFile {
                name: "completeness.parquet".to_string(),
                sha256: "a".repeat(64),
            }],
        },
        weight_cap: 5.0,
        magnitude_bins: vec![10.0, 15.0, 20.0],
        colour_bins: vec![0.0, 1.0, 2.0],
        healpix_nside: 1,
        coordinate_frame: crate::starlight::healpix::HealpixCoordinateFrame::Equatorial,
        ordering: crate::starlight::healpix::HealpixOrderingScheme::Nested,
        completeness_table: vec![CompletenessEntry {
            healpix: 0,
            magnitude_bin: 1,
            colour_bin: 0,
            completeness: 0.5,
        }],
        m10_map: Vec::new(),
        colour_marginalisation: ColourMarginalisation::MarginaliseUniform,
        faint_tail: FaintTailModel {
            enabled: true,
            magnitude_limit_g: 16.0,
            residual_fraction_per_pixel: 0.08,
            systematic_fraction: 0.1,
        },
        training_command: "fixture-generated, not trained".to_string(),
        software_version: "nsb-data-tools-test-fixture".to_string(),
    };
    let temporary = TempDir::new().unwrap();
    let path = temporary.path().join("selection.json");
    let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
    fs::write(&path, &bytes).unwrap();
    let selection = SelectionCorrection::load(&path, &checksum_io::sha256_bytes(&bytes)).unwrap();

    let mut shard = PartitionShard::new("fixture-weighted", 4).unwrap();
    let gaia_source = source_entry(Some(17.0), None);
    let source_id = fixture_source_id(1);
    admit_weighted_source(
        &mut shard,
        source_id,
        &gaia_source,
        10.0,
        1.0,
        0.0,
        StarlightProductBand::Measured336To650,
        None,
        Some(&selection),
    )
    .unwrap();

    let pixel = &shard.pixels[&fixture_pixel(source_id, 4)];
    // weight = 1 / 0.5 = 2 (below weight_cap).
    assert_eq!(pixel.flux_ph_m2_s.value(), 20.0);
    assert_eq!(pixel.statistical_variance.value().sqrt(), 2.0);
    // selection systematic fraction = faint_tail.systematic_fraction = 0.1
    // (g_mag=17 > magnitude_limit_g=16); systematic = hypot(0, 0.1 * 20) = 2.
    assert_eq!(pixel.selected_systematic_uncertainty(), 2.0);
}

/// Fixture 5: UV correction path with known numbers, using the repository's
/// pinned synthetic (non-production) UV artifact fixture: `flux = intercept +
/// coefficient * x`, `score_variance` from the diagonal covariance, and
/// `combine_with_measured` per the documented formulas.
#[test]
fn uv_correction_path_matches_hand_computed_numbers() {
    let correction = load_uv_fixture();
    let predictors = std::collections::BTreeMap::from([("x".to_string(), 5.0)]);
    let evaluation = correction
        .evaluate(crate::starlight::uv::UvEvaluationInput {
            predictors: &predictors,
            measured_band: Some(crate::starlight::uv::MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            }),
        })
        .unwrap();
    // model = linear([10, 2]) . [1, 5] = 10 + 2*5 = 20.
    assert_eq!(evaluation.flux_300_336_ph_m2_s, Some(20.0));
    // score_variance = [1,5] . diag(1, 0.25) . [1,5] = 1 + 0.25*25 = 7.25;
    // statistical = hypot(sqrt(7.25), statistical_floor=0.5).
    let expected_uv_statistical = 7.25_f64.sqrt().hypot(0.5);
    assert!(
        (evaluation.statistical_uncertainty_300_336_ph_m2_s.unwrap() - expected_uv_statistical)
            .abs()
            < 1.0e-12
    );

    let combined = correction
        .combine_with_measured(100.0, 4.0, &evaluation)
        .unwrap();
    assert_eq!(combined.flux_300_336_ph_m2_s, 20.0);
    assert_eq!(combined.flux_336_650_ph_m2_s, 100.0);
    assert_eq!(combined.flux_300_650_ph_m2_s, 120.0);
    // combined_variance = 4^2 + uv_statistical^2 + 2*rho*4*uv_statistical,
    // rho = artifact's measured_conditional_residual_statistical_correlation
    // = 0.25.
    let expected_combined_statistical = (4.0_f64.powi(2)
        + expected_uv_statistical.powi(2)
        + 2.0 * 0.25 * 4.0 * expected_uv_statistical)
        .sqrt();
    assert!(
        (combined.statistical_uncertainty_300_650_ph_m2_s - expected_combined_statistical).abs()
            < 1.0e-12
    );
}

/// Fixture 6: relative/log uncertainty. For the natural-log response, the
/// absolute statistical uncertainty on the UV-band flux is `flux * sigma_ln` (first
/// order propagation of `d(exp(ln_flux))/d(ln_flux) = flux`), with all other
/// terms (statistical floor, measured residual correlation, measured
/// uncertainty) pinned to zero so the formula is exact, not approximate.
#[test]
fn log_ratio_response_propagates_relative_uncertainty_as_flux_times_sigma_ln() {
    let mut artifact = base_log_ratio_artifact();
    let sigma_ln = 0.2_f64;
    let CorrectionModel::Linear {
        parameters,
        covariance,
    } = &mut artifact.model;
    *parameters = vec![0.1_f64.ln(), 0.0];
    *covariance = vec![vec![sigma_ln.powi(2), 0.0], vec![0.0, 0.0]];
    artifact.uncertainty_model.statistical_floor_ph_m2_s = 0.0;
    artifact.uncertainty_model.systematic_floor_ph_m2_s = 0.0;
    artifact
        .uncertainty_model
        .measured_conditional_residual_statistical_correlation = 0.0;
    let correction = load_uv_correction(&artifact);

    let predictors = std::collections::BTreeMap::from([("x".to_string(), 5.0)]);
    let evaluation = correction
        .evaluate(crate::starlight::uv::UvEvaluationInput {
            predictors: &predictors,
            measured_band: Some(crate::starlight::uv::MeasuredBandInput {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 0.0,
            }),
        })
        .unwrap();
    // ratio = exp(ln(0.1)) = 0.1; flux = 100 * 0.1 = 10.
    let flux = evaluation.flux_300_336_ph_m2_s.unwrap();
    assert!((flux - 10.0).abs() < 1.0e-12);
    let expected = flux * sigma_ln;
    assert!(
        (evaluation.statistical_uncertainty_300_336_ph_m2_s.unwrap() - expected).abs() < 1.0e-12
    );
}

/// Fixture 7: a single Combined 300-650 nm shard admits sources whose
/// systematic terms have genuinely different correlation-scope *origins* — the UV
/// artifact's declared [`SystematicCorrelation`] and the caller-supplied
/// photometric/selection systematic — but `admit_weighted_source` folds
/// them into one number via `hypot` before admission, and that folded value
/// is then filed entirely under the UV artifact's declared correlation
/// bucket. This is a documented simplification (see
/// `docs/nsb_components/starlight/uncertainty-contract.md`): the
/// photometric/selection component is not actually independent-per-source
/// once folded in this way, it inherits whatever correlation scope the UV
/// artifact declares.
#[test]
fn pixel_admits_combine_uv_and_photometric_systematics_before_tagging_correlation() {
    let correction = load_uv_fixture();
    assert_eq!(
        correction
            .artifact()
            .uncertainty_model
            .systematic_correlation,
        SystematicCorrelation::FullyCorrelatedBetweenSources
    );
    let metadata = UvCorrectionShardMetadata {
        model_id: correction.artifact().model_id.clone(),
        artifact_sha256: correction.artifact_sha256().to_string(),
        // Shard construction requires `Validated` metadata regardless of the
        // fixture artifact's own (intentionally non-production) status.
        calibration_status: CalibrationStatus::Validated,
        response: correction.artifact().response.clone(),
        measured_conditional_residual_statistical_correlation_bits: correction
            .artifact()
            .uncertainty_model
            .measured_conditional_residual_statistical_correlation
            .to_bits(),
        systematic_correlation: SystematicCorrelation::FullyCorrelatedBetweenSources,
    };
    let mut shard = PartitionShard::new_with_policy(
        "fixture-mixed-systematics",
        4,
        StarlightProductBand::Combined300To650,
        Some(metadata),
    )
    .unwrap();
    let gaia_source = GaiaSourceEntry {
        predictors: Some(std::collections::BTreeMap::from([("x".to_string(), 5.0)])),
        ..source_entry(None, None)
    };
    let photometric_systematic = 3.0;
    let source_id = fixture_source_id(1);
    admit_weighted_source(
        &mut shard,
        source_id,
        &gaia_source,
        100.0,
        4.0,
        photometric_systematic,
        StarlightProductBand::Combined300To650,
        Some(&correction),
        None,
    )
    .unwrap();
    let pixel = &shard.pixels[&fixture_pixel(source_id, 4)];
    // The photometric systematic (conceptually independent-per-source) was
    // hypot-folded into the UV correction's fully-correlated systematic
    // before admission, so it appears entirely in the linear-sum bucket and
    // none of it appears in the independent-quadrature bucket.
    assert_eq!(pixel.systematic_variance.value(), 0.0);
    assert!(pixel.systematic_correlated_uncertainty.value() > photometric_systematic);
}

fn base_log_ratio_artifact() -> crate::starlight::uv::UvCalibrationArtifact {
    let mut artifact: crate::starlight::uv::UvCalibrationArtifact =
        serde_json::from_slice(&fs::read(uv_fixture_path()).unwrap()).unwrap();
    artifact.response = ModelResponse::NaturalLogUvToMeasuredFluxRatio {
        denominator_band_nm: [336, 650],
    };
    artifact.uncertainty_model.statistical_floor_ph_m2_s = 0.0;
    artifact.uncertainty_model.systematic_floor_ph_m2_s = 0.0;
    artifact
}
