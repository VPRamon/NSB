// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use chrono::{DateTime, Utc};
use nsb::components::airglow::{AirglowGeometryModel, VanRhijnConfig};
mod common;

use common::ctao_south_planning;
use nsb::{
    AirglowModel, AirglowSelection, CalibrationStatus, ComponentCalibrationStatus, ComponentMask,
    NsbComponent, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, Target, DEG,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Meters};
use tempoch::{Time, UTC};

/// Automatic policy resolves to Oxygen; explicit Oxygen must match bit-for-bit.
const REFERENCE_MODEL: AirglowModel = AirglowModel::ParanalPalaceOxygenV1;

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

fn near_zenith_target(observer: &Geodetic<ECEF>) -> Target {
    // Automatic Oxygen requires zenith distance ≤ 60°. Point near local zenith.
    Target::new(0.0 * DEG, observer.lat())
}

fn evaluate_airglow(config: NsbModelConfig, observer: Geodetic<ECEF>) -> NsbComponent {
    NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &PointQuery::new(observer, time(), near_zenith_target(&observer))
                .with_components(ComponentMask::AIRGLOW),
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
fn oxygen_selection_does_not_promote_scientific_maturity_in_descriptors_or_results() {
    let location = observer(-70.4044, -24.6275, 2_635.0);
    for selection in [
        AirglowSelection::Automatic,
        AirglowSelection::Explicit(REFERENCE_MODEL),
    ] {
        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky()
                .with_site_profile(ctao_south_planning())
                .with_airglow_selection(selection),
        )
        .unwrap();
        let descriptors = evaluator
            .describe_components(location, ComponentMask::AIRGLOW)
            .unwrap();
        let descriptor = &descriptors[0].metadata;
        assert_eq!(descriptor.status, ComponentCalibrationStatus::Experimental);
        assert_eq!(descriptor.status.as_str(), "experimental");
        assert!(descriptor
            .validated_domain
            .contains("not an independent absolute observational admission"));
        let resolved = descriptor.airglow_selection.as_ref().unwrap();
        assert_eq!(resolved.resolved_model, Some(REFERENCE_MODEL));
        assert_eq!(
            resolved.used_automatic_fallback,
            selection == AirglowSelection::Automatic
        );

        let result = evaluator
            .evaluate(
                &PointQuery::new(location, time(), near_zenith_target(&location))
                    .with_components(ComponentMask::AIRGLOW),
            )
            .unwrap();
        let metadata = &result.components[0].metadata;
        assert_eq!(metadata.status, ComponentCalibrationStatus::Experimental);
        assert_eq!(metadata.status.as_str(), "experimental");
        assert_eq!(
            metadata.airglow_selection.as_ref().unwrap().resolved_model,
            resolved.resolved_model
        );
        assert!(metadata.provenance.contains("oxygen asset"));
        assert!(metadata
            .validated_domain
            .contains("EXPERIMENTAL OXYGEN EXTENSION"));
    }

    let continuum = evaluate_airglow(
        NsbModelConfig::generic_clear_sky()
            .with_airglow_model(AirglowModel::ParanalPalaceV1),
        location,
    );
    assert_eq!(
        continuum.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    assert!(!continuum
        .metadata
        .validated_domain
        .contains("EXPERIMENTAL OXYGEN EXTENSION"));
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
    let changed_site = base.clone().with_site_profile(ctao_south_planning());

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
    assert!(matches!(
        NsbEvaluator::with_config(changed_geometry),
        Err(nsb::NsbError::Unsupported(_))
    ));
    let changed_f107 = evaluate_airglow(changed_f107, paranal);
    let changed_site = evaluate_airglow(changed_site, paranal);

    for component in [&base_arbitrary, &base_paranal, &changed_f107, &changed_site] {
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
        ComponentCalibrationStatus::Experimental
    );
    assert_eq!(
        base_paranal.metadata.status,
        ComponentCalibrationStatus::Experimental
    );
    // CTAO planning site profile does not upgrade Oxygen beyond Experimental.
    assert_eq!(
        changed_site.metadata.status,
        ComponentCalibrationStatus::Experimental
    );
}

#[test]
fn automatic_and_explicit_oxygen_require_standard_geometry() {
    for selection in [
        AirglowSelection::Automatic,
        AirglowSelection::Explicit(REFERENCE_MODEL),
    ] {
        let config = NsbModelConfig::generic_clear_sky().with_airglow_selection(selection);
        assert!(NsbEvaluator::with_config(config.clone()).is_ok());
        let nonstandard = config.with_airglow_geometry(AirglowGeometryModel::VanRhijn(
            VanRhijnConfig::new(Kilometers::new(110.0)).unwrap(),
        ));
        assert!(matches!(
            NsbEvaluator::with_config(nonstandard.clone()),
            Err(nsb::NsbError::Unsupported(_))
        ));
        // Custom continuum geometry remains an explicit supported capability.
        assert!(NsbEvaluator::with_config(
            nonstandard.with_airglow_model(AirglowModel::ParanalPalaceV1)
        )
        .is_ok());
    }
}

#[test]
fn daytime_out_of_domain_is_rejected_for_automatic_and_explicit_oxygen() {
    use nsb::units::length::Nanometers;

    let location = observer(-70.4044, -24.6275, 2635.0);
    let daylight = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T16:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let query = PointQuery::new(location, daylight, Target::new(0.0 * DEG, location.lat()))
        .with_components(ComponentMask::AIRGLOW);
    let lower = Nanometers::new(300.0);
    let upper = Nanometers::new(650.0);

    for selection in [
        AirglowSelection::Automatic,
        AirglowSelection::Explicit(REFERENCE_MODEL),
    ] {
        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky().with_airglow_selection(selection),
        )
        .unwrap();
        assert!(
            matches!(evaluator.evaluate(&query), Err(nsb::NsbError::OutOfRange(message)) if message.contains("zenith distance <= 60"))
        );
        assert!(
            matches!(evaluator.integrate_band(&query, lower, upper), Err(nsb::NsbError::OutOfRange(message)) if message.contains("zenith distance <= 60"))
        );
    }

    // The historical continuum explicitly permits high zenith distances, and
    // a geometrically valid daytime query remains a physical zero.
    let continuum = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_airglow_model(AirglowModel::ParanalPalaceV1),
    )
    .unwrap();
    assert_eq!(continuum.evaluate(&query).unwrap().integrated.value(), 0.0);
    assert_eq!(
        continuum
            .integrate_band(&query, lower, upper)
            .unwrap()
            .value(),
        0.0
    );
}

#[test]
fn automatic_and_explicit_oxygen_return_identical_domain_errors() {
    let location = observer(-70.4044, -24.6275, 2635.0);
    let query = PointQuery::new(location, time(), Target::new(90.0 * DEG, location.lat()))
        .with_components(ComponentMask::AIRGLOW);
    for selection in [
        AirglowSelection::Automatic,
        AirglowSelection::Explicit(REFERENCE_MODEL),
    ] {
        let evaluator = NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky().with_airglow_selection(selection),
        )
        .unwrap();
        assert!(
            matches!(evaluator.evaluate(&query), Err(nsb::NsbError::OutOfRange(message)) if message.contains("zenith distance <= 60"))
        );
    }
    assert!(NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_airglow_model(AirglowModel::ParanalPalaceV1)
    )
    .unwrap()
    .evaluate(&query)
    .is_ok());
}
