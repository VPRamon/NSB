use chrono::{DateTime, Utc};
use nsb::{
    AirglowGeometryModel, AirglowWavelengthApplicability, BandDiagnostic,
    ComponentCalibrationStatus, ComponentMask, NsbEvaluator, PointQuery, Target,
    ValidatedZenithDomain, VerticalEmissionProfile, VerticalEmissionProfileDefinition,
    VerticalProfileNormalization, DEG, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Meters, Nanometers};
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

fn synthetic_profile() -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: "science-metadata-synthetic".into(),
        altitude_km: vec![
            Kilometers::new(80.0),
            Kilometers::new(90.0),
            Kilometers::new(105.0),
        ],
        relative_emissivity: vec![0.0, 1.0, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic-300-650-nm".into(),
        },
        assumptions: "synthetic metadata validation profile; not production data".into(),
        provenance: "deterministic NSB integration test".into(),
        license: "CC0-1.0 synthetic fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .unwrap()
}

#[test]
fn point_results_expose_calibration_provenance_uncertainty_and_band_convention() {
    let evaluator = NsbEvaluator::new().unwrap();
    let result = evaluator
        .evaluate(
            &PointQuery::new(
                observatories::EL_PARANAL.geodetic(),
                parse_utc("2023-09-04T01:48:00Z"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::ALL),
        )
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
    assert!(airglow
        .metadata
        .provenance
        .contains("calibration_status planning-proxy"));
    assert!(airglow
        .metadata
        .provenance
        .contains("baseline_source Cerro Paranal / Noll / SkyCalc-derived"));
    assert!(airglow
        .metadata
        .provenance
        .contains("site_calibrated false"));
    assert!(airglow
        .metadata
        .provenance
        .contains("measured F10.7 does not make Airglow site-calibrated"));
    let solar = airglow
        .metadata
        .solar_activity
        .as_ref()
        .expect("airglow evaluation exposes resolved F10.7");
    assert!(solar.value.value().is_finite() && solar.value.value() > 0.0);
    assert!(
        airglow.metadata.provenance.contains("Cerro Paranal")
            && airglow.metadata.provenance.contains("FORS1"),
        "runtime provenance must surface Paranal/FORS1 lineage from the asset registry"
    );
    assert!(
        airglow
            .metadata
            .validated_domain
            .contains("Noll-2012 effective Rayleigh/Mie airglow scattering"),
        "validated_domain must record applied Noll scattering stage"
    );
    assert!(
        airglow
            .metadata
            .validated_domain
            .contains("molecular atmospheric absorption"),
        "validated_domain must record missing molecular absorption"
    );
    assert!(
        !airglow
            .metadata
            .validated_domain
            .contains("does not apply the upstream Cerro Paranal atmospheric extinction"),
        "validated_domain must not claim extinction is absent"
    );
    let geometry = airglow
        .metadata
        .airglow_geometry
        .as_ref()
        .expect("airglow metadata includes geometry");
    assert_eq!(geometry.model, "van_rhijn");
    assert_eq!(geometry.emission_height_km.unwrap().value(), 90.0);
    assert_eq!(geometry.validated_zenith.min.value(), 0.0);
    assert_eq!(geometry.validated_zenith.max.value(), 90.0);
    assert!(geometry.assumptions.contains("thin"));
    assert!(airglow
        .metadata
        .validated_domain
        .contains("weaker evidence at the UV end"));
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
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW))
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
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW))
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
fn vertical_profile_identity_reaches_metadata_without_upgrading_maturity() {
    let profile = synthetic_profile();
    let expected_checksum = profile.checksum_sha256().to_string();
    let config = nsb::NsbModelConfig::generic_clear_sky()
        .with_airglow_geometry(AirglowGeometryModel::VerticalProfile(profile));
    let result = NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &PointQuery::new(
                observatories::EL_PARANAL.geodetic(),
                parse_utc("2023-09-04T01:48:00Z"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::AIRGLOW),
        )
        .unwrap();
    let airglow = result.components.first().unwrap();
    assert_eq!(
        airglow.metadata.status,
        ComponentCalibrationStatus::GenericClearSky
    );
    let geometry = airglow.metadata.airglow_geometry.as_ref().unwrap();
    assert_eq!(geometry.model, "vertical_profile");
    assert_eq!(
        geometry.profile_id.as_deref(),
        Some("science-metadata-synthetic")
    );
    assert_eq!(geometry.profile_schema_version, Some(1));
    assert_eq!(
        geometry.checksum_sha256.as_deref(),
        Some(expected_checksum.as_str())
    );
    assert_eq!(geometry.normalization, Some("unit-vertical-integral"));
    assert_eq!(geometry.altitude_min_km.unwrap().value(), 80.0);
    assert_eq!(geometry.altitude_max_km.unwrap().value(), 105.0);
    assert_eq!(geometry.wavelength_min_nm.unwrap().value(), 300.0);
    assert_eq!(geometry.wavelength_max_nm.unwrap().value(), 650.0);
    assert!(geometry.provenance.contains("deterministic"));
    assert!(airglow
        .metadata
        .validated_domain
        .contains("independent Noll-2012 effective Rayleigh/Mie"));
}

#[test]
fn airglow_site_profiles_differ_when_atmosphere_differs() {
    let observer = observatories::EL_PARANAL.geodetic();
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let generic = NsbEvaluator::with_config(nsb::NsbModelConfig::generic_clear_sky())
        .unwrap()
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW))
        .unwrap();
    let generic_airglow = generic
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();

    let cta_s = NsbEvaluator::with_config(nsb::NsbModelConfig::cta_s_planning())
        .unwrap()
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW))
        .unwrap();
    let cta_s_airglow = cta_s
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();

    assert_ne!(
        cta_s_airglow.integrated.value(),
        generic_airglow.integrated.value(),
        "CTAO-S Paranal-like atmosphere should differ from generic altitude-derived pressure"
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
        .evaluate(&PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW))
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
    // Geographic genericity of the API must not be confused with global calibration.
    assert!(airglow
        .metadata
        .provenance
        .contains("site_calibrated false"));
    assert!(airglow.metadata.validated_domain.contains("planning proxy"));
}

#[test]
fn airglow_runtime_provenance_tracks_bundled_asset_metadata() {
    use nsb::assets::bundled_asset;

    let asset = bundled_asset("airglow_cont.dat").expect("airglow continuum must be registered");
    let evaluator = NsbEvaluator::new().unwrap();
    let result = evaluator
        .evaluate(
            &PointQuery::new(
                observatories::EL_PARANAL.geodetic(),
                parse_utc("2023-09-04T01:48:00Z"),
                sgr_a_star(),
            )
            .with_components(ComponentMask::AIRGLOW),
        )
        .unwrap();
    let airglow = result
        .components
        .iter()
        .find(|c| c.name == "airglow")
        .unwrap();

    assert!(airglow
        .metadata
        .provenance
        .contains(&format!("sha256 {}", asset.sha256)));
    assert!(airglow
        .metadata
        .provenance
        .contains(&format!("schema {}", asset.schema)));
    assert!(airglow
        .metadata
        .provenance
        .contains(&format!("calibration_status {}", asset.calibration_status)));
    assert!(airglow.metadata.provenance.contains(asset.source));
    assert!(airglow.metadata.provenance.contains(asset.license));
    assert!(airglow.metadata.provenance.contains(asset.generator));
    assert!(airglow
        .metadata
        .provenance
        .contains(asset.validation_report));
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
            .evaluate(
                &PointQuery::new(observer, time, target).with_components(ComponentMask::AIRGLOW),
            )
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
