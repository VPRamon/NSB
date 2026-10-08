// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Explicit photometric zero points and template band integration.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Photometric zero-point declaration for one input band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandZeroPoint {
    pub band_id: String,
    pub system: String,
    pub zero_point_convention: String,
    pub reference_spectrum: String,
    pub flux_density_convention: String,
    pub units: String,
    /// Zero-point flux density in W m^-2 m^-1 for mag = 0 (f_λ convention).
    pub zero_point_f_lambda_si: f64,
    pub citation: String,
    pub response_curve_sha256: String,
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

/// Checksum-pinned dimensionless system response sampled versus wavelength.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricBandResponse {
    pub band_id: String,
    pub wavelengths_m: Vec<f64>,
    pub throughput: Vec<f64>,
    pub detector_convention: String,
    pub citation: String,
    pub sha256: String,
}

/// Physical magnitude calibration for a broad passband.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricBandCalibration {
    pub system: String,
    pub band_id: String,
    pub zero_point_convention: String,
    pub reference_spectrum: String,
    /// Band-integrated detected photon flux at magnitude zero.
    pub zero_point_photon_flux_ph_m2_s: f64,
    pub citation: String,
}

/// Convert a magnitude to f_λ using an explicit zero point.
pub fn magnitude_to_f_lambda_si(magnitude: f64, zp: &BandZeroPoint) -> Result<f64> {
    if !magnitude.is_finite()
        || !zp.zero_point_f_lambda_si.is_finite()
        || zp.zero_point_f_lambda_si <= 0.0
    {
        bail!("magnitude and zero-point must be finite with positive ZP flux");
    }
    if zp.band_id.trim().is_empty()
        || zp.system.trim().is_empty()
        || zp.zero_point_convention.trim().is_empty()
        || zp.reference_spectrum.trim().is_empty()
        || zp.flux_density_convention != "f_lambda"
        || zp.units != "W m^-2 m^-1"
        || zp.citation.trim().is_empty()
        || zp.response_curve_sha256.len() != 64
        || !zp
            .response_curve_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        bail!("photometric zero point lacks a complete physical definition");
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
    let coverage_tolerance = 16.0 * f64::EPSILON * lambda_max_m;
    if template.wavelengths_m[0] - lambda_min_m > coverage_tolerance
        || lambda_max_m - *template.wavelengths_m.last().expect("length validated")
            > coverage_tolerance
    {
        bail!("spectral template does not fully cover the requested wavelength interval");
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
        if !l0.is_finite()
            || !l1.is_finite()
            || !f0.is_finite()
            || !f1.is_finite()
            || f0 < 0.0
            || f1 < 0.0
            || l1 <= l0
        {
            bail!("template samples must be finite, non-negative, and strictly increasing in wavelength");
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

/// Convert a catalogue magnitude to band-integrated photon flux. This is the
/// only magnitude primitive intended for real Hp/B_T/V_T reconstruction.
pub fn magnitude_to_band_photon_flux(
    magnitude: f64,
    calibration: &PhotometricBandCalibration,
) -> Result<f64> {
    if !magnitude.is_finite()
        || !calibration.zero_point_photon_flux_ph_m2_s.is_finite()
        || calibration.zero_point_photon_flux_ph_m2_s <= 0.0
        || calibration.system.trim().is_empty()
        || calibration.band_id.trim().is_empty()
        || calibration.zero_point_convention.trim().is_empty()
        || calibration.reference_spectrum.trim().is_empty()
        || calibration.citation.trim().is_empty()
    {
        bail!("broad-band calibration is incomplete or non-physical");
    }
    Ok(calibration.zero_point_photon_flux_ph_m2_s * 10_f64.powf(-0.4 * magnitude))
}

/// Integrate a template through the actual dimensionless passband response.
pub fn integrate_template_through_response(
    template: &SpectralTemplate,
    response: &PhotometricBandResponse,
) -> Result<f64> {
    validate_template(template)?;
    validate_response(response)?;
    let start = response.wavelengths_m[0];
    let end = *response
        .wavelengths_m
        .last()
        .expect("response length validated");
    if template.wavelengths_m[0] > start
        || *template
            .wavelengths_m
            .last()
            .expect("template length validated")
            < end
    {
        bail!("spectral template does not cover the complete photometric response");
    }
    const HC_J_M: f64 = 1.986_445_86e-25;
    let mut sum = 0.0;
    for index in 0..response.wavelengths_m.len() - 1 {
        let l0 = response.wavelengths_m[index];
        let l1 = response.wavelengths_m[index + 1];
        let f0 = interpolate_template(template, l0)?;
        let f1 = interpolate_template(template, l1)?;
        let g0 = l0 / HC_J_M * f0 * response.throughput[index];
        let g1 = l1 / HC_J_M * f1 * response.throughput[index + 1];
        sum += 0.5 * (g0 + g1) * (l1 - l0);
    }
    if !sum.is_finite() || sum <= 0.0 {
        bail!("synthetic broad-band photon flux is not positive and finite");
    }
    Ok(sum)
}

/// Scale a relative template to an observed broad-band magnitude, then
/// integrate the requested science band.
pub fn reconstruct_template_band_flux(
    template: &SpectralTemplate,
    response: &PhotometricBandResponse,
    calibration: &PhotometricBandCalibration,
    magnitude: f64,
    lambda_min_m: f64,
    lambda_max_m: f64,
) -> Result<f64> {
    if response.band_id != calibration.band_id {
        bail!("response and calibration band identities differ");
    }
    let observed = magnitude_to_band_photon_flux(magnitude, calibration)?;
    let synthetic = integrate_template_through_response(template, response)?;
    let science = integrate_template_photon_flux(template, lambda_min_m, lambda_max_m)?;
    let reconstructed = science * observed / synthetic;
    if !reconstructed.is_finite() || reconstructed <= 0.0 {
        bail!("reconstructed science-band flux is not positive and finite");
    }
    Ok(reconstructed)
}

fn validate_template(template: &SpectralTemplate) -> Result<()> {
    if template.wavelengths_m.len() < 2
        || template.wavelengths_m.len() != template.f_lambda_si.len()
        || template.template_id.trim().is_empty()
        || template.provenance.trim().is_empty()
        || template
            .wavelengths_m
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        || template
            .f_lambda_si
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        || template
            .wavelengths_m
            .windows(2)
            .any(|pair| pair[1] <= pair[0])
    {
        bail!("spectral template contract is invalid");
    }
    Ok(())
}

fn validate_response(response: &PhotometricBandResponse) -> Result<()> {
    if response.wavelengths_m.len() < 2
        || response.wavelengths_m.len() != response.throughput.len()
        || response.band_id.trim().is_empty()
        || response.detector_convention != "photon_counting"
        || response.citation.trim().is_empty()
        || response.sha256.len() != 64
        || !response.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || response
            .wavelengths_m
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        || response
            .throughput
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        || response
            .wavelengths_m
            .windows(2)
            .any(|pair| pair[1] <= pair[0])
    {
        bail!("photometric response contract is invalid");
    }
    Ok(())
}

fn interpolate_template(template: &SpectralTemplate, wavelength: f64) -> Result<f64> {
    let upper = template
        .wavelengths_m
        .partition_point(|value| *value < wavelength);
    if upper == 0 {
        return Ok(template.f_lambda_si[0]);
    }
    if upper >= template.wavelengths_m.len() {
        return Ok(*template.f_lambda_si.last().expect("validated"));
    }
    let lower = upper - 1;
    let fraction = (wavelength - template.wavelengths_m[lower])
        / (template.wavelengths_m[upper] - template.wavelengths_m[lower]);
    Ok(template.f_lambda_si[lower]
        + fraction * (template.f_lambda_si[upper] - template.f_lambda_si[lower]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_zero_returns_zero_point() {
        let zp = BandZeroPoint {
            band_id: "test".into(),
            system: "AB-like-test".into(),
            zero_point_convention: "fixture".into(),
            reference_spectrum: "fixture".into(),
            flux_density_convention: "f_lambda".into(),
            units: "W m^-2 m^-1".into(),
            zero_point_f_lambda_si: 1.0e-8,
            citation: "unit-test".into(),
            response_curve_sha256: "0".repeat(64),
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
            zero_point_convention: "t".into(),
            reference_spectrum: "t".into(),
            flux_density_convention: "f_lambda".into(),
            units: "W m^-2 m^-1".into(),
            zero_point_f_lambda_si: 1.0,
            citation: "t".into(),
            response_curve_sha256: "0".repeat(64),
        };
        assert!(magnitude_to_f_lambda_si(f64::NAN, &zp).is_err());
    }

    fn template(start_nm: f64, end_nm: f64, values: [f64; 2]) -> SpectralTemplate {
        SpectralTemplate {
            template_id: "coverage".into(),
            wavelengths_m: vec![start_nm * 1e-9, end_nm * 1e-9],
            f_lambda_si: values.to_vec(),
            provenance: "fixture".into(),
        }
    }

    #[test]
    fn template_integration_rejects_partial_or_invalid_coverage() {
        assert!(integrate_template_photon_flux(
            &template(336.0, 650.0, [1.0, 1.0]),
            336e-9,
            650e-9
        )
        .is_ok());
        assert!(integrate_template_photon_flux(
            &template(400.0, 650.0, [1.0, 1.0]),
            336e-9,
            650e-9
        )
        .is_err());
        assert!(integrate_template_photon_flux(
            &template(336.0, 600.0, [1.0, 1.0]),
            336e-9,
            650e-9
        )
        .is_err());
        assert!(integrate_template_photon_flux(
            &template(336.0, 650.0, [-1.0, 1.0]),
            336e-9,
            650e-9
        )
        .is_err());
        assert!(integrate_template_photon_flux(
            &template(336.0, 650.0, [f64::NAN, 1.0]),
            336e-9,
            650e-9
        )
        .is_err());
    }

    #[test]
    fn broad_band_reconstruction_uses_response_and_zero_point() {
        let spectrum = template(300.0, 700.0, [1.0e-8, 1.0e-8]);
        let response = PhotometricBandResponse {
            band_id: "Hp".into(),
            wavelengths_m: vec![400e-9, 500e-9, 600e-9],
            throughput: vec![0.0, 1.0, 0.0],
            detector_convention: "photon_counting".into(),
            citation: "fixture".into(),
            sha256: "0".repeat(64),
        };
        let calibration = PhotometricBandCalibration {
            system: "Hipparcos".into(),
            band_id: "Hp".into(),
            zero_point_convention: "Vega".into(),
            reference_spectrum: "fixture-vega".into(),
            zero_point_photon_flux_ph_m2_s: 1.0e10,
            citation: "fixture".into(),
        };
        let flux0 =
            reconstruct_template_band_flux(&spectrum, &response, &calibration, 0.0, 336e-9, 650e-9)
                .unwrap();
        let flux5 =
            reconstruct_template_band_flux(&spectrum, &response, &calibration, 5.0, 336e-9, 650e-9)
                .unwrap();
        assert!((flux5 / flux0 - 0.01).abs() < 1e-12);
    }
}
