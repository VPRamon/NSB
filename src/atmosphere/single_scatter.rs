//! Single-scattering correction tables (`sscatcor_m15s1.dat`).
//!
//! Scientific role:
//! this file is reserved for the atmospheric single-scattering correction data
//! needed by a more detailed moonlight model.
//!
//! Contribution to the science:
//! this module re-exports the production parser/interpolator for the bundled
//! Mie phase and multiple-scattering correction grids.

pub use crate::single_scatter::{ScatterGrid, ScatterGridKind};
