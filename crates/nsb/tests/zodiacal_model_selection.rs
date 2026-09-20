use chrono::{DateTime, Utc};
use nsb::{
    ComponentMask, NsbComponent, NsbEvaluator, NsbModelConfig, PointQuery, Target,
    ZodiacalExtinction, ZodiacalModel, DEG,
};
use siderust::catalogs::observatories;
use tempoch::{Time, UTC};

const REFERENCE_MODEL: ZodiacalModel = ZodiacalModel::Leinert1998;

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

fn below_horizon_target() -> Target {
    Target::new(0.0 * DEG, 89.0 * DEG)
}

fn evaluate_zodiacal_for_target(config: NsbModelConfig, target: Target) -> NsbComponent {
    NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &PointQuery::new(observatories::EL_PARANAL.geodetic(), time(), target)
                .with_components(ComponentMask::ZODIACAL),
        )
        .unwrap()
        .components
        .into_iter()
        .next()
        .unwrap()
}

fn evaluate_zodiacal(config: NsbModelConfig) -> NsbComponent {
    evaluate_zodiacal_for_target(config, target())
}

#[test]
fn zodiacal_model_identity_default_and_builder_are_stable() {
    let default = NsbModelConfig::default();
    assert_eq!(default.zodiacal_model(), REFERENCE_MODEL);
    assert_eq!(default.zodiacal_model, REFERENCE_MODEL);
    assert_eq!(REFERENCE_MODEL.as_str(), "leinert-1998");

    let explicit = default.with_zodiacal_model(REFERENCE_MODEL);
    assert_eq!(explicit.zodiacal_model(), REFERENCE_MODEL);
}

#[test]
fn explicit_default_model_is_bitwise_identical() {
    let default = evaluate_zodiacal(NsbModelConfig::default());
    let explicit = evaluate_zodiacal(
        NsbModelConfig::default().with_zodiacal_model(ZodiacalModel::Leinert1998),
    );

    assert_eq!(
        default.integrated.value().to_bits(),
        explicit.integrated.value().to_bits()
    );
    assert_eq!(
        default.b_flux_s10.value().to_bits(),
        explicit.b_flux_s10.value().to_bits()
    );
    assert_eq!(
        default.v_flux_s10.value().to_bits(),
        explicit.v_flux_s10.value().to_bits()
    );
}

#[test]
fn zodiacal_extinction_builder_getter_and_identity_are_stable() {
    let default = NsbModelConfig::default();
    assert_eq!(
        default.zodiacal_extinction(),
        ZodiacalExtinction::Noll2012Approx
    );
    assert_eq!(
        ZodiacalExtinction::Noll2012Approx.as_str(),
        "noll-2012-approximation"
    );
    assert_eq!(ZodiacalExtinction::None.as_str(), "none");

    let none = default.with_zodiacal_extinction(ZodiacalExtinction::None);
    assert_eq!(none.zodiacal_extinction(), ZodiacalExtinction::None);
    assert_eq!(none.zodiacal_model(), REFERENCE_MODEL);
}

#[test]
fn explicit_default_extinction_is_bitwise_identical() {
    let default = evaluate_zodiacal(NsbModelConfig::default());
    let explicit = evaluate_zodiacal(
        NsbModelConfig::default().with_zodiacal_extinction(ZodiacalExtinction::Noll2012Approx),
    );

    assert_eq!(
        default.integrated.value().to_bits(),
        explicit.integrated.value().to_bits()
    );
    assert_eq!(
        default.b_flux_s10.value().to_bits(),
        explicit.b_flux_s10.value().to_bits()
    );
    assert_eq!(
        default.v_flux_s10.value().to_bits(),
        explicit.v_flux_s10.value().to_bits()
    );
}

#[test]
fn none_extinction_dispatches_through_evaluator_and_metadata_is_truthful() {
    let noll = evaluate_zodiacal(NsbModelConfig::default());
    let none = evaluate_zodiacal(
        NsbModelConfig::default().with_zodiacal_extinction(ZodiacalExtinction::None),
    );

    assert!(none.integrated.value() >= noll.integrated.value());
    assert!(none.b_flux_s10.value() >= noll.b_flux_s10.value());
    assert!(none.v_flux_s10.value() >= noll.v_flux_s10.value());

    assert!(noll
        .metadata
        .provenance
        .contains("scientific model leinert-1998"));
    assert!(noll.metadata.provenance.contains("noll-2012-approximation"));
    assert!(noll.metadata.provenance.contains("Noll+2012"));

    assert!(none
        .metadata
        .provenance
        .contains("scientific model leinert-1998"));
    assert!(none
        .metadata
        .provenance
        .contains("atmospheric propagation none"));
    assert!(!none.metadata.provenance.contains("Noll+2012"));
    assert!(none
        .metadata
        .validated_domain
        .contains("no atmospheric attenuation applied"));
    assert!(none.metadata.validated_domain.contains("ground-observer"));
    assert!(!none.metadata.validated_domain.contains("exoatmospheric"));
}

#[test]
fn none_extinction_preserves_ground_observer_horizon_gating() {
    let none = evaluate_zodiacal_for_target(
        NsbModelConfig::default().with_zodiacal_extinction(ZodiacalExtinction::None),
        below_horizon_target(),
    );

    assert_eq!(none.integrated.value(), 0.0);
    assert_eq!(none.b_flux_s10.value(), 0.0);
    assert_eq!(none.v_flux_s10.value(), 0.0);
    assert!(none.metadata.validated_domain.contains("horizon gating"));
    assert!(!none.metadata.validated_domain.contains("exoatmospheric"));
}
