//! Spherical line-of-sight numerical integration for vertical emissivity profiles.
//!
//! This module owns only the reference integrator. It consumes validated numeric
//! inputs and does not own persistence, schema parsing, model selection,
//! climatology, or Airglow spectral semantics.

use siderust::qtty::Kilometers;

/// Current implementation identifier for the reference spherical LOS integrator.
pub(crate) const VERTICAL_PROFILE_INTEGRATOR_VERSION: &str = "spherical-los-simpson-v1";
/// Mean spherical Earth radius used by the reference vertical-profile integrator.
///
/// Chosen to match the mean Earth radius baked into Siderust's Van Rhijn
/// implementation so thin-shell comparisons remain consistent.
pub(crate) const AIRGLOW_MEAN_EARTH_RADIUS_KM: Kilometers = Kilometers::new(6_371.0);
/// Production reference resolution per profile interval (must be even).
pub(crate) const VERTICAL_PROFILE_REFERENCE_SUBSTEPS: usize = 64;

/// Integrate `j(h(s)) ds` over every profile interval using composite Simpson.
///
/// With observer radius `r0 = R + h_obs` and above-horizon zenith angle `z`,
/// the spherical ray is
///
/// `h(s) = sqrt(r0² + s² + 2 r0 s cos(z)) - R`.
///
/// Interval endpoints are transformed exactly from altitude to path length,
/// avoiding a plane-parallel approximation and keeping the horizon finite.
///
/// `altitudes_km` must be strictly increasing and the same length as
/// `relative_emissivity`. Callers are expected to supply already-validated
/// profile samples.
pub(super) fn integrate_profile_los(
    altitudes_km: &[Kilometers],
    relative_emissivity: &[f64],
    observer_height_km: f64,
    zenith_rad: f64,
    substeps: usize,
) -> f64 {
    debug_assert_eq!(altitudes_km.len(), relative_emissivity.len());
    debug_assert!(altitudes_km.len() >= 2);

    let r0 = AIRGLOW_MEAN_EARTH_RADIUS_KM.value() + observer_height_km;
    let sin_z = zenith_rad.sin();
    let cos_z = zenith_rad.cos().max(0.0);
    let mut total = 0.0;

    for index in 0..altitudes_km.len() - 1 {
        let bin_low = altitudes_km[index].value();
        let bin_high = altitudes_km[index + 1].value();
        let low = bin_low.max(observer_height_km);
        if low >= bin_high {
            continue;
        }
        let s_low = distance_to_altitude(r0, low, sin_z, cos_z);
        let s_high = distance_to_altitude(r0, bin_high, sin_z, cos_z);
        let ds = (s_high - s_low) / substeps as f64;
        let mut weighted = 0.0;
        for step in 0..=substeps {
            let s = s_low + ds * step as f64;
            let radius = (r0 * r0 + s * s + 2.0 * r0 * s * cos_z).sqrt();
            let altitude = radius - AIRGLOW_MEAN_EARTH_RADIUS_KM.value();
            let fraction = ((altitude - bin_low) / (bin_high - bin_low)).clamp(0.0, 1.0);
            let emissivity = relative_emissivity[index]
                + fraction * (relative_emissivity[index + 1] - relative_emissivity[index]);
            let weight = if step == 0 || step == substeps {
                1.0
            } else if step % 2 == 0 {
                2.0
            } else {
                4.0
            };
            weighted += weight * emissivity;
        }
        total += ds * weighted / 3.0;
    }
    total
}

fn distance_to_altitude(r0: f64, altitude_km: f64, sin_z: f64, cos_z: f64) -> f64 {
    let radius = AIRGLOW_MEAN_EARTH_RADIUS_KM.value() + altitude_km;
    let discriminant = (radius * radius - r0 * r0 * sin_z * sin_z).max(0.0);
    (-r0 * cos_z + discriminant.sqrt()).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::super::vertical_profile::{
        AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
        VerticalEmissionProfileDefinition, VerticalProfileNormalization,
        VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
    };
    use crate::error::NsbError;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;
    use siderust::qtty::{Degrees, Kilometers, Meters, Nanometers};

    fn observer(height_m: f64) -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(12.345),
            Degrees::new(-43.21),
            Meters::new(height_m),
        )
    }

    fn profile(id: &str, altitudes: &[f64], emissivities: &[f64]) -> VerticalEmissionProfile {
        VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
            schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
            profile_id: id.into(),
            altitude_km: altitudes.iter().copied().map(Kilometers::new).collect(),
            relative_emissivity: emissivities.to_vec(),
            normalization: VerticalProfileNormalization::UnitVerticalIntegral,
            wavelength: AirglowWavelengthApplicability {
                min: Nanometers::new(300.0),
                max: Nanometers::new(650.0),
                band: "synthetic-300-650-nm".into(),
            },
            assumptions: "synthetic mathematical validation profile; not physical data".into(),
            provenance: "generated in deterministic NSB unit test".into(),
            license: "CC0-1.0 synthetic fixture".into(),
            validated_zenith: ValidatedZenithDomain {
                min: Degrees::new(0.0),
                max: Degrees::new(90.0),
            },
        })
        .unwrap()
    }

    #[test]
    fn vertical_profile_is_exactly_normalized_at_zenith() {
        let profile = profile("broad-triangle", &[75.0, 90.0, 110.0], &[0.0, 1.0, 0.0]);
        for height_m in [-400.0, 0.0, 2_400.0, 5_000.0] {
            assert_eq!(
                profile
                    .geometry_factor(observer(height_m), Degrees::new(0.0))
                    .unwrap()
                    .value(),
                1.0
            );
        }
    }

    #[test]
    fn profile_without_visible_emission_above_observer_fails_at_all_angles() {
        let profile = profile(
            "emission-below-observer",
            &[0.0, 1.0, 2.0],
            &[1.0, 0.0, 0.0],
        );
        let location = observer(1_500.0);
        for zenith in [0.0, 30.0, 90.0] {
            let error = profile
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("contains no visible emission above observer altitude"));
        }
    }

    #[test]
    fn zero_padding_above_visible_emitting_layer_remains_valid() {
        let profile = profile(
            "zero-padded-visible-layer",
            &[80.0, 90.0, 100.0, 120.0],
            &[0.0, 1.0, 0.0, 0.0],
        );
        let location = observer(2_000.0);
        assert_eq!(
            profile
                .geometry_factor(location, Degrees::new(0.0))
                .unwrap()
                .value(),
            1.0
        );
        for zenith in [30.0, 90.0] {
            let factor = profile
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap()
                .value();
            assert!(factor.is_finite() && factor > 0.0);
        }
    }

    #[test]
    fn thin_profile_converges_to_same_height_van_rhijn_shell() {
        use super::super::van_rhijn::VanRhijnConfig;
        use super::super::AirglowGeometryModel;

        let location = observer(0.0);
        let van_rhijn = AirglowGeometryModel::VanRhijn(VanRhijnConfig::default());
        let thin = profile(
            "thin-shell-90km-width-20m",
            &[89.99, 90.0, 90.01],
            &[0.0, 1.0, 0.0],
        );
        for (zenith, tolerance) in [
            (0.0, 0.0),
            (30.0, 2.0e-9),
            (60.0, 2.0e-8),
            (85.0, 2.0e-6),
            (90.0, 2.0e-5),
        ] {
            let expected = van_rhijn
                .geometry_factor(location, Degrees::new(zenith))
                .unwrap()
                .value();
            let actual = thin
                .geometry_factor_with_substeps(location, Degrees::new(zenith), 128)
                .unwrap()
                .value();
            let relative = ((actual - expected) / expected).abs();
            assert!(
                relative <= tolerance,
                "z={zenith}: vertical={actual}, van_rhijn={expected}, rel={relative}, tolerance={tolerance}"
            );
        }
    }

    #[test]
    fn representative_profiles_are_finite_positive_through_horizon() {
        let profiles = [
            profile("narrow", &[85.0, 90.0, 95.0], &[0.0, 1.0, 0.0]),
            profile("broad", &[75.0, 85.0, 100.0, 115.0], &[0.0, 0.8, 1.0, 0.0]),
            profile(
                "two-layer",
                &[75.0, 85.0, 90.0, 100.0, 110.0, 120.0],
                &[0.0, 1.0, 0.1, 0.2, 0.8, 0.0],
            ),
        ];
        for profile in profiles {
            let mut previous = 1.0;
            for zenith in [0.0, 30.0, 60.0, 75.0, 85.0, 89.0, 90.0] {
                let factor = profile
                    .geometry_factor(observer(2_000.0), Degrees::new(zenith))
                    .unwrap()
                    .value();
                assert!(factor.is_finite() && factor > 0.0);
                assert!(factor >= previous);
                previous = factor;
            }
        }
    }

    #[test]
    fn integration_converges_under_resolution_refinement() {
        let profile = profile(
            "asymmetric-broad",
            &[70.0, 78.0, 88.0, 97.0, 113.0],
            &[0.0, 0.25, 1.0, 0.35, 0.0],
        );
        let location = observer(1_700.0);
        let f16 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 16)
            .unwrap()
            .value();
        let f32 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 32)
            .unwrap()
            .value();
        let f64 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 64)
            .unwrap()
            .value();
        let f128 = profile
            .geometry_factor_with_substeps(location, Degrees::new(88.0), 128)
            .unwrap()
            .value();
        assert!((f64 - f128).abs() < (f32 - f64).abs());
        assert!((f32 - f64).abs() < (f16 - f32).abs());
        assert!(((f64 - f128) / f128).abs() < 1.0e-10);
    }

    #[test]
    fn observer_altitude_changes_vertical_profile_factor() {
        let profile = profile("altitude-test", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        let sea_level = profile
            .geometry_factor(observer(0.0), Degrees::new(80.0))
            .unwrap()
            .value();
        let mountain = profile
            .geometry_factor(observer(4_500.0), Degrees::new(80.0))
            .unwrap()
            .value();
        assert!(mountain > sea_level);
        assert!((mountain - sea_level) / sea_level > 1.0e-3);
    }

    #[test]
    fn arbitrary_longitude_and_latitude_are_supported() {
        let profile = profile("global-math", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        for location in [
            Geodetic::new_raw(Degrees::new(151.2), Degrees::new(-33.9), Meters::new(58.0)),
            Geodetic::new_raw(Degrees::new(-149.9), Degrees::new(61.2), Meters::new(350.0)),
            Geodetic::new_raw(Degrees::new(0.0), Degrees::new(0.0), Meters::new(0.0)),
        ] {
            let factor = profile
                .geometry_factor(location, Degrees::new(70.0))
                .unwrap()
                .value();
            assert!(factor.is_finite() && factor > 1.0);
        }
    }

    #[test]
    fn odd_substep_count_is_rejected() {
        let profile = profile("substeps", &[80.0, 90.0, 100.0], &[0.0, 1.0, 0.0]);
        let error = profile
            .geometry_factor_with_substeps(observer(0.0), Degrees::new(30.0), 3)
            .unwrap_err();
        assert!(matches!(error, NsbError::OutOfRange(_)));
    }
}
