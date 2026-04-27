//! Rayleigh scattering (re-export).
//!
//! Behavior is upstreamed in [`siderust::atmosphere::rayleigh`].

use qtty::length::{Kilometers, Nanometers};
use siderust::atmosphere::rayleigh as upstream;

/// Rayleigh optical depth — wrapper preserving the `f64`-only NSB signature.
#[inline]
pub fn optical_depth(lambda_nm: f64, pressure_hpa: f64, h_km: f64) -> f64 {
    upstream::rayleigh_optical_depth_bodhaine99(
        Nanometers::new(lambda_nm),
        pressure_hpa,
        Kilometers::new(h_km),
        upstream::DEFAULT_SCALE_HEIGHT_KM,
    )
}

/// Rayleigh phase function `P(θ) = 3/(16π) · (1 + cos²θ)`.
#[inline]
pub fn phase(cos_theta: f64) -> f64 {
    upstream::rayleigh_phase(cos_theta)
}
