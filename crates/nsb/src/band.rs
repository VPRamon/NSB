// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Restricted integrals of the existing component spectra; no spectral completion.
use crate::evaluator::{airglow_selection_metadata, moonlight_metadata, zodiacal_metadata};
use crate::solar_activity::ResolvedSolarActivity;
use crate::units::length::Nanometers;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, NsbError, NsbEvaluator, PointQuery, Result};
use std::sync::Arc;

pub(crate) fn integrate_linear(x: &[f64], y: &[f64], lower: f64, upper: f64) -> f64 {
    siderust::optica::spectrum::integrate::trapz_range(x, y, lower, upper)
}

/// A component integral over the explicitly reported restricted band.
///
/// Uses the selected component's existing propagation convention. In particular,
/// this is not an unattenuated source to feed through atmosphere a second time.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RestrictedBandComponent {
    /// Stable component name.
    pub name: &'static str,
    /// Band-integrated photon radiance, photons/(cm2 ns sr).
    pub integrated: Radiance,
    /// Model/configuration provenance; descriptor metadata, not a full-band result.
    pub metadata: crate::NsbComponentMetadata,
    /// Band statistical uncertainty; unknown for these component paths.
    pub statistical_uncertainty: Option<Radiance>,
    /// Band systematic uncertainty; unknown is not zero.
    pub systematic_uncertainty: Option<Radiance>,
}

/// Explicit band bounds and component contributions, without B/V proxy fields.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RestrictedBandResult {
    /// Lower wavelength boundary.
    pub lower: Nanometers,
    /// Upper wavelength boundary.
    pub upper: Nanometers,
    /// Sum of selected contributions, photons/(cm2 ns sr).
    pub integrated: Radiance,
    /// Selected contributions, including physically zero supported components.
    pub components: Vec<RestrictedBandComponent>,
}

// Only supported physical contributions reach the result builder. Borrowing the
// resolved solar record avoids a large enum or cloning it on the scalar path.
enum BandContribution<'a> {
    Zodiacal(Radiance),
    Airglow(Radiance, &'a ResolvedSolarActivity),
    Moon,
}

impl NsbEvaluator {
    /// Resolve a restricted-band integral into its selected physical components.
    ///
    /// Applies exactly the same domain and unsupported-component gates as
    /// [`Self::integrate_band`]. No partial successful result is returned when a
    /// requested component is unsupported. No continuous spectrum, line completion,
    /// instrument response or calibrated total-sky observable is implied.
    /// Propagation follows the evaluator configuration (including the explicit
    /// ZodiacalExtinction::None option); callers must not apply extinction again.
    /// Metadata follows describe_components semantics. Missing band uncertainties
    /// remain None; full-band uncertainties cannot be transferred to a sub-band.
    pub fn integrate_band_components(
        &self,
        query: &PointQuery,
        lower: Nanometers,
        upper: Nanometers,
    ) -> Result<RestrictedBandResult> {
        let mut components = Vec::with_capacity(query.components.bits().count_ones() as usize);
        let config = self.model_config();
        let integrated = self.integrate_band_with(query, lower, upper, |contribution| {
            let (name, integrated, metadata) = match contribution {
                BandContribution::Zodiacal(value) => (
                    "zodiacal",
                    value,
                    zodiacal_metadata(config.zodiacal_model(), config.zodiacal_extinction()),
                ),
                BandContribution::Airglow(value, solar) => (
                    "airglow",
                    value,
                    airglow_selection_metadata(
                        self.airglow_resolved(),
                        config.site_profile(),
                        query.observer,
                        config.airglow_geometry(),
                        Some(solar),
                    ),
                ),
                BandContribution::Moon => (
                    "moon",
                    Radiance::new(0.0),
                    moonlight_metadata(
                        config.moonlight_model(),
                        config.site_profile(),
                        query.observer,
                    ),
                ),
            };
            components.push(RestrictedBandComponent {
                name,
                integrated,
                metadata,
                statistical_uncertainty: None,
                systematic_uncertainty: None,
            });
        })?;
        Ok(RestrictedBandResult {
            lower,
            upper,
            integrated,
            components,
        })
    }

    /// Integrate existing wavelength-resolved components over a top-hat sub-band.
    ///
    /// Units: photons cm^-2 ns^-1 sr^-1. Bounds must lie within 300–650 nm.
    /// This preserves the selected models, atmospheric propagation, solar activity
    /// and maturity. It does not add missing emission lines or invent spectra.
    /// Starlight maps can expose native TOA pixel-band integrals, but those
    /// cannot be added to ground-level component totals without a validated
    /// atmospheric/instrumental transport model. Query them on StarlightMap.
    /// Moonlight is supported only when its existing evaluator returns physical zero;
    /// nonzero lunar spectra and KS91 band reconstruction remain unsupported.
    /// This is not FORS, Johnson, AB, Vega or other calibrated passband photometry.
    pub fn integrate_band(
        &self,
        query: &PointQuery,
        lower: Nanometers,
        upper: Nanometers,
    ) -> Result<Radiance> {
        self.integrate_band_with(query, lower, upper, |_| {})
    }

    // Gate geometry/band/support before emitting contributions. Evaluation errors
    // return no public result. Keep the historical Zodiacal-then-Airglow sum order;
    // physical-zero Moon is checked once up front and contributes no arithmetic.
    fn integrate_band_with(
        &self,
        query: &PointQuery,
        lower: Nanometers,
        upper: Nanometers,
        mut emit: impl FnMut(BandContribution<'_>),
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
                "starlight TOA maps cannot be added to ground-level restricted-band NSB without validated atmospheric transport".into(),
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
            let value = self.zodiacal().compute_band(
                query.time,
                query.observer,
                query.target,
                lower,
                upper,
            )?;
            total += value;
            emit(BandContribution::Zodiacal(value));
        }
        if query.components.contains(ComponentMask::AIRGLOW) {
            let config = self.model_config();
            let solar = crate::solar_activity::resolve_f107(query.time, config.solar_activity())?;
            let profile = config.site_profile().resolve(query.observer);
            let value = crate::components::airglow::Airglow::with_shared_continuum(
                query.observer,
                Arc::clone(self.airglow_continuum()),
            )
            .with_atmosphere(profile.atmosphere)
            .with_geometry(config.airglow_geometry().clone())
            .with_solar_radio_flux(solar.value)
            .with_scale(profile.airglow.scale)
            .with_calibration(config.site_profile().airglow_profile.clone())
            .compute_band(query.time, query.target, lower.value(), upper.value())?;
            total += value;
            emit(BandContribution::Airglow(value, &solar));
        }
        if query.components.contains(ComponentMask::MOON) {
            emit(BandContribution::Moon);
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

    #[test]
    fn experimental_spectral_starlight_uses_exact_band_without_default_relabeling() {
        use crate::components::starlight::StarlightMap;
        use crate::{NsbModelConfig, StarlightProduct};
        use sha2::{Digest, Sha256};

        let value = serde_json::json!({
            "schema":"nsb-starlight-spectral-map-v2",
            "product_id":"synthetic-starlight-2bins",
            "band_nm":[360,420],"bin_edges_nm":[360,390,420],
            "nside":1,"ordering":"ring","coordinate_frame":"galactic",
            "domain":"top-of-atmosphere","units":"photon/(cm2 ns sr)",
            "wavelength_convention":"vacuum","maturity":"experimental",
            "provenance":{
                "source_catalogue":"test-only",
                "source_license":"test-only",
                "source_selection":"synthetic",
                "generation_method":"unit test",
                "limitations":["not a physical sky model"]
            },
            "catalogue_rows":12,"admitted_sources":12,"excluded_sources":{},
            "pixels":(0..12).map(|i| serde_json::json!({
                "pixel":i,"sources":1,"radiance":[1.0,2.0],
                "statistical_sigma":[0.1,0.2],
                "integrated_statistical_sigma":0.25
            })).collect::<Vec<_>>()
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let sha: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let map = StarlightMap::from_json_bytes(&bytes, &sha).unwrap();
        assert_eq!(map.band_nm(), [360, 420]);
        use siderust::coordinates::spherical;
        let galactic = spherical::Direction::<siderust::coordinates::frames::Galactic>::new(
            crate::units::angular::Degrees::new(0.0),
            crate::units::angular::Degrees::new(0.0),
        )
        .to_cartesian();
        let native = map
            .integrate_band(galactic, Nanometers::new(360.0), Nanometers::new(420.0))
            .unwrap();
        assert_eq!(native.0.value(), 3.0);
        let config = NsbModelConfig::generic_clear_sky()
            .with_starlight_product(StarlightProduct::with_experimental_map(map));
        let evaluator = NsbEvaluator::with_config(config).unwrap();
        let q = query(ComponentMask::STARLIGHT, "2023-09-15T05:00:00Z");
        assert!(matches!(
            evaluator.integrate_band(&q, Nanometers::new(360.0), Nanometers::new(420.0)),
            Err(NsbError::Unsupported(_))
        ));
        assert!(matches!(
            evaluator.evaluate(&q),
            Err(NsbError::Unsupported(_))
        ));
        assert!(matches!(
            evaluator.integrate_band(&q, Nanometers::new(375.0), Nanometers::new(420.0)),
            Err(NsbError::Unsupported(_))
        ));
    }
    #[test]
    fn restricted_components_preserve_sum_provenance_and_unknown_uncertainty() {
        let evaluator = NsbEvaluator::new().unwrap();
        let q = query(
            ComponentMask::ZODIACAL | ComponentMask::AIRGLOW | ComponentMask::MOON,
            "2023-09-15T05:00:00Z",
        );
        let lo = Nanometers::new(360.0);
        let hi = Nanometers::new(570.0);
        let result = evaluator.integrate_band_components(&q, lo, hi).unwrap();
        assert_eq!((result.lower, result.upper), (lo, hi));
        assert_eq!(
            result.integrated,
            evaluator.integrate_band(&q, lo, hi).unwrap()
        );
        assert_eq!(result.components.len(), 3);
        let sum: f64 = result.components.iter().map(|c| c.integrated.value()).sum();
        assert_eq!(sum, result.integrated.value());
        for component in &result.components {
            assert!(component.statistical_uncertainty.is_none());
            assert!(component.systematic_uncertainty.is_none());
            assert!(!component.metadata.provenance.is_empty());
        }
        assert_eq!(result.components[2].integrated.value(), 0.0);
        assert!(result.components[1].metadata.solar_activity.is_some());
        let empty = q.clone().with_components(ComponentMask::empty());
        let result = evaluator.integrate_band_components(&empty, lo, hi).unwrap();
        assert!(result.components.is_empty());
        assert_eq!(result.integrated.value(), 0.0);
        let unsupported = q.with_components(ComponentMask::ZODIACAL | ComponentMask::STARLIGHT);
        assert!(matches!(
            evaluator.integrate_band_components(&unsupported, lo, hi),
            Err(NsbError::Unsupported(_))
        ));
        assert!(evaluator.integrate_band_components(&empty, hi, lo).is_err());
        let moon = query(ComponentMask::MOON, "2023-09-29T03:00:00Z");
        assert!(matches!(
            evaluator.integrate_band_components(&moon, lo, hi),
            Err(NsbError::Unsupported(_))
        ));
    }
    #[test]
    fn scalar_structured_and_full_band_paths_preserve_composition_and_partitioning() {
        let evaluator = NsbEvaluator::new().unwrap();
        for bits in [0_u8, 1, 4, 5, 8, 9, 12, 13] {
            let q = query(
                ComponentMask::from_bits(bits).unwrap(),
                "2023-09-15T05:00:00Z",
            );
            for (lo, hi) in [
                (300.0, 650.0),
                (360.0, 570.0),
                (300.0, 300.000001),
                (649.999999, 650.0),
            ] {
                let lo = Nanometers::new(lo);
                let hi = Nanometers::new(hi);
                let out = evaluator.integrate_band_components(&q, lo, hi).unwrap();
                assert_eq!(
                    out.integrated,
                    evaluator.integrate_band(&q, lo, hi).unwrap()
                );
                assert_eq!(
                    out.components
                        .iter()
                        .map(|c| c.integrated.value())
                        .sum::<f64>(),
                    out.integrated.value()
                );
                assert_eq!(out.components.len(), bits.count_ones() as usize);
                for c in &out.components {
                    assert!(c.statistical_uncertainty.is_none());
                    assert!(c.systematic_uncertainty.is_none());
                }
                if lo.value() == 300.0 && hi.value() == 650.0 {
                    let original = evaluator.evaluate(&q).unwrap();
                    assert!(
                        (out.integrated.value() - original.integrated.value()).abs()
                            <= 1e-12 * original.integrated.value()
                    );
                }
            }
            let parts: Vec<_> = (0..7)
                .map(|i| {
                    evaluator
                        .integrate_band_components(
                            &q,
                            Nanometers::new(360.0 + f64::from(i) * 30.0),
                            Nanometers::new(390.0 + f64::from(i) * 30.0),
                        )
                        .unwrap()
                })
                .collect();
            let whole = evaluator
                .integrate_band_components(&q, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap();
            let sum: f64 = parts.iter().map(|p| p.integrated.value()).sum();
            assert!((sum - whole.integrated.value()).abs() <= 1e-12 * whole.integrated.value());
            for (i, component) in whole.components.iter().enumerate() {
                let sum: f64 = parts
                    .iter()
                    .map(|p| p.components[i].integrated.value())
                    .sum();
                assert!(
                    (sum - component.integrated.value()).abs()
                        <= 1e-12 * component.integrated.value()
                );
            }
        }
        // Public scalar reference for Automatic Oxygen (v0.2.0); continuum-only was 0.10387771910979773 at PR271.
        let q = query(
            ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
            "2023-09-15T05:00:00Z",
        );
        let value = evaluator
            .integrate_band(&q, Nanometers::new(360.0), Nanometers::new(570.0))
            .unwrap()
            .value();
        // Automatic Airglow is PALACE Oxygen (v0.2.0); continuum-only golden was 0.10387771910979773.
        assert!((value - 0.120_065_033_165_254).abs() < 1e-14);
    }

    #[test]
    fn restricted_provenance_uses_the_applied_solar_record_and_extinction_selection() {
        use crate::units::SolarFluxUnits;
        use crate::{NsbModelConfig, ZodiacalExtinction};
        let q = query(
            ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
            "2023-09-15T05:00:00Z",
        );
        for extinction in [ZodiacalExtinction::None, ZodiacalExtinction::Noll2012Approx] {
            let evaluator = NsbEvaluator::with_config(
                NsbModelConfig::generic_clear_sky()
                    .with_zodiacal_extinction(extinction)
                    .with_solar_radio_flux(SolarFluxUnits::new(99.0)),
            )
            .unwrap();
            let out = evaluator
                .integrate_band_components(&q, Nanometers::new(360.0), Nanometers::new(570.0))
                .unwrap();
            let full = evaluator.evaluate(&q).unwrap();
            let descriptors = evaluator
                .describe_components(q.observer, q.components)
                .unwrap();
            assert_eq!(
                out.components[0].metadata.provenance,
                descriptors[0].metadata.provenance
            );
            assert_eq!(
                out.components[0].metadata.provenance,
                full.components[0].metadata.provenance
            );
            assert_eq!(
                out.components[0].metadata.status,
                full.components[0].metadata.status
            );
            let air = &out.components[1].metadata;
            let solar = air.solar_activity.as_ref().unwrap();
            assert_eq!(solar.value.value(), 99.0);
            assert_eq!(solar.resolution_step, "explicit-override");
            assert_eq!(
                air.solar_activity,
                full.components[1].metadata.solar_activity
            );
            assert!(air.provenance.contains(&solar.provenance_fragment()));
            assert_eq!(air.status, full.components[1].metadata.status);
            assert_eq!(
                air.airglow_selection,
                full.components[1].metadata.airglow_selection
            );
            // A full-band evaluation outcome is not relabelled as a sub-band one.
            assert!(air.airglow_evaluation.is_none());
        }
    }

    #[test]
    fn solar_resolution_errors_and_invalid_geometry_reject_the_entire_band_result() {
        use crate::units::SolarFluxUnits;
        use crate::NsbModelConfig;
        let q = query(
            ComponentMask::ZODIACAL | ComponentMask::AIRGLOW,
            "2023-09-15T05:00:00Z",
        );
        let lo = Nanometers::new(360.0);
        let hi = Nanometers::new(570.0);
        for flux in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let evaluator = NsbEvaluator::with_config(
                NsbModelConfig::generic_clear_sky()
                    .with_solar_radio_flux(SolarFluxUnits::new(flux)),
            )
            .unwrap();
            assert!(matches!(
                evaluator.integrate_band_components(&q, lo, hi),
                Err(NsbError::OutOfRange(_))
            ));
            assert!(matches!(
                evaluator.integrate_band(&q, lo, hi),
                Err(NsbError::OutOfRange(_))
            ));
            // Unselected Airglow must not resolve or validate an unrelated source.
            let only_zodiacal = q.clone().with_components(ComponentMask::ZODIACAL);
            assert!(evaluator
                .integrate_band_components(&only_zodiacal, lo, hi)
                .is_ok());
        }
        let evaluator = NsbEvaluator::new().unwrap();
        let mut invalid = q;
        invalid.observer.height = crate::units::length::Meters::new(f64::NAN);
        assert!(matches!(
            evaluator.integrate_band_components(&invalid, lo, hi),
            Err(NsbError::OutOfRange(_))
        ));
        assert!(matches!(
            evaluator.integrate_band(&invalid, lo, hi),
            Err(NsbError::OutOfRange(_))
        ));
    }
}
