use super::scan::{
    above_threshold_periods, coalesce_periods, complement_periods, tt_mjd_period_to_utc,
    tt_mjd_to_utc_time, utc_period_to_tt_mjd,
};
use super::types::{ThresholdQuery, ThresholdQueryResult};
use crate::components::airglow;
use crate::error::Result;
use crate::evaluator::{ComponentMask, NsbEvaluator, Observer, Target};
use chrono::{DateTime, Duration, Utc};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
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
        sample_step: Second::new(1_800.0),
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
    }
}

fn scan_threshold_periods(
    evaluator: &NsbEvaluator,
    query: &ThresholdQuery,
) -> Result<ThresholdQueryResult> {
    crate::planning::prepare::validate_threshold(query)?;
    let tt_window = utc_period_to_tt_mjd(query.window);
    let prepared = crate::planning::prepare::prepare_threshold(evaluator, query, tt_window)?;
    let step = query.sample_step.to::<Day>();
    let f = |mjd_tt: ModifiedJulianDate| -> Result<BandPhotonRadiance> {
        crate::planning::threshold::evaluate_integrated(evaluator, &prepared, mjd_tt)
    };

    let mut darker_periods = Vec::new();
    for candidate in &prepared.candidate_windows {
        let brighter = above_threshold_periods(*candidate, step, &f, query.threshold)?;
        darker_periods.extend(complement_periods(*candidate, &brighter));
    }
    coalesce_periods(&mut darker_periods);

    Ok(ThresholdQueryResult {
        threshold: query.threshold,
        periods: darker_periods
            .into_iter()
            .map(|period| tt_mjd_period_to_utc(period, prepared.tt_window, query.window))
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
fn threshold_airglow_does_not_use_point_night_search_hot_path() {
    let evaluator = NsbEvaluator::new().unwrap();
    let query = threshold_query(
        paranal(),
        target_sgr_a(),
        "2023-09-04T00:00:00Z",
        12,
        ComponentMask::AIRGLOW,
    );

    airglow::temporal::forbid_point_night_search_for_test(|| {
        evaluator.periods_below_threshold(&query).unwrap();
    });
}

#[test]
fn authoritative_threshold_search_matches_scan_oracle_for_representative_window() {
    let evaluator = NsbEvaluator::new().unwrap();
    let start = parse("2023-09-04T02:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(4));
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, end),
        BandPhotonRadiance::new(0.21),
    )
    .with_components(ComponentMask::ZODIACAL)
    .with_sample_step(Second::new(1_800.0))
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let authoritative = evaluator.periods_below_threshold(&query).unwrap();
    let scan = scan_threshold_periods(&evaluator, &query).unwrap();

    assert_periods_match_within_seconds(&authoritative, &scan, 2);
}

#[test]
fn authoritative_search_matches_exact_scan_across_components_and_year_boundary() {
    let evaluator = NsbEvaluator::new().unwrap();
    for (start, hours, target, components, threshold) in [
        (
            "2026-12-30T00:00:00Z",
            72,
            target_sgr_a(),
            ComponentMask::ALL,
            0.25,
        ),
        (
            "2026-01-14T00:00:00Z",
            72,
            Target::new(83.6331 * crate::DEG, 22.0145 * crate::DEG),
            ComponentMask::MOON,
            0.02,
        ),
        (
            "2026-06-01T00:00:00Z",
            48,
            Target::new(120.0 * crate::DEG, -10.0 * crate::DEG),
            ComponentMask::AIRGLOW | ComponentMask::ZODIACAL,
            0.20,
        ),
    ] {
        let mut query = threshold_query(paranal(), target, start, hours, components);
        query.threshold = BandPhotonRadiance::new(threshold);
        query.sample_step = Second::new(600.0);
        let authoritative = evaluator.periods_below_threshold(&query).unwrap();
        let scan = scan_threshold_periods(&evaluator, &query).unwrap();
        assert_periods_match_within_seconds(&authoritative, &scan, 2);
    }
}

#[test]
fn moon_rise_and_set_threshold_boundaries_match_exact_scan() {
    let evaluator = NsbEvaluator::new().unwrap();
    let start = parse("2026-01-14T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + Duration::hours(72));
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, end),
        BandPhotonRadiance::new(0.0),
    )
    .with_components(ComponentMask::MOON)
    .with_sample_step(Second::new(1_800.0))
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);

    let authoritative = evaluator.periods_below_threshold(&query).unwrap();
    let scan = scan_threshold_periods(&evaluator, &query).unwrap();
    assert!(!scan.periods.is_empty());
    assert_periods_match_within_seconds(&authoritative, &scan, 2);
}

#[test]
fn reusable_site_context_matches_independent_queries_for_multiple_targets() {
    let evaluator = NsbEvaluator::new().unwrap();
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
    let evaluator = NsbEvaluator::new().unwrap();
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
    let evaluator = NsbEvaluator::new().unwrap();
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
    let evaluator = NsbEvaluator::new().unwrap();
    let other = NsbEvaluator::new().unwrap();
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
}

#[test]
fn target_visibility_matches_precise_altitude_scan_across_regimes() {
    let evaluator = NsbEvaluator::new().unwrap();
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
        let prepared =
            crate::planning::prepare::prepare_threshold(&evaluator, &query, tt_window).unwrap();
        let direction = direction::ICRS::new(target.ra(), target.dec());
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
    let evaluator = NsbEvaluator::new().unwrap();
    let observer = Geodetic::new_raw(
        Degrees::new(-70.3147),
        Degrees::new(-24.6834),
        Meters::new(2_147.0),
    );
    let target = Target::new(0.0 * crate::DEG, 45.0 * crate::DEG);
    let target_dir = direction::ICRS::new(target.ra(), target.dec());
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

    let prepared =
        crate::planning::prepare::prepare_threshold(&evaluator, &query, tt_window).unwrap();
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
    let context = NsbEvaluator::new()
        .unwrap()
        .prepare_site_window_context(&query)
        .unwrap();
    assert_eq!(context.moon_visible_periods.as_deref().unwrap(), precise);
}

#[test]
fn empty_visibility_candidates_produce_no_periods() {
    let evaluator = NsbEvaluator::new().unwrap();
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
    let evaluator = NsbEvaluator::new().unwrap();
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
    let prepared = crate::planning::prepare::prepare_threshold(
        &evaluator,
        &query,
        utc_period_to_tt_mjd(query.window),
    )
    .unwrap();

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
    let evaluator = NsbEvaluator::new().unwrap();
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
    let prepared = crate::planning::prepare::prepare_threshold(
        &evaluator,
        &query,
        utc_period_to_tt_mjd(query.window),
    )
    .unwrap();

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
    let evaluator = NsbEvaluator::new().unwrap();
    let observer = paranal();
    let target = target_sgr_a();
    let query = threshold_query(
        observer,
        target,
        "2023-09-04T00:00:00Z",
        168,
        ComponentMask::MOON,
    );
    let prepared = crate::planning::prepare::prepare_threshold(
        &evaluator,
        &query,
        utc_period_to_tt_mjd(query.window),
    )
    .unwrap();
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
