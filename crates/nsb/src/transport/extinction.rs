//! Extinction ingredient selection and optical-depth composition.
//!
//! Optical-depth kernels come from Siderust. This module only selects which
//! terms participate and sums them for Beer–Lambert transmission.

use crate::site::atmosphere::rayleigh_optical_depth_local_pressure;
use crate::site::AtmosphericConditions;
use siderust::atmosphere::{mie_optical_depth, ozone};
use siderust::qtty::{Nanometers, OpticalDepths};

/// Molecular-absorption treatment for direct transmission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MolecularAbsorption {
    /// No molecular absorption term.
    #[default]
    None,

    /// Bundled Siderust ozone transmittance table converted to vertical
    /// optical depth `τ_O₃(λ) = −ln T_table(λ)`, then slanted with airmass.
    ///
    /// The table encodes a fixed ozone column (see Siderust ozone provenance).
    /// Selecting this term does **not** imply site-calibrated ozone.
    OzoneBundledTable,
}

impl MolecularAbsorption {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::OzoneBundledTable => "ozone-bundled-table",
        }
    }
}

/// Which extinction contributors participate in a direct path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtinctionIngredients {
    /// Include Bodhaine Rayleigh scattering optical depth.
    pub rayleigh: bool,
    /// Include Patat Mie / aerosol optical depth from site Mie parameters.
    pub mie: bool,
    /// Molecular absorption treatment.
    pub absorption: MolecularAbsorption,
}

impl ExtinctionIngredients {
    /// Rayleigh + Mie only (no molecular absorption).
    pub const RAYLEIGH_MIE: Self = Self {
        rayleigh: true,
        mie: true,
        absorption: MolecularAbsorption::None,
    };

    /// Rayleigh + Mie + bundled ozone table.
    pub const RAYLEIGH_MIE_OZONE: Self = Self {
        rayleigh: true,
        mie: true,
        absorption: MolecularAbsorption::OzoneBundledTable,
    };

    /// Construct an explicit ingredient set.
    pub const fn new(rayleigh: bool, mie: bool, absorption: MolecularAbsorption) -> Self {
        Self {
            rayleigh,
            mie,
            absorption,
        }
    }

    /// Stable machine-readable summary.
    pub fn as_str(self) -> String {
        let mut parts = Vec::new();
        if self.rayleigh {
            parts.push("rayleigh");
        }
        if self.mie {
            parts.push("mie");
        }
        match self.absorption {
            MolecularAbsorption::None => {}
            MolecularAbsorption::OzoneBundledTable => parts.push("ozone"),
        }
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join("+")
        }
    }
}

impl Default for ExtinctionIngredients {
    fn default() -> Self {
        Self::RAYLEIGH_MIE
    }
}

/// Per-wavelength optical-depth breakdown for diagnostics and tests.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct OpticalDepthBreakdown {
    /// Rayleigh contribution (zero when disabled).
    pub rayleigh: OpticalDepths,
    /// Mie contribution (zero when disabled).
    pub mie: OpticalDepths,
    /// Molecular absorption contribution (zero when disabled).
    pub absorption: OpticalDepths,
    /// Total vertical optical depth.
    pub total: OpticalDepths,
}

/// Compose vertical optical depth from selected ingredients and site conditions.
///
/// Uses the local-pressure Rayleigh helper so site-profile surface pressure is
/// not double-reduced by observer altitude (see
/// [`crate::site::AtmosphericConditions`] docs).
pub(crate) fn optical_depth_breakdown(
    wavelength: Nanometers,
    atmosphere: AtmosphericConditions,
    ingredients: ExtinctionIngredients,
) -> OpticalDepthBreakdown {
    let rayleigh = if ingredients.rayleigh {
        rayleigh_optical_depth_local_pressure(wavelength, atmosphere)
    } else {
        OpticalDepths::new(0.0)
    };
    let mie = if ingredients.mie {
        mie_optical_depth(&atmosphere.mie_params, wavelength)
    } else {
        OpticalDepths::new(0.0)
    };
    let absorption = match ingredients.absorption {
        MolecularAbsorption::None => OpticalDepths::new(0.0),
        MolecularAbsorption::OzoneBundledTable => ozone_vertical_optical_depth(wavelength),
    };
    let total = OpticalDepths::new(rayleigh.value() + mie.value() + absorption.value());
    OpticalDepthBreakdown {
        rayleigh,
        mie,
        absorption,
        total,
    }
}

fn ozone_vertical_optical_depth(wavelength: Nanometers) -> OpticalDepths {
    let transmittance = ozone::transmittance_at(wavelength).value();
    if !(transmittance.is_finite()) || transmittance <= 0.0 {
        // Fail closed to total absorption for a non-physical table sample.
        return OpticalDepths::new(f64::INFINITY);
    }
    if transmittance >= 1.0 {
        return OpticalDepths::new(0.0);
    }
    OpticalDepths::new((-transmittance.ln()).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::atmosphere::AtmosphereProfile;

    fn paranal() -> AtmosphericConditions {
        AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
    }

    #[test]
    fn zero_ingredients_means_zero_optical_depth() {
        let breakdown = optical_depth_breakdown(
            Nanometers::new(550.0),
            paranal(),
            ExtinctionIngredients::new(false, false, MolecularAbsorption::None),
        );
        assert_eq!(breakdown.total.value(), 0.0);
    }

    #[test]
    fn rayleigh_mie_is_positive_at_paranal() {
        let breakdown = optical_depth_breakdown(
            Nanometers::new(550.0),
            paranal(),
            ExtinctionIngredients::RAYLEIGH_MIE,
        );
        assert!(breakdown.rayleigh.value() > 0.0);
        assert!(breakdown.mie.value() > 0.0);
        assert!(breakdown.total.value() > breakdown.rayleigh.value());
    }

    #[test]
    fn ozone_increases_total_optical_depth_in_uv() {
        let without = optical_depth_breakdown(
            Nanometers::new(320.0),
            paranal(),
            ExtinctionIngredients::RAYLEIGH_MIE,
        );
        let with = optical_depth_breakdown(
            Nanometers::new(320.0),
            paranal(),
            ExtinctionIngredients::RAYLEIGH_MIE_OZONE,
        );
        assert!(with.absorption.value() > 0.0);
        assert!(with.total.value() > without.total.value());
    }
}
