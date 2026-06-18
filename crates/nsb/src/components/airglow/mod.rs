//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB exposes a single site-bound empirical continuum model:
//!
//! ```ignore
//! let airglow = Airglow::standard_clear_sky(location)?;
//! let out = airglow.compute(time, target)?;
//! ```
//!
//! `standard_clear_sky` uses the bundled SkyCalc-derived empirical continuum
//! template. It is suitable for generic clear-sky planning, but it is not a
//! site-calibrated high-precision prediction for every observing location. For
//! site-specific calibration, load or build an [`AirglowContinuum`] and pass it
//! to [`Airglow::with_continuum`].

pub(crate) mod calibration;
mod continuum;
mod geometry;
mod model;
mod output;
mod temporal;
mod units;

pub use calibration::AirglowContinuum;
pub(crate) use calibration::load_builtin_standard;
pub use model::Airglow;
pub use output::AirglowOutputs;
pub use units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};

#[cfg(test)]
mod tests;
