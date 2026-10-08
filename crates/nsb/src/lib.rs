// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! `nsb` — Night Sky Background model.
//!
//! Computes the photon flux reaching a ground-based observer from a configurable
//! sum of zodiacal light, integrated starlight, airglow, and scattered moonlight.
//! Integrated starlight is included in defaults only when the build embeds a
//! validated production catalogue-derived map.
//!
//! The library API is intentionally typed and CLI-free: callers pass
//! [`Observer`] values, `Time<UTC>` instants, and equatorial [`Target`]
//! directions directly. Named-site parsing, command-line flags, and output
//! formatting belong in a separate CLI crate that consumes this library.
//!
//! # Supported public API
//!
//! The first-release surface is classified in
//! [`docs/developer-guide/public-api.md`](../../docs/developer-guide/public-api.md).
//!
//! 1. **Core root API** — normal evaluator/planning integrations and durable
//!    component selectors.
//! 2. **Advanced nested API** — `components::starlight`, `components::airglow`,
//!    `site`, and `solar_activity` for supported scientific workflows.
//! 3. **Metadata/schema API** — intentional persisted or inspectable records
//!    (bundled assets, F10.7 stores).
//! 4. **Private implementation** — concrete component evaluators, spectra,
//!    planning internals, and test/tooling helpers.
//!
//! The recommended application path is:
//!
//! 1. Build an [`NsbEvaluator`] from [`NsbModelConfig`].
//! 2. Evaluate a [`PointQuery`] or search with [`ThresholdQuery`].
//! 3. Read [`NsbResult`] / [`ThresholdQueryResult`] and per-component
//!    [`NsbComponentMetadata`].
//!
//! # Airglow selection
//!
//! [`NsbModelConfig::generic_clear_sky`], [`NsbModelConfig::default`], and
//! [`NsbEvaluator::new`] use [`AirglowSelection::Automatic`]. Explicit
//! [`NsbModelConfig::with_airglow_model`] selection wins and never silently
//! switches models. Until the global climatological model is admitted (#157),
//! automatic policy resolves to the temporary Paranal-derived planning fallback
//! with typed fallback metadata in [`NsbComponentMetadata::airglow_selection`].
//! Physical evaluation outcomes live in
//! [`NsbComponentMetadata::airglow_evaluation`] and are absent from
//! [`NsbEvaluator::describe_components`] descriptors.
//!
//! # Dependency types
//!
//! NSB uses Siderust, `qtty`, and `tempoch` types at the supported boundary
//! (`Observer`, `Target`, `Time<UTC>`, radiances, angles). The [`units`]
//! module is the canonical NSB-facing entry point for `qtty`: it re-exports
//! the upstream crate and the quantity modules used by NSB. [`DEG`] remains
//! re-exported at the crate root for the documented getting-started path.
//!
//! # Architecture
//!
//! Shared physical spectra used across components live in the internal `spectra`
//! module; component-specific calibrations and grids live inside their component
//! modules.
//!
//! `siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
//! photometry, and passbands. NSB owns NSB-specific component composition,
//! atmospheric-transport orchestration ([`transport`]), observing-window
//! planning, and site-profile metadata that distinguishes generic fallbacks
//! from explicit named planning presets.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Advanced component models used to construct or inspect individual contributors.
pub mod components;
/// Runtime scientific data and build-verified bundled metadata.
pub mod data;
pub(crate) mod error;
mod evaluator;
mod planning;
#[cfg(feature = "python")]
mod python;
/// Site profiles and shared atmospheric assumptions.
pub mod site;
/// Offline F10.7 resolution used by airglow configuration.
pub mod solar_activity;
mod spectra;
/// Atmospheric transport of spectral sky radiance (identity and direct paths).
pub mod transport;
/// Physical quantities, NSB-specific conventions, and the `qtty` facade.
pub mod units;

pub use components::airglow::AirglowModel;
pub use components::airglow::AirglowSelection;
pub use components::moonlight::MoonlightModel;
pub use components::starlight::StarlightProduct;
pub use components::zodiacal::{ZodiacalExtinction, ZodiacalModel};
pub use error::{NsbError, Result};
pub use evaluator::{
    BandDiagnostic, CalibrationStatus as ComponentCalibrationStatus, ComponentMask, NsbComponent,
    NsbComponentDescriptor, NsbComponentMetadata, NsbEvaluator, NsbModelConfig, NsbResult,
    Observer, PointQuery, Target,
};
pub use planning::{SiteWindowContext, ThresholdQuery, ThresholdQueryResult};
pub use site::{CalibrationStatus, GenericClearSky, SiteProfile, SiteProfileTag};
use units::photometry::SurfaceBrightness;
pub use units::{SolarFluxUnit, SolarFluxUnits};

/// Angle unit used with [`Target::new`] in documented examples.
pub use siderust::qtty::DEG;

pub(crate) const NSB_S10_ZP: SurfaceBrightness = SurfaceBrightness::new(27.78);

/// Version of the NSB library crate.
pub const NSB_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Versioned model-composition contract used in operational metadata.
pub const MODEL_VERSION: &str = "nsb-model-v1";
/// Siderust package version represented by the locked dependency.
pub const SIDERUST_VERSION: &str = "0.12.0";
/// Truthful package-source identity for the Siderust dependency.
pub const SIDERUST_SOURCE: &str = "crates.io:siderust:0.12.0";
