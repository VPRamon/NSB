use chrono::{DateTime, Utc};
use nsb::{
    BandDiagnostic, CalibrationStatus, ComponentMask, NsbEvaluator, PointQuery, Target, DEG,
};
use siderust::catalogs::observatories;
use tempoch::{Time, UTC};

fn parse_utc(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .expect("RFC3339 timestamp")
            .with_timezone(&Utc),
    )
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

#[test]
fn point_results_expose_calibration_provenance_uncertainty_and_band_convention() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let result = evaluator
        .evaluate(&PointQuery {
            observer: observatories::EL_PARANAL.geodetic(),
            time: parse_utc("2023-09-04T01:48:00Z"),
            target: sgr_a_star(),
            components: ComponentMask::ALL,
        })
        .expect("point query");

    assert_eq!(
        result.band_diagnostic,
        BandDiagnostic::MONOCHROMATIC_S10_PROXY
    );
    assert_eq!(
        result.band_diagnostic.convention,
        "monochromatic-central-wavelength-s10-proxy"
    );

    let zodiacal = result
        .components
        .iter()
        .find(|component| component.name == "zodiacal")
        .expect("zodiacal component");
    assert_eq!(zodiacal.metadata.status, CalibrationStatus::GenericClearSky);
    assert!(zodiacal.metadata.provenance.contains("Leinert+1998"));
    assert_eq!(
        zodiacal.metadata.band_diagnostic,
        BandDiagnostic::MONOCHROMATIC_S10_PROXY
    );

    let airglow = result
        .components
        .iter()
        .find(|component| component.name == "airglow")
        .expect("airglow component");
    assert_eq!(airglow.metadata.status, CalibrationStatus::GenericClearSky);
    assert!(airglow.metadata.provenance.contains("airglow_cont.dat"));
    let airglow_uncertainty = airglow
        .relative_uncertainty
        .expect("airglow relative uncertainty");
    assert!(
        airglow_uncertainty.is_finite() && airglow_uncertainty > 0.0,
        "airglow uncertainty must be exposed as a positive relative sigma"
    );

    let moon = result
        .components
        .iter()
        .find(|component| component.name == "moon")
        .expect("moon component");
    assert_eq!(moon.metadata.status, CalibrationStatus::GenericClearSky);
    assert!(moon.metadata.provenance.contains("Jones+2013"));
    assert!(moon.metadata.validated_domain.contains("generic clear-sky"));
}
