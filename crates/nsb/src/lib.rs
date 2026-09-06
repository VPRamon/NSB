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
//! The recommended application path is:
//!
//! 1. Build an [`NsbEvaluator`] from [`NsbModelConfig`].
//! 2. Evaluate a [`PointQuery`] or search with [`ThresholdQuery`].
//! 3. Read [`NsbResult`] / [`ThresholdQueryResult`] and per-component
//!    [`NsbComponentMetadata`].
//!
//! Root re-exports are the supported crate contract. Nested `pub mod` paths
//! exist for advanced component construction and scientific metadata; they are
//! not a second, larger accidental API. Implementation helpers remain
//! `pub(crate)`.
//!
//! # Dependency types
//!
//! NSB uses Siderust, `qtty`, and `tempoch` types at the supported boundary
//! (`Observer`, `Target`, `Time<UTC>`, radiances, angles). [`DEG`] is re-exported
//! because equatorial constructors are part of the documented getting-started
//! path. Callers may depend on those crates for construction; NSB does not wrap
//! them solely to hide the dependency.
//!
//! # Architecture
//!
//! Shared reference inputs live in internal `reference` modules; component-
//! specific calibrations and grids live inside their component modules.
//!
//! `siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
//! photometry, and passbands. NSB owns NSB-specific component composition,
//! planning windows, and site-profile metadata that distinguishes generic
//! fallbacks from explicit named planning presets.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Build-time verified scientific-asset metadata (checksums, provenance, maturity).
pub mod assets;
/// Advanced component models used to construct or inspect individual contributors.
pub mod components;
/// Public error type and crate [`Result`].
pub mod error;
/// Evaluator, queries, results, and component-selection types.
pub mod evaluator;
mod reference;
/// Site profiles, shared atmosphere, and canonical calibration evidence.
pub mod site;
/// Compatibility facade for the site-calibration evidence contract.
pub mod site_calibration;
/// Offline F10.7 resolution used by airglow configuration.
pub mod solar_activity;
pub(crate) mod units;
mod window_search;

pub use components::airglow::{
    Airglow, AirglowContinuum, AirglowGeometryMetadata, AirglowGeometryModel, AirglowOutputs,
    AirglowWavelengthApplicability, SolarFluxUnits, ValidatedZenithDomain, VanRhijnConfig,
    VerticalEmissionProfile, VerticalEmissionProfileDefinition, VerticalEmissionProfileError,
    VerticalProfileNormalization, DEFAULT_SOLAR_RADIO_FLUX, DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
pub use components::moonlight::{Jones2013Spectral, KrisciunasSchaefer1991, DEFAULT_K_EXT};
pub use components::starlight::{
    Starlight, StarlightMap, StarlightOutputs, StarlightPixel, StarlightProvenance,
    StarlightValidationDiagnostics, ValidatedStarlightMap,
};
pub use components::zodiacal::{
    ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalExtinction, ZodiacalLight,
    ZodiacalOutputs, ZodiacalSpectrum,
};
pub use error::{NsbError, Result};
pub use evaluator::{
    BandDiagnostic, CalibrationStatus as ComponentCalibrationStatus, ComponentMask, MoonlightModel,
    NsbComponent, NsbComponentDescriptor, NsbComponentMetadata, NsbEvaluator, NsbModelConfig,
    NsbResult, Observer, PointQuery, StarlightModel, Target, ThresholdQuery, ThresholdQueryResult,
};
pub use site::{
    AirglowSiteCalibration, AtmosphericConditions, CalibrationStatus,
    CalibrationStatus as SiteCalibrationStatus, SiteProfile, SiteProfileId,
};
pub use site_calibration::{
    AirglowCalibrationEvidence, AtmosphericSiteCalibration, CalibratedSiteId, SiteCalibrationAsset,
    SiteCalibrationAssetError, SiteCalibrationReference, SiteCalibrationValidity,
};
pub use solar_activity::{
    bundled_f107_store, resolve_f107, F107Kind, F107Record, F107Store, F107StoreError,
    F107ValidationError, MonthlyCompleteness, MonthlyF107Evidence, ResolvedSolarActivity,
    SolarActivitySource, F107_STORE_SCHEMA_VERSION,
};
pub use units::{
    MagnitudesPerAirmass, ScaleFactors, SolarFluxUnit, SolarSpectralIrradiance,
    SolarSpectralIrradianceUnit,
};

/// Angle unit used with [`Target::new`] in documented examples.
pub use siderust::qtty::DEG;

pub(crate) const NSB_S10_ZP: qtty::photometry::SurfaceBrightness =
    qtty::photometry::SurfaceBrightness::new(27.78);

/// Version of the NSB library crate.
pub const NSB_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Versioned model-composition contract used in operational metadata.
pub const MODEL_VERSION: &str = "nsb-model-2026.1";
/// Siderust package version represented by the locked dependency.
pub const SIDERUST_VERSION: &str = "0.11.1";
/// Truthful package-source identity for the Siderust dependency.
pub const SIDERUST_SOURCE: &str = "crates.io:siderust:0.11.1";
