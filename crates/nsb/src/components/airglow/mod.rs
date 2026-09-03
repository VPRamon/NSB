//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB exposes an arbitrary-location empirical continuum model
//! with an explicit emitting-volume geometry:
//!
//! ```no_run
//! use nsb::{Airglow, AirglowGeometryModel, Observer, Target, VanRhijnConfig};
//! use tempoch::{Time, UTC};
//!
//! fn evaluate(location: Observer, time: Time<UTC>, target: Target) -> nsb::Result<()> {
//!     let airglow = Airglow::standard_clear_sky(location)?
//!         .with_geometry(AirglowGeometryModel::VanRhijn(VanRhijnConfig::default()));
//!     let _output = airglow.compute(time, target)?;
//!     Ok(())
//! }
//! ```
//!
//! `standard_clear_sky` uses generic clear-sky atmospheric assumptions derived
//! from the observer location for Noll effective Rayleigh/Mie scattering.
//! Van Rhijn is the unchanged default thin-shell approximation. A validated
//! [`VerticalEmissionProfile`] can instead be selected through
//! [`AirglowGeometryModel::VerticalProfile`]. Emitting-volume geometry is
//! independent of the Noll Rayleigh/Mie atmospheric attenuation stage.
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
    NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG,
};
pub use geometry::{
    AirglowGeometryMetadata, AirglowGeometryModel, AirglowWavelengthApplicability,
    ValidatedZenithDomain, VanRhijnConfig, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalEmissionProfileError, VerticalProfileNormalization,
    AIRGLOW_MEAN_EARTH_RADIUS_KM, DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM,
    VAN_RHIJN_IMPLEMENTATION_VERSION, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
    VERTICAL_PROFILE_INTEGRATOR_VERSION, VERTICAL_PROFILE_REFERENCE_SUBSTEPS,
};
pub use model::Airglow;
pub use output::AirglowOutputs;
pub use units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};

#[cfg(test)]
mod tests;
