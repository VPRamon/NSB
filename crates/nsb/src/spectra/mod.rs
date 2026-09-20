//! Shared physical spectra used across NSB components.
//!
//! Spectral inputs whose scientific ownership spans multiple night-sky
//! components live here. Component-specific spectra, calibrations, and grids
//! remain inside their owning component modules.
//!
//! Currently provides:
//! - [`solar`]: the bundled solar spectral irradiance used by zodiacal-light
//!   and spectral moonlight models.

pub(crate) mod solar;
