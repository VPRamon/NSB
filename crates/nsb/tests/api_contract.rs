//! Supported public API surface contracts: site inventory and error diagnostics.
//!
//! Evaluation behaviour lives in `query_api.rs` and `end_to_end_validation.rs`.
//! This suite only pins contracts that those suites do not own.

use nsb::site::{AtmosphericConditions, CalibrationStatus, SiteProfile, SiteProfileTag};
use nsb::{GenericClearSky, NsbError, NsbModelConfig};
use siderust::qtty::{Hectopascals, Kilometers};

#[test]
fn nsb_error_documented_variants_expose_non_empty_diagnostics() {
    let samples: Vec<(NsbError, &'static str)> = vec![
        (
            NsbError::DataParse {
                file: "fixture.csv",
                message: "bad header".into(),
            },
            "data parse error",
        ),
        (
            NsbError::DataMissing {
                file: "map.csv",
                message: "not registered".into(),
            },
            "required data missing",
        ),
        (
            NsbError::InvalidMap {
                message: "bad nside".into(),
            },
            "invalid starlight map",
        ),
        (NsbError::OutOfRange("zenith".into()), "out of range"),
        (NsbError::Unsupported("model".into()), "unsupported"),
        (NsbError::Interpolation("grid".into()), "interpolation"),
        (NsbError::Io(std::io::Error::other("disk")), "io error"),
    ];

    for (err, needle) in samples {
        let message = err.to_string();
        assert!(
            !message.is_empty(),
            "documented NsbError variant must render a diagnostic"
        );
        assert!(
            message.to_lowercase().contains(needle),
            "unexpected Display for {err:?}: {message}"
        );
        // Wildcard arm required: NsbError is #[non_exhaustive].
        let _ = match &err {
            NsbError::DataParse { .. }
            | NsbError::DataMissing { .. }
            | NsbError::InvalidMap { .. }
            | NsbError::OutOfRange(_)
            | NsbError::Unsupported(_)
            | NsbError::Interpolation(_)
            | NsbError::Io(_) => "known",
            _ => "future-variant",
        };
    }
}

#[test]
fn external_marker_defines_planning_profile_without_modifying_core() {
    struct MyObservatory;
    impl SiteProfileTag for MyObservatory {
        const NAME: &'static str = "my-observatory-v1";
    }

    let custom = SiteProfile::<MyObservatory>::planning(
        Kilometers::new(1.5),
        AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(850.0)).unwrap(),
        "integration-test caller-defined profile",
    )
    .unwrap();
    assert_eq!(custom.name(), "my-observatory-v1");
    assert_eq!(
        custom.calibration_status(),
        CalibrationStatus::PlanningPreset
    );

    let config = NsbModelConfig::generic_clear_sky().with_site_profile(custom);
    assert_eq!(config.site_profile_name(), "my-observatory-v1");
    assert_eq!(
        config.airglow_calibration_status(),
        CalibrationStatus::PlanningPreset
    );
    assert!(!config.is_airglow_site_calibrated());
}

#[test]
fn planning_profiles_reject_nonphysical_pressure() {
    for pressure in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(pressure)).is_err()
        );
    }
}

#[test]
fn generic_clear_sky_has_dedicated_typed_identity() {
    let profile = SiteProfile::<GenericClearSky>::generic_clear_sky();
    assert_eq!(profile.name(), GenericClearSky::NAME);
    assert_eq!(
        profile.calibration_status(),
        CalibrationStatus::GenericFallback
    );
    assert_eq!(
        NsbModelConfig::generic_clear_sky().site_profile_name(),
        "generic-clear-sky"
    );
}

#[test]
fn core_public_api_does_not_freeze_observatory_named_profiles() {
    let snapshot = include_str!("../api/public-api.txt");
    for forbidden in [
        "SiteProfileId",
        "SiteProfileSpec",
        "CtaNorth",
        "CtaSouth",
        "cta_n_",
        "cta_s_",
        "NsbModelConfig::cta_n_planning",
        "NsbModelConfig::cta_s_planning",
        "AtmosphericConditions::cta_n_clear_sky",
        "AtmosphericConditions::cta_s_clear_sky",
        "RepresentativeAltitude",
        "AtmosphereSource",
        "AirglowSiteCalibration",
        "ResolvedSiteProfile",
        "ErasedSiteProfile",
        "with_airglow_calibration",
    ] {
        assert!(
            !snapshot.contains(forbidden),
            "public API snapshot must not expose forbidden site-profile symbol {forbidden}"
        );
    }
    assert!(
        !snapshot.lines().any(|line| {
            (line.contains("site::SiteProfile") || line.contains("site::Atmospheric"))
                && line.contains("Cow<")
        }),
        "site-profile public API must not expose Cow<'static, str> storage"
    );
    assert!(
        snapshot.contains("SiteProfileTag"),
        "public API must expose SiteProfileTag for external markers"
    );
    assert!(
        snapshot.contains("GenericClearSky"),
        "public API must expose GenericClearSky"
    );
    assert!(
        snapshot.contains("SiteProfile::generic_clear_sky")
            || snapshot.contains("SiteProfile<nsb::site::GenericClearSky>::generic_clear_sky")
            || snapshot.contains("generic_clear_sky"),
        "public API must retain generic clear-sky construction"
    );
}
