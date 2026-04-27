//! Mie scattering optical depth (re-export).
//!
//! Behavior is upstreamed in [`siderust::atmosphere::mie`] using the
//! [`siderust::atmosphere::mie::MieParams::PARANAL`] preset.

use crate::error::Result;
use qtty::length::Nanometers;
use siderust::atmosphere::mie::{mie_optical_depth, MieParams};

/// Mie (aerosol) optical depth — wrapper preserving the `f64`-only NSB signature.
pub fn optical_depth(lambda_nm: f64) -> Result<f64> {
    Ok(mie_optical_depth(&MieParams::PARANAL, Nanometers::new(lambda_nm)))
}
