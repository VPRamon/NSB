// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Ensure `nsb/serde` enables the upstream domain types used in NSB's Rust API.

#![cfg(feature = "serde")]

use chrono::{DateTime, Utc};
use nsb::{Observer, Target, DEG};
use siderust::catalogs::observatories;
use tempoch::{J2000s, Time, UTC};

#[test]
fn serde_feature_serializes_upstream_observer_target_and_utc_time() {
    let observer: Observer = observatories::EL_PARANAL.geodetic();
    let json = serde_json::to_string(&observer).expect("serialize NSB observer");
    let restored: Observer = serde_json::from_str(&json).expect("deserialize NSB observer");
    assert_eq!(observer.lon, restored.lon);
    assert_eq!(observer.lat, restored.lat);
    assert_eq!(observer.height, restored.height);

    let target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    let json = serde_json::to_string(&target).expect("serialize NSB target");
    let restored: Target = serde_json::from_str(&json).expect("deserialize NSB target");
    assert_eq!(target.ra(), restored.ra());
    assert_eq!(target.dec(), restored.dec());

    let instant = DateTime::<Utc>::from_timestamp(1_704_067_200, 0).expect("valid UTC instant");
    let time = Time::<UTC>::from_chrono(instant);
    let json = serde_json::to_string(&time).expect("serialize NSB UTC time");
    let restored: Time<UTC> = serde_json::from_str(&json).expect("deserialize NSB UTC time");
    assert_eq!(time.to::<J2000s>(), restored.to::<J2000s>());
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct PhotometricConstraint {
    #[serde(with = "nsb::serde_support::surface_brightness")]
    minimum_nsb_in_mag_arcsec2: nsb::units::photometry::SurfaceBrightness,
    #[serde(with = "nsb::serde_support::surface_brightness")]
    maximum_nsb_in_mag_arcsec2: nsb::units::photometry::SurfaceBrightness,
}

#[test]
fn serde_feature_supports_numeric_surface_brightness_in_application_contracts() {
    let input = serde_json::json!({
        "minimum_nsb_in_mag_arcsec2": 21.39,
        "maximum_nsb_in_mag_arcsec2": 30.0
    });
    let constraint: PhotometricConstraint =
        serde_json::from_value(input.clone()).expect("numeric NSB bounds");
    assert_eq!(constraint.minimum_nsb_in_mag_arcsec2.value(), 21.39);
    assert_eq!(constraint.maximum_nsb_in_mag_arcsec2.value(), 30.0);
    assert_eq!(
        serde_json::to_value(&constraint).expect("serialize typed bounds"),
        input
    );

    // Deserialization only establishes a type and wire format; the service
    // still owns finite, positive and ordered range validation.
    for scalar in [0.0, -1.5] {
        let value: PhotometricConstraint = serde_json::from_value(serde_json::json!({
            "minimum_nsb_in_mag_arcsec2": scalar,
            "maximum_nsb_in_mag_arcsec2": 20.0
        }))
        .expect("photometric type is not a domain constraint validator");
        assert_eq!(value.minimum_nsb_in_mag_arcsec2.value(), scalar);
    }
}

#[test]
fn serde_feature_rejects_non_numeric_surface_brightness_wire_values() {
    for invalid in [
        serde_json::json!("21.39"),
        serde_json::json!({"value": 21.39, "unit": "mag/arcsec2"}),
        serde_json::Value::Null,
        serde_json::json!(true),
    ] {
        let input = serde_json::json!({
            "minimum_nsb_in_mag_arcsec2": invalid,
            "maximum_nsb_in_mag_arcsec2": 30.0
        });
        assert!(serde_json::from_value::<PhotometricConstraint>(input).is_err());
    }
}
