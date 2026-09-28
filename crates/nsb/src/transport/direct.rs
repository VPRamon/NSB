//! Direct Beer–Lambert transmission of celestial spectral radiance.
//!
//! ```text
//! I(λ) = I₀(λ) · exp(−τ(λ) · X(z))
//! ```
//!
//! Optical depth `τ` is composed from selectable Rayleigh, Mie, and molecular
//! absorption ingredients using Siderust primitives and NSB site
//! [`crate::site::AtmosphericConditions`]. Airmass `X(z)` uses a selectable
//! Siderust airmass formula.

use qtty::angular::Radian;
use qtty::dimensionless::Transmittances;
use qtty::radiometry::WattsPerSquareMeterSteradianNanometer;
use siderust::atmosphere::{
    airmass, transmission as beer_lambert_transmission, KrisciunasSchaefer1991, PlaneParallel,
    Rozenberg1966, Young1994,
};
use siderust::qtty::Nanometers;

use super::extinction::{optical_depth_breakdown, ExtinctionIngredients, OpticalDepthBreakdown};
use super::geometry::DirectPathGeometry;
use super::metadata::{
    AbsorptionTreatment, ApproximationState, ExtinctionIngredientFlags, ScatteringIngredientFlags,
    TransportMetadata, TransportPathKind, UncertaintyReporting,
};
use super::model::AirmassModel;
use crate::site::AtmosphericConditions;

/// Direct-transmission atmospheric path for top-of-atmosphere sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DirectTransmission {
    /// Extinction ingredients included in `τ(λ)`.
    pub ingredients: ExtinctionIngredients,
    /// Airmass formula used for the slant path.
    pub airmass: AirmassModel,
}

impl DirectTransmission {
    /// Rayleigh + Mie extinction with Young (1994) airmass.
    pub const fn rayleigh_mie_young1994() -> Self {
        Self {
            ingredients: ExtinctionIngredients::RAYLEIGH_MIE,
            airmass: AirmassModel::Young1994,
        }
    }

    /// Rayleigh + Mie + bundled ozone with Young (1994) airmass.
    pub const fn rayleigh_mie_ozone_young1994() -> Self {
        Self {
            ingredients: ExtinctionIngredients::RAYLEIGH_MIE_OZONE,
            airmass: AirmassModel::Young1994,
        }
    }

    /// Construct an explicit direct-transmission configuration.
    pub const fn new(ingredients: ExtinctionIngredients, airmass: AirmassModel) -> Self {
        Self {
            ingredients,
            airmass,
        }
    }

    /// Vertical optical-depth breakdown at `wavelength`.
    pub fn optical_depth(
        &self,
        wavelength: Nanometers,
        atmosphere: AtmosphericConditions,
    ) -> OpticalDepthBreakdown {
        optical_depth_breakdown(wavelength, atmosphere, self.ingredients)
    }

    /// Slant-path transmission `T = exp(−τ X)`.
    pub fn transmission(
        &self,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Transmittances {
        let tau = self.optical_depth(wavelength, atmosphere).total;
        let path = self.airmass_value(geometry);
        beer_lambert_transmission(tau, path)
    }

    /// Apply direct transmission to spectral radiance.
    pub fn apply_spectral(
        &self,
        incident: WattsPerSquareMeterSteradianNanometer,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> WattsPerSquareMeterSteradianNanometer {
        let t = self.transmission(wavelength, geometry, atmosphere);
        WattsPerSquareMeterSteradianNanometer::new(incident.value() * t.value())
    }

    /// Scientific metadata for this direct path.
    pub fn metadata(&self, atmosphere_profile_id: &'static str) -> TransportMetadata {
        TransportMetadata {
            model_id: "direct-transmission",
            path_kind: TransportPathKind::Direct,
            atmosphere_profile_id,
            extinction: ExtinctionIngredientFlags {
                rayleigh: self.ingredients.rayleigh,
                mie: self.ingredients.mie,
                absorption: match self.ingredients.absorption {
                    super::extinction::MolecularAbsorption::None => AbsorptionTreatment::None,
                    super::extinction::MolecularAbsorption::OzoneBundledTable => {
                        AbsorptionTreatment::OzoneBundledTable
                    }
                },
            },
            scattering: ScatteringIngredientFlags {
                rayleigh_phase: false,
                mie_phase: false,
            },
            airmass_model_id: self.airmass.as_str(),
            approximation: ApproximationState::ClearSkySingleColumn,
            validated_domain: concat!(
                "clear-sky celestial direct path; zenith distances supported by the ",
                "selected Siderust airmass formula; not a site-calibrated extinction law"
            ),
            provenance: concat!(
                "NSB direct atmospheric transport composing Siderust Bodhaine Rayleigh, ",
                "Patat Mie, optional Siderust ozone transmittance table, and Siderust ",
                "Beer–Lambert transmission; NSB local-pressure Rayleigh helper avoids ",
                "double-counting site-profile surface pressure with altitude"
            ),
            uncertainty: UncertaintyReporting::Absent,
        }
    }

    fn airmass_value(&self, geometry: DirectPathGeometry) -> siderust::qtty::Airmasses {
        let zenith = geometry.zenith.to::<Radian>();
        match self.airmass {
            AirmassModel::Young1994 => airmass::<Young1994>(zenith),
            AirmassModel::KrisciunasSchaefer1991 => airmass::<KrisciunasSchaefer1991>(zenith),
            AirmassModel::PlaneParallel => airmass::<PlaneParallel>(zenith),
            AirmassModel::Rozenberg1966 => airmass::<Rozenberg1966>(zenith),
        }
    }
}

impl Default for DirectTransmission {
    fn default() -> Self {
        Self::rayleigh_mie_young1994()
    }
}
