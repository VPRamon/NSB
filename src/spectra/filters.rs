//! Photometric filters used by the Python NSB model.
//!
//! The Python code uses simple top-hat-ish definitions of Johnson `B` and `V`
//! to compute the magnitude in those bands. We model them as Gaussian-ish
//! response curves centred on canonical wavelengths.
//!
//! TODO: replace with the exact Bessell B/V passbands sampled from the file
//! Python actually uses if and when we discover one.

use super::spectrum::Spectrum;

/// Johnson `B` band, centred at 445 nm, FWHM ~94 nm.
pub fn b_band() -> Spectrum {
    sample_gaussian(445.0, 94.0 / 2.355)
}

/// Johnson `V` band, centred at 551 nm, FWHM ~88 nm.
pub fn v_band() -> Spectrum {
    sample_gaussian(551.0, 88.0 / 2.355)
}

fn sample_gaussian(centre_nm: f64, sigma_nm: f64) -> Spectrum {
    let n = 401;
    let lo = centre_nm - 4.0 * sigma_nm;
    let hi = centre_nm + 4.0 * sigma_nm;
    let mut lam = Vec::with_capacity(n);
    let mut flx = Vec::with_capacity(n);
    for i in 0..n {
        let x = lo + (hi - lo) * (i as f64) / (n as f64 - 1.0);
        let z = (x - centre_nm) / sigma_nm;
        lam.push(x);
        flx.push((-0.5 * z * z).exp());
    }
    Spectrum::new(lam, flx)
}
