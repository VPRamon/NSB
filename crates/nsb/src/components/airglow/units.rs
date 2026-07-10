pub use crate::units::SolarFluxUnits;

pub(crate) fn is_valid_solar_flux(flux: SolarFluxUnits) -> bool {
    flux.is_finite() && flux > SolarFluxUnits::new(0.0)
}

/// Solar radio flux for which the bundled continuum correction is neutral.
pub const DEFAULT_SOLAR_RADIO_FLUX: SolarFluxUnits =
    SolarFluxUnits::new((1.0 - 2.068e-1) / 6.139e-3);
