//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB supports geometry at arbitrary valid Earth locations,
//! but geographic support is not scientific site calibration. The bundled
//! empirical continuum is Paranal-derived (Noll/SkyCalc/FORS1 lineage) and is
//! explicitly a generic/planning proxy unless a validated scientific profile is
//! selected.
//!
//! This module is the deliberately narrow advanced Airglow configuration API.
//! Normal applications select the scientific model and configure geometry through
//! [`crate::NsbModelConfig`], then evaluate with [`crate::NsbEvaluator`]; they do
//! not construct component evaluators or continuum calibrations directly.
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
//! A custom vertical-emission profile is configuration for emitting-volume
//! geometry, not a calibration-evidence contract. It does not upgrade the
//! maturity reported by evaluator result metadata.

pub(crate) mod calibration;
mod continuum;
mod domain;
mod extinction;
mod geometry;
mod model;
mod output;
pub(crate) mod temporal;
pub(crate) mod units;

pub(crate) use calibration::load_builtin_standard;
pub(crate) use calibration::AirglowContinuum;
pub(crate) use domain::AirglowNightPhase;
pub(crate) use extinction::NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG;
pub use geometry::{
    AirglowGeometryMetadata, AirglowGeometryModel, AirglowWavelengthApplicability,
    ValidatedZenithDomain, VanRhijnConfig, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalEmissionProfileError, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
pub(crate) use model::Airglow;
pub use model::AirglowModel;
pub(crate) use output::AirglowOutputs;

#[cfg(test)]
mod tests;
