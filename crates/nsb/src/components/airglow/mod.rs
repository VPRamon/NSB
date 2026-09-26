//! Empirical continuum airglow component.
//!
//! Airglow is terrestrial atmospheric emission and a natural part of the night
//! sky background. NSB supports geometry at arbitrary valid Earth locations,
//! but geographic support is not scientific site calibration.
//!
//! # Model selection
//!
//! Configuration distinguishes **automatic** selection
//! ([`AirglowSelection::Automatic`]) from **explicit** caller selection
//! ([`AirglowSelection::Explicit`]). Explicit selection always wins and never
//! silently switches models. Automatic selection is deterministic; until the
//! global climatological model (#157) is admitted it resolves to the temporary
//! Paranal-derived planning fallback with that fallback visible in
//! [`AirglowSelectionMetadata`] via [`AirglowFallbackReason`].
//!
//! The bundled Paranal-derived continuum (Noll/SkyCalc/FORS1 lineage) remains a
//! supported explicit legacy/reference model and the temporary automatic
//! fallback. It is not intrinsically the generic global scientific contract.
//! Future climatology adds a new [`AirglowModel`] variant when admitted — the
//! enum is `#[non_exhaustive]` so no speculative public placeholder is frozen.
//!
//! # Outcome semantics
//!
//! [`AirglowSelectionMetadata`] is known from configuration (and current
//! construction-time resolution). [`AirglowEvaluationOutcome`] is known only
//! after a time-dependent evaluation. [`crate::NsbEvaluator::describe_components`]
//! must not invent an evaluation outcome.
//!
//! Physical zero (for example outside astronomical night), invalid
//! input/configuration, and deliberate automatic fallback are distinct.
//! Invalid inputs error; they are never converted into a plausible zero
//! radiance — even when the query is outside astronomical night.
//!
//! This module is the deliberately narrow advanced Airglow configuration API.
//! Normal applications select the scientific model and configure geometry through
//! [`crate::NsbModelConfig`], then evaluate with [`crate::NsbEvaluator`]; they do
//! not construct component evaluators or continuum calibrations directly.
//!
//! Van Rhijn is the unchanged default thin-shell approximation. A validated
//! [`VerticalEmissionProfile`] can instead be selected through
//! [`AirglowGeometryModel::VerticalProfile`]. Emitting-volume geometry is
//! independent of the Noll Rayleigh/Mie atmospheric attenuation stage and does
//! not change calibration maturity.

pub(crate) mod calibration;
mod continuum;
mod domain;
mod extinction;
mod geometry;
mod model;
mod output;
mod selection;
pub(crate) mod temporal;
pub(crate) mod units;

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
pub(crate) use selection::{
    load_continuum_for_model, resolve_airglow_selection, ResolvedAirglowSelection,
};
pub use selection::{
    AirglowEvaluationOutcome, AirglowFallbackReason, AirglowPhysicalOutcome,
    AirglowPhysicalZeroReason, AirglowSelection, AirglowSelectionKind, AirglowSelectionMetadata,
};

#[cfg(test)]
mod tests;
