//! NSB quantity facade and domain-specific unit conventions.
//!
//! Code inside NSB should import physical quantities through this module rather
//! than depending on `qtty` paths directly. The upstream crate remains
//! available as [`qtty`] for callers that need units outside NSB's common
//! surface.

pub use ::qtty;
pub use ::qtty::{angular, area, dimensionless, energy, length};
pub use ::qtty::{photometry, power, radiometry, solid_angle, unit};
pub use ::qtty::{Per, Quantity, Second, Unit};

mod calibration;
mod conventions;

pub(crate) use calibration::S10_TO_W_M2_SR_UM;
pub(crate) use calibration::{s10_for_spectral_photon_radiance, S10_TO_W_M2_SR_NM};
pub(crate) use conventions::SolarSpectralIrradianceUnit;
pub(crate) use conventions::WattPerSquareMeterSteradianMicrometer;
pub(crate) use conventions::{MagnitudesPerAirmass, Nanolamberts};
pub(crate) use conventions::{PixelIntegratedPhotonFlux, ScaleFactors};
pub(crate) use conventions::{SkyCalcSpectralPhotonRadiance, SolarSpectralIrradiance};
pub use conventions::{SolarFluxUnit, SolarFluxUnits};
