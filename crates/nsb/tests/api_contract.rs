//! Supported public API construction and evaluation contract.

use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{
    ComponentMask, NsbError, NsbEvaluator, NsbModelConfig, PointQuery, SiteProfileId,
    ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::catalogs::observatories;
use tempoch::{Period, Time, UTC};

fn parse_utc(value: &str) -> Time<UTC> {
    let ndt = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ").expect("parse utc");
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc);
    Time::<UTC>::from_chrono(dt)
}

#[test]
fn point_query_constructor_runs_default_evaluation() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let query = PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        parse_utc("2023-09-04T01:48:00Z"),
        nsb::Target::new(266.41683 * DEG, -29.00781 * DEG),
    )
    .with_components(ComponentMask::ZODIACAL | ComponentMask::AIRGLOW);

    let result = evaluator.evaluate(&query).expect("point evaluation");
    assert!(result.integrated.value().is_finite());
    assert!(!result.components.is_empty());
}

#[test]
fn threshold_query_builder_applies_defaults_and_runs() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse_utc("2023-09-04T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::hours(6));
    let query = ThresholdQuery::new(
        observatories::EL_PARANAL.geodetic(),
        nsb::Target::new(266.41683 * DEG, -29.00781 * DEG),
        Period::new(start, end),
        BandPhotonRadiance::new(0.21),
    )
    .with_components(ComponentMask::ALL)
    .with_sample_step(Second::new(3_600.0));

    assert_eq!(query.sample_step, Second::new(3_600.0));
    assert_eq!(
        query.sun_altitude_ceiling,
        Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING)
    );

    let result = evaluator
        .periods_below_threshold(&query)
        .expect("threshold search");
    assert!(result.threshold.value().is_finite());
}

#[test]
fn model_config_preset_and_field_assignment_compile_and_evaluate() {
    let mut config = NsbModelConfig::cta_s_planning();
    config.site_profile = SiteProfileId::GenericClearSky;
    let evaluator = NsbEvaluator::with_config(config).expect("evaluator");
    let query = PointQuery::new(
        observatories::EL_PARANAL.geodetic(),
        parse_utc("2023-09-04T01:48:00Z"),
        nsb::Target::new(266.41683 * DEG, -29.00781 * DEG),
    );
    evaluator.evaluate(&query).expect("configured evaluation");
}

#[test]
fn nsb_error_non_exhaustive_matching_covers_documented_variants() {
    let samples: Vec<NsbError> = vec![
        NsbError::OutOfRange("zenith".into()),
        NsbError::Unsupported("model".into()),
        NsbError::UnknownSite("nowhere".into()),
    ];

    for err in samples {
        let label = match &err {
            NsbError::OutOfRange(_) => "out-of-range",
            NsbError::Unsupported(_) => "unsupported",
            NsbError::UnknownSite(_) => "unknown-site",
            // Wildcard required: NsbError is #[non_exhaustive].
            _ => "other",
        };
        assert_ne!(label, "other");
        assert!(!err.to_string().is_empty());
    }
}
