//! Shared reference spectra used across NSB components.
//!
//! Reference data that is not owned by a single component lives here.
//! Component-specific calibrations and grids live inside their component
//! modules.
//!
//! Currently provides:
//! - [`solar`]: the bundled solar irradiance reference spectrum used by the
//!   zodiacal-light model.

pub(crate) mod solar;
