use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{
    bundled_f107_store, Airglow, AirglowGeometryModel, AirglowScientificProfile,
    AtmosphericConditions, CalibrationStatus, ComponentCalibrationStatus, ComponentMask,
    NsbEvaluator, NsbModelConfig, PointQuery, ScaleFactors, SiteProfileId, SolarFluxUnits, Target,
    VanRhijnConfig, DEG,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Meters};
use std::sync::Arc;
use tempoch::{Time, UTC};

fn observer(lon_deg: f64, lat_deg: f64, height_m: f64) -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(lon_deg),
        Degrees::new(lat_deg),
        Meters::new(height_m),
    )
}

fn paranal() -> Geodetic<ECEF> {
    observer(-70.4044, -24.6275, 2_635.0)
}

fn arbitrary_location() -> Geodetic<ECEF> {
    observer(18.4, -33.9, 120.0)
}

fn parse_obstime(input: &str) -> Time<UTC> {
    let naive = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M:%S").unwrap();
    Time::<UTC>::from_chrono(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn target() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn descriptor_status(
    config: NsbModelConfig,
    location: Geodetic<ECEF>,
) -> ComponentCalibrationStatus {
    let evaluator = NsbEvaluator::with_config(config).unwrap();
    evaluator
        .describe_components(location, ComponentMask::AIRGLOW)
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .metadata
        .status
}

#[test]
fn scientific_profile_is_machine_readable_and_uses_site_maturity_as_source_of_truth() {
    let generic = AirglowScientificProfile::BuiltIn(SiteProfileId::GenericClearSky);
    let north = AirglowScientificProfile::BuiltIn(SiteProfileId::CtaNorth);
    let south = AirglowScientificProfile::BuiltIn(SiteProfileId::CtaSouth);
    let custom = AirglowScientificProfile::UnvalidatedCustomContinuum;

    assert_eq!(generic.as_str(), "generic-clear-sky");
    assert_eq!(north.as_str(), "ctao-north-planning");
    assert_eq!(south.as_str(), "ctao-south-planning");
    assert_eq!(custom.as_str(), "unvalidated-custom-continuum");

    assert_eq!(generic.site_profile(), Some(SiteProfileId::GenericClearSky));
    assert_eq!(north.site_profile(), Some(SiteProfileId::CtaNorth));
    assert_eq!(south.site_profile(), Some(SiteProfileId::CtaSouth));
    assert_eq!(custom.site_profile(), None);

    assert_eq!(
        generic.calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert_eq!(
        north.calibration_status(),
        CalibrationStatus::PlanningPreset
    );
    assert_eq!(
        south.calibration_status(),
        CalibrationStatus::PlanningPreset
    );
    assert_eq!(
        custom.calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert!(!generic.is_site_calibrated());
    assert!(!north.is_site_calibrated());
    assert!(!south.is_site_calibrated());
    assert!(!custom.is_site_calibrated());
}

#[test]
fn component_metadata_status_is_deliberately_derived_from_site_calibration_status() {
    assert_eq!(
        ComponentCalibrationStatus::from(CalibrationStatus::GenericFallback),
        ComponentCalibrationStatus::GenericClearSky
    );
    assert_eq!(
        ComponentCalibrationStatus::from(CalibrationStatus::PlanningPreset),
        ComponentCalibrationStatus::PlanningPreset
    );
    assert_eq!(
        ComponentCalibrationStatus::from(CalibrationStatus::Calibrated),
        ComponentCalibrationStatus::Production
    );
}

#[test]
fn arbitrary_location_and_paranal_are_generic_without_explicit_profile_selection() {
    for location in [arbitrary_location(), paranal()] {
        let model = Airglow::standard_clear_sky(location).unwrap();
        assert_eq!(
            model.scientific_profile(),
            AirglowScientificProfile::BuiltIn(SiteProfileId::GenericClearSky)
        );
        assert_eq!(
            model.calibration_status(),
            CalibrationStatus::GenericFallback
        );
        assert!(!model.is_site_calibrated());
    }

    let output = Airglow::standard_clear_sky(arbitrary_location())
        .unwrap()
        .compute(parse_obstime("2023-06-21 22:00:00"), target())
        .unwrap();
    assert!(output.integrated.value().is_finite());
    assert!(output.integrated.value() >= 0.0);
}

#[test]
fn explicit_ctao_profiles_remain_planning_presets_at_any_observer_location() {
    for profile in [SiteProfileId::CtaNorth, SiteProfileId::CtaSouth] {
        let model = Airglow::for_site_profile(arbitrary_location(), profile).unwrap();
        assert_eq!(
            model.scientific_profile(),
            AirglowScientificProfile::BuiltIn(profile)
        );
        assert_eq!(
            model.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert!(!model.is_site_calibrated());
        assert_eq!(
            profile.calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert!(!profile.is_site_calibrated());
    }

    assert_eq!(
        SiteProfileId::GenericClearSky.calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert!(!SiteProfileId::GenericClearSky.is_site_calibrated());
}

#[test]
fn direct_airglow_operational_builders_cannot_upgrade_maturity() {
    let location = paranal();
    let expected = CalibrationStatus::GenericFallback;
    let model = Airglow::standard_clear_sky(location).unwrap();
    assert_eq!(model.calibration_status(), expected);

    let geometry =
        AirglowGeometryModel::VanRhijn(VanRhijnConfig::new(Kilometers::new(105.0)).unwrap());
    let model = model.with_geometry(geometry);
    assert_eq!(model.calibration_status(), expected);

    let model = model.with_solar_radio_flux(SolarFluxUnits::new(220.0));
    assert_eq!(model.calibration_status(), expected);

    let model = model.with_f10_7(SolarFluxUnits::new(95.0));
    assert_eq!(model.calibration_status(), expected);

    let model = model.with_atmosphere(AtmosphericConditions::paranal_average());
    assert_eq!(model.calibration_status(), expected);

    let model = model.with_scale(ScaleFactors::new(1.75));
    assert_eq!(model.calibration_status(), expected);
    assert!(!model.is_site_calibrated());
}

#[test]
fn model_config_maturity_is_invariant_under_f107_geometry_and_observer_changes() {
    let generic = NsbModelConfig::generic_clear_sky();
    assert_eq!(
        generic.airglow_scientific_profile(),
        AirglowScientificProfile::BuiltIn(SiteProfileId::GenericClearSky)
    );
    assert_eq!(
        generic.airglow_calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert!(!generic.is_airglow_site_calibrated());

    let explicit_f107 = generic
        .clone()
        .with_solar_radio_flux(SolarFluxUnits::new(170.0));
    assert_eq!(
        explicit_f107.airglow_calibration_status(),
        CalibrationStatus::GenericFallback
    );

    let dataset = Arc::new(bundled_f107_store().unwrap().clone());
    let dataset_f107 = generic.clone().with_f107_store(dataset);
    assert_eq!(
        dataset_f107.airglow_calibration_status(),
        CalibrationStatus::GenericFallback
    );

    let changed_geometry = generic
        .clone()
        .with_airglow_geometry(AirglowGeometryModel::VanRhijn(
            VanRhijnConfig::new(Kilometers::new(110.0)).unwrap(),
        ));
    assert_eq!(
        changed_geometry.airglow_calibration_status(),
        CalibrationStatus::GenericFallback
    );

    assert_eq!(
        descriptor_status(generic.clone(), arbitrary_location()),
        ComponentCalibrationStatus::GenericClearSky
    );
    assert_eq!(
        descriptor_status(generic, paranal()),
        ComponentCalibrationStatus::GenericClearSky
    );
}

#[test]
fn explicit_planning_config_and_result_metadata_agree_without_calibration_promotion() {
    for config in [
        NsbModelConfig::cta_n_planning(),
        NsbModelConfig::cta_s_planning(),
    ] {
        assert_eq!(
            config.airglow_calibration_status(),
            CalibrationStatus::PlanningPreset
        );
        assert!(!config.is_airglow_site_calibrated());
        assert_eq!(
            descriptor_status(config.clone(), arbitrary_location()),
            ComponentCalibrationStatus::PlanningPreset
        );

        let evaluator = NsbEvaluator::with_config(config).unwrap();
        let result = evaluator
            .evaluate(
                &PointQuery::new(paranal(), parse_obstime("2023-09-04 01:48:00"), target())
                    .with_components(ComponentMask::AIRGLOW),
            )
            .unwrap();
        let airglow = result
            .components
            .iter()
            .find(|component| component.name == "airglow")
            .unwrap();
        assert_eq!(
            airglow.metadata.status,
            ComponentCalibrationStatus::PlanningPreset
        );
        assert!(airglow.metadata.provenance.contains("Paranal-derived"));
        assert!(airglow
            .metadata
            .provenance
            .contains("site_calibrated false"));
        assert!(airglow
            .metadata
            .validated_domain
            .contains("not globally or automatically locally calibrated"));
    }
}
