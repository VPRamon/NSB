//! End-to-end validation gates for the production NSB evaluator.
//!
//! These tests intentionally exercise the public `NsbEvaluator` API rather than
//! component internals.  They cover point evaluation, component composition,
//! Galactic-contrast behaviour with an explicit starlight fixture, and threshold
//! windows checked against independent sampled curves / observability intervals.

use chrono::{DateTime, Duration, Utc};
use nsb::{
    ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightMap, StarlightModel,
    StarlightProvenance, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::coordinates::spherical::direction;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::qtty::{Degrees, Meters};
use siderust::time::{intersect_periods, Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const COMPONENT_SUM_TOLERANCE: f64 = 1.0e-12;
const EVENT_BOUNDARY_TOLERANCE_SECS: f64 = 120.0;

#[derive(Debug, Clone, Copy)]
struct ReferenceEnvelope {
    name: &'static str,
    time: &'static str,
    target: Target,
    components: ComponentMask,
    accepted_min: f64,
    accepted_max: f64,
    expected_components: &'static [&'static str],
}

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

fn ctao_s() -> Geodetic<ECEF> {
    Geodetic::<ECEF>::new_raw(
        Degrees::new(-70.406944),
        Degrees::new(-24.627222),
        Meters::new(2100.0),
    )
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

#[test]
fn production_all_matches_reference_envelopes() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let cases = [
        ReferenceEnvelope {
            name: "dark-time high Galactic latitude",
            time: "2023-09-04T01:48:00Z",
            target: north_galactic_pole(),
            components: ComponentMask::ALL,
            accepted_min: 1.0e-8,
            accepted_max: 1.0e9,
            expected_components: &["zodiacal", "airglow", "moon"],
        },
        ReferenceEnvelope {
            name: "near Galactic plane planning field",
            time: "2023-09-04T01:48:00Z",
            target: sgr_a_star(),
            components: ComponentMask::ALL,
            accepted_min: 1.0e-8,
            accepted_max: 1.0e9,
            expected_components: &["zodiacal", "airglow", "moon"],
        },
        ReferenceEnvelope {
            name: "bright-Moon field",
            time: "2023-09-29T04:00:00Z",
            target: crab_nebula(),
            components: ComponentMask::ALL,
            accepted_min: 1.0e-8,
            accepted_max: 1.0e12,
            expected_components: &["zodiacal", "airglow", "moon"],
        },
        ReferenceEnvelope {
            name: "astronomical-twilight boundary",
            time: "2023-09-04T23:30:00Z",
            target: sgr_a_star(),
            components: ComponentMask::ALL,
            accepted_min: 1.0e-8,
            accepted_max: 1.0e12,
            expected_components: &["zodiacal", "airglow", "moon"],
        },
    ];

    for case in cases {
        let result = evaluator
            .evaluate(&PointQuery {
                observer: ctao_s(),
                time: parse(case.time),
                target: case.target,
                components: case.components,
            })
            .unwrap_or_else(|err| panic!("{} failed: {err}", case.name));

        let integrated = result.integrated.value();
        assert!(
            integrated.is_finite() && integrated > 0.0,
            "{} produced non-physical total {integrated}",
            case.name
        );
        assert!(
            (case.accepted_min..=case.accepted_max).contains(&integrated),
            "{} total {integrated} outside accepted [{}, {}] envelope",
            case.name,
            case.accepted_min,
            case.accepted_max
        );

        let component_names: Vec<_> = result.components.iter().map(|component| component.name).collect();
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
    }
}

#[test]
fn all_supported_with_fixture_starlight_preserves_galactic_contrast() {
    let mut config = NsbModelConfig::standard();
    config.starlight_model = StarlightModel::with_map(fixture_starlight_map());
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");

    let evaluate = |target| {
        evaluator
            .evaluate(&PointQuery {
                observer: ctao_s(),
                time: parse("2023-09-04T01:48:00Z"),
                target,
                components: ComponentMask::ALL_SUPPORTED,
            })
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
            "all-supported total {total} is not the sum of reported components {component_sum}"
        );
    }
}

#[test]
fn threshold_windows_match_independent_sampled_curve() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let observer = ctao_s();
    let target = sgr_a_star();
    let start = parse("2023-09-04T00:00:00Z");
    let end = parse("2023-09-05T00:00:00Z");
    let samples = sampled_curve(&evaluator, observer, target, start, end, ComponentMask::ALL);

    let min = samples
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::INFINITY, f64::min);
    let max = samples
        .iter()
        .map(|(_, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(max > min, "sampled reference curve must vary over the night");

    let threshold = BandPhotonRadiance::new(0.5 * (min + max));
    let result = evaluator
        .periods_below_threshold(&ThresholdQuery {
            observer,
            target,
            window: Period::new(start, end),
            threshold,
            components: ComponentMask::ALL,
            sample_step: Second::new(600.0),
            sun_altitude_ceiling: None,
            target_altitude_floor: None,
        })
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
    assert!(!result.periods.is_empty(), "threshold search should report at least one dark window");
}

#[test]
fn unrestrictive_threshold_windows_match_independent_observability_intervals() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let observer = ctao_s();
    let target = sgr_a_star();
    let start = parse("2023-09-04T00:00:00Z");
    let end = parse("2023-09-05T00:00:00Z");
    let query = ThresholdQuery {
        observer,
        target,
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(1.0e12),
        components: ComponentMask::ALL,
        sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
    };

    let actual = evaluator
        .periods_below_threshold(&query)
        .expect("threshold query");
    let expected = independent_observability_intervals(&query);

    assert!(!expected.is_empty(), "expected at least one observable interval");
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
    let mut t = start.to_chrono().unwrap() + Duration::minutes(15);
    let end = end.to_chrono().unwrap();
    while t < end {
        let time = Time::<UTC>::from_chrono(t);
        let value = evaluator
            .evaluate(&PointQuery {
                observer,
                time,
                target,
                components,
            })
            .expect("sampled point query")
            .integrated
            .value();
        out.push((time, value));
        t += Duration::minutes(15);
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
    TimePeriod::new(utc_time_to_tt_mjd(window.start), utc_time_to_tt_mjd(window.end))
}

fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn mjd_seconds_diff(a: ModifiedJulianDate, b: ModifiedJulianDate) -> f64 {
    (a.raw().value() - b.raw().value()).abs() * 86_400.0
}
