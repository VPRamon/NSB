use chrono::{DateTime, Utc};
use nsb::components::airglow::{AirglowGeometryModel, VanRhijnConfig};
use nsb::{
    AirglowModel, AirglowSelection, CalibrationStatus, ComponentCalibrationStatus, ComponentMask,
    NsbComponent, NsbEvaluator, NsbModelConfig, PointQuery, SiteProfileId, SolarFluxUnits, Target,
    DEG,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Meters};
use tempoch::{Time, UTC};

const REFERENCE_MODEL: AirglowModel = AirglowModel::ParanalNollSkyCalcFors1;

fn observer(lon_deg: f64, lat_deg: f64, height_m: f64) -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(lon_deg),
        Degrees::new(lat_deg),
        Meters::new(height_m),
    )
}

fn time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn evaluate_airglow(config: NsbModelConfig, observer: Geodetic<ECEF>) -> NsbComponent {
    NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &PointQuery::new(observer, time(), target()).with_components(ComponentMask::AIRGLOW),
        )
        .unwrap()
        .components
        .into_iter()
        .next()
        .unwrap()
}

#[test]
fn default_and_explicit_model_selection_are_inspectable_and_numerically_identical() {
    let default = NsbModelConfig::default();
    assert_eq!(default.airglow_selection(), AirglowSelection::Automatic);
    assert_eq!(default.airglow_model(), None);

    let explicit = default.clone().with_airglow_model(REFERENCE_MODEL);
    assert_eq!(
        explicit.airglow_selection(),
        AirglowSelection::Explicit(REFERENCE_MODEL)
    );
    assert_eq!(explicit.airglow_model(), Some(REFERENCE_MODEL));

    let location = observer(-70.4044, -24.6275, 2_635.0);
    let default_result = evaluate_airglow(default, location);
    let explicit_result = evaluate_airglow(explicit, location);

    assert_eq!(
        default_result.integrated.value().to_bits(),
        explicit_result.integrated.value().to_bits()
    );
    assert_eq!(
        default_result.b_flux_s10.value().to_bits(),
        explicit_result.b_flux_s10.value().to_bits()
    );
    assert_eq!(
        default_result.v_flux_s10.value().to_bits(),
        explicit_result.v_flux_s10.value().to_bits()
    );
    assert_eq!(
        explicit_result
            .metadata
            .airglow_selection
            .as_ref()
            .unwrap()
            .resolved_model,
        Some(REFERENCE_MODEL)
    );
    let report = explicit_result.metadata.airglow_selection.as_ref().unwrap();
    assert!(!report.used_automatic_fallback);
    let default_report = default_result.metadata.airglow_selection.as_ref().unwrap();
    assert!(default_report.used_automatic_fallback);
    assert!(default_result.metadata.airglow_evaluation.is_some());
}

#[test]
fn scientific_model_identity_is_independent_of_geometry_f107_location_and_site_maturity() {
    let base = NsbModelConfig::generic_clear_sky();
    let changed_geometry = base
        .clone()
        .with_airglow_geometry(AirglowGeometryModel::VanRhijn(
            VanRhijnConfig::new(Kilometers::new(110.0)).unwrap(),
        ));
    let changed_f107 = base
        .clone()
        .with_solar_radio_flux(SolarFluxUnits::new(170.0));
    let changed_site = base.clone().with_site_profile(SiteProfileId::CtaSouth);

    for config in [&base, &changed_geometry, &changed_f107, &changed_site] {
        assert_eq!(config.airglow_selection(), AirglowSelection::Automatic);
        assert_eq!(config.airglow_model(), None);
    }
    assert_eq!(
        base.airglow_calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert_eq!(
        changed_site.airglow_calibration_status(),
        CalibrationStatus::PlanningPreset
    );

    let arbitrary = observer(18.4, -33.9, 120.0);
    let paranal = observer(-70.4044, -24.6275, 2_635.0);
    let base_arbitrary = evaluate_airglow(base.clone(), arbitrary);
    let base_paranal = evaluate_airglow(base, paranal);
    let changed_geometry = evaluate_airglow(changed_geometry, arbitrary);
    let changed_f107 = evaluate_airglow(changed_f107, paranal);
    let changed_site = evaluate_airglow(changed_site, paranal);

    for component in [
        &base_arbitrary,
        &base_paranal,
        &changed_geometry,
        &changed_f107,
        &changed_site,
    ] {
        assert_eq!(
            component
                .metadata
                .airglow_selection
                .as_ref()
                .unwrap()
                .resolved_model,
            Some(REFERENCE_MODEL)
        );
    }
    assert_eq!(
        base_arbitrary.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert_eq!(
        base_paranal.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert_eq!(
        changed_site.metadata.status,
        ComponentCalibrationStatus::PlanningPreset
    );
}
