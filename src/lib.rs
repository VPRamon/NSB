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

#![forbid(unsafe_code)]

pub mod atmosphere;
pub mod components;
pub mod data;
pub mod error;
pub mod evaluator;
pub mod site;
pub mod spectra;

pub use error::{NsbError, Result};
pub use evaluator::{
    ComponentMask, Location, NsbComponent, NsbEvaluator, NsbResult, PointQuery, Target,
    ThresholdQuery, ThresholdQueryResult,
};
pub use site::Site;

pub use siderust::coordinates::frames::EquatorialMeanJ2000;
pub use siderust::coordinates::spherical::Direction as SphericalDirection;
pub use siderust::qtty::DEG;
