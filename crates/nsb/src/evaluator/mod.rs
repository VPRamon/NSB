// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! NSB evaluator: point evaluation and component composition.
//!
//! This module constructs immutable component models and evaluates night-sky
//! background at a single observing instant. Observing-window planning and
//! threshold search live in [`crate::planning`].

mod core;
mod metadata;
mod point;
mod spectral_starlight;
mod target;
pub use spectral_starlight::SpectralStarlightMode;
pub use spectral_starlight::SpectralStarlightResult;
pub use spectral_starlight::StarlightDirect;
pub use spectral_starlight::StarlightIncident;
mod types;

pub use core::NsbEvaluator;
pub(crate) use metadata::{airglow_selection_metadata, moonlight_metadata, zodiacal_metadata};
pub use metadata::{
    BandDiagnostic, ComponentCalibrationStatus as CalibrationStatus, NsbComponentMetadata,
};
pub(crate) use target::resolve_target_direction;
pub use target::{HorizontalTarget, TargetDirection};
pub(crate) use types::validate_geometry;
pub use types::{
    ComponentMask, NsbComponent, NsbComponentDescriptor, NsbModelConfig, NsbResult, Observer,
    PointQuery, Target,
};
