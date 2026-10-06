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
fn repository_staged_starlight_pair_is_registered_but_not_selected_for_production() {
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
        "pending redistribution must not activate bundled production Starlight"
    );

    let map = manifest
        .assets
        .iter()
        .find(|asset| asset.path == "starlight_nside128.release.csv")
        .expect("staged runtime map registration");
    let sidecar = manifest
        .assets
        .iter()
        .find(|asset| asset.path == "starlight_nside128.manifest.toml")
        .expect("staged runtime sidecar registration");

    for asset in [map, sidecar] {
        assert_eq!(asset.calibration_status, "candidate");
        assert!(!asset.runtime_embedded);
        let bytes = fs::read(data_dir.join(&asset.path)).expect("read staged asset");
        assert_eq!(hex_sha256(&bytes), asset.sha256);
    }
}
