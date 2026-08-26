use chrono::{DateTime, Utc};
use nsb::{
    BandDiagnostic, ComponentCalibrationStatus, ComponentMask, NsbEvaluator, PointQuery, Target,
    DEG,
};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use tempoch::{Time, UTC};

fn parse_utc(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

#[test]
fn point_results_expose_calibration_provenance_uncertainty_and_band_convention() {
    let evaluator = NsbEvaluator::new().unwrap();
    let result = evaluator
        .evaluate(&PointQuery {
            observer: observatories::EL_PARANAL.geodetic(),
            time: parse_utc("2023-09-04T01:48:00Z"),
            target: sgr_a_star(),
            components: ComponentMask::ALL,
        })
        .unwrap();

    assert_eq!(
        result.band_diagnostic,
        BandDiagnostic::MONOCHROMATIC_S10_PROXY
    );

    let zodiacal = result
        .components
        .iter()
        .find(|c| c.name == "zodiacal")
        .unwrap();
    assert_eq!(
        zodiacal.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert!(zodiacal.metadata.provenance.contains("Leinert+1998"));

    let airglow = result
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();
    assert_eq!(
        airglow.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert!(airglow.metadata.provenance.contains("airglow_cont.dat"));
    // Bundled scientific baseline identity is pinned and machine-checkable.
    assert!(airglow
        .metadata
        .provenance
        .contains("sha256 d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f"));
    assert!(airglow
        .metadata
        .provenance
        .contains("schema skycalc-airglow-continuum-v1"));
    let airglow_uncertainty = airglow.relative_uncertainty.unwrap();
    assert!(airglow_uncertainty.is_finite() && airglow_uncertainty > 0.0);

    let moon = result.components.iter().find(|c| c.name == "moon").unwrap();
    assert_eq!(
        moon.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert!(moon.metadata.provenance.contains("Jones+2013"));
    assert!(moon.metadata.validated_domain.contains("generic-clear-sky"));
}

#[test]
fn airglow_component_metadata_reflects_site_profile_maturity() {
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();
    let observer = observatories::EL_PARANAL.geodetic();

    let generic = NsbEvaluator::with_config(nsb::NsbModelConfig::generic_clear_sky())
        .unwrap()
        .evaluate(&PointQuery {
            observer,
            time,
            target,
            components: ComponentMask::AIRGLOW,
        })
        .unwrap();

    let airglow = generic
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();
    assert_eq!(
        airglow.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );

    let cta_s = NsbEvaluator::with_config(nsb::NsbModelConfig::cta_s_planning())
        .unwrap()
        .evaluate(&PointQuery {
            observer,
            time,
            target,
            components: ComponentMask::AIRGLOW,
        })
        .unwrap();
    let airglow = cta_s
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();
    assert_eq!(
        airglow.metadata.status,
        ComponentCalibrationStatus::PlanningPreset
    );
}

#[test]
fn airglow_values_do_not_change_between_neutral_scale_site_profiles() {
    let observer = observatories::EL_PARANAL.geodetic();
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let generic = NsbEvaluator::with_config(nsb::NsbModelConfig::generic_clear_sky())
        .unwrap()
        .evaluate(&PointQuery {
            observer,
            time,
            target,
            components: ComponentMask::AIRGLOW,
        })
        .unwrap();
    let generic_airglow = generic
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();

    let cta_n = NsbEvaluator::with_config(nsb::NsbModelConfig::cta_n_planning())
        .unwrap()
        .evaluate(&PointQuery {
            observer,
            time,
            target,
            components: ComponentMask::AIRGLOW,
        })
        .unwrap();
    let cta_n_airglow = cta_n
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();

    assert!((cta_n_airglow.integrated.value() - generic_airglow.integrated.value()).abs() < 1e-12);
    assert_eq!(
        cta_n_airglow.relative_uncertainty,
        generic_airglow.relative_uncertainty
    );
}

#[test]
fn generic_airglow_metadata_and_values_work_for_arbitrary_location() {
    // High Arctic is explicitly not a bundled Paranal/CTAO location.
    let observer =
        Geodetic::<ECEF>::new_raw(Degrees::new(0.0), Degrees::new(89.0), Meters::new(0.0));
    let time = parse_utc("2023-12-21T12:00:00Z");
    let target = Target::new(0.0 * DEG, 89.0 * DEG);

    let evaluator = NsbEvaluator::new().unwrap(); // generic clear-sky
    let result = evaluator
        .evaluate(&PointQuery {
            observer,
            time,
            target,
            components: ComponentMask::AIRGLOW,
        })
        .unwrap();

    let airglow = result
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();
    assert_eq!(
        airglow.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert!(airglow.integrated.value() > 0.0);
}

#[test]
fn daytime_queries_return_zero_without_false_calibration_claims() {
    let observer = observatories::EL_PARANAL.geodetic();
    let time = parse_utc("2023-09-04T16:00:00Z");
    let target = sgr_a_star();

    for (config, expected_status) in [
        (
            nsb::NsbModelConfig::generic_clear_sky(),
            ComponentCalibrationStatus::GenericClearSky,
        ),
        (
            nsb::NsbModelConfig::cta_s_planning(),
            ComponentCalibrationStatus::PlanningPreset,
        ),
    ] {
        let evaluator = NsbEvaluator::with_config(config).unwrap();
        let result = evaluator
            .evaluate(&PointQuery {
                observer,
                time,
                target,
                components: ComponentMask::AIRGLOW,
            })
            .unwrap();
        let airglow = result
            .components
            .iter()
            .find(|c| c.name == "airglow")
            .unwrap();
        assert_eq!(airglow.metadata.status, expected_status);
        assert_eq!(airglow.integrated.value(), 0.0);
    }
}
