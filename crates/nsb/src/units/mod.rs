//! NSB quantity facade and domain-specific unit conventions.
//!
//! Code inside NSB should import physical quantities through this module rather
//! than depending on `qtty` paths directly. The upstream crate remains
//! available as [`qtty`] for callers that need units outside NSB's common
//! surface.

pub use ::qtty;
pub use ::qtty::{
    angular, area, dimensionless, energy, length, photometry, power, radiometry, solid_angle, unit,
    Per, Quantity, Second, Unit,
};

mod calibration;
mod conventions;

pub(crate) use calibration::{
    s10_for_spectral_photon_radiance, S10_TO_W_M2_SR_NM, S10_TO_W_M2_SR_UM,
};
pub(crate) use conventions::{
    MagnitudesPerAirmass, Nanolamberts, PixelIntegratedPhotonFlux, ScaleFactors,
    SkyCalcSpectralPhotonRadiance, SolarSpectralIrradiance, SolarSpectralIrradianceUnit,
    WattPerSquareMeterSteradianMicrometer,
};
#[cfg(test)]
pub(crate) use conventions::WattsPerSquareMeterSteradianMicrometer;
pub use conventions::{SolarFluxUnit, SolarFluxUnits};
