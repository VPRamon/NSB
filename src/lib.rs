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
//! # Architecture
//!
//! `siderust` owns astronomy, time, coordinates, events, atmosphere, lunar
//! photometry, and passbands. NSB owns only NSB-specific tables, component
//! composition, and planner windows.

#![forbid(unsafe_code)]

pub mod components;
pub mod error;
pub mod evaluator;
pub mod leinert;
pub mod single_scatter;
pub mod site;
pub mod sites;
pub mod spectra;

pub use components::moonlight::{AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991};
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

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;

/// V-band S10 zero-point used by `band_flux_to_surface_brightness` and by each
/// component that converts between S10 surface brightness and AB magnitudes.
pub(crate) const NSB_S10_ZP: f64 = 27.78;
