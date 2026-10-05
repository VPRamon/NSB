//! Spectral-type/template reconstruction for sources without usable Gaia XP.

use super::artifact::CorrelatedUncertainty;
use super::builder::SpectralEstimate;
use super::catalogue::{Hipparcos2Record, XhipRecord};
use super::photometry::{
    reconstruct_template_band_flux, PhotometricBandCalibration, PhotometricBandResponse,
    SpectralTemplate,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const SPECTRAL_RECONSTRUCTION_MODEL_ID_V1: &str = "xhip-sptype-ck04-v2-hp-bessell2000-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateAssignment {
    pub temperature_code: u16,
    pub luminosity_class_code: u8,
    pub template_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedSpectralAssignment {
    pub temperature_code: u16,
    pub luminosity_class_code: u8,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralReconstructionModel {
    pub model_id: String,
    pub builder_software_commit: String,
    pub assignments: Vec<TemplateAssignment>,
    pub templates: Vec<SpectralTemplate>,
    pub hp_response: PhotometricBandResponse,
    pub hp_calibration: PhotometricBandCalibration,
    pub template_mismatch_fraction: f64,
    pub spectral_type_mapping_fraction: f64,
    pub hp_zero_point_fraction: f64,
    pub uncertainty_calibration_status: String,
    pub uncertainty_calibration_sha256: String,
    pub spectral_mapping_status: String,
    pub spectral_mapping_sha256: String,
    pub unsupported_assignments: Vec<UnsupportedSpectralAssignment>,
    pub template_library_citation: String,
    pub spectral_type_mapping_citation: String,
}

impl SpectralReconstructionModel {
    pub fn validate(&self) -> Result<()> {
        self.validate_for_product_band(super::artifact::BRIGHT_STAR_PRODUCT_BAND_ID)
    }

    /// Validate the spectral coverage required by the requested output product.
    ///
    /// The historical measured-only model is intentionally valid without a
    /// sample at or below 300 nm. Combined products additionally require the
    /// complete 300--336 nm CK04 interval and therefore fail closed when it is
    /// unavailable.
    pub fn validate_for_product_band(&self, product_band: &str) -> Result<()> {
        let combined = match product_band {
            super::artifact::BRIGHT_STAR_PRODUCT_BAND_ID => false,
            super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID => true,
            _ => bail!("unsupported bright-star product band {product_band}"),
        };
        if self.model_id != SPECTRAL_RECONSTRUCTION_MODEL_ID_V1
            || self.hp_response.band_id != "Hipparcos/Hipparcos.Hp_bes"
            || !valid_commit_identity(&self.builder_software_commit)
            || self.hp_calibration.band_id != self.hp_response.band_id
            || self.template_library_citation.trim().is_empty()
            || self.spectral_type_mapping_citation.trim().is_empty()
            || self.uncertainty_calibration_status != "provisional-uncalibrated"
            || self.spectral_mapping_status != "experimental-provisional"
            || !is_sha256(&self.uncertainty_calibration_sha256)
            || !is_sha256(&self.spectral_mapping_sha256)
        {
            bail!("unknown or incomplete bright-star spectral reconstruction model");
        }
        for value in [
            self.template_mismatch_fraction,
            self.spectral_type_mapping_fraction,
            self.hp_zero_point_fraction,
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!(
                    "spectral reconstruction uncertainty fractions must be finite and non-negative"
                );
            }
        }
        let mut templates = BTreeMap::new();
        for template in &self.templates {
            if templates
                .insert(template.template_id.as_str(), template)
                .is_some()
            {
                bail!("duplicate spectral template id {}", template.template_id);
            }
            // Every product needs the Hp response and measured interval.
            reconstruct_template_band_flux(
                template,
                &self.hp_response,
                &self.hp_calibration,
                0.0,
                336.0e-9,
                650.0e-9,
            )?;
            if combined {
                reconstruct_template_band_flux(
                    template,
                    &self.hp_response,
                    &self.hp_calibration,
                    0.0,
                    300.0e-9,
                    336.0e-9,
                )?;
            }
        }
        let mut keys = BTreeSet::new();
        for assignment in &self.assignments {
            if !keys.insert((
                assignment.temperature_code,
                assignment.luminosity_class_code,
            )) {
                bail!("duplicate spectral-type template assignment");
            }
            if !templates.contains_key(assignment.template_id.as_str()) {
                bail!("template assignment references an unknown template");
            }
        }
        let mut unsupported = BTreeSet::new();
        for assignment in &self.unsupported_assignments {
            if assignment.reason != "unsupported_spectral_mapping"
                || !unsupported.insert((
                    assignment.temperature_code,
                    assignment.luminosity_class_code,
                ))
                || keys.contains(&(
                    assignment.temperature_code,
                    assignment.luminosity_class_code,
                ))
            {
                bail!("invalid unsupported spectral assignment contract");
            }
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_commit_identity(value: &str) -> bool {
    super::artifact::is_full_git_sha(value)
}

pub fn load_spectral_reconstruction_model(
    path: &Path,
    expected_sha256: &str,
    product_band: &str,
) -> Result<SpectralReconstructionModel> {
    let actual = crate::platform::checksum_io::sha256_file(path)?;
    if actual != expected_sha256 {
        bail!("spectral reconstruction model checksum mismatch");
    }
    let model: SpectralReconstructionModel = serde_json::from_slice(&fs::read(path)?)?;
    model.validate_for_product_band(product_band)?;
    Ok(model)
}

/// Reconstruct measured 336--650 nm and justified 300--336 nm fluxes from the
/// same Hp-scaled CK04 template. Missing/unsupported spectral evidence is
/// returned as an absent estimate and is therefore excluded by the artifact
/// builder rather than fabricated.
///
/// The 300--336 nm term is the template integral over that interval after the
/// template is absolutely scaled to the Hipparcos Hp magnitude through the
/// pinned Bessell (2000) response and CALSPEC Vega zero point. It is never a
/// relabelled copy of the 336--650 nm flux.
pub fn reconstruct_spectral_estimates(
    hipparcos: &[Hipparcos2Record],
    xhip_by_hip: &BTreeMap<u32, XhipRecord>,
    model: &SpectralReconstructionModel,
    product_band: &str,
) -> Result<BTreeMap<u32, SpectralEstimate>> {
    model.validate_for_product_band(product_band)?;
    let combined = product_band == super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID;
    let templates: BTreeMap<_, _> = model
        .templates
        .iter()
        .map(|template| (template.template_id.as_str(), template))
        .collect();
    let assignments: BTreeMap<_, _> = model
        .assignments
        .iter()
        .map(|assignment| {
            (
                (
                    assignment.temperature_code,
                    assignment.luminosity_class_code,
                ),
                assignment.template_id.as_str(),
            )
        })
        .collect();
    let mut estimates = BTreeMap::new();
    for hip in hipparcos {
        let Some(xhip) = xhip_by_hip.get(&hip.astrometry.hip) else {
            continue;
        };
        let (Some(temperature_code), Some(luminosity_class_code)) =
            (xhip.temperature_code, xhip.luminosity_class_code)
        else {
            continue;
        };
        let Some(template_id) = assignments.get(&(temperature_code, luminosity_class_code)) else {
            continue;
        };
        let template = templates
            .get(template_id)
            .with_context(|| format!("missing assigned template {template_id}"))?;
        let flux_336_650 = reconstruct_template_band_flux(
            template,
            &model.hp_response,
            &model.hp_calibration,
            hip.hp_mag,
            336.0e-9,
            650.0e-9,
        )?;
        let flux_300_336 = if combined {
            reconstruct_template_band_flux(
                template,
                &model.hp_response,
                &model.hp_calibration,
                hip.hp_mag,
                300.0e-9,
                336.0e-9,
            )?
        } else {
            0.0
        };
        let mag_fraction = 0.4 * std::f64::consts::LN_10 * hip.hp_mag_uncertainty;
        let independent_fraction = model
            .template_mismatch_fraction
            .hypot(model.spectral_type_mapping_fraction);
        let selected_flux = if combined {
            flux_300_336 + flux_336_650
        } else {
            flux_336_650
        };
        let estimate = SpectralEstimate {
            hip: hip.astrometry.hip,
            flux_300_336_ph_m2_s: flux_300_336,
            flux_336_650_ph_m2_s: flux_336_650,
            statistical_uncertainty_300_336_ph_m2_s: flux_300_336 * mag_fraction,
            statistical_uncertainty_336_650_ph_m2_s: flux_336_650 * mag_fraction,
            systematic_independent_uncertainty_300_336_ph_m2_s: flux_300_336 * independent_fraction,
            systematic_independent_uncertainty_336_650_ph_m2_s: flux_336_650 * independent_fraction,
            // Hp zero-point is a shared absolute scale; one correlated term on
            // the full 300--650 integral preserves linear addition in-group.
            systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                correlation_group_id: "hipparcos-hp-zero-point-bessell2000".into(),
                uncertainty_ph_m2_s: selected_flux * model.hp_zero_point_fraction,
            }],
            route: format!("{}:{template_id}", model.model_id),
            uv_completion_model_id: if combined {
                UV_COMPLETION_MODEL_ID_V1.into()
            } else {
                String::new()
            },
        };
        if estimates.insert(hip.astrometry.hip, estimate).is_some() {
            bail!("duplicate Hipparcos input for spectral reconstruction");
        }
    }
    Ok(estimates)
}

/// Versioned identifier for the CK04-template 300--336 nm completion route.
pub const UV_COMPLETION_MODEL_ID_V1: &str = "ck04-hp-scaled-uv-300-336-v1";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::bright_stars::HipparcosAstrometry;

    fn template() -> SpectralTemplate {
        SpectralTemplate {
            template_id: "ck04-t6000-g45".into(),
            wavelengths_m: vec![300e-9, 336e-9, 500e-9, 650e-9, 900e-9],
            f_lambda_si: vec![1.0; 5],
            provenance: "fixture".into(),
        }
    }

    #[test]
    fn reconstruction_propagates_photometric_and_correlated_uncertainty() {
        let model = SpectralReconstructionModel {
            model_id: SPECTRAL_RECONSTRUCTION_MODEL_ID_V1.into(),
            builder_software_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            assignments: vec![TemplateAssignment {
                temperature_code: 50,
                luminosity_class_code: 5,
                template_id: "ck04-t6000-g45".into(),
            }],
            templates: vec![template()],
            hp_response: PhotometricBandResponse {
                band_id: "Hipparcos/Hipparcos.Hp_bes".into(),
                wavelengths_m: vec![350e-9, 500e-9, 850e-9],
                throughput: vec![0.0, 1.0, 0.0],
                detector_convention: "photon_counting".into(),
                citation: "Bessell 2000".into(),
                sha256: "a".repeat(64),
            },
            hp_calibration: PhotometricBandCalibration {
                system: "Hipparcos Vega".into(),
                band_id: "Hipparcos/Hipparcos.Hp_bes".into(),
                zero_point_convention: "Vega/Pogson".into(),
                reference_spectrum: "CALSPEC alpha_lyr".into(),
                zero_point_photon_flux_ph_m2_s: 1.0e10,
                citation: "Bessell 2000; CALSPEC".into(),
            },
            template_mismatch_fraction: 0.1,
            spectral_type_mapping_fraction: 0.2,
            hp_zero_point_fraction: 0.01,
            uncertainty_calibration_status: "provisional-uncalibrated".into(),
            uncertainty_calibration_sha256: "b".repeat(64),
            spectral_mapping_status: "experimental-provisional".into(),
            spectral_mapping_sha256: "c".repeat(64),
            unsupported_assignments: vec![UnsupportedSpectralAssignment {
                temperature_code: 50,
                luminosity_class_code: 6,
                reason: "unsupported_spectral_mapping".into(),
            }],
            template_library_citation: "Castelli and Kurucz 2004".into(),
            spectral_type_mapping_citation: "fixture mapping".into(),
        };
        let hip = Hipparcos2Record {
            astrometry: HipparcosAstrometry {
                hip: 1,
                ra_deg_j1991_25: 0.0,
                dec_deg_j1991_25: 0.0,
                pm_ra_cosdec_mas_per_year: 0.0,
                pm_dec_mas_per_year: 0.0,
                parallax_mas: 1.0,
                position_uncertainty_mas: 1.0,
                proper_motion_uncertainty_mas_per_year: 1.0,
            },
            hp_mag: 0.0,
            hp_mag_uncertainty: 0.01,
            solution_type: 5,
            components: 1,
        };
        let xhip = BTreeMap::from([(
            1,
            XhipRecord {
                hip: 1,
                spectral_type: Some("G0V".into()),
                temperature_code: Some(50),
                luminosity_class_code: Some(5),
                radial_velocity_km_s: Some(0.0),
                radial_velocity_uncertainty_km_s: Some(1.0),
            },
        )]);
        let estimate = reconstruct_spectral_estimates(
            std::slice::from_ref(&hip),
            &xhip,
            &model,
            super::super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
        )
        .unwrap()
        .remove(&1)
        .unwrap();
        assert!(
            (estimate.statistical_uncertainty_336_650_ph_m2_s / estimate.flux_336_650_ph_m2_s
                - 0.4 * std::f64::consts::LN_10 * 0.01)
                .abs()
                < 1e-15
        );
        assert!(
            (estimate.systematic_independent_uncertainty_336_650_ph_m2_s
                / estimate.flux_336_650_ph_m2_s
                - 0.1_f64.hypot(0.2))
            .abs()
                < 1e-15
        );
        assert!(estimate.flux_300_336_ph_m2_s > 0.0);
        assert!(estimate.flux_300_336_ph_m2_s != estimate.flux_336_650_ph_m2_s);
        assert_eq!(estimate.uv_completion_model_id, UV_COMPLETION_MODEL_ID_V1);
        assert_eq!(estimate.systematic_catalogue_correlated.len(), 1);
        let expected_combined_zero_point =
            estimate.flux_300_650_ph_m2_s() * model.hp_zero_point_fraction;
        assert_eq!(
            estimate.systematic_catalogue_correlated[0].uncertainty_ph_m2_s,
            expected_combined_zero_point
        );

        let measured = reconstruct_spectral_estimates(
            std::slice::from_ref(&hip),
            &xhip,
            &model,
            super::super::artifact::BRIGHT_STAR_PRODUCT_BAND_ID,
        )
        .unwrap()
        .remove(&1)
        .unwrap();
        assert_eq!(measured.flux_300_336_ph_m2_s, 0.0);
        assert!(measured.uv_completion_model_id.is_empty());
        assert_eq!(
            measured.systematic_catalogue_correlated[0].uncertainty_ph_m2_s,
            measured.flux_336_650_ph_m2_s * model.hp_zero_point_fraction
        );
        assert!(
            measured.systematic_catalogue_correlated[0].uncertainty_ph_m2_s
                < expected_combined_zero_point
        );

        let mut old_measured_only_model = model.clone();
        old_measured_only_model.templates[0].wavelengths_m[0] = 301.0e-9;
        old_measured_only_model
            .validate_for_product_band(super::super::artifact::BRIGHT_STAR_PRODUCT_BAND_ID)
            .unwrap();
        assert!(old_measured_only_model
            .validate_for_product_band(super::super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID)
            .is_err());

        let unsupported = BTreeMap::from([(
            1,
            XhipRecord {
                hip: 1,
                spectral_type: Some("G0VI".into()),
                temperature_code: Some(50),
                luminosity_class_code: Some(6),
                radial_velocity_km_s: None,
                radial_velocity_uncertainty_km_s: None,
            },
        )]);
        assert!(reconstruct_spectral_estimates(
            std::slice::from_ref(&hip),
            &unsupported,
            &model,
            super::super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
        )
        .unwrap()
        .is_empty());
        let missing = BTreeMap::from([(
            1,
            XhipRecord {
                hip: 1,
                spectral_type: None,
                temperature_code: Some(50),
                luminosity_class_code: None,
                radial_velocity_km_s: None,
                radial_velocity_uncertainty_km_s: None,
            },
        )]);
        assert!(reconstruct_spectral_estimates(
            std::slice::from_ref(&hip),
            &missing,
            &model,
            super::super::artifact::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
        )
        .unwrap()
        .is_empty());
    }
}
