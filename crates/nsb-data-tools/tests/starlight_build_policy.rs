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
fn first_release_manifest_has_no_starlight_assets() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml");
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data");
    let raw = fs::read_to_string(&manifest_path).expect("read repository manifest");
    let manifest: Manifest = parse_manifest(&raw).expect("parse repository manifest");

    validate_manifest_structure(&manifest).expect("structure");
    validate_runtime_embedded_files(&data_dir, &manifest).expect("embedded checksums");

    assert!(
        select_production_starlight(&manifest)
            .expect("policy")
            .is_none(),
        "NSB 0.1.0 must not activate bundled production Starlight"
    );

    assert!(manifest
        .assets
        .iter()
        .all(|asset| !asset.path.contains("starlight") && !asset.schema.contains("starlight")));
}
