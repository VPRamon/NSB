//! Total atmospheric optical depth = Rayleigh + Mie (ozone applied separately).

use crate::error::Result;
use qtty::length::{Kilometers, Nanometers};
use qtty::pressure::Hectopascals;
use siderust::atmosphere::mie::MieParams;
use siderust::atmosphere::rayleigh::DEFAULT_SCALE_HEIGHT_KM;
use siderust::atmosphere::AtmosphereProfile;

/// Total optical depth at wavelength `lambda_nm`, pressure `pressure_hpa`,
/// observatory altitude `h_km` above sea level.
///
/// Delegates to [`AtmosphereProfile`] (Rayleigh + Mie). Ozone is **not**
/// included here; it is applied separately as a multiplicative transmittance
/// via `nsb::spectra::ozone`.
pub fn optical_depth(lambda_nm: f64, pressure_hpa: f64, h_km: f64) -> Result<f64> {
    let profile = AtmosphereProfile {
        surface_pressure: Hectopascals::new(pressure_hpa),
        observer_altitude: Kilometers::new(h_km),
        rayleigh_scale_height_km: DEFAULT_SCALE_HEIGHT_KM,
        mie_params: MieParams::PARANAL,
    };
    Ok(profile.optical_depth(Nanometers::new(lambda_nm)))
}

/// Multiplicative transmission `exp(-airmass · τ)` — re-export.
#[inline]
pub fn transmission(tau: f64, airmass: f64) -> f64 {
    siderust::atmosphere::extinction::transmission(tau, airmass)
}
