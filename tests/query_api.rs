use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{
    AirglowModel, ComponentMask, Location, MoonlightModel, NsbEvaluator, NsbModelConfig,
    PointQuery, Site, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::{Period, Time, UTC};

fn parse_obstime(s: &str) -> Time<UTC> {
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .expect("parse obstime");
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc);
    Time::<UTC>::from_chrono(dt)
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn default_components() -> ComponentMask {
    ComponentMask::ZODIACAL | ComponentMask::STARLIGHT | ComponentMask::AIRGLOW
}

#[test]
fn evaluator_defaults_to_best_science_config() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let config = evaluator.config();
    assert_eq!(config.airglow_model, AirglowModel::SkyCalcContinuum);
    assert_eq!(config.moonlight_model, MoonlightModel::Jones2013Spectral);
}

#[test]
fn python_parity_config_selects_legacy_models() {
    let config = NsbModelConfig::python_parity();
    assert_eq!(config.airglow_model, AirglowModel::PythonPolynomial);
    assert_eq!(
        config.moonlight_model,
        MoonlightModel::KrisciunasSchaefer1991
    );
}

#[test]
fn point_query_named_site_matches_geodetic_location() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let time = parse_obstime("2023-09-04 01:48:00");
    let target = sgr_a_star();
    let components = default_components();

    let named = evaluator
        .evaluate(&PointQuery {
            location: Location::NamedSite(Site::Paranal),
            time,
            target,
            components,
        })
        .expect("named-site query");

    let generic = evaluator
        .evaluate(&PointQuery {
            location: Location::Geodetic(Site::Paranal.geodetic()),
            time,
            target,
            components,
        })
        .expect("generic geodetic query");

    assert_eq!(named.integrated.value(), generic.integrated.value());
    assert_eq!(named.b_mag.value(), generic.b_mag.value());
    assert_eq!(named.v_mag.value(), generic.v_mag.value());
    assert_eq!(named.components.len(), generic.components.len());
}

#[test]
fn threshold_query_returns_full_window_for_large_threshold() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");
    let result = evaluator
        .periods_below_threshold(&ThresholdQuery {
            location: Location::NamedSite(Site::Paranal),
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
fn threshold_query_returns_empty_for_zero_threshold() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");
    let result = evaluator
        .periods_below_threshold(&ThresholdQuery {
            location: Location::NamedSite(Site::Paranal),
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
