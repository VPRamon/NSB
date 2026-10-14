// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::{
    bounds::BoundOutcome,
    discovery::{self, Limits},
    filters, intervals as scan, prepare, threshold,
};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, MoonlightModel, NsbEvaluator, Target, ThresholdQuery, DEG};
use siderust::qtty::{Days, Seconds};
use siderust::time::{Interval, ModifiedJulianDate as Mjd};
use tempoch::{Period, Time, UTC};

fn extent() -> Interval<Mjd> {
    Interval::new(Mjd::new(60_000.0), Mjd::new(60_000.0 + 600.0 / 86400.0))
}
fn seconds(t: Mjd) -> f64 {
    (t.raw().value() - 60_000.0) * 86400.0
}

#[test]
fn adaptive_resolves_close_pair_that_600_second_scan_misses() {
    let w = extent();
    let f = |t| Ok(Radiance::new(1.0 - (seconds(t) - 271.25).powi(2) / 4.0));
    let old = super::reference_scan::above_threshold_periods(
        w,
        Days::new(600.0 / 86400.0),
        &f,
        Radiance::new(0.5),
    )
    .unwrap();
    assert!(old.is_empty());
    let (out, cost) = discovery::search(
        w,
        Radiance::new(0.5),
        f,
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    let width = 2.0_f64.sqrt();
    assert!((seconds(out[0].end) - (271.25 - width)).abs() <= 0.1);
    assert!((seconds(out[1].start) - (271.25 + width)).abs() <= 0.1);
    assert!(seconds(out[0].end) <= 271.25 - width);
    assert!(seconds(out[1].start) >= 271.25 + width);
    assert!(cost.refinements >= 2 && cost.extrema >= 1);
}

#[test]
fn adaptive_subdivision_finds_narrow_grazing_excursion_of_smooth_peak() {
    let f = |t| {
        Ok(Radiance::new(
            (-(seconds(t) - 271.25).powi(2) / 10_000.0).exp(),
        ))
    };
    let threshold = Radiance::new(0.9999);
    assert!(super::reference_scan::above_threshold_periods(
        extent(),
        Days::new(600.0 / 86400.0),
        &f,
        threshold
    )
    .unwrap()
    .is_empty());
    let (out, cost) = discovery::search(
        extent(),
        threshold,
        f,
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    let half = (-10_000.0 * 0.9999_f64.ln()).sqrt();
    assert!((seconds(out[0].end) - (271.25 - half)).abs() <= 0.1);
    assert!((seconds(out[1].start) - (271.25 + half)).abs() <= 0.1);
    assert!(cost.subdivisions > 0);
}

#[test]
fn adaptive_extrema_flat_equality_and_multiple_events() {
    let w = extent();
    for (peak, expected) in [(0.50000001, 2), (0.49999999, 1)] {
        let f = |t| Ok(Radiance::new(peak - (seconds(t) - 271.25).powi(2) * 1e-5));
        let (out, _) = discovery::search(
            w,
            Radiance::new(0.5),
            f,
            BoundOutcome::Unsupported,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(out.len(), expected, "peak {peak}");
    }
    let tangent = |t| Ok(Radiance::new(0.5 - (seconds(t) - 271.25).powi(2) * 1e-5));
    let error = discovery::search(
        w,
        Radiance::new(0.5),
        tangent,
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("extremum indistinguishable"));
    let (flat, _) = discovery::search(
        w,
        Radiance::new(0.5),
        |_| Ok(Radiance::new(0.5)),
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(flat, vec![w]);
    let f = |t| {
        let x = seconds(t) / 600.0;
        Ok(Radiance::new(
            (x - 0.13) * (x - 0.17) * (x - 0.63) * (x - 0.67),
        ))
    };
    let (out, _) = discovery::search(
        w,
        Radiance::new(0.0),
        f,
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(out.len(), 2);
    for (p, (a, b)) in out.iter().zip([(78.0, 102.0), (378.0, 402.0)]) {
        assert!((seconds(p.start) - a).abs() <= 0.1);
        assert!((seconds(p.end) - b).abs() <= 0.1);
    }
}

#[test]
fn adaptive_errors_and_budget_exhaustion_return_no_partial_schedule() {
    // Relative time coordinates resolve adjacent absolute-MJD floats. Only
    // genuinely collapsed local nodes must fail, rather than imposing the
    // former absolute-MJD resolution on the higher-precision upstream type.
    let origin = Mjd::new(0.0);
    let adjacent = Interval::new(origin, origin + Days::new(f64::from_bits(1)));
    assert!(discovery::search(
        adjacent,
        Radiance::new(0.5),
        |_| Ok(Radiance::new(0.0)),
        BoundOutcome::Unsupported,
        Limits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("fitting nodes collapse"));
    for limits in [
        Limits {
            exact: 0,
            ..Limits::default()
        },
        Limits {
            fits: 0,
            ..Limits::default()
        },
        Limits {
            depth: 0,
            ..Limits::default()
        },
    ] {
        let w = if limits.depth == 0 {
            Interval::new(Mjd::new(60000.0), Mjd::new(60001.0))
        } else {
            extent()
        };
        assert!(discovery::search(
            w,
            Radiance::new(0.5),
            |_| Ok(Radiance::new(0.0)),
            BoundOutcome::Unsupported,
            limits
        )
        .is_err());
    }
    assert!(discovery::search(
        extent(),
        Radiance::new(0.5),
        |_| Ok(Radiance::new(f64::NAN)),
        BoundOutcome::Unsupported,
        Limits::default()
    )
    .is_err());
    let failure = discovery::search(
        extent(),
        Radiance::new(0.5),
        |_| Err(crate::NsbError::OutOfRange("physical failure".into())),
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap_err();
    assert!(failure.to_string().contains("physical failure"));
}

fn query(bits: u8) -> ThresholdQuery {
    let start = Time::<UTC>::from_chrono(
        "2026-01-03T00:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, start + Seconds::new(86400.0)),
        Radiance::new(0.25),
    )
    .with_components(ComponentMask::from_bits(bits).unwrap())
    .with_target_altitude_floor(Some(crate::units::angular::Degrees::new(20.0)))
}

pub(super) fn reference(
    evaluator: &NsbEvaluator,
    prepared: &super::types::PreparedThresholdQuery,
    query: &ThresholdQuery,
    step: f64,
) -> Vec<Interval<Mjd>> {
    let mut out = Vec::new();
    for w in super::filters::threshold_windows(prepared, true) {
        let above = super::reference_scan::above_threshold_periods_precise(
            w,
            Days::new(step / 86400.0),
            &|t| threshold::evaluate_integrated(evaluator, prepared, t),
            query.threshold,
        )
        .unwrap();
        out.extend(super::reference_scan::complement_periods(w, &above));
    }
    scan::coalesce_periods(&mut out);
    out
}

pub(super) fn assert_matches(actual: &[Interval<Mjd>], reference: &[Interval<Mjd>], label: &str) {
    if actual.len() != reference.len() && actual.len() < 10 && reference.len() < 10 {
        eprintln!(
            "{label}: adaptive {:?}; reference {:?}",
            actual
                .iter()
                .map(|w| (w.start.raw().value(), w.end.raw().value()))
                .collect::<Vec<_>>(),
            reference
                .iter()
                .map(|w| (w.start.raw().value(), w.end.raw().value()))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(actual.len(), reference.len(), "period count {label}");
    for (a, b) in actual.iter().zip(reference) {
        // Legacy/fine scanner uses 0.864 s midpoint roots; adaptive uses an
        // accepted-side 0.1 s bracket. Their combined maximum discrepancy is
        // 0.432+0.1 s. Never relax this to hide discovery differences.
        for (x, y) in [(a.start, b.start), (a.end, b.end)] {
            assert!(
                (x.raw().value() - y.raw().value()).abs() * 86400.0 <= 0.533,
                "boundary {label}: {x:?} vs {y:?}"
            );
        }
    }
}

#[test]
fn real_adaptive_all_masks_both_moon_models_match_fine_reference() {
    for moon in [
        MoonlightModel::KrisciunasSchaefer1991,
        MoonlightModel::Jones2013Spectral,
    ] {
        let evaluator = super::test_fixtures::evaluator_with_moon(false, true, moon);
        for bits in 0..=15 {
            let q = query(bits);
            let context = evaluator.prepare_site_window_context(&q).unwrap();
            let prepared = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
            let fine = reference(&evaluator, &prepared, &q, 30.0);
            let coarse = reference(&evaluator, &prepared, &q, 600.0);
            let result = evaluator
                .periods_below_threshold_adaptive_with_context(&context, &q)
                .unwrap_or_else(|e| panic!("mask {bits}, model {moon:?}: {e}"));
            let actual: Vec<_> = result
                .periods
                .iter()
                .copied()
                .map(scan::utc_period_to_tt_mjd)
                .collect();
            assert_matches(
                &actual,
                &fine,
                &format!("mask {bits}, model {moon:?}, fine"),
            );
            assert_matches(
                &actual,
                &coarse,
                &format!("mask {bits}, model {moon:?}, coarse"),
            );
            if bits == ComponentMask::ALL.bits() {
                assert!(!prepared.candidate_windows.is_empty());
            }
        }
    }
}

#[test]
fn adaptive_context_step_independence_and_parallel_determinism() {
    let evaluator = NsbEvaluator::new().unwrap();
    let q = query(ComponentMask::ALL.bits());
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let direct = evaluator.periods_below_threshold_adaptive(&q).unwrap();
    for threads in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for step in [f64::NAN, 0.0, -1.0, 1.0, 1e8] {
            let q = q.clone().with_sample_step(Seconds::new(step));
            let actual = pool
                .install(|| evaluator.periods_below_threshold_adaptive_with_context(&context, &q))
                .unwrap();
            assert_eq!(direct.periods, actual.periods);
            assert_eq!(
                evaluator.periods_below_threshold(&q).unwrap().periods,
                direct.periods
            );
            assert_eq!(
                evaluator
                    .periods_below_threshold_with_context(&context, &q)
                    .unwrap()
                    .periods,
                direct.periods
            );
        }
    }
    let other = NsbEvaluator::new().unwrap();
    assert!(other
        .periods_below_threshold_adaptive_with_context(&context, &q)
        .is_err());
    let empty = query(0).with_sample_step(Seconds::new(f64::NAN));
    assert!(evaluator.periods_below_threshold_certified(&empty).is_ok());
}

#[test]
fn adaptive_f107_issuance_and_local_month_regimes_are_partitioned() {
    use crate::solar_activity::{resolve_f107, F107Store};
    let store=F107Store::from_json_str(r#"{
      "schema_version":1,"dataset_id":"adaptive-test","snapshot_id":"test","convention":"test","climatology_sfu":129.20671119074768,
      "retrieved_at_utc":"2026-08-27T02:30:00Z",
      "records":[{"date":"2026-08-01","value_sfu":220.0,"kind":"forecast","provider":"noaa-swpc","product":"predicted-solar-cycle","retrieved_at_utc":"2026-08-27T02:30:00Z","valid_from":"2026-08-01","valid_through":"2026-08-31","cadence":"monthly"}]
    }"#).unwrap();
    let config =
        crate::NsbModelConfig::generic_clear_sky().with_f107_store(std::sync::Arc::new(store));
    let evaluator = NsbEvaluator::with_config(config.clone()).unwrap();
    let instant =
        |s: &str| Time::<UTC>::from_chrono(s.parse::<chrono::DateTime<chrono::Utc>>().unwrap());
    let transition = instant("2026-08-27T02:30:00Z");
    assert_ne!(
        resolve_f107(transition - Seconds::new(1.0), config.solar_activity())
            .unwrap()
            .value,
        resolve_f107(transition + Seconds::new(1.0), config.solar_activity())
            .unwrap()
            .value
    );
    let mut q = query(ComponentMask::AIRGLOW.bits());
    q.target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    q.window = Period::new(
        instant("2026-08-27T01:00:00Z"),
        instant("2026-08-27T06:00:00Z"),
    );
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let cuts = filters::adaptive_threshold_windows(&p).unwrap();
    let t = scan::utc_time_to_tt_mjd(transition);
    assert!(cuts.iter().any(|w| w.start == t));
    q.threshold = Radiance::new(
        (threshold::evaluate_integrated(
            &evaluator,
            &p,
            scan::utc_time_to_tt_mjd(transition - Seconds::new(1.0)),
        )
        .unwrap()
        .value()
            + threshold::evaluate_integrated(
                &evaluator,
                &p,
                scan::utc_time_to_tt_mjd(transition + Seconds::new(1.0)),
            )
            .unwrap()
            .value())
            * 0.5,
    );
    let actual: Vec<_> = evaluator
        .periods_below_threshold_adaptive_with_context(&context, &q)
        .unwrap()
        .periods
        .into_iter()
        .map(scan::utc_period_to_tt_mjd)
        .collect();
    assert_matches(
        &actual,
        &reference(&evaluator, &p, &q, 5.0),
        "F10.7 retrieval jump",
    );
    q.window = Period::new(
        instant("2026-01-31T20:00:00Z"),
        instant("2026-02-02T12:00:00Z"),
    );
    q.sun_altitude_ceiling = None;
    q.target_altitude_floor = None;
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let cuts = filters::adaptive_threshold_windows(&p).unwrap();
    let midnight = scan::utc_time_to_tt_mjd(instant("2026-02-01T00:00:00Z"));
    assert!(cuts.iter().any(|w| w.start == midnight));
    let local = crate::components::airglow::temporal::palace_climatology_boundaries_for_window(
        p.tt_window,
        p.observer,
    );
    for boundary in local.into_iter().filter(|b| {
        p.astronomical_night_periods
            .iter()
            .any(|n| n.period.start < *b && *b < n.period.end)
    }) {
        assert!(
            cuts.iter().any(|w| w.start == boundary),
            "PALACE local hour/month boundary missing"
        );
    }
}

#[test]
fn adaptive_zodiacal_corner_cells_and_reddening_boundaries_match_fine_scan() {
    use siderust::coordinates::{
        frames::EclipticMeanJ2000, spherical::Direction, transform::TransformFrame,
    };
    let evaluator = NsbEvaluator::new().unwrap();
    let start = Time::<UTC>::from_chrono(
        "2026-05-04T00:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    let mid = start + Seconds::new(1.5 * 86400.0);
    let sun = siderust::bodies::Sun::ecliptic_longitude_geocentric(
        mid.to::<tempoch::TT>().to::<tempoch::JD>(),
    )
    .value()
    .to_degrees();
    // Frozen physical fixtures. The beta=24 threshold was migrated after the
    // Noll correction using independent local extremum sampling; a 1 ms scan
    // below must still witness the short excursion and both boundaries.
    for (beta, delta, frozen_threshold, short_excursions) in [
        (5.0, 20.0, Some(0.9904286588014837), 0),
        (17.0, 25.0, Some(0.6621254733958847), 0),
        (24.0, 20.0, Some(1.472712454699566), 1),
        (30.0, 30.0, None, 0),
        (0.0, 90.0, Some(0.07658000509545837), 0),
        (89.0, 180.0, None, 0),
    ] {
        let mut q = query(ComponentMask::ZODIACAL.bits());
        q.window = Period::new(start, start + Seconds::new(3.0 * 86400.0));
        q.sun_altitude_ceiling = None;
        q.target = Direction::<EclipticMeanJ2000>::new((sun + delta) * DEG, beta * DEG).to_frame();
        let context = evaluator.prepare_site_window_context(&q).unwrap();
        let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
        let windows = filters::adaptive_threshold_windows(&p).unwrap();
        let mut values = Vec::new();
        for w in &windows {
            values.push(
                threshold::evaluate_integrated(
                    &evaluator,
                    &p,
                    Mjd::new(w.start.raw().value() + w.length().value() * 0.5),
                )
                .unwrap()
                .value(),
            );
        }
        if values.is_empty() {
            continue;
        }
        values.sort_by(f64::total_cmp);
        q.threshold = Radiance::new(frozen_threshold.unwrap_or(values[values.len() / 2]));
        let initial = evaluator.periods_below_threshold_adaptive_with_context(&context, &q);
        let ambiguous = initial.is_err();
        if let Err(error) = initial {
            // This median sample coincides with the maximum to floating-point
            // radiance precision. A scan's no-crossing result cannot settle it.
            assert!(error.to_string().contains("extremum indistinguishable"));
        }
        for offset in if ambiguous {
            vec![-1e-6, 1e-6]
        } else {
            vec![0.0]
        } {
            let mut q = q.clone();
            q.threshold += Radiance::new(offset);
            let actual: Vec<_> = evaluator
                .periods_below_threshold_adaptive_with_context(&context, &q)
                .unwrap_or_else(|e| panic!("beta {beta}, delta {delta}, offset {offset}: {e}"))
                .periods
                .into_iter()
                .map(scan::utc_period_to_tt_mjd)
                .collect();
            let mut fine = reference(&evaluator, &p, &q, 10.0);
            if short_excursions > 0 && offset == 0.0 {
                let mut confirmed = 0;
                for pair in actual.windows(2) {
                    let a = pair[0].end.raw().value();
                    let b = pair[1].start.raw().value();
                    if (b - a) * 86400.0 < 1.0 {
                        // The 10 s reference misses a real sub-second excursion.
                        // Resolve it independently on a local 1 ms grid. Keep
                        // the independently reconstructed near-extremum threshold.
                        let local =
                            Interval::new(Mjd::new(a - 1.0 / 86400.0), Mjd::new(b + 1.0 / 86400.0));
                        let above = super::reference_scan::above_threshold_periods(
                            local,
                            Days::new(0.001 / 86400.0),
                            &|t| threshold::evaluate_integrated(&evaluator, &p, t),
                            q.threshold,
                        )
                        .unwrap();
                        assert_eq!(
                            above.len(),
                            1,
                            "beta {beta}, delta {delta}, threshold {}, intervals {:?}",
                            q.threshold.value(),
                            above
                                .iter()
                                .map(|w| (
                                    w.start.raw().value(),
                                    w.end.raw().value(),
                                    (w.end - w.start).value() * 86400.0
                                ))
                                .collect::<Vec<_>>()
                        );
                        assert!((above[0].start.raw().value() - a).abs() * 86400.0 <= 0.1);
                        assert!((above[0].end.raw().value() - b).abs() * 86400.0 <= 0.1);
                        assert!(
                            threshold::evaluate_integrated(
                                &evaluator,
                                &p,
                                Mjd::new(a + (b - a) * 0.5)
                            )
                            .unwrap()
                                > q.threshold
                        );
                        fine = fine
                            .into_iter()
                            .flat_map(|w| super::reference_scan::complement_periods(w, &above))
                            .collect();
                        confirmed += 1;
                    }
                }
                assert_eq!(confirmed, short_excursions);
            }
            assert_matches(
                &actual,
                &fine,
                &format!("Zodiacal beta {beta}, delta {delta}, offset {offset}"),
            );
        }
    }
}

#[test]
fn adaptive_moon_horizons_near_and_far_targets_without_visibility_filters() {
    use siderust::{
        coordinates::{
            cartesian, centers::Geocentric, frames::EquatorialMeanJ2000, transform::TransformFrame,
        },
        ephemeris::{Ephemeris, Vsop87Ephemeris},
    };
    for north in [false, true] {
        for moon in [
            MoonlightModel::KrisciunasSchaefer1991,
            MoonlightModel::Jones2013Spectral,
        ] {
            let evaluator = super::test_fixtures::evaluator_with_moon(north, false, moon);
            let mut q = query(ComponentMask::ALL.bits());
            if north {
                q.observer = siderust::coordinates::centers::Geodetic::new_raw(
                    crate::units::angular::Degrees::new(-17.892),
                    crate::units::angular::Degrees::new(28.762),
                    siderust::qtty::Meters::new(2200.0),
                );
            }
            q.sun_altitude_ceiling = None;
            q.target_altitude_floor = None;
            let mid = q.window.start + Seconds::new(3.0 * 3600.0);
            let position: cartesian::Position<
                Geocentric,
                EquatorialMeanJ2000,
                siderust::qtty::Kilometer,
            > = Vsop87Ephemeris::moon_geocentric(mid.to::<tempoch::TT>().to::<tempoch::JD>())
                .to_frame();
            let close = position.to_spherical().direction();
            for target in [close, Target::new(266.41683 * DEG, -29.00781 * DEG)] {
                q.target = target;
                let context = evaluator.prepare_site_window_context(&q).unwrap();
                let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
                let result = evaluator
                    .periods_below_threshold_adaptive_with_context(&context, &q)
                    .unwrap_or_else(|e| {
                        panic!("north {north}, model {moon:?}, target {target:?}: {e}")
                    });
                let actual: Vec<_> = result
                    .periods
                    .into_iter()
                    .map(scan::utc_period_to_tt_mjd)
                    .collect();
                super::discovery_bench::compare(
                    &evaluator,
                    &context,
                    &q,
                    &actual,
                    &reference(&evaluator, &p, &q, 30.0),
                    "unfiltered Moon/horizon",
                    false,
                );
            }
        }
    }
}

#[test]
fn adaptive_airglow_configured_profile_and_outside_night_validation() {
    use crate::components::airglow::{
        AirglowGeometryModel, AirglowWavelengthApplicability, ValidatedZenithDomain,
        VerticalEmissionProfile, VerticalEmissionProfileDefinition, VerticalProfileNormalization,
        VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
    };
    use siderust::qtty::{Degrees, Kilometers, Nanometers};
    let geometry = |max| {
        AirglowGeometryModel::VerticalProfile(
            VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
                schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
                profile_id: "adaptive-test".into(),
                altitude_km: vec![
                    Kilometers::new(75.0),
                    Kilometers::new(90.0),
                    Kilometers::new(110.0),
                ],
                relative_emissivity: vec![0.0, 1.0, 0.0],
                normalization: VerticalProfileNormalization::UnitVerticalIntegral,
                wavelength: AirglowWavelengthApplicability {
                    min: Nanometers::new(300.0),
                    max: Nanometers::new(650.0),
                    band: "test".into(),
                },
                assumptions: "synthetic profile".into(),
                provenance: "test".into(),
                license: "test".into(),
                validated_zenith: ValidatedZenithDomain {
                    min: Degrees::new(0.0),
                    max: Degrees::new(max),
                },
            })
            .unwrap(),
        )
    };
    let evaluator = NsbEvaluator::with_config(
        crate::NsbModelConfig::generic_clear_sky().with_airglow_geometry(geometry(90.0)),
    )
    .unwrap();
    let mut q = query(ComponentMask::AIRGLOW.bits());
    q.window.start = Time::<UTC>::from_chrono(
        "2026-09-04T01:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    q.window.end = q.window.start + Seconds::new(3.0 * 3600.0);
    q.target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let actual: Vec<_> = evaluator
        .periods_below_threshold_adaptive_with_context(&context, &q)
        .unwrap()
        .periods
        .into_iter()
        .map(scan::utc_period_to_tt_mjd)
        .collect();
    assert_matches(
        &actual,
        &reference(&evaluator, &p, &q, 30.0),
        "configured vertical Airglow profile",
    );
    let restricted = NsbEvaluator::with_config(
        crate::NsbModelConfig::generic_clear_sky().with_airglow_geometry(geometry(80.0)),
    )
    .unwrap();
    q.window.start = Time::<UTC>::from_chrono(
        "2026-01-03T01:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    q.window.end = q.window.start + Seconds::new(3600.0);
    q.sun_altitude_ceiling = None;
    q.target_altitude_floor = None;
    q.threshold = Radiance::new(1e6);
    q.target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    let restricted_context = restricted.prepare_site_window_context(&q).unwrap();
    let restricted_prepared =
        prepare::prepare_target_threshold(&restricted, &restricted_context, &q).unwrap();
    let physical_error = threshold::evaluate_integrated(
        &restricted,
        &restricted_prepared,
        scan::utc_time_to_tt_mjd(q.window.start + Seconds::new(1800.0)),
    )
    .unwrap_err();
    let error = restricted.periods_below_threshold_adaptive(&q).unwrap_err();
    assert_eq!(error.to_string(), physical_error.to_string());
    // Outside-night zero still resolves and validates F10.7; no bound/fit may
    // bypass the existing invalid-source error merely because radiance is zero.
    let invalid = NsbEvaluator::with_config(
        crate::NsbModelConfig::generic_clear_sky()
            .with_solar_radio_flux(crate::units::SolarFluxUnits::new(-1.0)),
    )
    .unwrap();
    q.window.start = Time::<UTC>::from_chrono(
        "2026-01-03T12:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    q.window.end = q.window.start + Seconds::new(3600.0);
    assert!(invalid.periods_below_threshold_adaptive(&q).is_err());
}

/// Compact, C-infinity pulse: original degree-four checks all see exact zero.
/// The 600 s scanner witnesses the peak. This is a finite regression corpus,
/// not a completeness theorem for arbitrary compactly supported functions.
#[test]
fn adaptive_retains_scan_witness_hidden_from_original_fit_nodes() {
    let w: Interval<Mjd> = Interval::new(Mjd::new(60_000.0), Mjd::new(60_000.25));
    for center in [7200.0, 14_400.0, 6_667.018_930_457] {
        let f = |t| {
            let x = (seconds(t) - center) / 200.0;
            Ok(Radiance::new(if x.abs() < 1.0 {
                (1.0 - 1.0 / (1.0 - x * x)).exp()
            } else {
                0.0
            }))
        };
        // Reproduce the old node blindness explicitly, independently of search.
        for i in [0, 1, 2, 4, 6, 7, 8] {
            let x = (std::f64::consts::PI * i as f64 / 8.0).cos();
            let t = Mjd::new(w.start.raw().value() + 0.25 * (x + 1.0) * 0.5);
            assert_eq!(f(t).unwrap().value(), 0.0);
        }
        let old = super::reference_scan::above_threshold_periods(
            w,
            Days::new(600.0 / 86400.0),
            &f,
            Radiance::new(0.5),
        )
        .unwrap();
        assert_eq!(old.len(), 1, "legacy witness at {center}");
        let (out, _) = discovery::search(
            w,
            Radiance::new(0.5),
            f,
            BoundOutcome::Unsupported,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(out.len(), 2, "hidden pulse at {center}");
        assert!(seconds(out[0].end) < center && seconds(out[1].start) > center);
    }
}

#[test]
fn adaptive_joint_band_contracts_both_thresholds_inward() {
    for decreasing in [false, true] {
        let f = |t| {
            Ok(Radiance::new(if decreasing {
                600.0 - seconds(t)
            } else {
                seconds(t)
            }))
        };
        for (minimum, maximum) in [(100.123456, 500.654321), (100.0, 100.02), (0.0, 600.0)] {
            let (periods, _) = discovery::search_band(
                extent(),
                Radiance::new(maximum),
                Some(Radiance::new(minimum)),
                f,
                BoundOutcome::Unsupported,
                Limits::default(),
            )
            .unwrap();
            // A band narrower than the crossing uncertainty may be omitted.
            if maximum - minimum > 0.2 {
                assert_eq!(periods.len(), 1);
            }
            for p in periods {
                for t in [
                    p.start,
                    p.end,
                    Mjd::new(p.start.raw().value() + p.length().value() * 0.5),
                ] {
                    let y = f(t).unwrap().value();
                    assert!(
                        y >= minimum && y <= maximum,
                        "band [{minimum}, {maximum}] at {}: {y}",
                        seconds(t)
                    );
                }
                let expected_start = if decreasing { 600.0 - maximum } else { minimum };
                let expected_end = if decreasing { 600.0 - minimum } else { maximum };
                assert!((seconds(p.start) - expected_start).abs() <= 0.1);
                assert!((seconds(p.end) - expected_end).abs() <= 0.1);
            }
        }
    }
    let (empty, _) = discovery::search_band(
        extent(),
        Radiance::new(200.0),
        Some(Radiance::new(200.0)),
        |t| Ok(Radiance::new(seconds(t))),
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert!(
        empty.is_empty(),
        "isolated equality must not create positive duration"
    );
    let (equal, _) = discovery::search_band(
        extent(),
        Radiance::new(0.5),
        Some(Radiance::new(0.5)),
        |_| Ok(Radiance::new(0.5)),
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(equal, vec![extent()]);
}

#[test]
fn adaptive_joint_band_multiple_crossings_tangencies_and_jumps() {
    let f = |t| {
        Ok(Radiance::new(
            (seconds(t) * std::f64::consts::TAU / 300.0).sin(),
        ))
    };
    let (out, _) = discovery::search_band(
        extent(),
        Radiance::new(0.6),
        Some(Radiance::new(0.2)),
        f,
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(out.len(), 4);
    for p in out {
        for i in 0..=100 {
            let y = f(Mjd::new(
                p.start.raw().value() + p.length().value() * i as f64 / 100.0,
            ))
            .unwrap()
            .value();
            assert!((0.2..=0.6).contains(&y));
        }
    }
    let tangent = |t| Ok(Radiance::new(0.5 + (seconds(t) - 271.25).powi(2) * 1e-5));
    assert!(discovery::search_band(
        extent(),
        Radiance::new(5.0),
        Some(Radiance::new(0.5)),
        tangent,
        BoundOutcome::Unsupported,
        Limits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("extremum indistinguishable"));
    // Physical discontinuities are partitioned first; include both one-sided
    // values at the join and never manufacture duration around a singleton.
    let jump = Mjd::new(60000.0 + 300.0 / 86400.0);
    let windows = [
        Interval::new(extent().start, jump),
        Interval::new(jump, extent().end),
    ];
    let (out, _) = discovery::search_windows(
        &windows,
        Radiance::new(0.75),
        Some(Radiance::new(0.25)),
        |t| Ok(Radiance::new(if t < jump { 0.0 } else { 0.5 })),
        |_| Ok(BoundOutcome::Unsupported),
        false,
    )
    .unwrap();
    assert_eq!(out.len(), 1);
    assert!(out[0].start >= jump);
}

#[test]
fn adaptive_query_budget_is_bounded_and_parallel_deterministic() {
    let windows: Vec<Interval<Mjd>> = (0..3000)
        .map(|i| {
            Interval::new(
                Mjd::new(60000.0 + i as f64 / 24.0),
                Mjd::new(60000.0 + (i + 1) as f64 / 24.0),
            )
        })
        .collect();
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                discovery::search_windows(
                    &windows,
                    Radiance::new(0.5),
                    None,
                    |_| Ok(Radiance::new(0.25)),
                    |_| Ok(BoundOutcome::Unsupported),
                    false,
                )
                .unwrap()
            })
    };
    let a = run(1);
    let b = run(2);
    assert_eq!(a.0, b.0);
    assert_eq!(a.1.exact, b.1.exact);
    assert_eq!(a.1.fits, b.1.fits);
    assert!(a.1.exact <= 64 * windows.len() + 65536);
    assert!(a.1.peak_samples <= Limits::default().exact);
    let many = vec![extent(); 25_001];
    assert!(discovery::search_windows(
        &many,
        Radiance::new(0.5),
        None,
        |_| unreachable!(),
        |_| unreachable!(),
        false
    )
    .unwrap_err()
    .to_string()
    .contains("initial physical interval budget"));
}

#[test]
fn real_joint_band_all_masks_both_moon_models_have_no_sampled_false_positives() {
    for north in [false, true] {
        for moon in [
            MoonlightModel::KrisciunasSchaefer1991,
            MoonlightModel::Jones2013Spectral,
        ] {
            let evaluator = super::test_fixtures::evaluator_with_moon(north, true, moon);
            for bits in 0..=15 {
                let mut q = query(bits);
                if north {
                    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
                        crate::units::angular::Degrees::new(-17.892),
                        crate::units::angular::Degrees::new(28.762),
                        siderust::qtty::Meters::new(2200.0),
                    );
                }
                let context = evaluator.prepare_site_window_context(&q).unwrap();
                let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
                let windows = filters::adaptive_threshold_windows(&p).unwrap();
                let mut values = Vec::new();
                for w in windows {
                    for u in [0.1, 0.3, 0.5, 0.7, 0.9] {
                        values.push(
                            threshold::evaluate_integrated(
                                &evaluator,
                                &p,
                                Mjd::new(w.start.raw().value() + w.length().value() * u),
                            )
                            .unwrap()
                            .value(),
                        );
                    }
                }
                if values.is_empty() {
                    continue;
                }
                values.sort_by(f64::total_cmp);
                let min = Radiance::new(values[values.len() / 3]);
                q.threshold = Radiance::new(values[values.len() * 2 / 3]);
                let out = evaluator
                    .periods_in_band_adaptive_with_context(&context, &q, min)
                    .unwrap_or_else(|e| panic!("north {north} moon {moon:?} mask {bits}: {e}"));
                for period in out.periods {
                    let tt = scan::utc_period_to_tt_mjd(period);
                    // Includes native UTC boundaries after inward TT projection.
                    for i in 0..=100 {
                        let t = if i == 0 {
                            tt.start
                        } else if i == 100 {
                            tt.end
                        } else {
                            tt.start + tt.length() * (i as f64 / 100.0)
                        };
                        let value = threshold::evaluate_integrated(&evaluator, &p, t).unwrap();
                        assert!(value >= min && value <= q.threshold, "false-positive north {north} moon {moon:?} bits {bits}: {value} outside [{min}, {}]", q.threshold);
                    }
                }
            }
        }
    }
}

#[test]
fn adaptive_split_precision_midnight_does_not_fragment_moon_down_periods() {
    for moon in [
        MoonlightModel::Jones2013Spectral,
        MoonlightModel::KrisciunasSchaefer1991,
    ] {
        let evaluator = super::test_fixtures::evaluator_with_moon(true, false, moon);
        let mut q = query(ComponentMask::MOON.bits());
        q.observer = siderust::coordinates::centers::Geodetic::new_raw(
            crate::units::angular::Degrees::new(-17.892),
            crate::units::angular::Degrees::new(28.762),
            siderust::qtty::Meters::new(2200.0),
        );
        q.window.start = Time::<UTC>::from_chrono(
            "2026-01-01T00:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        );
        q.window.end = q.window.start + Seconds::new(30.0 * 86400.0);
        let context = evaluator.prepare_site_window_context(&q).unwrap();
        let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
        let actual: Vec<_> = evaluator
            .periods_below_threshold_adaptive_with_context(&context, &q)
            .unwrap()
            .periods
            .into_iter()
            .map(scan::utc_period_to_tt_mjd)
            .collect();
        let fine = reference(&evaluator, &p, &q, 30.0);
        super::discovery_bench::compare(
            &evaluator,
            &context,
            &q,
            &actual,
            &fine,
            "split precision Moon midnight",
            false,
        );
        // This accepted eight-hour period crosses UTC midnight and changes
        // from a fitted active-Moon piece to an exact Moon-down certificate.
        assert_eq!(
            actual
                .iter()
                .filter(|w| w.end.raw().value() > 61060.9 && w.start.raw().value() < 61061.1)
                .count(),
            1
        );
    }
}

#[test]
fn adaptive_adjacent_mjd_native_window_is_resolved_without_endpoint_rounding() {
    let w = Interval::new(Mjd::new(60000.0), Mjd::new(60000.0_f64.next_up()));
    let (periods, _) = discovery::search(
        w,
        Radiance::new(0.5),
        |_| Ok(Radiance::new(0.0)),
        BoundOutcome::Unsupported,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(periods, vec![w]);
}

#[test]
fn adaptive_airglow_moon_resolves_two_second_accepted_island_missed_by_scan30() {
    let evaluator =
        super::test_fixtures::evaluator_with_moon(true, false, MoonlightModel::Jones2013Spectral);
    let mut q = query(12);
    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
        crate::units::angular::Degrees::new(-17.892),
        crate::units::angular::Degrees::new(28.762),
        siderust::qtty::Meters::new(2200.0),
    );
    q.window.end = q.window.start + Seconds::new(7.0 * 86400.0);
    // Corrected first-order scattering replaces the old local maximum with a
    // minimum. Independently sampled trough +/-1.2 s defines this new fixture;
    // preserve the narrow-event challenge with its accepted/rejected dual.
    q.threshold = Radiance::new(0.7348273339030783);
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let actual: Vec<_> = evaluator
        .periods_below_threshold_adaptive_with_context(&context, &q)
        .unwrap()
        .periods
        .into_iter()
        .map(scan::utc_period_to_tt_mjd)
        .collect();
    let fine = reference(&evaluator, &p, &q, 30.0);
    let island = actual
        .iter()
        .find(|w| w.start.raw().value() > 61046.922 && w.end.raw().value() < 61046.923)
        .expect("corrected physical trough must be discovered");
    let (left, right) = (island.start, island.end);
    assert!((2.3..2.5).contains(&((right - left).value() * 86400.0)));
    assert!(fine.iter().all(|w| w.end <= left || w.start >= right));
    let local = Interval::new(left - Seconds::new(1.0), right + Seconds::new(1.0));
    let above = super::reference_scan::above_threshold_periods_precise(
        local,
        Days::new(0.001 / 86400.0),
        &|t| threshold::evaluate_integrated(&evaluator, &p, t),
        q.threshold,
    )
    .unwrap();
    let below = super::reference_scan::complement_periods(local, &above);
    assert_eq!(below.len(), 1);
    assert!((below[0].start - left).value().abs() * 86400.0 <= 0.101);
    assert!((below[0].end - right).value().abs() * 86400.0 <= 0.101);
    for fraction in [0.0, 0.1, 0.5, 0.9, 1.0] {
        assert!(
            threshold::evaluate_integrated(&evaluator, &p, left + (right - left) * fraction)
                .unwrap()
                <= q.threshold
        );
    }
    let comparison = super::discovery_bench::compare(
        &evaluator,
        &context,
        &q,
        &actual,
        &fine,
        "PALACE/Jones 2.4 s accepted island",
        false,
    );
    assert_eq!(comparison.fine_missed_accepted_excursions, 1);
}

#[test]
fn adaptive_rejects_eighty_seven_second_twilight_scan120_false_positive() {
    let evaluator =
        super::test_fixtures::evaluator_with_moon(false, false, MoonlightModel::Jones2013Spectral);
    let mut q = query(ComponentMask::ALL.bits());
    q.target = Target::new(
        (83.6331_f64 + 36.0 * 137.507764).rem_euclid(360.0) * DEG,
        -5.0 * DEG,
    );
    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
        crate::units::angular::Degrees::new(-70.31634444444444),
        crate::units::angular::Degrees::new(-24.683427777777776),
        siderust::qtty::Meters::new(2184.6),
    );
    q.window.start = Time::<UTC>::from_chrono(
        "2026-01-01T00:00:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    q.window.end = q.window.start + Seconds::new(30.0 * 86400.0);
    // The corrected curves move the independently witnessed twilight jump to
    // January 14. Freeze the radiance at 87.6 s after that night boundary.
    q.threshold = Radiance::new(0.44952273599635584);
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let actual: Vec<_> = evaluator
        .periods_below_threshold_adaptive_with_context(&context, &q)
        .unwrap()
        .periods
        .into_iter()
        .map(scan::utc_period_to_tt_mjd)
        .collect();
    // Deliberately retain the legacy endpoint/root convention here: the test
    // reproduces the old scan120 false positive, not the new precise scanner.
    let mut fine = Vec::new();
    for w in filters::threshold_windows(&p, true) {
        let above = super::reference_scan::above_threshold_periods(
            w,
            Days::new(120.0 / 86400.0),
            &|t| threshold::evaluate_integrated(&evaluator, &p, t),
            q.threshold,
        )
        .unwrap();
        fine.extend(super::reference_scan::complement_periods(w, &above));
    }
    scan::coalesce_periods(&mut fine);
    let r = fine
        .iter()
        .find(|w| w.start.raw().value() > 61054.0 && w.start.raw().value() < 61054.1)
        .unwrap();
    let a = actual
        .iter()
        .find(|w| w.start.raw().value() > 61054.0 && w.start.raw().value() < 61054.1)
        .unwrap();
    let excess = (a.start - r.start).value() * 86400.0;
    assert!(
        (87.5..87.8).contains(&excess),
        "excess {excess}, reference {:?}, adaptive {:?}",
        (r.start.raw().value(), r.end.raw().value()),
        (a.start.raw().value(), a.end.raw().value())
    );
    assert!(threshold::evaluate_integrated(&evaluator, &p, r.start).unwrap() < q.threshold);
    for seconds in [0.001, 1.0, 30.0, 60.0, 85.0] {
        assert!(
            threshold::evaluate_integrated(&evaluator, &p, r.start + Seconds::new(seconds))
                .unwrap()
                > q.threshold
        );
    }
    assert!(threshold::evaluate_integrated(&evaluator, &p, a.start).unwrap() <= q.threshold);
    let f = |t| threshold::evaluate_integrated(&evaluator, &p, t);
    let above = super::reference_scan::above_threshold_periods(
        Interval::new(r.start - Seconds::new(1.0), a.start + Seconds::new(1.0)),
        siderust::qtty::Days::new(0.01 / 86400.0),
        &f,
        q.threshold,
    )
    .unwrap();
    assert_eq!(above.len(), 1);
    assert!((above[0].start - r.start).value().abs() * 86400.0 < 0.106);
    assert!((above[0].end - a.start).value().abs() * 86400.0 < 0.106);
    let comparison = super::discovery_bench::compare(
        &evaluator,
        &context,
        &q,
        &actual,
        &fine,
        "twilight 87 s",
        false,
    );
    assert_eq!(comparison.fine_missed_excursions, 1);
}

/// Export observed boundary errors rather than just the acceptance allowance.
/// The full normal tests retain their all-component assertions; this repeatable
/// seven-day export is deliberately separate from timing measurements.
#[test]
#[ignore = "release physical boundary accuracy export"]
fn physical_boundary_accuracy_reference() {
    for north in [false, true] {
        for moon in [
            MoonlightModel::Jones2013Spectral,
            MoonlightModel::KrisciunasSchaefer1991,
        ] {
            let evaluator = super::test_fixtures::evaluator_with_moon(north, true, moon);
            for bits in 0..=15 {
                let mut q = query(bits);
                if north {
                    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
                        crate::units::angular::Degrees::new(-17.892),
                        crate::units::angular::Degrees::new(28.762),
                        siderust::qtty::Meters::new(2200.0),
                    );
                } else {
                    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
                        crate::units::angular::Degrees::new(-70.31634444444444),
                        crate::units::angular::Degrees::new(-24.683427777777776),
                        siderust::qtty::Meters::new(2184.6),
                    );
                }
                q.window.start = Time::<UTC>::from_chrono(
                    "2026-01-03T00:00:00Z"
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .unwrap(),
                );
                q.window.end = q.window.start + Seconds::new(7.0 * 86400.0);
                let context = evaluator.prepare_site_window_context(&q).unwrap();
                let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
                if bits & ComponentMask::STARLIGHT.bits() != 0
                    && bits != ComponentMask::STARLIGHT.bits()
                {
                    q.threshold += p.starlight_integrated;
                }
                let actual: Vec<_> = evaluator
                    .periods_below_threshold_adaptive_with_context(&context, &q)
                    .unwrap()
                    .periods
                    .into_iter()
                    .map(scan::utc_period_to_tt_mjd)
                    .collect();
                for interval in &actual {
                    for time in [interval.start, interval.end] {
                        assert!(
                            threshold::evaluate_integrated(&evaluator, &p, time).unwrap()
                                <= q.threshold
                        );
                    }
                }
                let fine = reference(&evaluator, &p, &q, 30.0);
                let comparison = super::discovery_bench::compare(
                    &evaluator,
                    &context,
                    &q,
                    &actual,
                    &fine,
                    "physical accuracy export",
                    false,
                );
                let mut errors = Vec::new();
                let mut unpaired_reference_boundaries = 0;
                for period in &fine {
                    for (t, start) in [(period.start, true), (period.end, false)] {
                        let nearest = actual
                            .iter()
                            .map(|a| {
                                ((if start { a.start } else { a.end }) - t).value().abs() * 86400.0
                            })
                            .min_by(f64::total_cmp);
                        if let Some(error) = nearest.filter(|e| *e <= 0.533) {
                            errors.push(error);
                        } else {
                            unpaired_reference_boundaries += 1;
                        }
                    }
                }
                println!(
                    "BOUNDARY_ACCURACY {}",
                    serde_json::json!({
                        "site":if north {"CTAO-N"} else {"CTAO-S"},"moonlight_model":moon.as_str(),"components_bits":bits,
                        "start":"2026-01-03T00:00:00Z","days":7,"reference_step_seconds":30,
                        "paired_boundary_errors_seconds":errors,"unpaired_reference_boundaries":unpaired_reference_boundaries,
                        "confirmed_reference_missed_excursions":comparison.fine_missed_excursions,
                        "reference_singleton_jump_artifacts":comparison.singleton_artifacts,
                        "accepted_native_boundary_count":actual.len()*2,"rejected_native_boundary_count":0,
                        "adaptive_periods":actual.len(),"reference_periods":fine.len()
                    })
                );
            }
        }
    }
}

#[test]
fn adaptive_parallel_errors_follow_physical_window_order() {
    let windows = [
        Interval::new(Mjd::new(60000.0), Mjd::new(60000.01)),
        Interval::new(Mjd::new(60001.0), Mjd::new(60001.01)),
    ];
    for threads in [1, 2, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let error = pool
            .install(|| {
                discovery::search_windows(
                    &windows,
                    Radiance::new(0.5),
                    None,
                    |time| {
                        let first = time < Mjd::new(60000.5);
                        if first {
                            // The later physical window fails sooner. Error identity
                            // must not depend on which Rayon worker wins a mutex.
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        Err(crate::NsbError::OutOfRange(
                            if first {
                                "first physical window"
                            } else {
                                "second physical window"
                            }
                            .into(),
                        ))
                    },
                    |_| Ok(BoundOutcome::Unsupported),
                    false,
                )
            })
            .unwrap_err();
        assert!(
            error.to_string().contains("first physical window"),
            "{threads} workers selected {error}"
        );
    }
}

#[test]
fn adaptive_accepted_islands_missed_by_scan120_have_millisecond_references() {
    // Independent samples eight seconds before the physical regime edge set
    // the corrected radiance thresholds. The original 0.25 fixtures no longer
    // contain these islands after Noll/Jones corrections. All coarse-miss,
    // eligibility, millisecond-reference and accepted-side checks remain.
    for (north, moon, index, left, right, level) in [
        (
            false,
            MoonlightModel::KrisciunasSchaefer1991,
            30usize,
            61166.04341806797,
            61166.04351066056,
            0.327_394_798_939_822_4,
        ),
        (
            true,
            MoonlightModel::KrisciunasSchaefer1991,
            70,
            61177.13374055555,
            61177.13383314815,
            0.334_941_602_217_590_6,
        ),
        (
            true,
            MoonlightModel::Jones2013Spectral,
            20,
            61183.05040722222,
            61183.05049981482,
            0.511_879_413_273_921_7,
        ),
    ] {
        let evaluator = super::test_fixtures::evaluator_with_moon(north, false, moon);
        let observer = siderust::coordinates::centers::Geodetic::new_raw(
            siderust::qtty::Degrees::new(if north { -17.892 } else { -70.31634444444444 }),
            siderust::qtty::Degrees::new(if north { 28.762 } else { -24.683427777777776 }),
            siderust::qtty::Meters::new(if north { 2200.0 } else { 2184.6 }),
        );
        let start = Time::<UTC>::from_chrono(
            "2026-01-01T00:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        );
        let q = ThresholdQuery::new(
            observer,
            Target::new(
                (83.6331 + index as f64 * 137.507764).rem_euclid(360.0) * DEG,
                (-60.0 + (index % 25) as f64 * 5.0) * DEG,
            ),
            Period::new(start, start + Seconds::new(365.0 * 86400.0)),
            Radiance::new(level),
        )
        .with_components(ComponentMask::ALL)
        .with_target_altitude_floor(Some(siderust::qtty::Degrees::new(20.0)));
        let context = evaluator.prepare_site_window_context(&q).unwrap();
        let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
        let expected = Interval::new(Mjd::new(left), Mjd::new(right));
        // Keep the original physical partition and grid phase: this reproduces
        // each annual scan120 miss without scanning the rest of the year.
        let legacy: Vec<_> = super::filters::threshold_windows(&p, true)
            .into_iter()
            .filter(|w| w.start < expected.end && w.end > expected.start)
            .collect();
        assert!(!legacy.is_empty());
        let f = |t| threshold::evaluate_integrated(&evaluator, &p, t);
        let mut coarse = Vec::new();
        for w in &legacy {
            let above = super::reference_scan::above_threshold_periods(
                *w,
                Seconds::new(120.0).to::<siderust::qtty::Day>(),
                &f,
                q.threshold,
            )
            .unwrap();
            let below = super::reference_scan::complement_periods(*w, &above);
            assert!(below
                .iter()
                .all(|a| a.end <= expected.start || a.start >= expected.end));
            coarse.extend(below);
        }
        let extent = Interval::new(legacy.first().unwrap().start, legacy.last().unwrap().end);
        let windows: Vec<_> = filters::adaptive_threshold_windows(&p)
            .unwrap()
            .into_iter()
            .filter_map(|w| {
                let intersection = Interval::new(w.start.max(extent.start), w.end.min(extent.end));
                (intersection.start < intersection.end).then_some(intersection)
            })
            .collect();
        let (actual, _) = discovery::search_windows(
            &windows,
            q.threshold,
            None,
            f,
            |w| super::bounds::prepared_bounds(&p, w),
            false,
        )
        .unwrap();
        scan::coalesce_periods(&mut coarse);
        let comparison = super::discovery_bench::compare(
            &evaluator,
            &context,
            &q,
            &actual,
            &coarse,
            "accepted island regression",
            false,
        );
        assert_eq!(comparison.fine_missed_accepted_excursions, 1);
        let island = actual
            .iter()
            .find(|a| {
                (a.start - expected.start).value().abs() * 86400.0 <= 0.1
                    && (a.end - expected.end).value().abs() * 86400.0 <= 0.1
            })
            .expect("adaptive must retain accepted island");
        let local = Interval::new(
            island.start - Seconds::new(1.0).to::<siderust::qtty::Day>(),
            island.end + Seconds::new(1.0).to::<siderust::qtty::Day>(),
        );
        let above = super::reference_scan::above_threshold_periods(
            local,
            Seconds::new(0.001).to::<siderust::qtty::Day>(),
            &f,
            q.threshold,
        )
        .unwrap();
        let dense = super::reference_scan::complement_periods(local, &above);
        assert_eq!(dense.len(), 1);
        assert!((dense[0].start - island.start).value().abs() * 86400.0 <= 0.106);
        assert!((dense[0].end - island.end).value().abs() * 86400.0 <= 0.106);
        for fraction in [0.0, 0.1, 0.5, 0.9, 1.0] {
            assert!(f(island.start + island.length() * fraction).unwrap() <= q.threshold);
        }
    }
}

#[test]
fn cached_astronomy_matches_uncached_authoritative_components_bit_for_bit() {
    for north in [false, true] {
        for moon in [
            MoonlightModel::Jones2013Spectral,
            MoonlightModel::KrisciunasSchaefer1991,
        ] {
            let evaluator = super::test_fixtures::evaluator_with_moon(north, true, moon);
            for bits in 0..16 {
                let mut q = query(bits);
                if north {
                    q.observer = siderust::coordinates::centers::Geodetic::new_raw(
                        siderust::qtty::Degrees::new(-17.892),
                        siderust::qtty::Degrees::new(28.762),
                        siderust::qtty::Meters::new(2200.0),
                    );
                }
                let context = evaluator.prepare_site_window_context(&q).unwrap();
                let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
                for u in [0.0, 0.01, 0.2, 0.5, 0.8, 0.99, 1.0] {
                    let t = p.tt_window.start + p.tt_window.length() * u;
                    let expected =
                        super::reference_scan::evaluate_integrated(&evaluator, &p, t).unwrap();
                    for _ in 0..2 {
                        assert_eq!(
                            threshold::evaluate_integrated(&evaluator, &p, t)
                                .unwrap()
                                .value()
                                .to_bits(),
                            expected.value().to_bits(),
                            "site north={north}, model {moon:?}, mask {bits}, fraction {u}"
                        );
                    }
                }
            }
        }
    }
}
