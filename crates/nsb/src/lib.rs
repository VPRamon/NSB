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
//! photometry, and passbands. NSB owns NSB-specific component composition and
//! planning windows.

#![forbid(unsafe_code)]

pub mod components;
pub mod error;
pub mod evaluator;
mod reference;

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
    ComponentMask, MoonlightModel, NsbComponent, NsbEvaluator, NsbModelConfig, NsbResult, Observer,
    PointQuery, StarlightModel, Target, ThresholdQuery, ThresholdQueryResult,
};

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

/// V-band S10 zero-point used by `band_flux_to_surface_brightness` and by each
/// component that converts between S10 surface brightness and AB magnitudes.
pub(crate) const NSB_S10_ZP: f64 = 27.78;
