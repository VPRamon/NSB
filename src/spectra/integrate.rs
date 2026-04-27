//! Spectral integration helpers shared by all components.
//!
//! Thin convenience wrappers that delegate to
//! `siderust::spectra::algo` for the numerical kernels. Kept as a
//! crate-local namespace because component call-sites read more naturally
//! when written as `integrate::band_integral(&s, lo, hi)`.

use super::spectrum::Spectrum;
use siderust::spectra::algo;

/// Integrate `s` over `[lo_nm, hi_nm]` (trapezoidal).
pub fn band_integral(s: &Spectrum, lo_nm: f64, hi_nm: f64) -> f64 {
    s.integrate_range(lo_nm, hi_nm)
}

/// Integrate `s · filter` over the filter's full support.
///
/// Mirrors the historical NSB behaviour of using the *filter*'s wavelength
/// grid as the integration grid (and not a union grid).
pub fn filter_integral(s: &Spectrum, filter: &Spectrum) -> f64 {
    algo::trapz_weighted(
        &s.lambda_nm,
        &s.flux,
        &filter.lambda_nm,
        &filter.flux,
    )
}
