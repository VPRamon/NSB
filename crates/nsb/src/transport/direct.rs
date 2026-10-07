// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Direct Beer–Lambert transmission configuration.
//!
//! ```text
//! I(λ) = I₀(λ) · exp(−τ(λ) · X(z))
//! ```
//!
//! Radiance application is only available through [`super::TransportModel`] so
//! [`super::RadianceOrigin`] checks cannot be bypassed.

use super::extinction::{optical_depth_breakdown, ExtinctionIngredients, OpticalDepthBreakdown};
use super::geometry::DirectPathGeometry;
use super::metadata::TransportModelMetadata;
use super::model::AirmassModel;
use crate::error::{NsbError, Result};
use crate::site::AtmosphericConditions;
use crate::units::angular::Radian;
use crate::units::dimensionless::Transmittances;
use siderust::atmosphere::{
    airmass, transmission as beer_lambert_transmission, KrisciunasSchaefer1991, PlaneParallel,
    Rozenberg1966, Young1994,
};
use siderust::qtty::Nanometers;

/// Direct-transmission configuration for top-of-atmosphere sources.
///
/// Not `Copy`: future aerosol tables or LUT handles must be able to land here
/// without a frozen `Copy` guarantee.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DirectTransmission {
    ingredients: ExtinctionIngredients,
    airmass: AirmassModel,
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

    /// Extinction ingredients.
    pub const fn ingredients(&self) -> ExtinctionIngredients {
        self.ingredients
    }

    /// Airmass model.
    pub const fn airmass(&self) -> AirmassModel {
        self.airmass
    }

    /// Model-only metadata for this configuration.
    pub fn metadata(&self) -> TransportModelMetadata {
        TransportModelMetadata::direct(self.ingredients, self.airmass)
    }

    /// Vertical optical-depth breakdown at `wavelength`.
    ///
    /// # Errors
    ///
    /// Returns [`NsbError::OutOfRange`] when `wavelength` is non-finite or not
    /// strictly positive.
    pub fn optical_depth(
        &self,
        wavelength: Nanometers,
        atmosphere: AtmosphericConditions,
    ) -> Result<OpticalDepthBreakdown> {
        validate_wavelength(wavelength)?;
        Ok(optical_depth_breakdown(
            wavelength,
            atmosphere,
            self.ingredients,
        ))
    }

    /// Slant-path transmission `T = exp(−τ X)`.
    ///
    /// # Errors
    ///
    /// Returns [`NsbError::OutOfRange`] for invalid wavelength. Geometry must
    /// already be validated via [`DirectPathGeometry::new`].
    pub fn transmission(
        &self,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<Transmittances> {
        let tau = self.optical_depth(wavelength, atmosphere)?.total;
        let path = self.airmass_value(geometry);
        Ok(beer_lambert_transmission(tau, path))
    }

    pub(crate) fn scale_factor(
        &self,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<f64> {
        Ok(self.transmission(wavelength, geometry, atmosphere)?.value())
    }

    fn airmass_value(&self, geometry: DirectPathGeometry) -> siderust::qtty::Airmasses {
        let zenith = geometry.zenith().to::<Radian>();
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

pub(crate) fn validate_wavelength(wavelength: Nanometers) -> Result<()> {
    let value = wavelength.value();
    if !value.is_finite() || value <= 0.0 {
        return Err(NsbError::OutOfRange(format!(
            "transport wavelength must be finite and > 0 nm, got {value}"
        )));
    }
    Ok(())
}
