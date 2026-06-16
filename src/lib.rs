//! `nsb` — Night Sky Background model.
//!
//! Computes the photon flux reaching a ground-based observatory from the dark
//! sky as the sum of:
//!
//! * **Zodiacal light** (`components::zodiacal`)
//! * **Integrated starlight** (`components::starlight`)
//! * **Airglow** (`components::airglow`)
//! * **Scattered moonlight** (`components::moonlight`)
//!
//! The library is built around a single [`NsbEvaluator`] that supports two
//! query shapes:
//!
//! * [`PointQuery`] — NSB at a single `(time, location, target)`.
//! * [`ThresholdQuery`] — UTC sub-periods within a window where the integrated
//!   linear NSB is below a given radiance threshold.
//!
//! Scientific role:
//! this crate provides an operational model of the main astrophysical and
//! atmospheric contributors to the optical night-sky background seen by a
//! ground-based observer. In practical terms, it helps answer "how bright is
//! the sky in this direction, at this time, from this site?"
//!
//! Architectural role:
//! this root module exposes the public API and gathers the science-specific
//! submodules:
//!
//! * `evaluator` orchestrates the full calculation
//! * `components` contains the individual physical/empirical contributors
//! * `spectra` and `data` load the bundled scientific reference inputs
//! * `site` and `sites` provide observing-site inputs
//! * `airglow`, `solar_spectrum`, and `single_scatter` expose supporting
//!   reference catalogues and simplified helper data structures

#![forbid(unsafe_code)]

pub mod airglow;
pub mod components;
pub mod data;
pub mod error;
pub mod evaluator;
pub mod single_scatter;
pub mod site;
pub mod sites;
pub mod solar_spectrum;
pub mod spectra;

pub use airglow::{AirglowLine, ALL_LINES, NUM_LINES};
pub use components::moonlight::{
    compute_jones2013, compute_jones2013_spectral, compute_jones2013_with_extinction,
};
pub use error::{NsbError, Result};
pub use evaluator::{
    AirglowModel, ComponentMask, Location, MoonlightModel, NsbComponent, NsbEvaluator,
    NsbModelConfig, NsbResult, PointQuery, Target, ThresholdQuery, ThresholdQueryResult,
};
pub use single_scatter::ScatterGrid;
pub use site::Site;
pub use sites::{
    CatalogSite, ALL_SITES, APACHE_POINT, CERRO_PARANAL, KITT_PEAK, MAUNA_KEA,
    ROQUE_DE_LOS_MUCHACHOS, SUBURBAN_REFERENCE,
};
pub use solar_spectrum::SolarSpectrum;

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

/// V-band S10 zero-point used by `band_flux_to_surface_brightness` and by each
/// component that converts between S10 surface brightness and AB magnitudes.
pub(crate) const NSB_S10_ZP: f64 = 27.78;
