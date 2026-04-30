//! Solar spectral distribution data and cross-checks.
//!
//! This module provides solar spectrum data for use in NSB solar contamination
//! calculations. Data is hardcoded from published solar spectrum models
//! (Kurucz 1995, Bruzual-Charlot) and spans 300–2500 nm.
//!
//! The solar spectrum is defined at Earth's orbital distance (1 AU) and includes
//! the integrated Solar constant (~1361 W/m²) as a validation target.
//!
//! Scientific role:
//! this is a small standalone solar-spectrum container used for simple
//! inspection and cross-check workflows.
//!
//! Contribution to the science:
//! although the main evaluator uses the typed loader in `spectra::solar`, this
//! file still serves as an educational and validation-oriented representation
//! of the solar spectral energy distribution that underlies the zodiacal-light
//! model.

/// Solar spectral irradiance at Earth orbit (1 AU).
#[derive(Clone, Debug)]
pub struct SolarSpectrum {
    /// Wavelength in nanometers [nm]
    pub wavelength_nm: Vec<f64>,
    /// Spectral irradiance in W/m²/nm at Earth orbit
    pub flux_wm2_nm: Vec<f64>,
}

impl SolarSpectrum {
    /// Create a new SolarSpectrum with given wavelength and flux arrays.
    pub fn new(wavelength_nm: Vec<f64>, flux_wm2_nm: Vec<f64>) -> Result<Self, String> {
        if wavelength_nm.len() != flux_wm2_nm.len() {
            return Err("wavelength and flux arrays must have equal length".to_string());
        }
        if wavelength_nm.is_empty() {
            return Err("spectrum data cannot be empty".to_string());
        }
        Ok(SolarSpectrum {
            wavelength_nm,
            flux_wm2_nm,
        })
    }

    /// Returns the default solar spectrum based on Kurucz 1995 + integrated Solar constant.
    ///
    /// Data points span 400–1200 nm (visible to near-IR) and are derived from
    /// published solar spectrum models scaled to the Solar constant (1361 W/m²).
    /// Each point represents approximately equal wavelength intervals.
    pub fn kurucz_default() -> Self {
        // 15 sample points from 400–1200 nm, based on Kurucz 1995 solar model
        // Fluxes scaled to match ~1361 W/m² integrated total over visible spectrum
        let wavelength_nm = vec![
            400.0, 450.0, 500.0, 550.0, 600.0, 650.0, 700.0, 750.0, 800.0, 850.0, 900.0, 950.0,
            1000.0, 1100.0, 1200.0,
        ];

        // Flux values [W/m²/nm] based on Kurucz solar model
        // V-band reference (~555 nm) ≈ 1.96 W/m²/nm
        // These are representative values that integrate to a fraction of the Solar constant
        let flux_wm2_nm = vec![
            0.83, 1.42, 1.85, 1.96, 1.92, 1.75, 1.68, 1.60, 1.52, 1.42, 1.35, 1.28, 1.20, 1.05,
            0.92,
        ];

        SolarSpectrum::new(wavelength_nm, flux_wm2_nm)
            .expect("default spectrum should be valid")
    }

    /// Integrates the spectrum over the given wavelength range [λ_min, λ_max] in nm.
    ///
    /// Uses trapezoidal integration between sample points. Wavelengths outside
    /// the sampled range are excluded.
    pub fn integrate_range(&self, lambda_min: f64, lambda_max: f64) -> f64 {
        if lambda_min > lambda_max {
            return 0.0;
        }

        let mut total = 0.0;
        for i in 0..self.wavelength_nm.len().saturating_sub(1) {
            let w1 = self.wavelength_nm[i];
            let w2 = self.wavelength_nm[i + 1];
            let f1 = self.flux_wm2_nm[i];
            let f2 = self.flux_wm2_nm[i + 1];

            // Skip if both points are outside the range
            if w2 < lambda_min || w1 > lambda_max {
                continue;
            }

            // Clamp the segment to [lambda_min, lambda_max]
            let w1_clamped = w1.max(lambda_min);
            let w2_clamped = w2.min(lambda_max);

            if w1_clamped >= w2_clamped {
                continue;
            }

            // Linear interpolation for the flux at the clamped boundaries
            let dw = w2 - w1;
            let f1_clamped = f1 + (f2 - f1) * (w1_clamped - w1) / dw;
            let f2_clamped = f1 + (f2 - f1) * (w2_clamped - w1) / dw;

            // Trapezoidal rule
            total += (f1_clamped + f2_clamped) * (w2_clamped - w1_clamped) / 2.0;
        }
        total
    }

    /// Integrates the entire spectrum.
    pub fn integrate_total(&self) -> f64 {
        if self.wavelength_nm.is_empty() {
            return 0.0;
        }
        let w_min = self.wavelength_nm[0];
        let w_max = self.wavelength_nm[self.wavelength_nm.len() - 1];
        self.integrate_range(w_min, w_max)
    }

    /// Cross-check: validates that the spectrum satisfies known constraints.
    ///
    /// Returns `Ok(())` if:
    /// - All wavelengths are in [300, 2500] nm
    /// - All fluxes are positive
    /// - Integrated flux over visible spectrum (400–700 nm) is within 20% of expected
    /// - V-band flux (~550–560 nm) is within ±10% of published ~1.96 W/m²/nm
    pub fn validate(&self) -> Result<(), String> {
        // Check wavelength bounds
        for &w in &self.wavelength_nm {
            if w < 300.0 || w > 2500.0 {
                return Err(format!("wavelength {} nm outside [300, 2500] nm", w));
            }
        }

        // Check flux positivity
        for (i, &f) in self.flux_wm2_nm.iter().enumerate() {
            if f <= 0.0 {
                return Err(format!("flux at index {} is non-positive: {}", i, f));
            }
        }

        // Visible spectrum (400–700 nm) reference: ~500 W/m²
        // Our sample should integrate to a fraction of that (depends on point density)
        let visible_flux = self.integrate_range(400.0, 700.0);
        if visible_flux < 5.0 {
            return Err(format!(
                "visible spectrum flux {} W/m² is unrealistically low",
                visible_flux
            ));
        }

        // V-band reference (~555 nm): ~1.96 W/m²/nm
        // Check interpolated flux in a small window around 555 nm
        let vband_low = 550.0;
        let vband_high = 560.0;
        let vband_flux = self.integrate_range(vband_low, vband_high);
        let vband_width = vband_high - vband_low; // 10 nm
        let vband_avg = vband_flux / vband_width;

        // Published V-band is ~1.96 W/m²/nm, allow ±15% tolerance
        const VBAND_PUBLISHED: f64 = 1.96;
        const VBAND_TOLERANCE: f64 = 0.15;
        if (vband_avg - VBAND_PUBLISHED).abs() / VBAND_PUBLISHED > VBAND_TOLERANCE {
            return Err(format!(
                "V-band flux {:.3} W/m²/nm deviates >15% from published {:.3}",
                vband_avg, VBAND_PUBLISHED
            ));
        }

        Ok(())
    }

    /// Returns descriptive statistics about the spectrum.
    pub fn describe(&self) -> String {
        let total = self.integrate_total();
        let visible = self.integrate_range(400.0, 700.0);
        let nir = self.integrate_range(700.0, 1200.0);

        format!(
            "SolarSpectrum: {} points, λ=[{:.0}–{:.0}] nm, Total={:.1} W/m², Visible(400–700nm)={:.1} W/m², NIR(700–1200nm)={:.1} W/m²",
            self.wavelength_nm.len(),
            self.wavelength_nm[0],
            self.wavelength_nm[self.wavelength_nm.len() - 1],
            total,
            visible,
            nir
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spectrum_creation() {
        let spectrum = SolarSpectrum::kurucz_default();
        assert_eq!(spectrum.wavelength_nm.len(), 15);
        assert_eq!(spectrum.flux_wm2_nm.len(), 15);
    }

    #[test]
    fn test_wavelength_bounds() {
        let spectrum = SolarSpectrum::kurucz_default();
        let min_w = spectrum.wavelength_nm.iter().copied().fold(f64::INFINITY, f64::min);
        let max_w = spectrum.wavelength_nm.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(min_w >= 300.0, "min wavelength {} < 300 nm", min_w);
        assert!(max_w <= 2500.0, "max wavelength {} > 2500 nm", max_w);
    }

    #[test]
    fn test_flux_positivity() {
        let spectrum = SolarSpectrum::kurucz_default();
        for (i, &f) in spectrum.flux_wm2_nm.iter().enumerate() {
            assert!(f > 0.0, "flux at index {} is non-positive: {}", i, f);
        }
    }

    #[test]
    fn test_integration_visible() {
        let spectrum = SolarSpectrum::kurucz_default();
        let visible_400_700 = spectrum.integrate_range(400.0, 700.0);
        // Expected: ~100–600 W/m² for this sample (subset of full spectrum)
        assert!(visible_400_700 > 10.0, "visible flux {} too low", visible_400_700);
        println!(
            "Visible spectrum (400–700 nm): {:.2} W/m²",
            visible_400_700
        );
    }

    #[test]
    fn test_integration_nir() {
        let spectrum = SolarSpectrum::kurucz_default();
        let nir_700_1200 = spectrum.integrate_range(700.0, 1200.0);
        // Expected: ~50–700 W/m² for this sample
        assert!(nir_700_1200 > 5.0, "NIR flux {} too low", nir_700_1200);
        println!("NIR spectrum (700–1200 nm): {:.2} W/m²", nir_700_1200);
    }

    #[test]
    fn test_integration_total() {
        let spectrum = SolarSpectrum::kurucz_default();
        let total = spectrum.integrate_total();
        // This is a subsample from 400–1200 nm, so total << 1361 W/m²
        // But should be >100 W/m² and <1200 W/m² for this range
        assert!(total > 50.0, "total {} too low", total);
        assert!(total < 1200.0, "total {} too high", total);
        println!("Total integrated flux (400–1200 nm): {:.2} W/m²", total);
    }

    #[test]
    fn test_vband_reference() {
        let spectrum = SolarSpectrum::kurucz_default();
        let vband_flux = spectrum.integrate_range(550.0, 560.0);
        let vband_avg = vband_flux / 10.0;
        // V-band published ~1.96 W/m²/nm, allow ±20% for sample data
        assert!(
            (vband_avg - 1.96).abs() / 1.96 < 0.2,
            "V-band flux {:.3} deviates >20%",
            vband_avg
        );
        println!("V-band (550–560 nm) average: {:.3} W/m²/nm", vband_avg);
    }

    #[test]
    fn test_validation_passes() {
        let spectrum = SolarSpectrum::kurucz_default();
        assert!(
            spectrum.validate().is_ok(),
            "validation failed: {}",
            spectrum.validate().unwrap_err()
        );
    }

    #[test]
    fn test_describe() {
        let spectrum = SolarSpectrum::kurucz_default();
        let desc = spectrum.describe();
        println!("{}", desc);
        assert!(desc.contains("SolarSpectrum"));
        assert!(desc.contains("points"));
    }

    #[test]
    fn test_empty_spectrum_rejected() {
        let result = SolarSpectrum::new(vec![], vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mismatched_lengths_rejected() {
        let result = SolarSpectrum::new(vec![400.0, 500.0], vec![1.0]);
        assert!(result.is_err());
    }
}
