// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
pub use crate::units::SolarFluxUnits;

pub(crate) fn is_valid_solar_flux(flux: SolarFluxUnits) -> bool {
    flux.is_finite() && flux > SolarFluxUnits::new(0.0)
}

/// PALACE v1.0 reference solar radio flux.
pub(crate) const DEFAULT_SOLAR_RADIO_FLUX: SolarFluxUnits = SolarFluxUnits::new(100.0);
