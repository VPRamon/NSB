// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use chrono::{DateTime, Utc};
use nsb::components::airglow::{AirglowGeometryModel, VanRhijnConfig};
use nsb::site::airglow_profile::{AirglowCalibrationProfile, AirglowCalibrationRequest};
use nsb::{
    AirglowModel, CalibrationStatus, ComponentCalibrationStatus, ComponentMask, NsbEvaluator,
    NsbModelConfig, PointQuery, SolarFluxUnits, Target,
};
use siderust::coordinates::centers::Geodetic;
use siderust::qtty::{Degrees, Kilometers, Meters, Nanometers};
use tempoch::{Time, UTC};

fn request() -> AirglowCalibrationRequest {
    toml::from_str(include_str!("../examples/airglow-calibration/request.toml")).unwrap()
}
fn edit_evidence(request: &mut AirglowCalibrationRequest, edit: impl FnOnce(&mut toml::Value)) {
    let mut document = toml::Value::try_from(&*request).unwrap();
    edit(&mut document["definition"]["evidence"]);
    *request = document.try_into().unwrap();
}
fn query() -> PointQuery {
    PointQuery::new(
        Geodetic::new_raw(
            Degrees::new(-70.4044),
            Degrees::new(-24.6275),
            Meters::new(2635.),
        ),
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ),
        Target::new(Degrees::new(266.41683), Degrees::new(-29.00781)),
    )
    .with_components(ComponentMask::AIRGLOW)
}
fn fit() -> AirglowCalibrationProfile {
    AirglowCalibrationProfile::fit(request()).unwrap()
}
fn calibrated(model: AirglowModel) -> NsbModelConfig {
    NsbModelConfig::generic_clear_sky()
        .with_solar_radio_flux(SolarFluxUnits::new(100.))
        .with_airglow_model(model)
        .with_airglow_calibration(fit())
        .unwrap()
}
#[test]
fn synthetic_recovery_covariance_and_serialization_are_reproducible() {
    let profile = fit();
    assert!((profile.scale().value() - 1.2).abs() < 1e-12);
    assert!((profile.variance() - 0.00002563653899902484).abs() < 1e-15);
    let raw = profile.to_toml_string().unwrap();
    let loaded = AirglowCalibrationProfile::from_toml_str(&raw).unwrap();
    assert_eq!(profile, loaded);
    assert_eq!(raw, loaded.to_toml_string().unwrap());
    assert_eq!(profile.checksum_sha256(), loaded.checksum_sha256());
    let mut req = request();
    for o in &mut req.observations {
        o.uncertainty *= 2.;
    }
    let noisy = AirglowCalibrationProfile::fit(req).unwrap();
    assert!((noisy.variance() / profile.variance() - 4.).abs() < 1e-10);
}
#[test]
fn schema_provenance_nonfinite_and_forged_maturity_fail_closed() {
    for edit in 0..9 {
        let mut req = request();
        match edit {
            0 => req.definition.schema_version = 2,
            1 => req.definition.base_asset_sha256 = "a".repeat(64),
            2 => req.definition.longitude_deg = f64::NAN,
            3 => edit_evidence(&mut req, |e| {
                e["atmosphere"]["surface_pressure_hpa"] = (-1.).into()
            }),
            4 => req.definition.measurement_provenance.clear(),
            5 => req.observations[0].uncertainty = 0.,
            6 => req.observations[0].continuum_radiance = f64::INFINITY,
            7 => edit_evidence(&mut req, |e| {
                e["airglow"]["temporal_correction_applied"] = true.into()
            }),
            _ => req.definition.solar_flux_range_sfu = [166., 67.],
        }
        assert!(AirglowCalibrationProfile::fit(req).is_err(), "edit {edit}");
    }
    let raw = fit().to_toml_string().unwrap();
    for changed in [
        raw.replace(
            "registered-fit-not-independently-validated",
            "independently-validated",
        ),
        raw.replacen("scale = 1.2", "scale = 1.3", 1),
        raw.replace("schema_version = 1", "schema_version = 2"),
        format!("unknown = true\n{raw}"),
        raw.replacen("variance = 0.", "variance = nan #", 1),
    ] {
        assert!(AirglowCalibrationProfile::from_toml_str(&changed).is_err());
    }
}
#[test]
fn fit_requires_distinct_nights_and_excludes_reserved_evaluation_nights() {
    let mut req = request();
    req.reserved_nights.push("2023-09-03".into());
    assert!(AirglowCalibrationProfile::fit(req).is_err());
    let mut req = request();
    req.observations[2].utc = "2023-09-04T03:00:00Z".into();
    assert!(AirglowCalibrationProfile::fit(req).is_err());
    let mut req = request();
    req.observations[1].id = req.observations[0].id.clone();
    assert!(AirglowCalibrationProfile::fit(req).is_err());
    let mut req = request();
    for o in &mut req.observations {
        o.utc = "2023-09-04T15:00:00Z".into();
    }
    assert!(AirglowCalibrationProfile::fit(req).is_err());
}
#[test]
fn runtime_uses_fit_atmosphere_scale_and_never_upgrades_trusted_maturity() {
    let config = calibrated(AirglowModel::ParanalPalaceV1);
    assert_eq!(
        config.airglow_calibration_status(),
        CalibrationStatus::PlanningPreset
    );
    assert!(!config.is_airglow_site_calibrated());
    let evaluator = NsbEvaluator::with_config(config).unwrap();
    let full = evaluator.evaluate(&query()).unwrap();
    let component = &full.components[0];
    assert!(
        (component.integrated.value() - request().observations[0].continuum_radiance).abs() < 1e-12
    );
    assert_eq!(component.integrated, full.integrated);
    assert_eq!(
        component.metadata.status,
        ComponentCalibrationStatus::Experimental
    );
    assert!(component
        .metadata
        .provenance
        .contains(fit().checksum_sha256()));
    assert!(component.relative_uncertainty.unwrap() > 0.);
    assert_eq!(
        full.integrated,
        evaluator
            .integrate_band(&query(), Nanometers::new(300.), Nanometers::new(650.))
            .unwrap()
    );
    let mut q = query();
    q.observer.height = Meters::new(3000.);
    assert!(evaluator.evaluate(&q).is_err());
    let mut q = query();
    q.time = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2024-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    assert!(evaluator
        .integrate_band(&q, Nanometers::new(300.), Nanometers::new(650.))
        .is_err());
    let bad =
        calibrated(AirglowModel::ParanalPalaceV1).with_solar_radio_flux(SolarFluxUnits::new(200.));
    assert!(NsbEvaluator::with_config(bad)
        .unwrap()
        .evaluate(&query())
        .is_err());
    let bad = calibrated(AirglowModel::ParanalPalaceV1).with_airglow_geometry(
        AirglowGeometryModel::VanRhijn(VanRhijnConfig::new(Kilometers::new(110.)).unwrap()),
    );
    assert!(NsbEvaluator::with_config(bad).is_err());
}
#[test]
fn oxygen_is_explicit_conserves_line_flux_and_does_not_scale_lines_with_continuum() {
    let base = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_solar_radio_flux(SolarFluxUnits::new(100.)),
    )
    .unwrap();
    let improved = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_solar_radio_flux(SolarFluxUnits::new(100.))
            .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1),
    )
    .unwrap();
    let band = |e: &NsbEvaluator, a, b| {
        e.integrate_band(&query(), Nanometers::new(a), Nanometers::new(b))
            .unwrap()
            .value()
    };
    let line = band(&improved, 300., 650.) - band(&base, 300., 650.);
    assert!(line > 0.02 && line < 0.1);
    assert_eq!(band(&improved, 400., 500.), band(&base, 400., 500.));
    let pieces: f64 = [
        (300., 557.8887),
        (557.8887, 600.),
        (600., 636.5536),
        (636.5536, 650.),
    ]
    .iter()
    .map(|&(a, b)| band(&improved, a, b))
    .sum();
    assert!((pieces - band(&improved, 300., 650.)).abs() < 1e-12);
    let cb = NsbEvaluator::with_config(calibrated(AirglowModel::ParanalPalaceV1)).unwrap();
    let ci = NsbEvaluator::with_config(calibrated(AirglowModel::ParanalPalaceOxygenV1)).unwrap();
    // Same oxygen addition with the same fixed atmosphere, irrespective of fit scale.
    let mut req = request();
    for o in &mut req.observations {
        o.continuum_radiance /= 1.2;
    }
    let unit = AirglowCalibrationProfile::fit(req).unwrap();
    let mk = |m| {
        NsbEvaluator::with_config(
            NsbModelConfig::generic_clear_sky()
                .with_solar_radio_flux(SolarFluxUnits::new(100.))
                .with_airglow_model(m)
                .with_airglow_calibration(unit.clone())
                .unwrap(),
        )
        .unwrap()
    };
    let ub = mk(AirglowModel::ParanalPalaceV1);
    let ui = mk(AirglowModel::ParanalPalaceOxygenV1);
    assert!(
        ((band(&ci, 300., 650.) - band(&cb, 300., 650.))
            - (band(&ui, 300., 650.) - band(&ub, 300., 650.)))
        .abs()
            < 1e-12
    );
    let result = improved.evaluate(&query()).unwrap();
    assert_eq!(
        result.components[0].metadata.status,
        ComponentCalibrationStatus::Experimental
    );
    assert_eq!(result.integrated.value(), band(&improved, 300., 650.));
}

#[test]
fn independent_synthetic_night_and_restricted_applicability_are_supported() {
    let evaluator = NsbEvaluator::with_config(calibrated(AirglowModel::ParanalPalaceV1)).unwrap();
    let mut q = query();
    q.time = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-07T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    // This reserved night never enters the fit; runtime can predict it.
    let held_out = evaluator.evaluate(&q).unwrap();
    assert!(held_out.integrated.value() > 0.);
    let mut req = request();
    edit_evidence(&mut req, |e| {
        e["validity"]["wavelength_nm"] = toml::Value::Array(vec![400.into(), 500.into()])
    });
    for o in &mut req.observations {
        o.wavelength_nm = [400., 500.];
    }
    let profile = AirglowCalibrationProfile::fit(req).unwrap();
    let narrow = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_solar_radio_flux(SolarFluxUnits::new(100.))
            .with_airglow_calibration(profile)
            .unwrap(),
    )
    .unwrap();
    assert!(narrow.evaluate(&query()).is_err());
    assert!(narrow
        .integrate_band(&query(), Nanometers::new(400.), Nanometers::new(500.))
        .is_ok());
    assert!(narrow
        .integrate_band(&query(), Nanometers::new(399.), Nanometers::new(500.))
        .is_err());
}

#[test]
fn night_isolation_uses_observatory_local_solar_noon_at_remote_longitudes() {
    let mut req = request();
    req.definition.longitude_deg = -155.;
    req.definition.latitude_deg = 20.;
    req.observations[0].utc = "2023-09-04T14:00:00Z".into();
    // Hawaii-like longitude: this is the September 3 local night, even though
    // it is after UTC noon. The error must be split overlap, before prediction.
    req.reserved_nights = vec!["2023-09-03".into()];
    let error = AirglowCalibrationProfile::fit(req).unwrap_err();
    assert!(error
        .to_string()
        .contains("training/evaluation night overlap"));
}

#[test]
fn portable_replay_retains_artifact_bytes_within_roundoff_and_rejects_negative_statistics() {
    let mut artifact: toml::Value = toml::from_str(&fit().to_toml_string().unwrap()).unwrap();
    let rounded = artifact["scale"].as_float().unwrap() * (1. + 1.0e-13);
    artifact["scale"] = rounded.into();
    artifact["request"]["definition"]["evidence"]["airglow"]["continuum_scale"] = rounded.into();
    let raw = toml::to_string(&artifact).unwrap();
    let loaded = AirglowCalibrationProfile::from_toml_str(&raw).unwrap();
    assert_eq!(loaded.scale().value(), rounded);
    assert_eq!(
        loaded,
        AirglowCalibrationProfile::from_toml_str(&loaded.to_toml_string().unwrap()).unwrap()
    );
    for key in ["scale", "variance", "chi_square"] {
        let mut changed = artifact.clone();
        changed[key] = (-1.0e-30).into();
        assert!(
            AirglowCalibrationProfile::from_toml_str(&toml::to_string(&changed).unwrap()).is_err()
        );
    }
}

#[test]
fn combined_oxygen_point_uncertainty_propagates_only_the_continuum_fit_covariance() {
    let low = fit();
    let mut req = request();
    for o in &mut req.observations {
        o.uncertainty *= 10.;
    }
    let high = AirglowCalibrationProfile::fit(req).unwrap();
    let config = |profile| {
        NsbModelConfig::generic_clear_sky()
            .with_solar_radio_flux(SolarFluxUnits::new(100.))
            .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1)
            .with_airglow_calibration(profile)
            .unwrap()
    };
    let low_result = NsbEvaluator::with_config(config(low.clone()))
        .unwrap()
        .evaluate(&query())
        .unwrap();
    let high_result = NsbEvaluator::with_config(config(high.clone()))
        .unwrap()
        .evaluate(&query())
        .unwrap();
    let total = low_result.integrated.value();
    assert!((total - high_result.integrated.value()).abs() < 1e-12);
    let sigma =
        |r: &nsb::NsbResult| r.components[0].relative_uncertainty.unwrap() * r.integrated.value();
    let continuum = NsbEvaluator::with_config(calibrated(AirglowModel::ParanalPalaceV1))
        .unwrap()
        .evaluate(&query())
        .unwrap()
        .integrated
        .value()
        / low.scale().value();
    let expected_variance_change = continuum.powi(2) * (high.variance() - low.variance());
    let actual_variance_change = sigma(&high_result).powi(2) - sigma(&low_result).powi(2);
    assert!((actual_variance_change - expected_variance_change).abs() < 1e-15);
}
