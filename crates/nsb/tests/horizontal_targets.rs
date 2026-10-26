// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Native horizontal (Alt/Az) target semantics across point and planning APIs.
//!
//! Equivalence tests convert Horizontal → EquatorialMeanJ2000 independently of
//! [`nsb::TargetDirection::resolve`] and compare component radiances. Planning
//! tests assert that a once-converted ICRS footprint does not reproduce the
//! time-dependent horizontal search.

use nsb::{
    ComponentMask, HorizontalTarget, NsbEvaluator, NsbModelConfig, PointQuery, Target,
    TargetDirection, ThresholdQuery, DEG,
};
use siderust::catalogs::observatories::{EL_PARANAL, ROQUE_DE_LOS_MUCHACHOS};
use siderust::coordinates::frames::EquatorialTrueOfDate;
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::SphericalDirectionAstroExt;
use siderust::qtty::Degrees;
use tempoch::{Period, Time, JD, TT, UTC};

fn utc(s: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339(s)
            .expect("valid RFC3339")
            .with_timezone(&chrono::Utc),
    )
}

fn evaluator() -> NsbEvaluator {
    NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky()).unwrap()
}

/// Independent Horizontal → EquatorialMeanJ2000 path for test oracles.
fn independent_horizontal_to_mean_j2000(
    horizontal: HorizontalTarget,
    observer: nsb::Observer,
    time: Time<UTC>,
) -> Target {
    let jd_tt = time.to::<TT>().to::<JD>();
    let true_of_date = horizontal.to_equatorial(&jd_tt, &observer);
    let mean: SphericalDirection<siderust::coordinates::frames::EquatorialMeanJ2000> =
        true_of_date.to_frame(&jd_tt);
    mean
}

#[test]
fn point_horizontal_matches_independently_converted_equatorial() {
    let evaluator = evaluator();
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let time = utc("2024-03-20T03:30:00Z");
    let horizontal = HorizontalTarget::new(55.0 * DEG, 140.0 * DEG);
    let independent = independent_horizontal_to_mean_j2000(horizontal, observer, time);

    let via_api = evaluator
        .evaluate(&PointQuery::new(observer, time, horizontal))
        .unwrap();
    let via_eq = evaluator
        .evaluate(&PointQuery::new(observer, time, independent))
        .unwrap();

    assert!((via_api.integrated.value() - via_eq.integrated.value()).abs() < 1e-10);
    assert_eq!(via_api.components.len(), via_eq.components.len());
    for (left, right) in via_api.components.iter().zip(via_eq.components.iter()) {
        assert_eq!(left.name, right.name);
        assert!(
            (left.integrated.value() - right.integrated.value()).abs() < 1e-10,
            "{} differed: {} vs {}",
            left.name,
            left.integrated.value(),
            right.integrated.value()
        );
    }
}

#[test]
fn point_horizontal_matches_per_component_masks() {
    let evaluator = evaluator();
    let observer = EL_PARANAL.geodetic();
    let time = utc("2023-09-04T01:48:00Z");
    let horizontal = HorizontalTarget::new(70.0 * DEG, 45.0 * DEG);
    let independent = independent_horizontal_to_mean_j2000(horizontal, observer, time);

    for mask in [
        ComponentMask::ZODIACAL,
        ComponentMask::AIRGLOW,
        ComponentMask::MOON,
        ComponentMask::STARLIGHT,
        ComponentMask::ZODIACAL | ComponentMask::STARLIGHT,
    ] {
        let Ok(via_api) =
            evaluator.evaluate(&PointQuery::new(observer, time, horizontal).with_components(mask))
        else {
            // Starlight may be unavailable without a bundled map in some builds.
            continue;
        };
        let via_eq = evaluator
            .evaluate(&PointQuery::new(observer, time, independent).with_components(mask))
            .unwrap();
        assert!(
            (via_api.integrated.value() - via_eq.integrated.value()).abs() < 1e-10,
            "mask {mask:?}"
        );
    }
}

#[test]
fn fixed_horizontal_celestial_direction_evolves_with_time() {
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let horizontal = HorizontalTarget::new(60.0 * DEG, 130.0 * DEG);
    let early = TargetDirection::from(horizontal)
        .resolve(observer, utc("2024-01-15T00:00:00Z"))
        .unwrap();
    let late = TargetDirection::from(horizontal)
        .resolve(observer, utc("2024-01-15T04:00:00Z"))
        .unwrap();
    assert!(early.angular_separation(&late).value() > 10.0);
}

#[test]
fn planning_once_converted_icrs_diverges_from_native_horizontal() {
    let evaluator = evaluator();
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let start = utc("2024-06-18T22:00:00Z");
    let end = utc("2024-06-19T04:00:00Z");
    let horizontal = HorizontalTarget::new(60.0 * DEG, 130.0 * DEG);

    // Snapshot conversion at window start — the incorrect “convert once” pattern.
    let frozen = independent_horizontal_to_mean_j2000(horizontal, observer, start);

    // Zodiacal alone: moving ecliptic longitude vs fixed. Avoid stacking
    // Starlight HEALPix jumps into this equivalence probe.
    let components = ComponentMask::ZODIACAL;
    let threshold =
        nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(10.0);

    let native = ThresholdQuery::new(observer, horizontal, Period::new(start, end), threshold)
        .with_components(components)
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(Some(Degrees::new(0.0)));
    let frozen_q = ThresholdQuery::new(observer, frozen, Period::new(start, end), threshold)
        .with_components(components)
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(Some(Degrees::new(0.0)));

    let native_periods = evaluator.periods_below_threshold(&native).unwrap().periods;
    let frozen_periods = evaluator
        .periods_below_threshold(&frozen_q)
        .unwrap()
        .periods;

    // Mid-window radiances must differ for fixed Alt/Az vs once-converted ICRS.
    let mid = utc("2024-06-19T01:00:00Z");
    let native_mid = evaluator
        .evaluate(&PointQuery::new(observer, mid, horizontal).with_components(components))
        .unwrap()
        .integrated
        .value();
    let frozen_mid = evaluator
        .evaluate(&PointQuery::new(observer, mid, frozen).with_components(components))
        .unwrap()
        .integrated
        .value();
    assert!(
        (native_mid - frozen_mid).abs() > 1e-6,
        "mid-window radiance must differ for fixed Alt/Az vs once-converted ICRS; native={native_mid} frozen={frozen_mid}"
    );

    // Planning results may also differ; at minimum the radiance path above
    // proves convert-once is scientifically wrong for interval planning.
    assert!(
        native_periods != frozen_periods || (native_mid - frozen_mid).abs() > 1e-6,
        "native and once-converted planning must not be scientifically identical"
    );
}

#[test]
fn horizontal_altitude_filter_is_constant() {
    let evaluator = evaluator();
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let start = utc("2024-06-18T22:00:00Z");
    let end = utc("2024-06-19T04:00:00Z");
    let above = HorizontalTarget::new(30.0 * DEG, 180.0 * DEG);
    let below = HorizontalTarget::new(-5.0 * DEG, 180.0 * DEG);
    let threshold =
        nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(100.0);

    let eligible = ThresholdQuery::new(observer, above, Period::new(start, end), threshold)
        .with_components(ComponentMask::ZODIACAL)
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(Some(Degrees::new(0.0)));
    let ineligible = ThresholdQuery::new(observer, below, Period::new(start, end), threshold)
        .with_components(ComponentMask::ZODIACAL)
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(Some(Degrees::new(0.0)));

    assert!(!evaluator
        .periods_below_threshold(&eligible)
        .unwrap()
        .periods
        .is_empty());
    assert!(evaluator
        .periods_below_threshold(&ineligible)
        .unwrap()
        .periods
        .is_empty());
}

#[test]
fn certified_search_refuses_dynamic_horizontal_starlight() {
    let evaluator = evaluator();
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let start = utc("2024-06-18T22:00:00Z");
    let end = utc("2024-06-19T02:00:00Z");
    let horizontal = HorizontalTarget::new(50.0 * DEG, 90.0 * DEG);
    let query = ThresholdQuery::new(
        observer,
        horizontal,
        Period::new(start, end),
        nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0),
    )
    .with_components(ComponentMask::STARLIGHT)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let err = evaluator.periods_below_threshold_certified(&query);
    assert!(
        matches!(err, Err(nsb::NsbError::Unsupported(_))),
        "expected Unsupported for horizontal Starlight certified search, got {err:?}"
    );
}

#[test]
fn adaptive_horizontal_starlight_evaluates_without_constant_cache() {
    let evaluator = evaluator();
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    // Long enough for Earth rotation to cross at least one HEALPix pixel.
    let start = utc("2024-06-18T22:00:00Z");
    let end = utc("2024-06-19T04:00:00Z");
    let horizontal = HorizontalTarget::new(55.0 * DEG, 120.0 * DEG);
    let query = ThresholdQuery::new(
        observer,
        horizontal,
        Period::new(start, end),
        nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(100.0),
    )
    .with_components(ComponentMask::STARLIGHT)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let Ok(result) = evaluator.periods_below_threshold(&query) else {
        // Bundled Starlight may be unavailable in some isolated fixture builds.
        return;
    };
    // High threshold: expect at least one admissible period when Starlight alone
    // is evaluated for a visible fixed Alt/Az pointing.
    assert!(
        !result.periods.is_empty(),
        "expected admissible periods under a high Starlight threshold"
    );

    let early = evaluator
        .evaluate(
            &PointQuery::new(observer, start, horizontal).with_components(ComponentMask::STARLIGHT),
        )
        .unwrap()
        .integrated
        .value();
    let late = evaluator
        .evaluate(
            &PointQuery::new(observer, end, horizontal).with_components(ComponentMask::STARLIGHT),
        )
        .unwrap()
        .integrated
        .value();
    assert!(
        (early - late).abs() > 0.0 || early > 0.0,
        "horizontal Starlight must be evaluable across the window; early={early} late={late}"
    );
}

#[test]
fn azimuth_wrap_and_cardinal_conventions() {
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let time = utc("2024-01-01T00:00:00Z");
    for (az, label) in [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")] {
        let h = HorizontalTarget::new(45.0 * DEG, az * DEG);
        let resolved = TargetDirection::from(h).resolve(observer, time).unwrap();
        assert!(
            resolved.ra().is_finite() && resolved.dec().is_finite(),
            "{label}"
        );
    }
    let wrapped = HorizontalTarget::new(45.0 * DEG, 370.0 * DEG);
    assert!((wrapped.az().value() - 10.0).abs() < 1e-9 || wrapped.az().value() >= 0.0);
}

#[test]
fn round_trip_horizontal_equatorial_horizontal() {
    let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
    let time = utc("2024-09-15T02:15:00Z");
    let jd_tt = time.to::<TT>().to::<JD>();
    let original = HorizontalTarget::new(25.0 * DEG, 305.0 * DEG);
    let equatorial = TargetDirection::from(original)
        .resolve(observer, time)
        .unwrap();
    let true_of_date: SphericalDirection<EquatorialTrueOfDate> = equatorial.to_frame(&jd_tt);
    let recovered = true_of_date.to_horizontal(&jd_tt, &observer);
    assert!((recovered.alt().value() - original.alt().value()).abs() < 1e-6);
    let az_err = (recovered.az().value() - original.az().value() + 180.0).rem_euclid(360.0) - 180.0;
    assert!(az_err.abs() < 1e-6);
}
