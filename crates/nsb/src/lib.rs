//! `nsb` — Night Sky Background model.
//!
//! Computes the photon flux reaching a ground-based observer from a configurable
//! sum of zodiacal light, integrated starlight, airglow, and scattered moonlight.
//! Integrated starlight is included in defaults only when the build embeds a
//! validated production catalogue-derived map.
//!
//! The library API is intentionally typed and CLI-free: callers pass
//! `Geodetic<ECEF>` observers, `Time<UTC>` instants, and equatorial target
//! directions directly. Named-site parsing, command-line flags, and output
//! formatting belong in a separate CLI crate that consumes this library.
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

pub mod assets;
pub mod components;
pub mod error;
pub mod evaluator;
mod reference;
pub mod site;
pub mod site_calibration;
pub mod solar_activity;
pub(crate) mod spectrum;
pub(crate) mod units;
mod window_search;

pub use components::airglow::{
    Airglow, AirglowContinuum, AirglowOutputs, SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX,
};
pub use components::moonlight::{AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991};
pub use components::starlight::{
    Starlight, StarlightMap, StarlightOutputs, StarlightProvenance, StarlightValidationDiagnostics,
    ValidatedStarlightMap,
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
    AirglowSiteCalibration, CalibrationStatus, CalibrationStatus as SiteCalibrationStatus,
    SiteProfile, SiteProfileId,
};
pub use site_calibration::{
    AirglowCalibrationEvidence, AtmosphericSiteCalibration, CalibratedSiteId, SiteCalibrationAsset,
    SiteCalibrationAssetError, SiteCalibrationReference, SiteCalibrationValidity,
    SITE_CALIBRATION_ASSET_SCHEMA_VERSION,
};
pub use solar_activity::{
    bundled_f107_store, days_in_month, is_finalized_monthly_observation, month_bounds_for,
    resolve_f107, F107Kind, F107Record, F107Store, F107StoreError, MonthlyCompleteness,
    MonthlyF107Evidence, ResolvedSolarActivity, SolarActivitySource, BUNDLED_F107_ASSET_PATH,
    BUNDLED_F107_RELATIVE_PATH, F107_STORE_SCHEMA_VERSION,
};
pub use units::{
    MagnitudePerAirmass, MagnitudesPerAirmass, ScaleFactors, SolarFluxUnit,
    WattPerSquareMeterSteradianMicrometer, WattsPerSquareMeterSteradianMicrometer,
};

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

pub(crate) const NSB_S10_ZP: qtty::photometry::SurfaceBrightness =
    qtty::photometry::SurfaceBrightness::new(27.78);

/// Version of the NSB library crate.
pub const NSB_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Versioned model-composition contract used in operational metadata.
pub const MODEL_VERSION: &str = "nsb-model-2026.1";
/// Siderust package version represented by the locked dependency.
pub const SIDERUST_VERSION: &str = "0.11.0";
/// Truthful package-source identity for the Siderust dependency.
pub const SIDERUST_SOURCE: &str = "crates.io:siderust:0.11.0";
