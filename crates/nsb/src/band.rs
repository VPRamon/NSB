// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Restricted integrals of the existing component spectra; no spectral completion.
use crate::units::length::Nanometers;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, NsbError, NsbEvaluator, PointQuery, Result};
use std::sync::Arc;

pub(crate) fn integrate_linear(x: &[f64], y: &[f64], lower: f64, upper: f64) -> f64 {
    x.windows(2)
        .zip(y.windows(2))
        .map(|(x, y)| {
            let a = lower.max(x[0]);
            let b = upper.min(x[1]);
            if b <= a {
                return 0.0;
            }
            let slope = (y[1] - y[0]) / (x[1] - x[0]);
            let ya = y[0] + slope * (a - x[0]);
            let yb = y[0] + slope * (b - x[0]);
            0.5 * (ya + yb) * (b - a)
        })
        .sum()
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
}
