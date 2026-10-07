// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Versioned Starlight science-policy registry.
//!
//! New production policies may be introduced over time. Historical merge reports
//! must remain independently verifiable against the policy version they were
//! emitted under — validation looks up the report's declared policy IDs in this
//! registry rather than requiring equality with the *current* emission constants.

use super::product::{PopulationCorrectionReport, SciencePolicyReport, SpectralCoverageReport};
use crate::starlight::uncertainty::CorrelationScope;
use crate::starlight::uv::CalibrationStatus;

/// Admission policy currently emitted by production finalization.
pub(crate) const CURRENT_ADMISSION_POLICY_ID: &str = "gaia-dr3-full-population-v1";

/// Population stub currently emitted when no selection-function artifact is configured.
pub(crate) const CURRENT_POPULATION_POLICY_ID: &str = "selection-function-identity-stub-v1";

/// Measured-only spectral policy currently emitted without UV correction.
pub(crate) const CURRENT_SPECTRAL_POLICY_ID: &str = "gaia-xp-continuous-336-650-v1";

/// UV-corrected spectral policy currently emitted for the combined 300–650 product.
pub(crate) const CURRENT_CORRECTED_SPECTRAL_POLICY_ID: &str =
    "gaia-xp-continuous-uv-corrected-300-650-v1";

/// Admission rules for `gaia-dr3-full-population-v1`.
///
/// Sources lacking UV predictors are excluded from the canonical combined
/// 300–650 map (`invalid_uv_predictors`); their measured 336–650 contribution
/// is quantified by Experiment C diagnostics rather than published as an
/// incomplete lower bound labelled as full-band flux.
pub(crate) const ADMISSION_RULES_V1: [&str; 8] = [
    "require_gaia_source_match",
    "exclude_calibration_failed",
    "exclude_non_positive_or_non_finite_flux",
    "exclude_invalid_statistical_uncertainty",
    "exclude_duplicated_source",
    "exclude_scientific_exclusion_nonstellar",
    "route_non_xp_via_photometric_inference",
    "exclude_no_xp_spectrum_without_photometric_artifact",
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct KnownAdmissionPolicy {
    pub id: &'static str,
    pub rules: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct KnownSpectralPolicy {
    pub id: &'static str,
    pub ultraviolet_correction_applied: bool,
}

/// Every admission policy that validators still accept.
pub(crate) fn known_admission_policies() -> &'static [KnownAdmissionPolicy] {
    &[KnownAdmissionPolicy {
        id: "gaia-dr3-full-population-v1",
        rules: &ADMISSION_RULES_V1,
    }]
}

/// Every spectral-coverage policy that validators still accept.
pub(crate) fn known_spectral_policies() -> &'static [KnownSpectralPolicy] {
    &[
        KnownSpectralPolicy {
            id: "gaia-xp-continuous-336-650-v1",
            ultraviolet_correction_applied: false,
        },
        KnownSpectralPolicy {
            id: "gaia-xp-continuous-uv-corrected-300-650-v1",
            ultraviolet_correction_applied: true,
        },
    ]
}

pub(crate) fn lookup_admission_policy(id: &str) -> Option<&'static KnownAdmissionPolicy> {
    known_admission_policies()
        .iter()
        .find(|policy| policy.id == id)
}

pub(crate) fn lookup_spectral_policy(id: &str) -> Option<&'static KnownSpectralPolicy> {
    known_spectral_policies()
        .iter()
        .find(|policy| policy.id == id)
}

fn population_policy_matches(population: &PopulationCorrectionReport) -> bool {
    if population.applied {
        !population.policy_id.trim().is_empty()
            && population.policy_id != CURRENT_POPULATION_POLICY_ID
            && population.minimum_weight == 1.0
            && population.maximum_weight.is_finite()
            && population.maximum_weight >= 1.0
            && !population.limitation.trim().is_empty()
    } else {
        population.policy_id == CURRENT_POPULATION_POLICY_ID
            && population.minimum_weight == 1.0
            && population.maximum_weight == 1.0
            && !population.residual_faint_tail_estimated
            && !population.limitation.trim().is_empty()
    }
}

fn spectral_policy_matches(spectral: &SpectralCoverageReport, known: &KnownSpectralPolicy) -> bool {
    if spectral.ultraviolet_correction_applied != known.ultraviolet_correction_applied {
        return false;
    }
    if known.ultraviolet_correction_applied {
        spectral.corrected_band_nm == Some([300, 336])
            && spectral.combined_band_nm == Some([300, 650])
            && spectral
                .correction_model_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            && spectral
                .correction_artifact_sha256
                .as_deref()
                .is_some_and(is_sha256)
            && spectral.calibration_status == Some(CalibrationStatus::Validated)
            && spectral.model_response.is_some()
            && spectral
                .measured_conditional_residual_statistical_correlation
                .is_some_and(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
            && spectral.systematic_correlation.is_some()
            && spectral.systematic_correlation_scope
                == spectral.systematic_correlation.map(CorrelationScope::from)
    } else {
        spectral.corrected_band_nm.is_none()
            && spectral.combined_band_nm.is_none()
            && spectral.correction_model_id.is_none()
            && spectral.correction_artifact_sha256.is_none()
            && spectral.calibration_status.is_none()
            && spectral.model_response.is_none()
            && spectral
                .measured_conditional_residual_statistical_correlation
                .is_none()
            && spectral.systematic_correlation.is_none()
            && spectral.systematic_correlation_scope.is_none()
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Validate a merge-report science-policy block against the historical registry.
///
/// Unknown policy IDs fail closed. Known IDs must match their registered rules /
/// spectral contract exactly — a report that claims `v1` but carries `v2` rules
/// is rejected even if `v2` also exists in the registry.
pub(crate) fn science_policy_matches_registry(policy: &SciencePolicyReport) -> bool {
    let Some(admission) = lookup_admission_policy(&policy.admission_policy_id) else {
        return false;
    };
    let Some(spectral_known) = lookup_spectral_policy(&policy.spectral_coverage.policy_id) else {
        return false;
    };
    let rules_match = policy.admission_rules.len() == admission.rules.len()
        && policy
            .admission_rules
            .iter()
            .zip(admission.rules.iter())
            .all(|(observed, expected)| observed == expected);
    policy.schema_version == 2
        && rules_match
        && population_policy_matches(&policy.population_correction)
        && policy.spectral_coverage.target_band_nm == [300, 650]
        && policy.spectral_coverage.directly_integrated_band_nm == [336, 650]
        && spectral_policy_matches(&policy.spectral_coverage, spectral_known)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::uv::{ModelResponse, SystematicCorrelation};

    fn valid_v1_corrected_policy() -> SciencePolicyReport {
        SciencePolicyReport {
            schema_version: 2,
            admission_policy_id: CURRENT_ADMISSION_POLICY_ID.to_string(),
            admission_rules: ADMISSION_RULES_V1
                .iter()
                .map(|rule| (*rule).to_string())
                .collect(),
            population_correction: PopulationCorrectionReport {
                policy_id: CURRENT_POPULATION_POLICY_ID.to_string(),
                applied: false,
                minimum_weight: 1.0,
                maximum_weight: 1.0,
                residual_faint_tail_estimated: false,
                limitation: "stub".to_string(),
            },
            spectral_coverage: SpectralCoverageReport {
                policy_id: CURRENT_CORRECTED_SPECTRAL_POLICY_ID.to_string(),
                target_band_nm: [300, 650],
                directly_integrated_band_nm: [336, 650],
                corrected_band_nm: Some([300, 336]),
                combined_band_nm: Some([300, 650]),
                ultraviolet_correction_applied: true,
                correction_model_id: Some("calspec-linear-log-ratio-v2".to_string()),
                correction_artifact_sha256: Some("a".repeat(64)),
                calibration_status: Some(CalibrationStatus::Validated),
                model_response: Some(ModelResponse::NaturalLogUvToMeasuredFluxRatio {
                    denominator_band_nm: [336, 650],
                }),
                measured_conditional_residual_statistical_correlation: Some(0.0),
                systematic_correlation: Some(SystematicCorrelation::FullyCorrelatedBetweenSources),
                systematic_correlation_scope: Some(CorrelationScope::GlobalCorrelated),
                limitation: "model-corrected UV".to_string(),
            },
        }
    }

    #[test]
    fn known_v1_policy_validates() {
        assert!(science_policy_matches_registry(&valid_v1_corrected_policy()));
    }

    #[test]
    fn unknown_admission_policy_fails_closed() {
        let mut policy = valid_v1_corrected_policy();
        policy.admission_policy_id = "gaia-dr3-full-population-v999".to_string();
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn unknown_spectral_policy_fails_closed() {
        let mut policy = valid_v1_corrected_policy();
        policy.spectral_coverage.policy_id =
            "gaia-xp-continuous-uv-corrected-300-650-v999".to_string();
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn known_admission_policy_with_mismatching_rules_fails() {
        let mut policy = valid_v1_corrected_policy();
        policy
            .admission_rules
            .push("retain_measured_336_650_when_uv_predictors_unavailable".to_string());
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn measured_only_spectral_v1_validates() {
        let mut policy = valid_v1_corrected_policy();
        policy.spectral_coverage = SpectralCoverageReport {
            policy_id: CURRENT_SPECTRAL_POLICY_ID.to_string(),
            target_band_nm: [300, 650],
            directly_integrated_band_nm: [336, 650],
            corrected_band_nm: None,
            combined_band_nm: None,
            ultraviolet_correction_applied: false,
            correction_model_id: None,
            correction_artifact_sha256: None,
            calibration_status: None,
            model_response: None,
            measured_conditional_residual_statistical_correlation: None,
            systematic_correlation: None,
            systematic_correlation_scope: None,
            limitation: "measured only".to_string(),
        };
        assert!(science_policy_matches_registry(&policy));
    }

    #[test]
    fn known_spectral_policy_with_wrong_uv_flag_fails() {
        let mut policy = valid_v1_corrected_policy();
        policy.spectral_coverage.ultraviolet_correction_applied = false;
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn applied_population_policy_with_stub_id_fails() {
        let mut policy = valid_v1_corrected_policy();
        policy.population_correction.applied = true;
        policy.population_correction.policy_id = CURRENT_POPULATION_POLICY_ID.to_string();
        policy.population_correction.maximum_weight = 2.0;
        policy.population_correction.limitation = "applied".to_string();
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn applied_population_policy_with_empty_id_fails() {
        let mut policy = valid_v1_corrected_policy();
        policy.population_correction.applied = true;
        policy.population_correction.policy_id = String::new();
        policy.population_correction.maximum_weight = 2.0;
        policy.population_correction.limitation = "applied".to_string();
        assert!(!science_policy_matches_registry(&policy));
    }

    #[test]
    fn applied_population_policy_with_valid_metadata_matches() {
        let mut policy = valid_v1_corrected_policy();
        policy.population_correction.applied = true;
        policy.population_correction.policy_id = "gaia-selection-function-v1".to_string();
        policy.population_correction.minimum_weight = 1.0;
        policy.population_correction.maximum_weight = 2.5;
        policy.population_correction.limitation = "applied selection weights".to_string();
        assert!(science_policy_matches_registry(&policy));
    }

    #[test]
    fn measured_only_spectral_claiming_uv_fields_fails() {
        let mut policy = valid_v1_corrected_policy();
        policy.spectral_coverage = SpectralCoverageReport {
            policy_id: CURRENT_SPECTRAL_POLICY_ID.to_string(),
            target_band_nm: [300, 650],
            directly_integrated_band_nm: [336, 650],
            corrected_band_nm: Some([300, 336]),
            combined_band_nm: None,
            ultraviolet_correction_applied: false,
            correction_model_id: None,
            correction_artifact_sha256: None,
            calibration_status: None,
            model_response: None,
            measured_conditional_residual_statistical_correlation: None,
            systematic_correlation: None,
            systematic_correlation_scope: None,
            limitation: "measured only".to_string(),
        };
        assert!(!science_policy_matches_registry(&policy));
    }
}
