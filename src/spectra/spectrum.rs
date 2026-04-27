//! `Spectrum` mirroring the Python helper class.
//!
//! A `Spectrum` is a pair of equally-sized vectors `(lambda, flux)` plus
//! an optional tag. Wavelengths are stored in nanometres unless noted.
//!
//! The numerical kernels (linear interpolation with endpoint clamping and
//! trapezoidal integration) are delegated to `siderust::spectra::algo`,
//! which preserves NSB's historical `numpy.interp`-style semantics
//! bit-for-bit.

use siderust::spectra::algo;
use siderust::spectra::{Interpolation, OutOfRange};

#[derive(Debug, Clone)]
pub struct Spectrum {
    pub lambda_nm: Vec<f64>,
    pub flux: Vec<f64>,
    pub tag: Option<String>,
}

impl Spectrum {
    pub fn new(lambda_nm: Vec<f64>, flux: Vec<f64>) -> Self {
        assert_eq!(lambda_nm.len(), flux.len(), "lambda and flux must be same length");
        Self { lambda_nm, flux, tag: None }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    pub fn len(&self) -> usize { self.lambda_nm.len() }
    pub fn is_empty(&self) -> bool { self.lambda_nm.is_empty() }

    /// Linear interpolation. Out-of-range queries clamp to endpoints
    /// (matches `np.interp`'s default behaviour).
    pub fn interp(&self, lambda_nm: f64) -> f64 {
        algo::interp(
            &self.lambda_nm,
            &self.flux,
            lambda_nm,
            Interpolation::Linear,
            OutOfRange::ClampToEndpoints,
        )
        .expect("interp with ClampToEndpoints cannot fail on a validated spectrum")
    }

    /// Trapezoidal integral over the full range.
    pub fn integrate(&self) -> f64 {
        algo::trapz(&self.lambda_nm, &self.flux)
    }

    /// Trapezoidal integral over `[lo, hi]` (in nm).
    pub fn integrate_range(&self, lo_nm: f64, hi_nm: f64) -> f64 {
        algo::trapz_range(&self.lambda_nm, &self.flux, lo_nm, hi_nm)
    }
}
