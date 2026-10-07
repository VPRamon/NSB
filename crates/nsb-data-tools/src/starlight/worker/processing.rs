// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Shared per-source evaluation for production and diagnostic ablation.

use super::gaia_source::GaiaSourceEntry;
use crate::starlight::config::StarlightProductBand;
use crate::starlight::healpix::{self, galactic_nested_pixel_from_icrs_position};
use crate::starlight::photometric::{
    PhotometricCorrection, PhotometricFeatures, PopulationBranch, RouteDecision,
};
use crate::starlight::selection::SelectionCorrection;
use crate::starlight::uv::{
    EvaluationDecision, MeasuredBandInput, UvCorrection, UvEvaluationInput,
};
use crate::starlight::xp::{integrate_photon_flux, integrate_photon_flux_uncertainty, XpProduct};
use serde::{Deserialize, Serialize};

/// Cumulative ablation stages for issue #116 causal experiments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AblationStage {
    /// XP continuous only; non-XP sources ignored.
    A,
    /// XP + photometric inference for non-XP sources.
    B,
    /// Combined 300–650 with UV correction.
    C,
    /// Selection inverse-completeness weighting.
    D,
    /// Full production admission/exclusion policy.
    E,
}

impl AblationStage {
    pub fn all() -> [Self; 5] {
        [Self::A, Self::B, Self::C, Self::D, Self::E]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A_xp_only",
            Self::B => "B_plus_photometric",
            Self::C => "C_plus_uv",
            Self::D => "D_plus_selection",
            Self::E => "E_full_production",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceOutcome {
    pub admitted: bool,
    pub exclusion_reason: Option<String>,
    pub population_branch: Option<String>,
    pub xp_available: bool,
    pub galactic_pixel: u32,
    pub selection_healpix: Option<u32>,
    pub raw_flux_336_650_ph_m2_s: f64,
    pub raw_flux_300_650_ph_m2_s: f64,
    pub uv_flux_300_336_ph_m2_s: f64,
    pub weighted_flux_300_650_ph_m2_s: f64,
    pub selection_weight: f64,
    pub selection_completeness: f64,
    pub selection_capped: bool,
    pub g_mag: Option<f64>,
    pub bp_rp: Option<f64>,
}

pub(crate) fn scientific_exclusion_reason(gaia_source: &GaiaSourceEntry) -> Option<&'static str> {
    if gaia_source.duplicated_source {
        return Some("duplicated_source");
    }
    if gaia_source.in_qso_candidates || gaia_source.in_galaxy_candidates {
        return Some("scientific_exclusion_nonstellar");
    }
    None
}

pub(crate) fn population_branch_reason(branch: PopulationBranch) -> &'static str {
    match branch {
        PopulationBranch::XpContinuous => "xp_continuous",
        PopulationBranch::PhotometricGBpRp => "photometric_g_bp_rp",
        PopulationBranch::PhotometricPartial => "photometric_partial",
        PopulationBranch::PhotometricGOnly => "photometric_g_only",
        PopulationBranch::NoUsablePhotometry => "no_usable_photometry",
        PopulationBranch::ScientificExclusion => "scientific_exclusion",
    }
}

/// Production XP measured-band gate shared by the worker and flux-attribution.
///
/// Order matches `worker.rs`: integrate flux, require finite positive flux,
/// then integrate uncertainty and require finite non-negative uncertainty.
/// Callers must treat calibration failure separately (never photometric fallback).
pub(crate) fn measured_xp_flux_and_uncertainty(
    product: &XpProduct,
) -> Result<(f64, f64), &'static str> {
    let flux = match integrate_photon_flux(product) {
        Ok(flux) if flux.is_finite() && flux > 0.0 => flux,
        _ => return Err("invalid_flux"),
    };
    let uncertainty = match integrate_photon_flux_uncertainty(product) {
        Ok(uncertainty) if uncertainty.is_finite() && uncertainty >= 0.0 => uncertainty,
        _ => return Err("invalid_uncertainty"),
    };
    Ok((flux, uncertainty))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate_source_for_diagnostic(
    gaia_source: &GaiaSourceEntry,
    xp_product: Option<&XpProduct>,
    stage: AblationStage,
    nside: u32,
    product_band: StarlightProductBand,
    ultraviolet_correction: Option<&UvCorrection>,
    photometric_correction: Option<&PhotometricCorrection>,
    selection_correction: Option<&SelectionCorrection>,
) -> SourceOutcome {
    let galactic_pixel = galactic_nested_pixel_from_icrs_position(
        gaia_source.icrs.ra_deg,
        gaia_source.icrs.dec_deg,
        nside,
    )
    .unwrap_or(0);
    let mut outcome = SourceOutcome {
        admitted: false,
        exclusion_reason: None,
        population_branch: None,
        xp_available: xp_product.is_some(),
        galactic_pixel,
        selection_healpix: None,
        raw_flux_336_650_ph_m2_s: 0.0,
        raw_flux_300_650_ph_m2_s: 0.0,
        uv_flux_300_336_ph_m2_s: 0.0,
        weighted_flux_300_650_ph_m2_s: 0.0,
        selection_weight: 1.0,
        selection_completeness: 1.0,
        selection_capped: false,
        g_mag: gaia_source.phot_g_mean_mag,
        bp_rp: gaia_source.bp_rp,
    };

    if stage == AblationStage::E {
        if let Some(reason) = scientific_exclusion_reason(gaia_source) {
            outcome.exclusion_reason = Some(reason.to_string());
            return outcome;
        }
    }

    let (raw_flux_336_650, branch) = if let Some(product) = xp_product {
        outcome.population_branch = Some("xp_continuous".to_string());
        match measured_xp_flux_and_uncertainty(product) {
            Ok((flux, _uncertainty)) => (flux, PopulationBranch::XpContinuous),
            Err(reason) => {
                outcome.exclusion_reason = Some(reason.to_string());
                return outcome;
            }
        }
    } else if stage >= AblationStage::B {
        let Some(photometric) = photometric_correction else {
            outcome.exclusion_reason = Some("no_xp_spectrum".to_string());
            return outcome;
        };
        let route = match photometric.route_and_evaluate(PhotometricFeatures {
            phot_g_mean_mag: gaia_source.phot_g_mean_mag,
            phot_bp_mean_mag: gaia_source.phot_bp_mean_mag,
            phot_rp_mean_mag: gaia_source.phot_rp_mean_mag,
            bp_rp: gaia_source.bp_rp,
            quality_flag: true,
        }) {
            Ok(route) => route,
            Err(_) => {
                outcome.exclusion_reason = Some("photometric_evaluation_failed".to_string());
                return outcome;
            }
        };
        let RouteDecision { branch, flux } = route;
        outcome.population_branch = Some(population_branch_reason(branch).to_string());
        let Some(estimate) = flux else {
            outcome.exclusion_reason = Some(population_branch_reason(branch).to_string());
            return outcome;
        };
        (estimate.flux_336_650_ph_m2_s, branch)
    } else {
        outcome.exclusion_reason = Some("no_xp_spectrum".to_string());
        return outcome;
    };

    outcome.raw_flux_336_650_ph_m2_s = raw_flux_336_650;
    let mut flux_300_650 = raw_flux_336_650;
    outcome.uv_flux_300_336_ph_m2_s = 0.0;

    if stage >= AblationStage::C && product_band == StarlightProductBand::Combined300To650 {
        let Some(correction) = ultraviolet_correction else {
            outcome.exclusion_reason = Some("uv_correction_missing".to_string());
            return outcome;
        };
        let Some(predictors) = &gaia_source.predictors else {
            outcome.exclusion_reason = Some("invalid_uv_predictors".to_string());
            return outcome;
        };
        let evaluation = match correction.evaluate(UvEvaluationInput {
            predictors,
            measured_band: Some(MeasuredBandInput {
                flux_336_650_ph_m2_s: raw_flux_336_650,
                statistical_uncertainty_336_650_ph_m2_s: 0.0,
            }),
        }) {
            Ok(evaluation) => evaluation,
            Err(_) => {
                outcome.exclusion_reason = Some("uv_evaluation_failed".to_string());
                return outcome;
            }
        };
        if evaluation.decision == EvaluationDecision::Rejected {
            outcome.exclusion_reason = Some("uv_out_of_domain".to_string());
            return outcome;
        }
        match correction.combine_with_measured(raw_flux_336_650, 0.0, &evaluation) {
            Ok(combined) => {
                outcome.uv_flux_300_336_ph_m2_s = combined.flux_300_336_ph_m2_s;
                flux_300_650 = combined.flux_300_650_ph_m2_s;
            }
            Err(_) => {
                outcome.exclusion_reason = Some("uv_evaluation_failed".to_string());
                return outcome;
            }
        }
    }

    outcome.raw_flux_300_650_ph_m2_s = flux_300_650;
    let mut weight = 1.0;
    if stage >= AblationStage::D {
        if let Some(selection) = selection_correction {
            match apply_selection_weight(selection, gaia_source, &mut outcome) {
                Ok(value) => weight = value,
                Err(reason) => {
                    outcome.exclusion_reason = Some(reason.to_string());
                    return outcome;
                }
            }
        }
    }

    outcome.weighted_flux_300_650_ph_m2_s = weight * flux_300_650;
    outcome.admitted = true;
    outcome.exclusion_reason = None;
    let _ = branch;
    outcome
}

fn apply_selection_weight(
    selection: &SelectionCorrection,
    gaia_source: &GaiaSourceEntry,
    outcome: &mut SourceOutcome,
) -> Result<f64, &'static str> {
    let Some(g_mag) = gaia_source.phot_g_mean_mag else {
        return Err("selection_missing_g_magnitude");
    };
    let healpix = healpix::icrs_equatorial_nested_pixel(
        gaia_source.icrs.ra_deg,
        gaia_source.icrs.dec_deg,
        selection.artifact().healpix_nside,
    )
    .map_err(|_| "selection_healpix_failed")?;
    outcome.selection_healpix = Some(healpix);
    let evaluation = selection
        .evaluate(healpix, g_mag, gaia_source.bp_rp)
        .map_err(|_| "selection_evaluation_failed")?;
    outcome.selection_weight = evaluation.weight;
    outcome.selection_completeness = evaluation.completeness;
    outcome.selection_capped = evaluation.capped;
    Ok(evaluation.weight)
}

impl PartialOrd for AblationStage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AblationStage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use crate::starlight::healpix::test_support::fixture_icrs_from_source_id;
    use crate::starlight::selection::{
        ColourMarginalisation, CompletenessEntry, FaintTailModel, SelectionArtifact,
        SelectionReferenceDataset, SelectionReferenceFile,
    };
    use crate::starlight::uv::CalibrationStatus;
    use crate::starlight::xp::XpProduct;
    use std::fs;
    use tempfile::TempDir;

    fn sample_source(source_id: u64) -> GaiaSourceEntry {
        GaiaSourceEntry {
            source_id,
            icrs: fixture_icrs_from_source_id(source_id),
            phot_g_mean_mag: Some(12.0),
            phot_bp_mean_mag: Some(12.5),
            phot_rp_mean_mag: Some(11.5),
            bp_rp: Some(1.0),
            duplicated_source: false,
            in_qso_candidates: false,
            in_galaxy_candidates: false,
            predictors: None,
        }
    }

    fn sample_xp(flux: f64) -> XpProduct {
        XpProduct {
            source_id: "1".to_string(),
            wavelengths_nm: vec![336.0, 650.0],
            flux_w_m2_nm: vec![flux, flux],
            flux_error_w_m2_nm: Some(vec![flux * 0.01, flux * 0.01]),
        }
    }

    fn load_selection_fixture(healpix: u32) -> (TempDir, SelectionCorrection) {
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
            table_spatial_nside: None,
            completeness_table: vec![CompletenessEntry {
                healpix,
                magnitude_bin: 0,
                colour_bin: 0,
                completeness: 0.5,
            }],
            m10_map: Vec::new(),
            colour_marginalisation: ColourMarginalisation::MarginaliseUniform,
            faint_tail: FaintTailModel {
                enabled: false,
                magnitude_limit_g: 20.0,
                residual_fraction_per_pixel: 0.0,
                systematic_fraction: 0.0,
            },
            training_command: "fixture-generated, not trained".to_string(),
            software_version: "nsb-data-tools-test-fixture".to_string(),
        };
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("selection.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        fs::write(&path, &bytes).unwrap();
        let correction =
            SelectionCorrection::load(&path, &checksum_io::sha256_bytes(&bytes)).unwrap();
        (temporary, correction)
    }

    #[test]
    fn stage_d_applies_selection_weight_on_measured_band() {
        let source = sample_source(7);
        let healpix =
            healpix::icrs_equatorial_nested_pixel(source.icrs.ra_deg, source.icrs.dec_deg, 1)
                .unwrap();
        let (_tmp, selection) = load_selection_fixture(healpix);
        let xp = sample_xp(1e-12);
        let outcome = evaluate_source_for_diagnostic(
            &source,
            Some(&xp),
            AblationStage::D,
            128,
            StarlightProductBand::Measured336To650,
            None,
            None,
            Some(&selection),
        );
        assert!(outcome.admitted);
        assert!(outcome.selection_healpix.is_some());
        assert!(outcome.selection_weight.is_finite() && outcome.selection_weight > 0.0);
        assert!(outcome.weighted_flux_300_650_ph_m2_s.is_finite());
    }

    #[test]
    fn stage_d_selection_missing_g_magnitude_excludes() {
        let source = GaiaSourceEntry {
            phot_g_mean_mag: None,
            ..sample_source(8)
        };
        let healpix =
            healpix::icrs_equatorial_nested_pixel(source.icrs.ra_deg, source.icrs.dec_deg, 1)
                .unwrap();
        let (_tmp, selection) = load_selection_fixture(healpix);
        let xp = sample_xp(1e-12);
        let outcome = evaluate_source_for_diagnostic(
            &source,
            Some(&xp),
            AblationStage::D,
            128,
            StarlightProductBand::Measured336To650,
            None,
            None,
            Some(&selection),
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("selection_missing_g_magnitude")
        );
    }
}
