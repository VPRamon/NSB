use chrono::{DateTime, Utc};
use nsb::{
    BandDiagnostic, ComponentCalibrationStatus, ComponentMask, NsbEvaluator, PointQuery, Target,
    DEG,
};
use siderust::catalogs::observatories;
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
