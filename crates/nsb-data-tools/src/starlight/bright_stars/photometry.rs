//! Explicit photometric zero points and template band integration.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Photometric zero-point declaration for one input band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandZeroPoint {
    pub band_id: String,
    pub system: String,
    /// Zero-point flux density in W m^-2 m^-1 for mag = 0 (f_λ convention).
    pub zero_point_f_lambda_si: f64,
    pub reference: String,
}

/// Simple piecewise-linear spectral template in W m^-2 m^-1 vs metres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralTemplate {
    pub template_id: String,
    pub wavelengths_m: Vec<f64>,
    pub f_lambda_si: Vec<f64>,
    pub provenance: String,
}

/// Convert a magnitude to f_λ using an explicit zero point.
pub fn magnitude_to_f_lambda_si(magnitude: f64, zp: &BandZeroPoint) -> Result<f64> {
    if !magnitude.is_finite()
        || !zp.zero_point_f_lambda_si.is_finite()
        || zp.zero_point_f_lambda_si <= 0.0
    {
        bail!("magnitude and zero-point must be finite with positive ZP flux");
    }
    Ok(zp.zero_point_f_lambda_si * 10_f64.powf(-0.4 * magnitude))
}

/// Integrate photon flux ∫ (λ / hc) f_λ dλ over [λ_min, λ_max] (metres).
///
/// Returns ph m^-2 s^-1. Uses trapezoidal integration on the template grid
/// restricted to the requested band.
pub fn integrate_template_photon_flux(
    template: &SpectralTemplate,
    lambda_min_m: f64,
    lambda_max_m: f64,
) -> Result<f64> {
    if template.wavelengths_m.len() < 2
        || template.wavelengths_m.len() != template.f_lambda_si.len()
    {
        bail!("template requires matching wavelength/flux grids with ≥2 points");
    }
    if !(lambda_min_m.is_finite()
        && lambda_max_m.is_finite()
        && lambda_min_m > 0.0
        && lambda_max_m > lambda_min_m)
    {
        bail!("band limits must be finite with lambda_max > lambda_min > 0");
    }
    // h = 6.62607015e-34, c = 2.99792458e8 → hc = 1.98644586e-25 J m
    const HC_J_M: f64 = 1.986_445_86e-25;
    let mut sum = 0.0;
    for (l_pair, f_pair) in template
        .wavelengths_m
        .windows(2)
        .zip(template.f_lambda_si.windows(2))
    {
        let (l0, l1) = (l_pair[0], l_pair[1]);
        let (f0, f1) = (f_pair[0], f_pair[1]);
        if !l0.is_finite() || !l1.is_finite() || !f0.is_finite() || !f1.is_finite() || l1 <= l0 {
            bail!("template samples must be finite and strictly increasing in wavelength");
        }
        let a = l0.max(lambda_min_m);
        let b = l1.min(lambda_max_m);
        if b <= a {
            continue;
        }
        // Linear interpolate f at a,b within [l0,l1].
        let t = |l: f64| (l - l0) / (l1 - l0);
        let fa = f0 + (f1 - f0) * t(a);
        let fb = f0 + (f1 - f0) * t(b);
        // Photon integrand g(λ) = (λ/hc) f_λ; trapezoid on [a,b].
        let ga = (a / HC_J_M) * fa;
        let gb = (b / HC_J_M) * fb;
        sum += 0.5 * (ga + gb) * (b - a);
    }
    if !(sum.is_finite() && sum > 0.0) {
        bail!("band integral must be finite and positive");
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_zero_returns_zero_point() {
        let zp = BandZeroPoint {
            band_id: "test".into(),
            system: "AB-like-test".into(),
            zero_point_f_lambda_si: 1.0e-8,
            reference: "unit-test".into(),
        };
        let f = magnitude_to_f_lambda_si(0.0, &zp).unwrap();
        assert!((f - 1.0e-8).abs() < 1e-20);
        let f5 = magnitude_to_f_lambda_si(5.0, &zp).unwrap();
        assert!((f5 - 1.0e-8 * 10_f64.powf(-2.0)).abs() / f5 < 1e-12);
    }

    #[test]
    fn flat_spectrum_photon_integral_scales_with_band() {
        // Flat f_λ = 1e-8 W m^-2 m^-1 from 300–700 nm.
        let template = SpectralTemplate {
            template_id: "flat".into(),
            wavelengths_m: vec![300e-9, 336e-9, 650e-9, 700e-9],
            f_lambda_si: vec![1e-8, 1e-8, 1e-8, 1e-8],
            provenance: "unit-test".into(),
        };
        let narrow = integrate_template_photon_flux(&template, 336e-9, 650e-9).unwrap();
        let wide = integrate_template_photon_flux(&template, 300e-9, 650e-9).unwrap();
        assert!(wide > narrow);
        assert!(narrow.is_finite() && narrow > 0.0);
    }

    #[test]
    fn invalid_magnitude_fails() {
        let zp = BandZeroPoint {
            band_id: "t".into(),
            system: "t".into(),
            zero_point_f_lambda_si: 1.0,
            reference: "t".into(),
        };
        assert!(magnitude_to_f_lambda_si(f64::NAN, &zp).is_err());
    }
}
