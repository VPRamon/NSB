// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Restricted integrals of the existing component spectra; no spectral completion.
use crate::units::length::Nanometers;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, NsbError, NsbEvaluator, PointQuery, Result};
use std::sync::Arc;

pub(crate) fn integrate_linear(x: &[f64], y: &[f64], lower: f64, upper: f64) -> f64 {
    siderust::optica::spectrum::integrate::trapz_range(x, y, lower, upper)
}

impl NsbEvaluator {
    /// Integrate existing wavelength-resolved components over a top-hat sub-band.
    ///
    /// Units: photons cm^-2 ns^-1 sr^-1. Bounds must lie within 300–650 nm.
    /// This preserves the selected models, atmospheric propagation, solar activity
    /// and maturity. It does not add missing emission lines or starlight spectra.
    /// Starlight is unsupported because its integrated map has no recoverable SED.
    /// Moonlight is supported only when its existing evaluator returns physical zero;
    /// nonzero lunar spectra and KS91 band reconstruction remain unsupported.
    /// This is not FORS, Johnson, AB, Vega or other calibrated passband photometry.
    pub fn integrate_band(
        &self,
        query: &PointQuery,
        lower: Nanometers,
        upper: Nanometers,
    ) -> Result<Radiance> {
        crate::evaluator::validate_geometry(query.observer, query.target)?;
        if !lower.is_finite()
            || !upper.is_finite()
            || lower.value() < 300.0
            || upper.value() > 650.0
            || lower >= upper
        {
            return Err(NsbError::OutOfRange(
                "band requires 300 <= lower < upper <= 650 nm".into(),
            ));
        }
        if query.components.contains(ComponentMask::STARLIGHT) {
            return Err(NsbError::Unsupported(
                "starlight has no wavelength-resolved product".into(),
            ));
        }
        let mut total = Radiance::new(0.0);
        if query.components.contains(ComponentMask::MOON)
            && self
                .evaluate_moonlight(query.observer, query.time, query.target)?
                .integrated
                .value()
                != 0.0
        {
            return Err(NsbError::Unsupported(
                "nonzero moonlight sub-band is not implemented".into(),
            ));
        }
        if query.components.contains(ComponentMask::ZODIACAL) {
            total += self.zodiacal().compute_band(
                query.time,
                query.observer,
                query.target,
                lower,
                upper,
            )?;
        }
        if query.components.contains(ComponentMask::AIRGLOW) {
            let config = self.model_config();
            let solar = crate::solar_activity::resolve_f107(query.time, config.solar_activity())?;
            let profile = config.site_profile().resolve(query.observer);
            total += crate::components::airglow::Airglow::with_shared_continuum(
                query.observer,
                Arc::clone(self.airglow_continuum()),
            )
            .with_atmosphere(profile.atmosphere)
            .with_geometry(config.airglow_geometry().clone())
            .with_solar_radio_flux(solar.value)
            .with_scale(profile.airglow.scale)
            .compute_band(query.time, query.target, lower.value(), upper.value())?;
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_linear_spectrum_has_analytic_integral() {
        // y=2x+3, integral from 0.25 to 1.75 is 7.5.
        assert_eq!(
            integrate_linear(&[0.0, 1.0, 2.0], &[3.0, 5.0, 7.0], 0.25, 1.75),
            7.5
        );
    }
    #[test]
    fn clipped_integrals_match_independent_polynomial_oracle_at_boundaries() {
        let xs = [300.0, 350.0, 470.0, 650.0];
        let ys = xs.map(|x| 2.0 * x + 3.0);
        for (a, b) in [
            (300.0, 650.0),
            (350.0, 470.0),
            (350.25, 469.75),
            (300.0, 300.000001),
            (649.999999, 650.0),
        ] {
            // Average endpoint height times width is an independent exact oracle.
            let expected = (a + b + 3.0) * (b - a);
            let actual = integrate_linear(&xs, &ys, a, b);
            assert!((actual - expected).abs() <= 1e-12 * expected);
        }
    }

    fn query(components: ComponentMask, utc: &str) -> PointQuery {
        use chrono::{DateTime, Utc};
        let time = tempoch::Time::<tempoch::UTC>::from_chrono(
            DateTime::parse_from_rfc3339(utc)
                .unwrap()
                .with_timezone(&Utc),
        );
        PointQuery::new(
            siderust::catalogs::observatories::EL_PARANAL.geodetic(),
            time,
            crate::Target::new(10.0 * crate::DEG, -20.0 * crate::DEG),
        )
        .with_components(components)
    }

    #[test]
    fn supported_component_integrals_reproduce_full_band_and_add_subbands() {
        let evaluator = NsbEvaluator::new().unwrap();
        for mask in [
            ComponentMask::ZODIACAL,
            ComponentMask::AIRGLOW,
            ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
        ] {
            let q = query(mask, "2023-09-15T05:00:00Z");
            let full = evaluator
                .integrate_band(&q, Nanometers::new(300.0), Nanometers::new(650.0))
                .unwrap()
                .value();
            let original = evaluator.evaluate(&q).unwrap().integrated.value();
            assert!(full > 0.0);
            assert!((full - original).abs() < 1e-12 * original);
            let left = evaluator
                .integrate_band(&q, Nanometers::new(300.0), Nanometers::new(450.25))
                .unwrap()
                .value();
            let right = evaluator
                .integrate_band(&q, Nanometers::new(450.25), Nanometers::new(650.0))
                .unwrap()
                .value();
            assert!((full - left - right).abs() < 1e-12 * full);
        }
    }

    #[test]
    fn invalid_domains_and_irrecoverable_starlight_fail() {
        let evaluator = NsbEvaluator::new().unwrap();
        let q = query(ComponentMask::ZODIACAL, "2023-09-15T05:00:00Z");
        for (lower, upper) in [
            (299.0, 650.0),
            (300.0, 651.0),
            (500.0, 400.0),
            (400.0, 400.0),
            (f64::NAN, 500.0),
            (400.0, f64::INFINITY),
        ] {
            assert!(matches!(
                evaluator.integrate_band(&q, Nanometers::new(lower), Nanometers::new(upper)),
                Err(NsbError::OutOfRange(_))
            ));
        }
        let star = query(ComponentMask::STARLIGHT, "2023-09-15T05:00:00Z");
        assert!(matches!(
            evaluator.integrate_band(&star, Nanometers::new(360.0), Nanometers::new(570.0)),
            Err(NsbError::Unsupported(_))
        ));
    }

    #[test]
    fn physical_zero_moon_is_supported_and_nonzero_moon_is_not_reconstructed() {
        let evaluator = NsbEvaluator::new().unwrap();
        let dark = query(ComponentMask::MOON, "2023-09-15T05:00:00Z");
        assert_eq!(evaluator.evaluate(&dark).unwrap().integrated.value(), 0.0);
        assert_eq!(
            evaluator
                .integrate_band(&dark, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap()
                .value(),
            0.0
        );
        let bright = query(ComponentMask::MOON, "2023-09-29T03:00:00Z");
        assert!(evaluator.evaluate(&bright).unwrap().integrated.value() > 0.0);
        assert!(matches!(
            evaluator.integrate_band(&bright, Nanometers::new(360.0), Nanometers::new(570.0)),
            Err(NsbError::Unsupported(_))
        ));
    }

    #[test]
    fn day_airglow_and_below_horizon_zodiacal_preserve_existing_zeros() {
        let evaluator = NsbEvaluator::new().unwrap();
        let day = query(ComponentMask::AIRGLOW, "2023-09-15T15:00:00Z");
        assert_eq!(
            evaluator
                .integrate_band(&day, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap()
                .value(),
            0.0
        );
        let mut below = query(ComponentMask::ZODIACAL, "2023-09-15T05:00:00Z");
        below.target = crate::Target::new(190.0 * crate::DEG, 20.0 * crate::DEG);
        assert_eq!(evaluator.evaluate(&below).unwrap().integrated.value(), 0.0);
        assert_eq!(
            evaluator
                .integrate_band(&below, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap()
                .value(),
            0.0
        );
        let empty = query(ComponentMask::empty(), "2023-09-15T05:00:00Z");
        assert_eq!(
            evaluator
                .integrate_band(&empty, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap()
                .value(),
            0.0
        );
    }
}
