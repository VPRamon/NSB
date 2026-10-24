// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::intervals::{
    coalesce_periods, tt_mjd_period_to_utc, tt_mjd_to_utc_time, utc_period_to_tt_mjd,
};
use super::reference_scan::{above_threshold_periods, complement_periods};
use super::types::{ThresholdQuery, ThresholdQueryResult};
use crate::error::Result;
use crate::evaluator::{ComponentMask, NsbEvaluator, Observer, Target};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use crate::units::Second;
use chrono::{DateTime, Duration, Utc};
use siderust::bodies::Moon as MoonBody;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{
    above_threshold as siderust_above_threshold, AltitudeProvider, SearchOpts,
};
use siderust::qtty::{Day, Degree, Degrees, Hours, Meters};
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
use tempoch::{Period, Time, UTC};

fn parse(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn continuum_default_evaluator() -> NsbEvaluator {
    NsbEvaluator::with_config(
        crate::NsbModelConfig::generic_clear_sky()
            .with_airglow_model(crate::AirglowModel::ParanalPalaceV1),
    )
    .unwrap()
}

fn tt_time(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<siderust::time::TT>().to::<tempoch::MJD>())
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn high_arctic() -> Geodetic<ECEF> {
    Geodetic::new_raw(Degrees::new(0.0), Degrees::new(89.0), Meters::new(0.0))
}

fn target_sgr_a() -> Target {
    Target::new(266.41683 * crate::DEG, -29.00781 * crate::DEG)
}

fn polar_target() -> Target {
    Target::new(0.0 * crate::DEG, 89.0 * crate::DEG)
}

fn threshold_query(
    observer: Observer,
    target: Target,
    start: &str,
    hours: i64,
    components: ComponentMask,
) -> ThresholdQuery {
    let start = parse(start);
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(hours));
    ThresholdQuery {
        observer,
        target,
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(0.21),
        components,
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
    }
}

fn prepare_threshold(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    tt_window: TimePeriod<ModifiedJulianDate>,
) -> Result<super::types::PreparedThresholdQuery> {
    let context = crate::planning::prepare::prepare_site_context(evaluator, query, tt_window)?;
    crate::planning::prepare::prepare_target_threshold(evaluator, &context, query)
}

fn scan_threshold_native(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
    step: Second,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>> {
    crate::planning::prepare::validate_query(query)?;
    let tt_window = utc_period_to_tt_mjd(query.window);
    let prepared = prepare_threshold(evaluator, query, tt_window)?;
    let step = step.to::<Day>();
    let f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
        crate::planning::threshold::evaluate_integrated(evaluator, &prepared, mjd_tt)
    };

    let mut darker_periods = Vec::new();
    for candidate in &prepared.candidate_windows {
        let brighter = above_threshold_periods(*candidate, step, &f, query.threshold)?;
        darker_periods.extend(complement_periods(*candidate, &brighter));
    }
    coalesce_periods(&mut darker_periods);

    Ok(darker_periods)
}

fn scan_threshold_periods(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    let tt_window = utc_period_to_tt_mjd(query.window);
    Ok(ThresholdQueryResult {
        threshold: query.threshold,
        periods: scan_threshold_native(evaluator, query, Second::new(1_800.0))?
            .into_iter()
            .map(|period| tt_mjd_period_to_utc(period, tt_window, query.window))
            .collect(),
    })
}

fn assert_periods_match_within_seconds(
    actual: &ThresholdQueryResult,
    expected: &ThresholdQueryResult,
    tolerance_seconds: i64,
) {
    assert_eq!(actual.periods.len(), expected.periods.len());
    for (actual, expected) in actual.periods.iter().zip(&expected.periods) {
        let start_delta = (actual.start.to_chrono().unwrap() - expected.start.to_chrono().unwrap())
            .num_seconds()
            .abs();
        let end_delta = (actual.end.to_chrono().unwrap() - expected.end.to_chrono().unwrap())
            .num_seconds()
            .abs();
        assert!(
            start_delta <= tolerance_seconds,
            "period start differs by {start_delta}s"
        );
        assert!(
            end_delta <= tolerance_seconds,
            "period end differs by {end_delta}s"
        );
    }
}

#[test]
fn threshold_airglow_splits_palace_hour_boundaries_for_coarse_scans() {
    let evaluator = continuum_default_evaluator();
    let mut query = threshold_query(
        paranal(),
        target_sgr_a(),
        "2023-09-04T00:00:00Z",
        12,
        ComponentMask::AIRGLOW,
    );
    query.target_altitude_floor = None;
    let tt_window = utc_period_to_tt_mjd(query.window);
    let prepared = prepare_threshold(&evaluator, &query, tt_window).unwrap();

    assert!(!prepared.airglow_phase_periods.is_empty());
    let windows = crate::planning::filters::threshold_windows(&prepared, true);
    assert!(!windows.is_empty());
    let max_window_seconds = windows
        .iter()
        .map(|window| (window.end.raw().value() - window.start.raw().value()) * 86_400.0)
        .fold(0.0, f64::max);
    assert!(
        max_window_seconds <= 3_600.1,
        "PALACE climatology discontinuities must split coarse threshold scans; longest window was {max_window_seconds}s"
    );

    let sample = prepared.airglow_phase_periods[0].period.start;
    let _ = crate::planning::threshold::evaluate_integrated(&evaluator, &prepared, sample).unwrap();
}
#[test]
fn authoritative_threshold_search_matches_scan_oracle_for_representative_window() {
    let evaluator = continuum_default_evaluator();
    let start = parse("2023-09-04T02:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(4));
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, end),
        BandPhotonRadiance::new(0.21),
    )
    .with_components(ComponentMask::ZODIACAL)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let authoritative = evaluator.periods_below_threshold(&query).unwrap();
    let scan = scan_threshold_periods(&evaluator, &query).unwrap();

    assert_periods_match_within_seconds(&authoritative, &scan, 2);
}

#[test]
fn authoritative_search_matches_exact_scan_across_components_and_year_boundary() {
    let evaluator = continuum_default_evaluator();
    for (fixture, start, hours, target, components, threshold) in [
        (
            "all-sgr-a",
            "2026-12-30T00:00:00Z",
            72,
            target_sgr_a(),
            ComponentMask::ALL,
            0.25,
        ),
        (
            "moon-only",
            "2026-01-14T00:00:00Z",
            72,
            Target::new(83.6331 * crate::DEG, 22.0145 * crate::DEG),
            ComponentMask::MOON,
            0.02,
        ),
        (
            "airglow-zodiacal",
            "2026-06-01T00:00:00Z",
            48,
            Target::new(120.0 * crate::DEG, -10.0 * crate::DEG),
            ComponentMask::AIRGLOW | ComponentMask::ZODIACAL,
            0.20,
        ),
    ] {
        let mut query = threshold_query(paranal(), target, start, hours, components);
        query.threshold = BandPhotonRadiance::new(threshold);
        let authoritative = evaluator.periods_below_threshold(&query).unwrap();
        // The corrected zodiacal model exposes a sub-second crossing at a
        // PALACE boundary; keep the independent oracle finer than the
        // coarse test scanner so it cannot skip it.
        let reference = scan_threshold_native(&evaluator, &query, Second::new(10.0)).unwrap();
        let context = evaluator.prepare_site_window_context(&query).unwrap();
        let prepared =
            super::prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
        let windows = super::filters::adaptive_threshold_windows(&prepared).unwrap();
        let (native, _) = super::discovery::search_windows(
            &windows,
            query.threshold,
            None,
            |t| super::threshold::evaluate_integrated(&evaluator, &prepared, t),
            |w| super::bounds::prepared_bounds(&prepared, w),
            false,
        )
        .unwrap();
        let comparison = super::discovery_bench::compare(
            &evaluator, &context, &query, &native, &reference, fixture, false,
        );
        // Two accepted twilight singleton endpoints become false positive
        // durations in the midpoint scanner. The comparator independently
        // requires exact physical-boundary identity, an accepted endpoint,
        // and rejected radiance at five interior fractions of each interval.
        assert_eq!(
            comparison.singleton_artifacts,
            if fixture == "airglow-zodiacal" { 2 } else { 0 }
        );
        assert_eq!(comparison.fine_missed_excursions, 0);
        let projected: Vec<_> = native
            .into_iter()
            .map(|w| {
                super::intervals::inward_tt_mjd_period_to_utc(w, prepared.tt_window, query.window)
                    .unwrap()
            })
            .collect();
        assert_eq!(authoritative.periods, projected, "{fixture}");
    }
}

#[test]
fn moon_rise_and_set_threshold_boundaries_match_exact_scan() {
    let evaluator = continuum_default_evaluator();
    let start = parse("2026-01-14T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(72));
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, end),
        BandPhotonRadiance::new(0.0),
    )
    .with_components(ComponentMask::MOON)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let authoritative = evaluator.periods_below_threshold(&query).unwrap();
    let scan = scan_threshold_periods(&evaluator, &query).unwrap();
    assert!(!scan.periods.is_empty());
    assert_periods_match_within_seconds(&authoritative, &scan, 2);
}

#[test]
fn reusable_site_context_matches_independent_queries_for_multiple_targets() {
    let evaluator = continuum_default_evaluator();
    let first = threshold_query(
        paranal(),
        target_sgr_a(),
        "2026-01-01T00:00:00Z",
        72,
        ComponentMask::ALL,
    );
    let context = evaluator.prepare_site_window_context(&first).unwrap();

    for (ra, dec, floor, threshold) in [
        (83.6331, 22.0145, 20.0, 0.25),
        (120.0, -10.0, 30.0, 0.22),
        (266.41683, -29.00781, 0.0, 0.20),
    ] {
        let mut query = first.clone();
        query.target = Target::new(ra * crate::DEG, dec * crate::DEG);
        query.target_altitude_floor = Some(Degrees::new(floor));
        query.threshold = BandPhotonRadiance::new(threshold);
        let reused = evaluator
            .periods_below_threshold_with_context(&context, &query)
            .unwrap();
        let independent = evaluator.periods_below_threshold(&query).unwrap();
        assert_eq!(reused.periods, independent.periods);
    }
}

#[test]
fn one_site_context_serves_max_and_min_threshold_queries() {
    let evaluator = continuum_default_evaluator();
    let max_query = threshold_query(
        paranal(),
        target_sgr_a(),
        "2026-01-01T00:00:00Z",
        48,
        ComponentMask::ALL,
    );
    let context = evaluator.prepare_site_window_context(&max_query).unwrap();
    let reused_max = evaluator
        .periods_below_threshold_with_context(&context, &max_query)
        .unwrap();

    let mut min_query = max_query.clone();
    min_query.threshold = BandPhotonRadiance::new(0.18);
    let reused_min = evaluator
        .periods_below_threshold_with_context(&context, &min_query)
        .unwrap();

    let independent_max = evaluator.periods_below_threshold(&max_query).unwrap();
    let independent_min = evaluator.periods_below_threshold(&min_query).unwrap();
    assert_eq!(reused_max.threshold, independent_max.threshold);
    assert_eq!(reused_max.periods, independent_max.periods);
    assert_eq!(reused_min.threshold, independent_min.threshold);
    assert_eq!(reused_min.periods, independent_min.periods);
}

#[test]
fn exact_search_output_is_deterministic_across_rayon_parallelism() {
    let evaluator = continuum_default_evaluator();
    let query = threshold_query(
        paranal(),
        Target::new(83.6331 * crate::DEG, 22.0145 * crate::DEG),
        "2026-01-12T00:00:00Z",
        72,
        ComponentMask::ALL,
    );
    let context = evaluator.prepare_site_window_context(&query).unwrap();
    let run = |threads| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap()
            .install(|| {
                evaluator
                    .periods_below_threshold_with_context(&context, &query)
                    .unwrap()
            })
    };

    let sequential = run(1);
    let parallel = run(4);
    assert_eq!(sequential.threshold, parallel.threshold);
    assert_eq!(sequential.periods, parallel.periods);
}

#[test]
fn site_context_rejects_another_evaluator_or_incompatible_site_state() {
    let evaluator = continuum_default_evaluator();
    let other = continuum_default_evaluator();
    let query = threshold_query(
        paranal(),
        target_sgr_a(),
        "2026-01-01T00:00:00Z",
        24,
        ComponentMask::ALL,
    );
    let context = evaluator.prepare_site_window_context(&query).unwrap();
    assert!(other
        .periods_below_threshold_with_context(&context, &query)
        .is_err());

    let mut incompatible = query.clone();
    incompatible.sun_altitude_ceiling = Some(Degrees::new(-12.0));
    assert!(evaluator
        .periods_below_threshold_with_context(&context, &incompatible)
        .is_err());
    // A populated context cannot carry observer-dependent Moon state to a
    // different site, even when all other query settings match.
    context.astronomy.moon(query.window.start);
    incompatible = query.clone();
    incompatible.observer = high_arctic();
    assert!(matches!(
        evaluator.periods_below_threshold_with_context(&context, &incompatible),
        Err(crate::NsbError::OutOfRange(_))
    ));
}

#[test]
fn target_visibility_matches_precise_altitude_scan_across_regimes() {
    let evaluator = continuum_default_evaluator();
    for (observer, target, floor) in [
        (paranal(), target_sgr_a(), 20.0),
        (paranal(), polar_target(), 20.0),
        (high_arctic(), polar_target(), 20.0),
    ] {
        let start = parse("2026-01-01T00:00:00Z");
        let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::days(30));
        let query = ThresholdQuery::new(
            observer,
            target,
            Period::new(start, end),
            BandPhotonRadiance::new(0.25),
        )
        .with_components(ComponentMask::ZODIACAL)
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(Some(Degrees::new(floor)));
        let tt_window = utc_period_to_tt_mjd(query.window);
        let prepared = prepare_threshold(&evaluator, &query, tt_window).unwrap();
        let direction: direction::ICRS =
            siderust::coordinates::transform::TransformFrame::to_frame(&target);
        let exact_altitude = |time: ModifiedJulianDate| -> Result<Degrees> {
            Ok(direction.altitude_at(&observer, time).to::<Degree>())
        };
        let exact = above_threshold_periods(
            tt_window,
            Hours::new(10.0 / 60.0).to::<Day>(),
            &exact_altitude,
            Degrees::new(floor),
        )
        .unwrap();

        assert_eq!(prepared.candidate_windows.len(), exact.len());
        for (actual, expected) in prepared.candidate_windows.iter().zip(exact) {
            assert!(
                (actual.start.raw().value() - expected.start.raw().value()).abs() * 86_400.0 <= 2.0
            );
            assert!(
                (actual.end.raw().value() - expected.end.raw().value()).abs() * 86_400.0 <= 2.0
            );
        }
    }
}

#[test]
fn grazing_target_excursion_between_four_hour_samples_is_not_pruned() {
    let evaluator = continuum_default_evaluator();
    let observer = Geodetic::new_raw(
        Degrees::new(-70.3147),
        Degrees::new(-24.6834),
        Meters::new(2_147.0),
    );
    let target = Target::new(0.0 * crate::DEG, 45.0 * crate::DEG);
    let target_dir: direction::ICRS =
        siderust::coordinates::transform::TransformFrame::to_frame(&target);
    let floor = Degrees::new(20.0);
    let day_start = tt_time(parse("2026-01-01T00:00:00Z"));

    let mut transit = day_start;
    let mut maximum = target_dir.altitude_at(&observer, transit);
    for minute in 1..=1_440 {
        let candidate = ModifiedJulianDate::new(day_start.raw().value() + minute as f64 / 1_440.0);
        let altitude = target_dir.altitude_at(&observer, candidate);
        if altitude > maximum {
            maximum = altitude;
            transit = candidate;
        }
    }
    assert!(maximum.to::<Degree>() > floor);

    let tt_window = TimePeriod::new(
        ModifiedJulianDate::new(transit.raw().value() - 2.0 / 24.0),
        ModifiedJulianDate::new(transit.raw().value() + 6.0 / 24.0),
    );
    let query = ThresholdQuery::new(
        observer,
        target,
        Period::new(
            tt_mjd_to_utc_time(tt_window.start),
            tt_mjd_to_utc_time(tt_window.end),
        ),
        BandPhotonRadiance::new(0.25),
    )
    .with_components(ComponentMask::ZODIACAL)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(Some(floor));

    let precise = siderust_above_threshold(
        &target_dir,
        &observer,
        tt_window,
        floor,
        SearchOpts::default(),
    );
    assert_eq!(
        precise.len(),
        1,
        "Siderust must resolve the grazing transit"
    );
    assert!(
        precise[0].length() < Hours::new(1.0).to::<Day>(),
        "the adversarial excursion should be much shorter than four hours"
    );

    let prepared = prepare_threshold(&evaluator, &query, tt_window).unwrap();
    assert_eq!(prepared.candidate_windows, precise);
}

#[test]
fn siderust_moon_events_preserve_short_grazing_visibility() {
    let observer = Geodetic::new_raw(Degrees::new(0.0), Degrees::new(61.0), Meters::new(0.0));
    let transit = ModifiedJulianDate::new(61_055.390_384_074_07);
    let tt_window = TimePeriod::new(
        ModifiedJulianDate::new(transit.raw().value() - 2.0 / 24.0),
        ModifiedJulianDate::new(transit.raw().value() + 6.0 / 24.0),
    );
    let precise = siderust_above_threshold(
        &MoonBody,
        &observer,
        tt_window,
        Degrees::new(0.0),
        SearchOpts::default(),
    );
    assert_eq!(precise.len(), 1, "Siderust must resolve the grazing Moon");
    assert!(
        precise[0].length() < Hours::new(4.0).to::<Day>(),
        "the fixture must remain a short Moon-visibility excursion"
    );

    let query = ThresholdQuery::new(
        observer,
        target_sgr_a(),
        Period::new(
            tt_mjd_to_utc_time(tt_window.start),
            tt_mjd_to_utc_time(tt_window.end),
        ),
        BandPhotonRadiance::new(0.25),
    )
    .with_components(ComponentMask::MOON)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let context = continuum_default_evaluator()
        .prepare_site_window_context(&query)
        .unwrap();
    assert_eq!(context.moon_visible_periods.as_deref().unwrap(), precise);
}

#[test]
fn empty_visibility_candidates_produce_no_periods() {
    let evaluator = continuum_default_evaluator();
    let query = threshold_query(
        paranal(),
        polar_target(),
        "2026-01-01T00:00:00Z",
        72,
        ComponentMask::ALL,
    );
    let result = evaluator.periods_below_threshold(&query).unwrap();
    assert!(result.periods.is_empty());
}

#[test]
fn threshold_airglow_context_matches_exact_point_airglow() {
    let evaluator = continuum_default_evaluator();
    let observer = paranal();
    let target = target_sgr_a();
    let query = threshold_query(
        observer,
        target,
        "2023-09-04T00:00:00Z",
        12,
        ComponentMask::AIRGLOW,
    );
    let time = parse("2023-09-04T04:00:00Z");
    let prepared =
        prepare_threshold(&evaluator, &query, utc_period_to_tt_mjd(query.window)).unwrap();

    let context =
        crate::planning::threshold::evaluate_integrated(&evaluator, &prepared, tt_time(time))
            .unwrap();
    let exact = evaluator
        .evaluate_airglow_resolved(observer, time, target)
        .unwrap()
        .0;

    assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
}

#[test]
fn threshold_airglow_context_matches_continuous_high_latitude_night() {
    let evaluator = continuum_default_evaluator();
    let observer = high_arctic();
    let target = polar_target();
    let query = threshold_query(
        observer,
        target,
        "2023-12-20T00:00:00Z",
        72,
        ComponentMask::AIRGLOW,
    );
    let time = parse("2023-12-21T12:00:00Z");
    let prepared =
        prepare_threshold(&evaluator, &query, utc_period_to_tt_mjd(query.window)).unwrap();

    let context =
        crate::planning::threshold::evaluate_integrated(&evaluator, &prepared, tt_time(time))
            .unwrap();
    let exact = evaluator
        .evaluate_airglow_resolved(observer, time, target)
        .unwrap()
        .0;

    assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
}

#[test]
fn threshold_moon_context_skips_only_moon_down_samples() {
    let evaluator = continuum_default_evaluator();
    let observer = paranal();
    let target = target_sgr_a();
    let query = threshold_query(
        observer,
        target,
        "2023-09-04T00:00:00Z",
        168,
        ComponentMask::MOON,
    );
    let prepared =
        prepare_threshold(&evaluator, &query, utc_period_to_tt_mjd(query.window)).unwrap();
    let moon_periods = prepared
        .moon_visible_periods
        .as_ref()
        .expect("moon visibility context");
    let start = query.window.start.to_chrono().unwrap();
    let mut checked_down = false;
    let mut checked_up = false;

    for hour in 0..168 {
        let time = Time::<UTC>::from_chrono(start + Duration::hours(hour));
        let mjd = tt_time(time);
        let context =
            crate::planning::threshold::evaluate_integrated(&evaluator, &prepared, mjd).unwrap();
        let exact = evaluator
            .evaluate_moonlight(observer, time, target)
            .unwrap();

        if crate::planning::filters::contains_time(moon_periods, mjd) {
            assert!((context.value() - exact.integrated.value()).abs() < 1.0e-12);
            checked_up = true;
        } else {
            assert_eq!(context.value(), 0.0);
            assert_eq!(exact.integrated.value(), 0.0);
            checked_down = true;
        }
    }

    assert!(checked_down, "test window should include Moon-down samples");
    assert!(checked_up, "test window should include Moon-up samples");
}

#[test]
fn split_precision_partitions_keep_distinct_edges_and_interval_order() {
    let evaluator = continuum_default_evaluator();
    let start = parse("2026-01-01T00:00:00Z");
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, start + Second::new(1.0)),
        BandPhotonRadiance::new(0.0),
    )
    .with_components(ComponentMask::MOON)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let tt = utc_period_to_tt_mjd(query.window);
    let mut prepared = prepare_threshold(&evaluator, &query, tt).unwrap();
    let a = tt.start + Second::new(1e-9).to::<Day>();
    let b = tt.start + Second::new(3e-9).to::<Day>();
    assert!(a < b);
    assert_eq!(a.raw().value(), b.raw().value());
    prepared.moon_visible_periods = Some(std::sync::Arc::from(vec![TimePeriod::new(a, b)]));
    let windows = super::filters::threshold_windows(&prepared, false);
    assert_eq!(
        windows,
        vec![
            TimePeriod::new(tt.start, a),
            TimePeriod::new(a, b),
            TimePeriod::new(b, tt.end)
        ]
    );
    let mut periods = vec![
        TimePeriod::new(b, b + Second::new(1e-9).to::<Day>()),
        TimePeriod::new(tt.start, a),
    ];
    coalesce_periods(&mut periods);
    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].start, tt.start);
}

#[test]
fn target_eligibility_uses_the_upstream_frame_transformation() {
    use siderust::coordinates::transform::TransformFrame;
    let evaluator = continuum_default_evaluator();
    let start = parse("2026-01-01T00:00:00Z");
    let target = target_sgr_a();
    let query = ThresholdQuery::new(
        paranal(),
        target,
        Period::new(start, start + Second::new(86400.0)),
        BandPhotonRadiance::new(0.0),
    )
    .with_components(ComponentMask::empty())
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(Some(Degrees::new(20.0)));
    let tt = utc_period_to_tt_mjd(query.window);
    let prepared = prepare_threshold(&evaluator, &query, tt).unwrap();
    let icrs: direction::ICRS = target.to_frame();
    let expected = siderust_above_threshold(
        &icrs,
        &query.observer,
        tt,
        Degrees::new(20.0),
        SearchOpts::default(),
    );
    assert_eq!(prepared.candidate_windows, expected);
}

#[test]
fn native_search_can_partition_a_window_starting_in_a_leap_second() {
    let evaluator = continuum_default_evaluator();
    let start = parse("2016-12-31T23:59:60Z");
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, start + Second::new(2.0)),
        BandPhotonRadiance::new(1e6),
    )
    .with_components(ComponentMask::MOON)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let result = evaluator.periods_below_threshold(&query).unwrap();
    for period in result.periods {
        assert!(
            query.window.start <= period.start
                && period.start < period.end
                && period.end <= query.window.end
        );
    }
}

#[test]
fn invalid_geometry_and_filters_fail_before_eligibility_can_hide_them() {
    let evaluator = continuum_default_evaluator();
    let query = threshold_query(
        paranal(),
        target_sgr_a(),
        "2026-01-01T00:00:00Z",
        24,
        ComponentMask::ZODIACAL,
    )
    .with_sun_altitude_ceiling(None);
    for field in 0..7 {
        let mut q = query.clone();
        match field {
            0 => q.target = Target::new(Degrees::new(f64::NAN), Degrees::new(0.0)),
            1 => q.target = Target::new(Degrees::new(0.0), Degrees::new(f64::NAN)),
            2 => q.observer.lon = Degrees::new(f64::NAN),
            3 => q.observer.lat = Degrees::new(f64::NAN),
            4 => q.observer.height = Meters::new(f64::INFINITY),
            5 => q.sun_altitude_ceiling = Some(Degrees::new(f64::NAN)),
            _ => q.target_altitude_floor = Some(Degrees::new(f64::INFINITY)),
        }
        assert!(
            matches!(
                evaluator.periods_below_threshold(&q),
                Err(crate::NsbError::OutOfRange(_))
            ),
            "field {field}"
        );
        if field < 5 {
            let point = crate::PointQuery::new(q.observer, q.window.start, q.target)
                .with_components(ComponentMask::ZODIACAL);
            assert!(
                matches!(
                    evaluator.evaluate(&point),
                    Err(crate::NsbError::OutOfRange(_))
                ),
                "point field {field}"
            );
        }
    }
}

#[test]
fn corrected_frame_recovers_the_independently_validated_north_october_window() {
    use siderust::coordinates::transform::TransformFrame;
    let observer = Geodetic::<ECEF>::new_raw(
        Degrees::new(-17.892),
        Degrees::new(28.762),
        Meters::new(2200.0),
    );
    let target = Target::new(Degrees::new(51.17192), Degrees::new(-35.0));
    let icrs: direction::ICRS = target.to_frame();
    let start = parse("2026-01-01T00:00:00Z");
    let day_start = parse("2026-10-21T00:00:00Z");
    let day_end = day_start + Second::new(86400.0);
    for moon in [
        crate::MoonlightModel::Jones2013Spectral,
        crate::MoonlightModel::KrisciunasSchaefer1991,
    ] {
        let evaluator = super::test_fixtures::evaluator_with_moon(true, true, moon);
        let mut q = ThresholdQuery::new(
            observer,
            target,
            Period::new(start, start + Second::new(365.0 * 86400.0)),
            // The corrected zodiacal/Moon curves require 0.35 rather than the
            // historical 0.25. Independent per-second geometry and uncached
            // radiance checks below still validate the same ~7744 s window.
            BandPhotonRadiance::new(0.35),
        )
        .with_components(ComponentMask::from_bits(15).unwrap())
        .with_target_altitude_floor(Some(Degrees::new(20.0)));
        q.threshold += evaluator.evaluate_starlight(target).unwrap().integrated;
        let context = evaluator.prepare_site_window_context(&q).unwrap();
        let prepared = super::prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
        let out = evaluator
            .periods_below_threshold_with_context(&context, &q)
            .unwrap();
        let periods: Vec<_> = out
            .periods
            .into_iter()
            .filter(|w| day_start < w.end && w.start < day_end)
            .collect();
        assert_eq!(periods.len(), 1, "{moon:?}");
        let period = periods[0];
        let duration = (period.end - period.start).value();
        assert!(
            duration > 7700.0 && duration < 7800.0,
            "{moon:?}: {period:?}, {duration}"
        );
        eprintln!(
            "NORTH_OCTOBER_WINDOW {moon:?} {} {} {duration}",
            period.start.to_chrono().unwrap(),
            period.end.to_chrono().unwrap()
        );
        // Point samples are independent of the annual event enumeration. They
        // establish that the added schedule is eligible and physically below
        // threshold; the fine matrix alone shares the eligibility preparation.
        for second in 1..duration.floor() as i64 {
            let t = period.start + Second::new(second as f64);
            let tt = super::intervals::utc_time_to_tt_mjd(t);
            assert!(
                icrs.altitude_at(&observer, tt)
                    >= Degrees::new(20.0).to::<siderust::qtty::Radian>()
            );
            assert!(
                siderust::bodies::Sun.altitude_at(&observer, tt)
                    <= Degrees::new(-18.0).to::<siderust::qtty::Radian>()
            );
            assert!(
                super::reference_scan::evaluate_integrated(&evaluator, &prepared, tt).unwrap()
                    <= q.threshold
            );
        }
    }
}
