//! Spectral integration helpers shared by all components.

use super::spectrum::Spectrum;

/// Integrate `s` over `[lo_nm, hi_nm]` (trapezoidal).
pub fn band_integral(s: &Spectrum, lo_nm: f64, hi_nm: f64) -> f64 {
    s.integrate_range(lo_nm, hi_nm)
}

/// Integrate `s · filter` over the filter's full support.
pub fn filter_integral(s: &Spectrum, filter: &Spectrum) -> f64 {
    let mut sum = 0.0;
    for i in 1..filter.lambda_nm.len() {
        let a = filter.lambda_nm[i - 1];
        let b = filter.lambda_nm[i];
        let fa = s.interp(a) * filter.flux[i - 1];
        let fb = s.interp(b) * filter.flux[i];
        sum += 0.5 * (fa + fb) * (b - a);
    }
    sum
}

/// Photometric zero-point conversion mirroring `get_NSB.py`:
/// `mag = 27.78 - 2.5 · log10(flux)`.
#[inline]
pub fn flux_to_mag(flux: f64) -> f64 {
    27.78 - 2.5 * flux.log10()
}
