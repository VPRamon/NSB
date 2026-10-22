// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Build-time scientific asset validation and codegen for the `nsb` crate.

pub mod generate;
pub mod types;
pub mod validate;

use self::generate::generate_bundled_assets_rs;
use self::validate::{
    parse_manifest, select_production_starlight, validate_manifest_structure,
    validate_runtime_embedded_files, verified_runtime_embedded_assets,
};
use std::env;
use std::fs;
use std::path::PathBuf;

const GENERATED: &str = "nsb_bundled_assets.rs";

/// Entry point invoked from `build.rs`.
pub fn run() {
    println!("cargo:rerun-if-changed=data/manifest.toml");
    println!("cargo:rustc-check-cfg=cfg(nsb_bundled_production_starlight)");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let data_dir = manifest_dir.join("data");
    let manifest_path = data_dir.join("manifest.toml");
    let manifest_raw = fs::read_to_string(&manifest_path).unwrap_or_else(|err| {
        panic!("failed to read {}: {err}", manifest_path.display());
    });
    let mut manifest = parse_manifest(&manifest_raw).unwrap_or_else(|err| panic!("{err}"));
    // The original manifest is checksum-pinned by immutable Starlight approval.
    // Append only this component-owned experimental asset; supplemental metadata
    // cannot replace or confer admission on any original asset.
    println!("cargo:rerun-if-changed=data/airglow-oxygen-manifest.toml");
    let oxygen_raw = fs::read_to_string(data_dir.join("airglow-oxygen-manifest.toml"))
        .expect("read experimental oxygen registry");
    let oxygen = parse_manifest(&oxygen_raw).expect("parse experimental oxygen registry");
    assert_eq!(oxygen.schema_version, manifest.schema_version);
    assert_eq!(oxygen.assets.len(), 1);
    let asset = &oxygen.assets[0];
    assert_eq!(asset.path, "airglow_palace_oxygen_v1.json");
    assert_eq!(asset.schema, "nsb-airglow-palace-oxygen-v1");
    assert_eq!(asset.calibration_status, "experimental-planning-proxy");
    assert!(asset.runtime_embedded);
    manifest.assets.extend(oxygen.assets);
    validate_manifest_structure(&manifest).unwrap_or_else(|err| {
        panic!("scientific asset manifest validation failed: {err}");
    });
    validate_runtime_embedded_files(&data_dir, &manifest).unwrap_or_else(|err| {
        panic!("scientific asset integrity validation failed: {err}");
    });

    for asset in verified_runtime_embedded_assets(&manifest) {
        println!("cargo:rerun-if-changed=data/{}", asset.path);
    }

    let starlight = select_production_starlight(&manifest).unwrap_or_else(|err| panic!("{err}"));
    if starlight.is_some() {
        println!("cargo:rustc-cfg=nsb_bundled_production_starlight");
    }

    let verified = verified_runtime_embedded_assets(&manifest);
    let generated = generate_bundled_assets_rs(manifest.schema_version, &verified, starlight);
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join(GENERATED);
    fs::write(&out_path, generated)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", out_path.display()));
}
