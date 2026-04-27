//! Total atmospheric optical depth = Rayleigh + Mie + ozone.

use crate::error::Result;

/// Total optical depth at wavelength `lambda_nm`, pressure `pressure_hpa`,
/// observatory altitude `h_km` above sea level.
pub fn optical_depth(lambda_nm: f64, pressure_hpa: f64, h_km: f64) -> Result<f64> {
    let tau_r = super::rayleigh::optical_depth(lambda_nm, pressure_hpa, h_km);
    let tau_m = super::mie::optical_depth(lambda_nm)?;
    Ok(tau_r + tau_m)
}

/// Multiplicative transmission `exp(-airmass · τ)`.
#[inline]
pub fn transmission(tau: f64, airmass: f64) -> f64 {
    (-airmass * tau).exp()
}
