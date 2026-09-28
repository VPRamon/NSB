//! Supported public API surface contracts: site inventory and error diagnostics.
//!
//! Evaluation behaviour lives in `query_api.rs` and `end_to_end_validation.rs`.
//! This suite only pins contracts that those suites do not own.

use nsb::site::{AtmosphericConditions, CalibrationStatus, SiteProfileSpec};
use nsb::{NsbError, NsbModelConfig, SiteProfileId};
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
fn caller_defined_site_profiles_need_no_core_enum_changes() {
    let custom = SiteProfileSpec::planning(
        "my-observatory-v1",
        "My Observatory planning",
        Kilometers::new(1.5),
        AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(850.0)),
        "integration-test caller-defined profile",
    );
    assert_eq!(custom.id().as_str(), "my-observatory-v1");
    assert_eq!(
        custom.calibration_status(),
        CalibrationStatus::PlanningPreset
    );

    let config = NsbModelConfig::generic_clear_sky().with_site_profile(custom.clone());
    assert_eq!(config.site_profile().id().as_str(), "my-observatory-v1");
    assert_eq!(
        config.site_profile().calibration_status(),
        CalibrationStatus::PlanningPreset
    );
}

#[test]
fn site_profile_id_supports_arbitrary_strings_without_core_inventory() {
    let id = SiteProfileId::new("future-telescope-planning-2028");
    assert_eq!(id.as_str(), "future-telescope-planning-2028");
    assert_eq!(
        SiteProfileId::GENERIC_CLEAR_SKY.as_str(),
        "generic-clear-sky"
    );
}

#[test]
fn core_public_api_does_not_freeze_observatory_named_profiles() {
    let snapshot = include_str!("../api/public-api.txt");
    for forbidden in [
        "SiteProfileId::CtaNorth",
        "SiteProfileId::CtaSouth",
        "SiteProfileId::GenericClearSky",
        "SiteProfileId::all",
        "NsbModelConfig::cta_n_planning",
        "NsbModelConfig::cta_s_planning",
        "SiteProfileId::profile",
        "SiteProfileId::calibration_status",
        "AtmosphericConditions::cta_n_clear_sky",
        "AtmosphericConditions::cta_s_clear_sky",
        "pub nsb::site::SiteProfileSpec::calibration_status:",
        "pub fn nsb::site::SiteProfileSpec::calibrated",
    ] {
        assert!(
            !snapshot.contains(forbidden),
            "public API snapshot must not expose frozen observatory preset {forbidden}"
        );
    }
    assert!(
        snapshot.contains("SiteProfileSpec"),
        "public API must expose SiteProfileSpec for caller-defined profiles"
    );
    assert!(
        snapshot.contains("SiteProfileId::GENERIC_CLEAR_SKY"),
        "public API must retain the generic clear-sky identifier constant"
    );
}
