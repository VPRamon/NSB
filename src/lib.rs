//! `nsb` — Night Sky Background model.
//!
//! Rust port of the CTAO `darknsb` Python package. See
//! `docs/DARKNSB_REPORT.md` and `docs/NSB_STAGED_IMPLEMENTATION_PLAN.md`
//! for context.
//!
//! The crate computes the photon flux reaching a ground-based observatory
//! from the dark night sky, with contributions from:
//!
//! * **Zodiacal light** (`components::zodiacal`)
//! * **Integrated starlight** (`components::starlight`)
//! * **Airglow** (`components::airglow`)
//! * **Scattered moonlight** (`components::moonlight`)
//!
//! The top-level entry point is [`calculate`].

#![forbid(unsafe_code)]

pub mod error;
pub mod units;
pub mod geometry;
pub mod spectra;
pub mod atmosphere;
pub mod ephemeris;
pub mod components;
pub mod data;

mod nsb;
pub use nsb::{calculate, ComponentMask, NsbComponent, NsbResult, ObservationRequest, Site, Source};
pub use nsb::{SphericalDirection, EquatorialMeanJ2000, EclipticMeanJ2000, DEG};

pub use error::NsbError;

#[cfg(feature = "python")]
mod pybind;
