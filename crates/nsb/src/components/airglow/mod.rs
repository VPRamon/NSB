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
//! `for_site_profile` records the named site assumptions used for the airglow
//! continuum. CTAO profiles currently use the bundled SkyCalc-derived empirical
//! continuum template with a neutral site scale and explicit uncalibrated
//! provenance. `standard_clear_sky` remains available as a generic fallback, but
//! CTAO callers should prefer named profiles so the calibration maturity is
//! visible at the API boundary.
//!
//! For site-specific calibration, load or build an [`AirglowContinuum`] and pass
//! it to [`Airglow::with_continuum`].

pub(crate) mod calibration;
mod continuum;
mod geometry;
mod model;
mod output;
pub(crate) mod temporal;
mod units;

pub(crate) use calibration::load_builtin_standard;
pub use calibration::AirglowContinuum;
pub use model::Airglow;
pub use output::AirglowOutputs;
pub use units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};

#[cfg(test)]
mod tests;
