//! Supported public API surface contracts: site inventory and error diagnostics.
//!
//! Evaluation behaviour lives in `query_api.rs` and `end_to_end_validation.rs`.
//! This suite only pins contracts that those suites do not own.

use nsb::{NsbError, SiteProfileId};

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
        (NsbError::Ephemeris("moon".into()), "ephemeris"),
        (NsbError::Interpolation("grid".into()), "interpolation"),
        (NsbError::UnknownSite("nowhere".into()), "unknown site"),
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
            | NsbError::Ephemeris(_)
            | NsbError::Interpolation(_)
            | NsbError::UnknownSite(_)
            | NsbError::Io(_) => "known",
            _ => "future-variant",
        };
    }
}

#[test]
fn site_profile_id_all_is_complete_unique_inventory() {
    let profiles = SiteProfileId::all();
    assert!(
        profiles.len() >= 3,
        "inventory must expose the supported planning profiles"
    );
    let mut unique = Vec::new();
    for id in profiles {
        assert!(
            !unique.contains(id),
            "SiteProfileId::all must not repeat entries"
        );
        unique.push(*id);
    }
    assert_eq!(unique.len(), profiles.len());
    assert!(profiles.contains(&SiteProfileId::GenericClearSky));
    assert!(profiles.contains(&SiteProfileId::CtaNorth));
    assert!(profiles.contains(&SiteProfileId::CtaSouth));
}
