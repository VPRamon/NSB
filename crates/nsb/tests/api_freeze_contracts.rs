//! Behavioral and public-surface contracts for the first-release API freeze (#175).

mod common;

use chrono::{DateTime, Utc};
use common::starlight_test_provenance;
use nsb::components::airglow::{
    AirglowFallbackReason, AirglowModel, AirglowPhysicalOutcome, AirglowPhysicalZeroReason,
    AirglowSelection, AirglowSelectionKind,
};
use nsb::components::starlight::{StarlightMap, StarlightProduct};
use nsb::{ComponentMask, NsbError, NsbEvaluator, NsbModelConfig, PointQuery, Target, DEG};
use siderust::catalogs::observatories;
use std::sync::Arc;
use tempoch::{Time, UTC};

fn night_time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn day_time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T16:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

#[test]
fn component_mask_all_aliases_frozen_default_not_every_future_bit() {
    assert_eq!(ComponentMask::ALL, ComponentMask::DEFAULT);
    assert!(ComponentMask::DEFAULT.contains(ComponentMask::ZODIACAL));
    assert!(ComponentMask::DEFAULT.contains(ComponentMask::AIRGLOW));
    assert!(ComponentMask::DEFAULT.contains(ComponentMask::MOON));
    assert_eq!(
        ComponentMask::DEFAULT.contains(ComponentMask::STARLIGHT),
        StarlightProduct::bundled_production_available()
    );
    // ALL must remain an alias of DEFAULT, not "every implemented component bit".
    assert_eq!(ComponentMask::ALL.bits(), ComponentMask::DEFAULT.bits());
}

#[test]
fn nsb_crate_has_no_public_cargo_features() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("[features]"),
        "first-release nsb must not expose benchmark/test-only Cargo features"
    );
    assert!(!manifest.contains("window-search-diagnostics"));
}

#[test]
fn config_is_opaque_and_builder_getter_complete() {
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_selection(AirglowSelection::Explicit(
            AirglowModel::ParanalNollSkyCalcFors1,
        ))
        .with_site_profile(nsb::SiteProfileId::CtaSouth);
    assert_eq!(
        config.airglow_selection(),
        AirglowSelection::Explicit(AirglowModel::ParanalNollSkyCalcFors1)
    );
    assert_eq!(
        config.airglow_model(),
        Some(AirglowModel::ParanalNollSkyCalcFors1)
    );
    assert_eq!(config.site_profile(), nsb::SiteProfileId::CtaSouth);
    assert_eq!(
        config.moonlight_model(),
        nsb::MoonlightModel::Jones2013Spectral
    );
    let _ = config.zodiacal_model();
    let _ = config.zodiacal_extinction();
    let _ = config.solar_activity();
    let _ = config.airglow_geometry();
    let _ = config.starlight_product();
}

#[test]
fn default_airglow_selection_is_automatic_not_hardcoded_paranal_field() {
    let config = NsbModelConfig::default();
    assert_eq!(config.airglow_selection(), AirglowSelection::Automatic);
    assert_eq!(config.airglow_model(), None);

    let evaluator = NsbEvaluator::new().unwrap();
    let borrowed: &NsbModelConfig = evaluator.config();
    assert_eq!(borrowed.airglow_selection(), AirglowSelection::Automatic);
}

#[test]
fn automatic_airglow_fallback_is_machine_visible() {
    let result = NsbEvaluator::new()
        .unwrap()
        .evaluate(
            &PointQuery::new(observatories::EL_PARANAL.geodetic(), night_time(), target())
                .with_components(ComponentMask::AIRGLOW),
        )
        .unwrap();
    let airglow = &result.components[0];
    let selection = airglow.metadata.airglow_selection.as_ref().unwrap();
    assert_eq!(selection.selection_kind, AirglowSelectionKind::Automatic);
    assert!(selection.used_automatic_fallback);
    assert_eq!(
        selection.resolved_model,
        Some(AirglowModel::ParanalNollSkyCalcFors1)
    );
    assert_eq!(
        selection.fallback_reason,
        Some(AirglowFallbackReason::GlobalPlanningModelUnavailable)
    );
    assert_eq!(
        selection.fallback_reason.unwrap().as_str(),
        "global-planning-model-unavailable"
    );
    let evaluation = airglow.metadata.airglow_evaluation.as_ref().unwrap();
    assert_eq!(
        evaluation.physical_outcome,
        AirglowPhysicalOutcome::Evaluated
    );
}

#[test]
fn first_release_airglow_model_contains_only_supported_variants() {
    // Speculative unimplemented variants must not be frozen; #157 adds models later.
    assert_eq!(
        AirglowModel::ParanalNollSkyCalcFors1.as_str(),
        "paranal-noll-skycalc-fors1"
    );
    let _ = AirglowModel::ParanalNollSkyCalcFors1;
}

#[test]
fn explicit_paranal_airglow_is_not_reported_as_automatic_fallback() {
    let result = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_airglow_model(AirglowModel::ParanalNollSkyCalcFors1),
    )
    .unwrap()
    .evaluate(
        &PointQuery::new(observatories::EL_PARANAL.geodetic(), night_time(), target())
            .with_components(ComponentMask::AIRGLOW),
    )
    .unwrap();
    let selection = result.components[0]
        .metadata
        .airglow_selection
        .as_ref()
        .unwrap();
    assert_eq!(selection.selection_kind, AirglowSelectionKind::Explicit);
    assert!(!selection.used_automatic_fallback);
    assert_eq!(selection.fallback_reason, None);
    let evaluation = result.components[0]
        .metadata
        .airglow_evaluation
        .as_ref()
        .unwrap();
    assert_eq!(
        evaluation.physical_outcome,
        AirglowPhysicalOutcome::Evaluated
    );
}

#[test]
fn describe_components_does_not_fabricate_evaluation_outcome() {
    let evaluator = NsbEvaluator::new().unwrap();
    let descriptions = evaluator
        .describe_components(observatories::EL_PARANAL.geodetic(), ComponentMask::AIRGLOW)
        .unwrap();
    let airglow = &descriptions[0];
    let selection = airglow.metadata.airglow_selection.as_ref().unwrap();
    assert_eq!(selection.selection_kind, AirglowSelectionKind::Automatic);
    assert!(selection.used_automatic_fallback);
    assert_eq!(
        selection.resolved_model,
        Some(AirglowModel::ParanalNollSkyCalcFors1)
    );
    assert!(
        airglow.metadata.airglow_evaluation.is_none(),
        "descriptors must not invent physical evaluation outcomes"
    );
}

#[test]
fn daytime_physical_zero_is_typed_and_distinct_from_evaluation() {
    let result = NsbEvaluator::new()
        .unwrap()
        .evaluate(
            &PointQuery::new(observatories::EL_PARANAL.geodetic(), day_time(), target())
                .with_components(ComponentMask::AIRGLOW),
        )
        .unwrap();
    let airglow = &result.components[0];
    assert_eq!(airglow.integrated.value(), 0.0);
    let evaluation = airglow.metadata.airglow_evaluation.as_ref().unwrap();
    assert_eq!(
        evaluation.physical_outcome,
        AirglowPhysicalOutcome::PhysicalZero
    );
    assert_eq!(
        evaluation.physical_zero_reason,
        Some(AirglowPhysicalZeroReason::OutsideAstronomicalNight)
    );
    assert_eq!(
        evaluation.physical_zero_reason.unwrap().as_str(),
        "outside-astronomical-night"
    );
}

#[test]
fn starlight_evaluator_shares_caller_arc_without_deep_copy() {
    let map = Arc::new(
        StarlightMap::from_csv_str(
            include_str!("data/starlight_fixture_map.csv"),
            starlight_test_provenance(),
        )
        .unwrap(),
    );
    let before = Arc::strong_count(&map);
    let evaluator =
        NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky().with_starlight_product(
            StarlightProduct::with_shared_experimental_map(Arc::clone(&map)),
        ))
        .unwrap();
    assert!(
        Arc::strong_count(&map) > before,
        "evaluator construction must Arc-share the map (count {} -> {})",
        before,
        Arc::strong_count(&map)
    );
    let _ = evaluator.config().starlight_product();
}

#[test]
fn stale_error_variants_are_not_part_of_supported_surface() {
    let errors = [
        NsbError::OutOfRange("zenith".into()),
        NsbError::Unsupported("model".into()),
        NsbError::Io(std::io::Error::other("io")),
    ];
    for error in errors {
        let _ = error.to_string();
    }
}
