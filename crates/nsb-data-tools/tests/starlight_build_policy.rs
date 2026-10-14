// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Behavioral contract for build-time Starlight production selection.

#[allow(dead_code)]
#[path = "../../nsb/build/types.rs"]
mod types;
#[allow(dead_code)]
#[path = "../../nsb/build/validate.rs"]
mod validate;

use std::fs;
use std::path::PathBuf;
use types::Manifest;
use validate::{
    parse_manifest, select_production_starlight, validate_manifest_structure,
    validate_runtime_embedded_files,
};

#[test]
fn repository_starlight_selection_respects_pending_authorization() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml");
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data");
    let raw = fs::read_to_string(&manifest_path).expect("read repository manifest");
    let manifest: Manifest = parse_manifest(&raw).expect("parse repository manifest");

    validate_manifest_structure(&manifest).expect("structure");
    validate_runtime_embedded_files(&data_dir, &manifest).expect("embedded checksums");

    let selected = select_production_starlight(&manifest).expect("policy");
    let decision_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json");
    let decision: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(decision_path).unwrap()).unwrap();
    if decision["decision"] == "pending" {
        assert!(
            selected.is_none(),
            "pending human review must not activate bundled Starlight"
        );
    }
    // Candidate registration is evidence only; it must remain unembedded.
    assert!(manifest
        .assets
        .iter()
        .filter(|a| a.path == "starlight_nside128.csv")
        .all(|a| !a.runtime_embedded && a.calibration_status == "candidate"));
}
