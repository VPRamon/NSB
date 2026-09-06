//! End-to-end validation gates for the generic clear-sky NSB evaluator.
//!
//! These tests intentionally exercise the public `NsbEvaluator` API rather than
//! component internals. They cover point evaluation, component composition,
//! Galactic-contrast behaviour with an explicit starlight fixture, and threshold
//! windows checked against independent sampled curves / observability intervals.

use chrono::{DateTime, Duration, Utc};
use nsb::{
    ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, Starlight, StarlightMap,
    StarlightModel, StarlightProvenance, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const COMPONENT_SUM_TOLERANCE: f64 = 1.0e-12;
const EVENT_BOUNDARY_TOLERANCE_SECS: f64 = 120.0;

#[derive(Debug, Clone, Copy)]
struct CompositionCase {
    name: &'static str,
    time: &'static str,
    target: Target,
    components: ComponentMask,
    expected_components: &'static [&'static str],
}

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

fn ctao_s() -> Geodetic<ECEF> {
    siderust::catalogs::observatories::EL_PARANAL.geodetic()
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn north_galactic_pole() -> Target {
    Target::new(192.85948 * DEG, 27.12825 * DEG)
}

fn crab_nebula() -> Target {
    Target::new(83.6331 * DEG, 22.0145 * DEG)
}

fn fixture_starlight_map() -> StarlightMap {
    StarlightMap::from_csv_str(
        include_str!("data/starlight_fixture_map.csv"),
        StarlightProvenance::test_fixture(),
    )
    .expect("starlight fixture")
}

fn expected_all_components() -> &'static [&'static str] {
    if Starlight::bundled_production_available() {
        &["zodiacal", "starlight", "airglow", "moon"]
    } else {
        &["zodiacal", "airglow", "moon"]
    }
}

#[test]
fn production_all_preserves_component_composition_and_scene_contrast() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let cases = [
        CompositionCase {
            name: "dark-time high Galactic latitude",
            time: "2023-09-04T01:48:00Z",
            target: north_galactic_pole(),
            components: ComponentMask::ALL,
            expected_components: expected_all_components(),
        },
        CompositionCase {
            name: "near Galactic plane planning field",
            time: "2023-09-04T01:48:00Z",
            target: sgr_a_star(),
            components: ComponentMask::ALL,
            expected_components: expected_all_components(),
        },
        CompositionCase {
            name: "bright-Moon field",
            // 05:00 UTC keeps the Moon above the horizon for Crab at CTAO-S;
            // 04:00 previously produced a zero moonlight contribution.
            time: "2023-09-29T05:00:00Z",
            target: crab_nebula(),
            components: ComponentMask::ALL,
            expected_components: expected_all_components(),
        },
        CompositionCase {
            name: "astronomical-twilight boundary",
            time: "2023-09-04T23:30:00Z",
            target: sgr_a_star(),
            components: ComponentMask::ALL,
            expected_components: expected_all_components(),
        },
    ];

    let mut totals = std::collections::BTreeMap::<&'static str, f64>::new();
    let mut moon_contribution = None;

    for case in cases {
        let result = evaluator
            .evaluate(
                &PointQuery::new(ctao_s(), parse(case.time), case.target)
                    .with_components(case.components),
            )
            .unwrap_or_else(|err| panic!("{} failed: {err}", case.name));

        let integrated = result.integrated.value();
        assert!(
            integrated.is_finite() && integrated > 0.0,
            "{} produced non-physical total {integrated}",
            case.name
        );

        let component_names: Vec<_> = result
            .components
            .iter()
            .map(|component| component.name)
            .collect();
        assert_eq!(component_names, case.expected_components, "{}", case.name);

        let component_sum: f64 = result
            .components
            .iter()
            .map(|component| component.integrated.value())
            .sum();
        assert!(
            (integrated - component_sum).abs() <= COMPONENT_SUM_TOLERANCE * integrated.max(1.0),
            "{} total {integrated} is not the sum of reported components {component_sum}",
            case.name
        );

        totals.insert(case.name, integrated);
        if case.name == "bright-Moon field" {
            let moon = result
                .components
                .iter()
                .find(|component| component.name == "moon")
                .expect("moon component")
                .integrated
                .value();
            moon_contribution = Some(moon);
        }
    }

    let dark = totals["dark-time high Galactic latitude"];
    let moonlit = totals["bright-Moon field"];
    let moon = moon_contribution.expect("moon contribution");
    assert!(
        moon > 0.5,
        "bright-Moon scene must include substantial moonlight, got {moon}"
    );
    assert!(
        moonlit > 2.0 * dark,
        "moonlit Crab total {moonlit} should dominate dark-time NGP total {dark}"
    );
}

#[test]
fn explicit_starlight_with_fixture_preserves_galactic_contrast() {
    let mut config = NsbModelConfig::generic_clear_sky();
    config.starlight_model = Some(StarlightModel::with_experimental_map(
        fixture_starlight_map(),
    ));
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");

    let evaluate = |target| {
        evaluator
            .evaluate(
                &PointQuery::new(ctao_s(), parse("2023-09-04T01:48:00Z"), target)
                    .with_components(ComponentMask::ALL | ComponentMask::STARLIGHT),
            )
            .expect("all-supported point query")
    };

    let plane = evaluate(sgr_a_star());
    let pole = evaluate(north_galactic_pole());

    let starlight = |result: &nsb::NsbResult| {
        result
            .components
            .iter()
            .find(|component| component.name == "starlight")
            .expect("starlight component")
            .integrated
            .value()
    };

    let plane_starlight = starlight(&plane);
    let pole_starlight = starlight(&pole);
    assert!(
        plane_starlight > 1.5 * pole_starlight,
        "fixture map should keep the Galactic plane brighter than the pole: {plane_starlight} <= {pole_starlight}"
    );

    for result in [&plane, &pole] {
        let total = result.integrated.value();
        let component_sum: f64 = result
            .components
            .iter()
            .map(|component| component.integrated.value())
            .sum();
        assert!(
            (total - component_sum).abs() <= COMPONENT_SUM_TOLERANCE * total.max(1.0),
            "explicit total {total} is not the sum of reported components {component_sum}"
        );
    }
}

#[test]
fn threshold_windows_match_independent_sampled_curve() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let observer = ctao_s();
    let target = sgr_a_star();
    let start = parse("2023-09-05T00:00:00Z");
    let end = parse("2023-09-05T06:00:00Z");
    let samples = sampled_curve(
        &evaluator,
        observer,
        target,
        start,
        end,
        ComponentMask::AIRGLOW,
    );

    let min = samples
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::INFINITY, f64::min);
    let max = samples
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max > min,
        "sampled reference curve must vary over the night"
    );

    let threshold = BandPhotonRadiance::new(0.5 * (min + max));
    let result = evaluator
        .periods_below_threshold(
            &ThresholdQuery::new(observer, target, Period::new(start, end), threshold)
                .with_components(ComponentMask::AIRGLOW)
                .with_sample_step(Second::new(3_600.0))
                .with_sun_altitude_ceiling(None)
                .with_target_altitude_floor(None),
        )
        .expect("threshold query");

    let mut saw_below = false;
    let mut saw_above = false;
    for (time, integrated) in samples {
        let expected_below = integrated <= threshold.value();
        saw_below |= expected_below;
        saw_above |= !expected_below;
        assert_eq!(
            expected_below,
            periods_contain(&result.periods, time),
            "sampled reference mismatch at {:?}: value={integrated}, threshold={}",
            time.to_chrono().unwrap(),
            threshold.value()
        );
    }

    assert!(saw_below, "reference curve should contain darker samples");
    assert!(saw_above, "reference curve should contain brighter samples");
    assert!(
        !result.periods.is_empty(),
        "threshold search should report at least one dark window"
    );
}

#[test]
fn unrestrictive_threshold_windows_match_independent_observability_intervals() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let observer = ctao_s();
    let target = sgr_a_star();
    let start = parse("2023-09-04T00:00:00Z");
    let end = parse("2023-09-05T00:00:00Z");
    let query = ThresholdQuery::new(
        observer,
        target,
        Period::new(start, end),
        BandPhotonRadiance::new(1.0e12),
    )
    .with_components(ComponentMask::AIRGLOW)
    .with_sample_step(ThresholdQuery::DEFAULT_SAMPLE_STEP)
    .with_sun_altitude_ceiling(Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING))
    .with_target_altitude_floor(Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR));

    let actual = evaluator
        .periods_below_threshold(&query)
        .expect("threshold query");
    let expected = independent_observability_intervals(&query);

    assert!(
        !expected.is_empty(),
        "expected at least one observable interval"
    );
    assert_eq!(
        actual.periods.len(),
        expected.len(),
        "unrestrictive threshold should return the independently computed observability intervals"
    );

    for (actual, expected) in actual.periods.iter().zip(expected.iter()) {
        let actual_tt = utc_period_to_tt_mjd(actual);
        assert!(
            mjd_seconds_diff(actual_tt.start, expected.start) <= EVENT_BOUNDARY_TOLERANCE_SECS,
            "actual start {:?} does not match expected TT MJD {}",
            actual.start.to_chrono().unwrap(),
            expected.start.raw().value()
        );
        assert!(
            mjd_seconds_diff(actual_tt.end, expected.end) <= EVENT_BOUNDARY_TOLERANCE_SECS,
            "actual end {:?} does not match expected TT MJD {}",
            actual.end.to_chrono().unwrap(),
            expected.end.raw().value()
        );
    }
}

fn sampled_curve(
    evaluator: &NsbEvaluator,
    observer: Geodetic<ECEF>,
    target: Target,
    start: Time<UTC>,
    end: Time<UTC>,
    components: ComponentMask,
) -> Vec<(Time<UTC>, f64)> {
    let mut out = Vec::new();
    let mut t = start.to_chrono().unwrap() + Duration::hours(1);
    let end = end.to_chrono().unwrap();
    while t < end {
        let time = Time::<UTC>::from_chrono(t);
        let value = evaluator
            .evaluate(&PointQuery::new(observer, time, target).with_components(components))
            .expect("sampled point query")
            .integrated
            .value();
        out.push((time, value));
        t += Duration::hours(1);
    }
    out
}

fn periods_contain(periods: &[Period<UTC>], time: Time<UTC>) -> bool {
    periods
        .iter()
        .any(|period| period.start <= time && time <= period.end)
}

fn independent_observability_intervals(
    query: &ThresholdQuery,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let tt_window = utc_period_to_tt_mjd(&query.window);
    let sun_below = SunBody.below_threshold(
        &query.observer,
        tt_window,
        query.sun_altitude_ceiling.expect("sun altitude ceiling"),
        SearchOpts::default(),
    );
    let target_dir = direction::ICRS::new(query.target.ra(), query.target.dec());
    let target_above = target_dir.above_threshold(
        &query.observer,
        tt_window,
        query.target_altitude_floor.expect("target altitude floor"),
        SearchOpts::default(),
    );
    intersect_periods(&sun_below, &target_above)
}

fn utc_period_to_tt_mjd(window: &Period<UTC>) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        utc_time_to_tt_mjd(window.start),
        utc_time_to_tt_mjd(window.end),
    )
}

fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn mjd_seconds_diff(a: ModifiedJulianDate, b: ModifiedJulianDate) -> f64 {
    (a.raw().value() - b.raw().value()).abs() * 86_400.0
}
