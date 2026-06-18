use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{
    ComponentMask, MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery, StarlightMap,
    StarlightModel, StarlightProvenance, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

fn parse_obstime(s: &str) -> Time<UTC> {
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .expect("parse obstime");
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc);
    Time::<UTC>::from_chrono(dt)
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn invalid_target() -> Target {
    Target::new(f64::NAN * DEG, 0.0 * DEG)
}

fn default_components() -> ComponentMask {
    ComponentMask::ZODIACAL | ComponentMask::AIRGLOW
}

fn fixture_starlight_map() -> StarlightMap {
    StarlightMap::from_csv_str(
        include_str!("data/starlight_fixture_map.csv"),
        StarlightProvenance::test_fixture(),
    )
    .expect("starlight fixture")
}

#[test]
fn evaluator_defaults_to_standard_science_config() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let config = evaluator.config();
    assert_eq!(config.moonlight_model, MoonlightModel::Jones2013Spectral);
}

#[test]
fn python_parity_config_selects_legacy_moon_model() {
    let config = NsbModelConfig::python_parity();
    assert_eq!(
        config.moonlight_model,
        MoonlightModel::KrisciunasSchaefer1991
    );
}

#[test]
fn point_query_uses_direct_geodetic_observer() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let result = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: sgr_a_star(),
            components: default_components(),
        })
        .expect("point query");

    assert!(result.integrated.value() > 0.0);
    assert!(!result.components.is_empty());
}

#[test]
fn point_query_propagates_selected_component_error() {
    let evaluator = NsbEvaluator::new().expect("evaluator");

    let err = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: invalid_target(),
            components: ComponentMask::ZODIACAL,
        })
        .expect_err("invalid selected component input must fail point evaluation");

    assert!(
        err.to_string().contains("not finite"),
        "unexpected error: {err}"
    );
}

#[test]
fn threshold_query_returns_full_window_for_large_threshold() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");
    let result = evaluator
        .periods_below_threshold(&ThresholdQuery {
            observer: paranal(),
            target: sgr_a_star(),
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(1.0e6),
            components: default_components(),
            sample_step: ThresholdQuery::DEFAULT_SAMPLE_STEP,
            sun_altitude_ceiling: None,
            target_altitude_floor: None,
        })
        .expect("threshold query");

    assert_eq!(result.periods.len(), 1);
    assert_eq!(result.periods[0].start, start);
    assert_eq!(result.periods[0].end, end);
}

#[test]
fn threshold_query_fails_closed_on_selected_component_error() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");

    let err = evaluator
        .periods_below_threshold(&ThresholdQuery {
            observer: paranal(),
            target: invalid_target(),
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(1.0e6),
            components: ComponentMask::ZODIACAL,
            sample_step: Second::new(600.0),
            sun_altitude_ceiling: None,
            target_altitude_floor: None,
        })
        .expect_err("threshold search must not treat a failed component as zero");

    assert!(
        err.to_string().contains("not finite"),
        "unexpected error: {err}"
    );
}

#[test]
fn threshold_query_returns_empty_for_zero_threshold() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");
    let result = evaluator
        .periods_below_threshold(&ThresholdQuery {
            observer: paranal(),
            target: sgr_a_star(),
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(0.0),
            components: default_components(),
            sample_step: Second::new(600.0),
            sun_altitude_ceiling: None,
            target_altitude_floor: None,
        })
        .expect("threshold query");

    assert!(result.periods.is_empty());
}

#[test]
fn threshold_starlight_is_target_dependent() {
    let mut config = NsbModelConfig::best_science();
    config.starlight_model = StarlightModel::with_map(fixture_starlight_map());
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");

    let query = |target| ThresholdQuery {
        observer: paranal(),
        target,
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(3.0),
        components: ComponentMask::STARLIGHT,
        sample_step: Second::new(600.0),
        sun_altitude_ceiling: None,
        target_altitude_floor: None,
    };

    let galactic_center = evaluator
        .periods_below_threshold(&query(Target::new(266.4051 * DEG, -28.936175 * DEG)))
        .expect("galactic-center starlight query");
    let north_galactic_pole = evaluator
        .periods_below_threshold(&query(Target::new(192.85948 * DEG, 27.12825 * DEG)))
        .expect("north-galactic-pole starlight query");

    assert!(galactic_center.periods.is_empty());
    assert_eq!(north_galactic_pole.periods.len(), 1);
    assert_eq!(north_galactic_pole.periods[0].start, start);
    assert_eq!(north_galactic_pole.periods[0].end, end);
}
