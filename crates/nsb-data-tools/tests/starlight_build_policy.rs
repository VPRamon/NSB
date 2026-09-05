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
    hex_sha256, parse_manifest, select_production_starlight, validate_manifest_structure,
    validate_runtime_embedded_files,
};

#[test]
fn repository_production_starlight_pair_is_selected_and_checksum_verified() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml");
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data");
    let raw = fs::read_to_string(&manifest_path).expect("read repository manifest");
    let manifest: Manifest = parse_manifest(&raw).expect("parse repository manifest");

    validate_manifest_structure(&manifest).expect("structure");
    validate_runtime_embedded_files(&data_dir, &manifest).expect("checksums");

    let pair = select_production_starlight(&manifest)
        .expect("policy")
        .expect("repository currently registers a production Starlight pair");

    assert!(pair.0.is_valid_production_starlight_map());
    assert!(pair.1.is_valid_production_starlight_manifest());
    assert_eq!(
        pair.0.starlight_release_stem(),
        pair.1.starlight_release_stem()
    );

    let map_bytes = fs::read(data_dir.join(&pair.0.path)).expect("read map");
    let sidecar_bytes = fs::read(data_dir.join(&pair.1.path)).expect("read sidecar");
    assert_eq!(hex_sha256(&map_bytes), pair.0.sha256);
    assert_eq!(hex_sha256(&sidecar_bytes), pair.1.sha256);
}
