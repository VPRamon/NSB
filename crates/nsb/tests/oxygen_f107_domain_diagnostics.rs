// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Public-API regression for experimental Oxygen F10.7 domain diagnostics (#294).

use chrono::{DateTime, Utc};
use nsb::components::airglow::{
    AirglowModel, OxygenF107ActualConditionsClaim, OxygenF107NumericalRange,
};
use nsb::solar_activity::{resolve_f107, SolarActivitySource};
use nsb::{
    ComponentMask, NsbComponent, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, Target,
    DEG,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use tempoch::{Time, UTC};

fn observer() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        Degrees::new(-70.4044),
        Degrees::new(-24.6275),
        Meters::new(2_635.0),
    )
}

fn parse_time(value: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn evaluate_airglow(config: NsbModelConfig, time: Time<UTC>) -> NsbComponent {
    let location = observer();
    // Oxygen requires zenith distance ≤ 60°. Sweep RA at the observer latitude
    // so date-specific Automatic F10.7 cases still have a supported geometry.
    let evaluator = NsbEvaluator::with_config(config).unwrap();
    for step in 0..24 {
        let ra = (step as f64) * 15.0 * DEG;
        let target = Target::new(ra, location.lat());
        match evaluator.evaluate(
            &PointQuery::new(location, time, target).with_components(ComponentMask::AIRGLOW),
        ) {
            Ok(result) => return result.components.into_iter().next().unwrap(),
            Err(nsb::NsbError::OutOfRange(message)) if message.contains("zenith distance") => {}
            Err(error) => panic!("unexpected airglow evaluation error: {error}"),
        }
    }
    panic!("no RA at observer latitude satisfied Oxygen zenith <= 60 deg");
}

#[test]
fn continuum_only_omits_oxygen_f107_domain_diagnostic() {
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_model(AirglowModel::ParanalPalaceV1)
        .with_solar_radio_flux(SolarFluxUnits::new(200.0));
    let air = evaluate_airglow(config, parse_time("2023-09-04T01:48:00Z"));
    assert!(air.metadata.solar_activity.is_some());
    assert!(air.metadata.oxygen_f107_domain.is_none());
    assert_ne!(air.metadata.status.as_str(), "experimental");
}

#[test]
fn explicit_above_range_marks_numerical_extrapolation() {
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1)
        .with_solar_radio_flux(SolarFluxUnits::new(166.001));
    let air = evaluate_airglow(config, parse_time("2023-09-04T01:48:00Z"));
    let domain = air
        .metadata
        .oxygen_f107_domain
        .as_ref()
        .expect("oxygen domain");
    assert_eq!(
        domain.numerical_range,
        OxygenF107NumericalRange::AboveTrainingInterval
    );
    assert!(domain.numerically_extrapolated);
    assert!(!domain.covariate_matches_training_statistic);
    assert!(!domain.claims_actual_conditions_within_training_domain);
    assert_eq!(
        domain.actual_conditions_claim,
        OxygenF107ActualConditionsClaim::ExplicitCallerScalarOnly
    );
    assert_eq!(air.metadata.status.as_str(), "experimental");
}

#[test]
fn explicit_endpoints_are_within_training_interval() {
    for value in [67.0, 100.0, 166.0] {
        let config = NsbModelConfig::generic_clear_sky()
            .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1)
            .with_solar_radio_flux(SolarFluxUnits::new(value));
        let air = evaluate_airglow(config, parse_time("2023-09-04T01:48:00Z"));
        let domain = air
            .metadata
            .oxygen_f107_domain
            .as_ref()
            .expect("oxygen domain");
        assert_eq!(
            domain.numerical_range,
            OxygenF107NumericalRange::WithinTrainingInterval,
            "value={value}"
        );
        assert!(!domain.numerically_extrapolated);
        assert!(!domain.claims_actual_conditions_within_training_domain);
        assert_eq!(domain.training_range_low_sfu, 67.0);
        assert_eq!(domain.training_range_high_sfu, 166.0);
    }
}

#[test]
fn explicit_below_range_marks_numerical_extrapolation() {
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1)
        .with_solar_radio_flux(SolarFluxUnits::new(66.999));
    let air = evaluate_airglow(config, parse_time("2023-09-04T01:48:00Z"));
    let domain = air
        .metadata
        .oxygen_f107_domain
        .as_ref()
        .expect("oxygen domain");
    assert_eq!(
        domain.numerical_range,
        OxygenF107NumericalRange::BelowTrainingInterval
    );
    assert!(domain.numerically_extrapolated);
    assert!(!domain.claims_actual_conditions_within_training_domain);
}

#[test]
fn automatic_patat_era_climatology_does_not_claim_actual_in_domain() {
    // Issue #295 discovery: Automatic → climatology ~129 sfu on Patat dates.
    let time = parse_time("2000-04-09T01:48:00Z");
    let resolved = resolve_f107(time, &SolarActivitySource::Automatic).unwrap();
    assert_eq!(resolved.record.kind.as_str(), "climatology");
    assert!((resolved.value.value() - 129.20671119074768).abs() < 1e-9);

    let config = NsbModelConfig::generic_clear_sky(); // Automatic → Oxygen
    let air = evaluate_airglow(config, time);
    assert_eq!(air.metadata.status.as_str(), "experimental");
    let domain = air
        .metadata
        .oxygen_f107_domain
        .as_ref()
        .expect("oxygen domain on Automatic evaluation");
    assert_eq!(
        domain.numerical_range,
        OxygenF107NumericalRange::WithinTrainingInterval
    );
    assert!(!domain.numerically_extrapolated);
    assert_eq!(domain.operational_quantity, "planning-climatology-fallback");
    assert_eq!(
        domain.actual_conditions_claim,
        OxygenF107ActualConditionsClaim::UnknownBecausePlanningFallback
    );
    assert!(
        !domain.claims_actual_conditions_within_training_domain,
        "climatology numerically inside [67,166] must not claim actual solar was in-domain"
    );
    assert!(!domain.covariate_matches_training_statistic);
    assert!(air
        .metadata
        .provenance
        .contains("claims_actual_conditions_within_training_domain=false"));
}

#[test]
fn automatic_with_monthly_observed_coverage_stays_experimental_and_nonmatching_covariate() {
    let time = parse_time("2023-09-04T01:48:00Z");
    let resolved = resolve_f107(time, &SolarActivitySource::Automatic).unwrap();
    assert_eq!(resolved.record.kind.as_str(), "observed");
    assert!(resolved.value.value() > 0.0);

    let config = NsbModelConfig::generic_clear_sky();
    let air = evaluate_airglow(config, time);
    let domain = air
        .metadata
        .oxygen_f107_domain
        .as_ref()
        .expect("oxygen domain");
    assert_eq!(air.metadata.status.as_str(), "experimental");
    assert!(!domain.covariate_matches_training_statistic);
    assert!(!domain.claims_actual_conditions_within_training_domain);
    assert_eq!(
        domain.actual_conditions_claim,
        OxygenF107ActualConditionsClaim::MonthlyObservedPlanningIndexOnly
    );
    assert_eq!(
        domain.operational_quantity,
        "monthly-observed-planning-mean"
    );
}

#[test]
fn describe_components_does_not_invent_oxygen_f107_domain_without_resolved_solar() {
    let location = observer();
    let descriptors = NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky())
        .unwrap()
        .describe_components(location, ComponentMask::AIRGLOW)
        .unwrap();
    let air = descriptors
        .into_iter()
        .find(|d| d.name == "airglow")
        .expect("airglow descriptor");
    assert!(air.metadata.solar_activity.is_none());
    assert!(air.metadata.oxygen_f107_domain.is_none());
    assert_eq!(air.metadata.status.as_str(), "experimental");
}

#[test]
fn oxygen_radiance_is_deterministic_with_domain_diagnostic_present() {
    let config = NsbModelConfig::generic_clear_sky()
        .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1)
        .with_solar_radio_flux(SolarFluxUnits::new(130.0));
    let time = parse_time("2023-09-04T01:48:00Z");
    let a = evaluate_airglow(config.clone(), time);
    let b = evaluate_airglow(config, time);
    assert_eq!(
        a.integrated.value().to_bits(),
        b.integrated.value().to_bits()
    );
    assert!(a.metadata.oxygen_f107_domain.is_some());
    assert!(a.integrated.value().is_finite() && a.integrated.value() > 0.0);
}
