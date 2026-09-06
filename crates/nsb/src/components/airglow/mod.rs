//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB supports geometry at arbitrary valid Earth locations,
//! but geographic support is not scientific site calibration. The bundled
//! empirical continuum is Paranal-derived (Noll/SkyCalc/FORS1 lineage) and is
//! explicitly a generic/planning proxy unless a validated scientific profile is
//! selected.
//!
//! ```no_run
//! use nsb::{
//!     Airglow, AirglowGeometryModel, CalibrationStatus, Observer, Target, VanRhijnConfig,
//! };
//! use tempoch::{Time, UTC};
//!
//! fn evaluate(location: Observer, time: Time<UTC>, target: Target) -> nsb::Result<()> {
//!     let airglow = Airglow::standard_clear_sky(location)?
//!         .with_geometry(AirglowGeometryModel::VanRhijn(VanRhijnConfig::default()));
//!     assert_eq!(airglow.calibration_status(), CalibrationStatus::GenericFallback);
//!     let _output = airglow.compute(time, target)?;
//!     Ok(())
//! }
//! ```
//!
//! `standard_clear_sky` uses generic clear-sky atmospheric assumptions derived
//! from the observer location for Noll effective Rayleigh/Mie scattering. The
//! location changes geometry and local inputs only; even selecting Paranal as
//! the observer does not promote the model to a dedicated Paranal calibration.
//! Van Rhijn is the unchanged default thin-shell approximation. A validated
//! [`VerticalEmissionProfile`] can instead be selected through
//! [`AirglowGeometryModel::VerticalProfile`]. Emitting-volume geometry is
//! independent of the Noll Rayleigh/Mie atmospheric attenuation stage and does
//! not change calibration maturity.
//!
//! A caller-provided [`AirglowContinuum`] is classified as
//! [`AirglowScientificProfile::UnvalidatedCustomContinuum`]. Providing custom
//! bytes, a scale, F10.7, atmosphere or geometry is not a calibration-evidence
//! contract. Future site-calibrated use must enter through an explicit validated
//! scientific profile/evidence path.

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
pub(crate) use extinction::NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG;
pub use geometry::{
    AirglowGeometryMetadata, AirglowGeometryModel, AirglowWavelengthApplicability,
    ValidatedZenithDomain, VanRhijnConfig, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalEmissionProfileError, VerticalProfileNormalization,
    DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
pub use model::{Airglow, AirglowScientificProfile};
pub use output::AirglowOutputs;
pub use units::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};

#[cfg(test)]
mod tests;
