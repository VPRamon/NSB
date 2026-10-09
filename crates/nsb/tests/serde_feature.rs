// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Ensure `nsb/serde` enables the upstream domain types used in NSB's Rust API.

#![cfg(feature = "serde")]

use chrono::{DateTime, Utc};
use nsb::{Observer, Target, DEG};
use siderust::catalogs::observatories;
use tempoch::{Time, J2000s, UTC};

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
