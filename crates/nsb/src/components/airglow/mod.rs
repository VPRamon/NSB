//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB exposes a site-bound empirical continuum model:
//!
//! ```no_run
//! use nsb::{Airglow, Observer, SiteProfileId, Target};
//! use tempoch::{Time, UTC};
//!
//! fn evaluate(location: Observer, time: Time<UTC>, target: Target) -> nsb::Result<()> {
//!     let airglow = Airglow::for_site_profile(location, SiteProfileId::CtaSouth)?;
//!     let _output = airglow.compute(time, target)?;
//!     Ok(())
//! }
//! ```
//!
//! `standard_clear_sky` uses generic clear-sky atmospheric assumptions derived
//! from the observer location for Noll effective Rayleigh/Mie scattering.
//! `for_site_profile` records the named site assumptions and applies the
//! profile atmosphere and scale.
//!
//! For site-specific calibration, load or build an [`AirglowContinuum`] and pass
//! it to [`Airglow::with_continuum`].

pub(crate) mod calibration;
mod continuum;
mod extinction;
mod geometry;
mod model;
mod output;
pub(crate) mod temporal;
mod units;

pub(crate) use calibration::load_builtin_standard;
pub use calibration::AirglowContinuum;
pub use extinction::{
    effective_airglow_airmass, noll_scattering_factors, spectral_airglow_scattering_transmission,
};
pub use model::Airglow;
pub use output::AirglowOutputs;
pub use units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};

#[cfg(test)]
mod tests;
