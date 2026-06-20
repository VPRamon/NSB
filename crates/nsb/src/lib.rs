//! `nsb` — Night Sky Background model.
//!
//! Computes the photon flux reaching a ground-based observer from a configurable
//! sum of zodiacal light, integrated starlight, airglow, and scattered moonlight.
//! Integrated starlight requires an explicit starlight map configuration until a
//! production catalogue-derived bundled map is shipped.
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

pub mod components;
pub mod error;
pub mod evaluator;
mod reference;
pub mod site;
mod window_search;

pub use components::airglow::{
    Airglow, AirglowContinuum, AirglowOutputs, SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX,
};
pub use components::moonlight::{AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991};
pub use components::starlight::{Starlight, StarlightMap, StarlightOutputs, StarlightProvenance};
pub use components::zodiacal::{
    ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalExtinction, ZodiacalLight,
    ZodiacalOutputs, ZodiacalSpectrum,
};
pub use error::{NsbError, Result};
pub use evaluator::{
    BandDiagnostic, CalibrationStatus as ComponentCalibrationStatus, ComponentMask, MoonlightModel,
    NsbComponent, NsbComponentMetadata, NsbEvaluator, NsbModelConfig, NsbResult, Observer,
    PointQuery, StarlightModel, Target, ThresholdQuery, ThresholdQueryResult,
};
pub use site::{
    AirglowSiteCalibration, CalibrationStatus, CalibrationStatus as SiteCalibrationStatus,
    SiteProfile, SiteProfileId,
};

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

pub(crate) const NSB_S10_ZP: f64 = 27.78;
