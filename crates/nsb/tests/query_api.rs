use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{
    CalibrationStatus, ComponentMask, MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery,
    SiteProfileId, Starlight, StarlightMap, StarlightModel, StarlightProvenance, Target,
    ThresholdQuery, DEG,
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

fn fixture_starlight_map_with_uncertainty() -> StarlightMap {
    let mut raw = String::from(concat!(
        "# map_type=healpix\n",
        "# nside=1\n",
        "# ordering=ring\n",
        "# coordinate_frame=galactic\n",
        "# s10_diagnostics=not_provided\n",
        "healpix_index,integrated_ph_cm2_ns_sr,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    for index in 0..12 {
        raw.push_str(&format!("{index},4.0,0.4,0.8,1.0\n"));
    }
    StarlightMap::from_csv_str(&raw, StarlightProvenance::test_fixture())
        .expect("uncertainty fixture")
}

#[test]
fn default_evaluator_config_matches_generic_clear_sky() {
    let default = NsbModelConfig::default();
    let explicit = NsbModelConfig::generic_clear_sky();
    assert_eq!(default.moonlight_model, explicit.moonlight_model);
    assert_eq!(default.moonlight_model, MoonlightModel::Jones2013Spectral);
    assert_eq!(default.site_profile, SiteProfileId::GenericClearSky);
    assert_eq!(explicit.site_profile, SiteProfileId::GenericClearSky);
    assert_eq!(
        default.starlight_model.is_some(),
        Starlight::bundled_production_available()
    );

    let evaluator = NsbEvaluator::new().expect("evaluator");
    let config = evaluator.config();
    assert_eq!(config.moonlight_model, default.moonlight_model);
    assert_eq!(config.site_profile, SiteProfileId::GenericClearSky);
    assert_eq!(
        config.starlight_model.is_some(),
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
        .evaluate(
            &PointQuery::new(
                paranal(),
                parse_obstime("2023-09-04 01:48:00"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::ALL),
        )
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
        .evaluate(
            &PointQuery::new(
                paranal(),
                parse_obstime("2023-09-04 01:48:00"),
                sgr_a_star(),
            )
            .with_components(default_components()),
        )
        .expect("point query");

    assert!(result.integrated.value() > 0.0);
    assert!(!result.components.is_empty());
}

#[test]
fn point_query_propagates_selected_component_error() {
    let evaluator = NsbEvaluator::new().expect("evaluator");

    let result = evaluator.evaluate(
        &PointQuery::new(
            paranal(),
            parse_obstime("2023-09-04 01:48:00"),
            invalid_target(),
        )
        .with_components(ComponentMask::ZODIACAL),
    );

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
        .evaluate(
            &PointQuery::new(
                paranal(),
                parse_obstime("2023-09-04 01:48:00"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::STARLIGHT),
        )
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
        .evaluate(
            &PointQuery::new(
                paranal(),
                parse_obstime("2023-09-04 01:48:00"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::STARLIGHT),
        )
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
        .evaluate(
            &PointQuery::new(
                paranal(),
                parse_obstime("2023-09-04 01:48:00"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::STARLIGHT),
        )
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
        .periods_below_threshold(
            &ThresholdQuery::new(
                paranal(),
                sgr_a_star(),
                Period::new(start, end),
                BandPhotonRadiance::new(1.0e6),
            )
            .with_components(default_components())
            .with_sample_step(ThresholdQuery::DEFAULT_SAMPLE_STEP)
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(None),
        )
        .expect("threshold query");

    assert_eq!(result.periods.len(), 1);
    assert_eq!(result.periods[0].start, start);
    assert_eq!(result.periods[0].end, end);
}

#[test]
fn threshold_query_returns_no_periods_when_threshold_is_unreachable() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");
    let result = evaluator
        .periods_below_threshold(
            &ThresholdQuery::new(
                paranal(),
                sgr_a_star(),
                Period::new(start, end),
                BandPhotonRadiance::new(0.0),
            )
            .with_components(default_components())
            .with_sample_step(ThresholdQuery::DEFAULT_SAMPLE_STEP)
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(None),
        )
        .expect("threshold query");

    assert!(result.periods.is_empty());
}

#[test]
fn threshold_query_empty_window_returns_no_periods() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let result = evaluator
        .periods_below_threshold(
            &ThresholdQuery::new(
                paranal(),
                sgr_a_star(),
                Period::new(start, start),
                BandPhotonRadiance::new(1.0e6),
            )
            .with_components(default_components())
            .with_sample_step(ThresholdQuery::DEFAULT_SAMPLE_STEP)
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(None),
        )
        .expect("empty window");

    assert!(result.periods.is_empty());
}

#[test]
fn threshold_query_rejects_non_finite_threshold_and_non_positive_step() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");

    let nan = evaluator
        .periods_below_threshold(
            &ThresholdQuery::new(
                paranal(),
                sgr_a_star(),
                Period::new(start, end),
                BandPhotonRadiance::new(f64::NAN),
            )
            .with_components(default_components())
            .with_sample_step(Second::new(600.0))
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(None),
        )
        .expect_err("NaN threshold");
    assert!(nan.to_string().contains("threshold must be finite"));

    let step = evaluator
        .periods_below_threshold(
            &ThresholdQuery::new(
                paranal(),
                sgr_a_star(),
                Period::new(start, end),
                BandPhotonRadiance::new(0.2),
            )
            .with_components(default_components())
            .with_sample_step(Second::new(0.0))
            .with_sun_altitude_ceiling(None)
            .with_target_altitude_floor(None),
        )
        .expect_err("zero sample step");
    assert!(step.to_string().contains("sample_step"));
}

#[test]
fn point_query_evaluates_individual_supported_components() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let time = parse_obstime("2023-09-04 01:48:00");
    for (mask, expected_name) in [
        (ComponentMask::ZODIACAL, "zodiacal"),
        (ComponentMask::AIRGLOW, "airglow"),
        (ComponentMask::MOON, "moon"),
    ] {
        let result = evaluator
            .evaluate(&PointQuery::new(paranal(), time, sgr_a_star()).with_components(mask))
            .expect("component evaluates");
        assert_eq!(result.components.len(), 1);
        assert_eq!(result.components[0].name, expected_name);
        assert!(result.integrated.value().is_finite());
    }
}

#[test]
fn explicit_f107_configuration_changes_airglow_relative_to_automatic() {
    let time = parse_obstime("2023-09-04 01:48:00");
    let query =
        PointQuery::new(paranal(), time, sgr_a_star()).with_components(ComponentMask::AIRGLOW);

    let automatic = NsbEvaluator::new()
        .expect("evaluator")
        .evaluate(&query)
        .expect("automatic F10.7");
    let explicit = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_solar_radio_flux(nsb::SolarFluxUnits::new(250.0)),
    )
    .expect("explicit evaluator")
    .evaluate(&query)
    .expect("explicit F10.7");

    assert_ne!(automatic.integrated.value(), explicit.integrated.value());
    assert!(automatic.integrated.value().is_finite());
    assert!(explicit.integrated.value().is_finite());
}

#[test]
fn threshold_query_fails_closed_on_selected_component_error() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_obstime("2023-09-04 01:00:00");
    let end = parse_obstime("2023-09-04 02:00:00");

    let result = evaluator.periods_below_threshold(
        &ThresholdQuery::new(
            paranal(),
            invalid_target(),
            Period::new(start, end),
            BandPhotonRadiance::new(1.0e6),
        )
        .with_components(ComponentMask::ZODIACAL)
        .with_sample_step(Second::new(600.0))
        .with_sun_altitude_ceiling(None)
        .with_target_altitude_floor(None),
    );

    assert!(
        result.is_err(),
        "threshold search must not treat a failed component as zero"
    );
}
