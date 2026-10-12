// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::{
    bounds::BoundOutcome,
    discovery::{self, Limits},
    filters, prepare, scan, threshold,
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
    let old = scan::authoritative_above_threshold_periods(
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
    assert!(scan::authoritative_above_threshold_periods(
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
    let adjacent = Interval::new(Mjd::new(60000.0), Mjd::new(60000.0_f64.next_up()));
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
    for w in filters::smooth_threshold_windows(prepared) {
        let above = scan::authoritative_above_threshold_periods(
            w,
            Days::new(step / 86400.0),
            &|t| threshold::evaluate_integrated(evaluator, prepared, t),
            query.threshold,
        )
        .unwrap();
        out.extend(scan::complement_periods(w, &above));
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
        let evaluator = super::adaptive_bench::evaluator_with_moon(false, true, moon);
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
            if !step.is_finite() || step <= 0.0 {
                assert!(evaluator.periods_below_threshold(&q).is_err());
            }
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
fn legacy_grid_phase_is_identical_after_certificate_integration() {
    let evaluator = NsbEvaluator::new().unwrap();
    let q = query(ComponentMask::MOON.bits()).with_sample_step(Seconds::new(917.0));
    let context = evaluator.prepare_site_window_context(&q).unwrap();
    let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
    let expected: Vec<_> = reference(&evaluator, &p, &q, 917.0)
        .into_iter()
        .map(|w| scan::tt_mjd_period_to_utc(w, p.tt_window, q.window))
        .collect();
    assert_eq!(
        evaluator.periods_below_threshold(&q).unwrap().periods,
        expected
    );
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
    for (beta, delta) in [
        (5.0, 20.0),
        (17.0, 25.0),
        (24.0, 20.0),
        (30.0, 30.0),
        (0.0, 90.0),
        (89.0, 180.0),
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
        q.threshold = Radiance::new(values[values.len() / 2]);
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
            if beta == 24.0 && offset == 0.0 {
                let mut confirmed = 0;
                for pair in actual.windows(2) {
                    let a = pair[0].end.raw().value();
                    let b = pair[1].start.raw().value();
                    if (b - a) * 86400.0 < 1.0 {
                        // The 10 s reference misses a real ~53 ms excursion.
                        // Resolve it independently on a local 1 ms grid. Keep
                        // the original near-extremum threshold, not a relaxed one.
                        let local =
                            Interval::new(Mjd::new(a - 1.0 / 86400.0), Mjd::new(b + 1.0 / 86400.0));
                        let above = scan::authoritative_above_threshold_periods(
                            local,
                            Days::new(0.001 / 86400.0),
                            &|t| threshold::evaluate_integrated(&evaluator, &p, t),
                            q.threshold,
                        )
                        .unwrap();
                        assert_eq!(above.len(), 1);
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
                            .flat_map(|w| scan::complement_periods(w, &above))
                            .collect();
                        confirmed += 1;
                    }
                }
                assert_eq!(confirmed, 1);
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
            let evaluator = super::adaptive_bench::evaluator_with_moon(north, false, moon);
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
