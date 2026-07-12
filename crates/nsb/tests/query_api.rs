use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::components::starlight::StarlightPixel;
use nsb::{
    CalibrationStatus, ComponentMask, MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery,
    SiteProfileId, Starlight, StarlightMap, StarlightModel, StarlightProvenance, Target,
    ThresholdQuery, DEG,
};
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use qtty::solid_angle::Steradians;
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

fn fixture_starlight_map_with_uncertainty() -> StarlightMap {
    let pixel = StarlightPixel::new(
        0.0 * DEG,
        0.0 * DEG,
        Steradians::new(1.0),
        BandPhotonRadiance::new(4.0),
        S10s::new(2.0),
        S10s::new(1.0),
    )
    .with_uncertainties(
        BandPhotonRadiance::new(0.4),
        BandPhotonRadiance::new(0.8),
        BandPhotonRadiance::new(1.0),
    );
    StarlightMap::from_pixels(vec![pixel], StarlightProvenance::test_fixture())
        .expect("uncertainty fixture")
}

#[test]
fn evaluator_defaults_to_generic_clear_sky_config() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let config = evaluator.config();
    assert_eq!(config.moonlight_model, MoonlightModel::Jones2013Spectral);
    assert_eq!(config.site_profile, SiteProfileId::GenericClearSky);
    assert_eq!(
        config.starlight_model.is_some(),
        Starlight::bundled_production_available()
    );
}

#[test]
fn default_model_config_is_generic_clear_sky() {
    let default = NsbModelConfig::default();
    let explicit = NsbModelConfig::generic_clear_sky();
    assert_eq!(default.moonlight_model, explicit.moonlight_model);
    assert_eq!(default.site_profile, SiteProfileId::GenericClearSky);
    assert_eq!(
        default.starlight_model.is_some(),
        Starlight::bundled_production_available()
    );
}

#[test]
fn cta_planning_configs_select_named_site_profiles() {
    let north = NsbModelConfig::cta_n_planning();
    let south = NsbModelConfig::cta_s_planning();

    assert_eq!(north.site_profile, SiteProfileId::CtaNorth);
    assert_eq!(south.site_profile, SiteProfileId::CtaSouth);
    assert_eq!(
        SiteProfileId::CtaNorth
            .profile(paranal())
            .calibration_status,
        CalibrationStatus::PlanningPreset
    );
    assert_eq!(
        SiteProfileId::CtaSouth
            .profile(paranal())
            .calibration_status,
        CalibrationStatus::PlanningPreset
    );
}

#[test]
fn all_components_are_the_production_safe_default() {
    assert_eq!(
        ComponentMask::ALL.contains(ComponentMask::STARLIGHT),
        Starlight::bundled_production_available()
    );

    let evaluator = NsbEvaluator::new().expect("evaluator");
    let result = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: sgr_a_star(),
            components: ComponentMask::ALL,
        })
        .expect("generic clear-sky ALL evaluates");

    assert!(result.integrated.value() > 0.0);
    assert!(result
        .components
        .iter()
        .any(|component| component.name == "zodiacal"));
    let starlight = result
        .components
        .iter()
        .find(|component| component.name == "starlight");
    assert_eq!(
        starlight.is_some(),
        Starlight::bundled_production_available()
    );
    if let Some(component) = starlight {
        assert!(component.integrated.value() > 0.0);
    }
    assert!(result
        .components
        .iter()
        .any(|component| component.name == "airglow"));
    assert!(result
        .components
        .iter()
        .any(|component| component.name == "moon"));
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

    let result = evaluator.evaluate(&PointQuery {
        observer: paranal(),
        time: parse_obstime("2023-09-04 01:48:00"),
        target: invalid_target(),
        components: ComponentMask::ZODIACAL,
    });

    assert!(
        result.is_err(),
        "invalid selected component input must fail point evaluation"
    );
}

#[test]
fn starlight_request_without_model_fails_explicitly() {
    let mut config = NsbModelConfig::generic_clear_sky();
    config.starlight_model = None;
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");
    let error = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: sgr_a_star(),
            components: ComponentMask::STARLIGHT,
        })
        .expect_err("unconfigured starlight must fail when requested");

    assert!(error.to_string().contains("starlight component requested"));
}

#[test]
fn custom_starlight_map_evaluates_when_explicitly_configured() {
    let config = NsbModelConfig::generic_clear_sky().with_starlight_model(
        StarlightModel::with_experimental_map(fixture_starlight_map()),
    );
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");

    let result = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: sgr_a_star(),
            components: ComponentMask::STARLIGHT,
        })
        .expect("explicit starlight map");

    assert_eq!(result.components.len(), 1);
    assert_eq!(result.components[0].name, "starlight");
    assert!(result.integrated.value() > 0.0);
}

#[test]
fn starlight_uncertainties_reach_nsb_component() {
    let config = NsbModelConfig::generic_clear_sky().with_starlight_model(
        StarlightModel::with_experimental_map(fixture_starlight_map_with_uncertainty()),
    );
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");
    let result = evaluator
        .evaluate(&PointQuery {
            observer: paranal(),
            time: parse_obstime("2023-09-04 01:48:00"),
            target: sgr_a_star(),
            components: ComponentMask::STARLIGHT,
        })
        .expect("starlight uncertainty evaluation");

    let component = &result.components[0];
    assert_eq!(component.statistical_uncertainty.unwrap().value(), 0.4);
    assert_eq!(component.systematic_uncertainty.unwrap().value(), 0.8);
    assert_eq!(component.total_uncertainty.unwrap().value(), 1.0);
    assert_eq!(component.relative_uncertainty, Some(0.25));
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

    let result = evaluator.periods_below_threshold(&ThresholdQuery {
        observer: paranal(),
        target: invalid_target(),
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(1.0e6),
        components: ComponentMask::ZODIACAL,
        sample_step: Second::new(600.0),
        sun_altitude_ceiling: None,
        target_altitude_floor: None,
    });

    assert!(
        result.is_err(),
        "threshold search must not treat a failed component as zero"
    );
}
