//! Versioned machine-readable scientific contracts shared with migration tools.
//!
//! Rust remains the sole production implementation of Gaia XP photon-flux
//! integration. The committed JSON is generated from the Rust constants and is
//! consumed by temporary GaiaXPy reference scripts so they cannot independently
//! redefine band edges, grids, column names, model identifiers or tolerances.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::gaia_xp::{
    BAND_MAX_NM, BAND_MIN_NM, NORMALIZED_FLUX_COLUMN, NORMALIZED_FLUX_ERROR_COLUMN,
    NORMALIZED_WAVELENGTH_COLUMN, PHOTOMETRY_MODEL as SAMPLED_PHOTOMETRY_MODEL, PHOTON_FLUX_COLUMN,
    XP_SAMPLED_BAND_END_INDEX, XP_SAMPLED_BAND_START_INDEX, XP_SAMPLED_GRID_END_NM,
    XP_SAMPLED_GRID_LEN, XP_SAMPLED_GRID_START_NM, XP_SAMPLED_GRID_STEP_NM,
};
use crate::gaia_xp_continuous::PHOTOMETRY_MODEL as CONTINUOUS_PHOTOMETRY_MODEL;

/// Supported schema version for the Gaia XP photon-integration contract.
pub const GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION: u32 = 1;
/// Stable identifier for the Gaia XP photon-integration contract.
pub const GAIA_XP_PHOTON_CONTRACT_ID: &str = "gaia_dr3_xp_photon_integration_v1";
/// Embedded generated contract consumed by Rust tests and migration scripts.
pub const GAIA_XP_PHOTON_CONTRACT_JSON: &str =
    include_str!("../contracts/gaia_xp_photon_integration_v1.json");

/// Inclusive wavelength band used by Gaia XP sampled and continuous products.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandContract {
    /// Inclusive lower wavelength in nanometres.
    pub min_nm: f64,
    /// Inclusive upper wavelength in nanometres.
    pub max_nm: f64,
    /// Required boundary selection policy.
    pub boundary_policy: String,
}

/// Official implicit Gaia XP sampled grid and in-band indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampledGridContract {
    /// First wavelength in nanometres.
    pub start_nm: f64,
    /// Last wavelength in nanometres.
    pub end_nm: f64,
    /// Uniform wavelength step in nanometres.
    pub step_nm: f64,
    /// Number of samples on the complete grid.
    pub length: usize,
    /// Inclusive first in-band index.
    pub band_start_index: usize,
    /// Inclusive last in-band index.
    pub band_end_index: usize,
}

/// Integration ownership and numerical-policy identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationContract {
    /// Authoritative production implementation.
    pub owner: String,
    /// Signed numerical integration rule.
    pub rule: String,
    /// Photon-energy conversion model identifier.
    pub photon_energy_model: String,
    /// Policy for finite negative samples.
    pub negative_finite_samples: String,
    /// Policy for NaN and infinite samples.
    pub non_finite_samples: String,
    /// Statistical uncertainty propagation model.
    pub uncertainty: String,
}

/// Stable model and column identifiers exchanged between stages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractIdentifiers {
    /// Gaia XP sampled model identifier.
    pub sampled_photometry_model: String,
    /// Gaia XP continuous reconstructed model identifier.
    pub continuous_photometry_model: String,
    /// Integrated photon-flux column.
    pub photon_flux_column: String,
    /// Normalized wavelength column.
    pub wavelength_column: String,
    /// Normalized flux column.
    pub flux_column: String,
    /// Normalized flux-error column.
    pub flux_error_column: String,
}

/// Frozen comparison tolerances for migration-oracle evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParityTolerances {
    /// Relative tolerance for per-sample flux parity.
    pub spectral_flux_relative: f64,
    /// Relative tolerance for integrated flux parity.
    pub integrated_flux_relative: f64,
    /// Relative tolerance for integrated uncertainty parity.
    pub integrated_uncertainty_relative: f64,
    /// Absolute denominator floor for relative comparisons.
    pub absolute_floor: f64,
}

/// Complete versioned Gaia XP photon-integration contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaiaXpPhotonIntegrationContract {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable contract identifier.
    pub contract_id: String,
    /// Inclusive integration band.
    pub band: BandContract,
    /// Official sampled grid.
    pub sampled_grid: SampledGridContract,
    /// Numerical policy and production owner.
    pub integration: IntegrationContract,
    /// Stable identifiers.
    pub identifiers: ContractIdentifiers,
    /// Frozen migration parity tolerances.
    pub parity_tolerances: ParityTolerances,
}

impl GaiaXpPhotonIntegrationContract {
    /// Validate schema, numerical consistency and fail-closed policy identifiers.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION {
            bail!(
                "unsupported Gaia XP scientific contract schema {}; expected {}",
                self.schema_version,
                GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION
            );
        }
        if self.contract_id != GAIA_XP_PHOTON_CONTRACT_ID {
            bail!(
                "unsupported Gaia XP scientific contract id {:?}",
                self.contract_id
            );
        }
        if !self.band.min_nm.is_finite()
            || !self.band.max_nm.is_finite()
            || self.band.min_nm >= self.band.max_nm
            || self.band.boundary_policy != "inclusive_exact_samples"
        {
            bail!("invalid Gaia XP integration-band contract");
        }
        let grid = &self.sampled_grid;
        if !grid.start_nm.is_finite()
            || !grid.end_nm.is_finite()
            || !grid.step_nm.is_finite()
            || grid.step_nm <= 0.0
            || grid.length < 2
            || grid.band_start_index >= grid.band_end_index
            || grid.band_end_index >= grid.length
        {
            bail!("invalid Gaia XP sampled-grid contract");
        }
        let derived_end = grid.start_nm + grid.step_nm * (grid.length - 1) as f64;
        if (derived_end - grid.end_nm).abs() > 1.0e-12 {
            bail!("Gaia XP sampled-grid end is inconsistent with start/step/length");
        }
        let band_start = grid.start_nm + grid.step_nm * grid.band_start_index as f64;
        let band_end = grid.start_nm + grid.step_nm * grid.band_end_index as f64;
        if (band_start - self.band.min_nm).abs() > 1.0e-12
            || (band_end - self.band.max_nm).abs() > 1.0e-12
        {
            bail!("Gaia XP band indices are inconsistent with the wavelength band");
        }
        if self.integration.owner != "nsb-data-tools::gaia_xp::integrate_photon_flux"
            || self.integration.rule != "trapezoidal_signed"
            || self.integration.photon_energy_model != "planck_times_c_over_wavelength"
            || self.integration.negative_finite_samples != "retain"
            || self.integration.non_finite_samples != "reject"
            || self.integration.uncertainty
                != "independent_sample_errors_weighted_by_trapezoid_coefficients"
        {
            bail!("unsupported Gaia XP integration policy");
        }
        for (name, tolerance) in [
            (
                "spectral_flux_relative",
                self.parity_tolerances.spectral_flux_relative,
            ),
            (
                "integrated_flux_relative",
                self.parity_tolerances.integrated_flux_relative,
            ),
            (
                "integrated_uncertainty_relative",
                self.parity_tolerances.integrated_uncertainty_relative,
            ),
            ("absolute_floor", self.parity_tolerances.absolute_floor),
        ] {
            if !tolerance.is_finite() || tolerance <= 0.0 {
                bail!("invalid Gaia XP parity tolerance {name}");
            }
        }
        Ok(())
    }
}

fn same_contract_float(left: f64, right: f64) -> bool {
    if left == right {
        return true;
    }
    if !left.is_finite() || !right.is_finite() {
        return false;
    }
    let scale = left.abs().max(right.abs());
    scale > 0.0 && (left - right).abs() <= 4.0 * f64::EPSILON * scale
}

/// Compare two contracts while allowing only serialization-scale floating-point rounding.
///
/// Schema versions, identifiers, policies, indices and lengths must match exactly.
/// Floating-point fields may differ by at most four relative machine epsilons,
/// accommodating decimal JSON parsing without accepting scientifically meaningful drift.
pub fn gaia_xp_photon_contracts_match(
    left: &GaiaXpPhotonIntegrationContract,
    right: &GaiaXpPhotonIntegrationContract,
) -> bool {
    left.schema_version == right.schema_version
        && left.contract_id == right.contract_id
        && same_contract_float(left.band.min_nm, right.band.min_nm)
        && same_contract_float(left.band.max_nm, right.band.max_nm)
        && left.band.boundary_policy == right.band.boundary_policy
        && same_contract_float(left.sampled_grid.start_nm, right.sampled_grid.start_nm)
        && same_contract_float(left.sampled_grid.end_nm, right.sampled_grid.end_nm)
        && same_contract_float(left.sampled_grid.step_nm, right.sampled_grid.step_nm)
        && left.sampled_grid.length == right.sampled_grid.length
        && left.sampled_grid.band_start_index == right.sampled_grid.band_start_index
        && left.sampled_grid.band_end_index == right.sampled_grid.band_end_index
        && left.integration == right.integration
        && left.identifiers == right.identifiers
        && same_contract_float(
            left.parity_tolerances.spectral_flux_relative,
            right.parity_tolerances.spectral_flux_relative,
        )
        && same_contract_float(
            left.parity_tolerances.integrated_flux_relative,
            right.parity_tolerances.integrated_flux_relative,
        )
        && same_contract_float(
            left.parity_tolerances.integrated_uncertainty_relative,
            right.parity_tolerances.integrated_uncertainty_relative,
        )
        && same_contract_float(
            left.parity_tolerances.absolute_floor,
            right.parity_tolerances.absolute_floor,
        )
}

/// Parse and strictly validate a versioned Gaia XP scientific contract.
pub fn parse_gaia_xp_photon_contract(raw: &str) -> Result<GaiaXpPhotonIntegrationContract> {
    let contract: GaiaXpPhotonIntegrationContract =
        serde_json::from_str(raw).context("invalid Gaia XP scientific contract JSON")?;
    contract.validate()?;
    Ok(contract)
}

/// Return the embedded generated contract after strict validation.
pub fn gaia_xp_photon_contract() -> &'static GaiaXpPhotonIntegrationContract {
    static CONTRACT: OnceLock<GaiaXpPhotonIntegrationContract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        parse_gaia_xp_photon_contract(GAIA_XP_PHOTON_CONTRACT_JSON)
            .expect("embedded Gaia XP scientific contract must be valid")
    })
}

/// Build the authoritative contract from production Rust constants.
pub fn authoritative_gaia_xp_photon_contract() -> GaiaXpPhotonIntegrationContract {
    GaiaXpPhotonIntegrationContract {
        schema_version: GAIA_XP_PHOTON_CONTRACT_SCHEMA_VERSION,
        contract_id: GAIA_XP_PHOTON_CONTRACT_ID.to_string(),
        band: BandContract {
            min_nm: BAND_MIN_NM,
            max_nm: BAND_MAX_NM,
            boundary_policy: "inclusive_exact_samples".to_string(),
        },
        sampled_grid: SampledGridContract {
            start_nm: XP_SAMPLED_GRID_START_NM,
            end_nm: XP_SAMPLED_GRID_END_NM,
            step_nm: XP_SAMPLED_GRID_STEP_NM,
            length: XP_SAMPLED_GRID_LEN,
            band_start_index: XP_SAMPLED_BAND_START_INDEX,
            band_end_index: XP_SAMPLED_BAND_END_INDEX,
        },
        integration: IntegrationContract {
            owner: "nsb-data-tools::gaia_xp::integrate_photon_flux".to_string(),
            rule: "trapezoidal_signed".to_string(),
            photon_energy_model: "planck_times_c_over_wavelength".to_string(),
            negative_finite_samples: "retain".to_string(),
            non_finite_samples: "reject".to_string(),
            uncertainty: "independent_sample_errors_weighted_by_trapezoid_coefficients".to_string(),
        },
        identifiers: ContractIdentifiers {
            sampled_photometry_model: SAMPLED_PHOTOMETRY_MODEL.to_string(),
            continuous_photometry_model: CONTINUOUS_PHOTOMETRY_MODEL.to_string(),
            photon_flux_column: PHOTON_FLUX_COLUMN.to_string(),
            wavelength_column: NORMALIZED_WAVELENGTH_COLUMN.to_string(),
            flux_column: NORMALIZED_FLUX_COLUMN.to_string(),
            flux_error_column: NORMALIZED_FLUX_ERROR_COLUMN.to_string(),
        },
        parity_tolerances: ParityTolerances {
            spectral_flux_relative: 1.0e-8,
            integrated_flux_relative: 1.0e-8,
            integrated_uncertainty_relative: 1.0e-6,
            absolute_floor: 1.0e-30,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_contract_matches_production_rust_authority() {
        assert!(gaia_xp_photon_contracts_match(
            gaia_xp_photon_contract(),
            &authoritative_gaia_xp_photon_contract(),
        ));
    }

    #[test]
    fn meaningful_tolerance_drift_is_rejected() {
        let generated = gaia_xp_photon_contract();
        let mut drifted = authoritative_gaia_xp_photon_contract();
        drifted.parity_tolerances.absolute_floor *= 2.0;
        assert!(!gaia_xp_photon_contracts_match(generated, &drifted));
    }

    #[test]
    fn corrupted_contract_version_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_str(GAIA_XP_PHOTON_CONTRACT_JSON).unwrap();
        value["schema_version"] = serde_json::json!(999);
        let error = parse_gaia_xp_photon_contract(&value.to_string()).expect_err("version");
        assert!(error.to_string().contains("unsupported"));
    }

    #[test]
    fn corrupted_grid_fails_closed() {
        let mut value: serde_json::Value =
            serde_json::from_str(GAIA_XP_PHOTON_CONTRACT_JSON).unwrap();
        value["sampled_grid"]["band_end_index"] = serde_json::json!(156);
        let error = parse_gaia_xp_photon_contract(&value.to_string()).expect_err("grid drift");
        assert!(error.to_string().contains("inconsistent"));
    }
}
