//! Unit tests for build-time scientific asset validation helpers.
//!
//! These tests exercise the same pure functions used by `crates/nsb/build.rs`
//! without spawning Cargo recursively.

#[path = "../build_support/generate.rs"]
mod generate;
#[path = "../build_support/types.rs"]
mod types;
#[path = "../build_support/validate.rs"]
mod validate;

use generate::generate_bundled_assets_rs;
use std::fs;
use std::path::PathBuf;
use types::{Asset, Manifest, EXPECTED_MANIFEST_SCHEMA_VERSION};
use validate::{
    hex_sha256, parse_manifest, select_production_starlight, validate_manifest_structure,
    validate_runtime_embedded_files, ManifestValidationError,
};

fn repository_manifest() -> Manifest {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/manifest.toml");
    let raw = fs::read_to_string(path).expect("read repository manifest");
    parse_manifest(&raw).expect("parse repository manifest")
}

fn sample_asset(path: &str, schema: &str, sha: &str, embedded: bool) -> Asset {
    Asset {
        path: path.into(),
        schema: schema.into(),
        sha256: sha.into(),
        source: "test source".into(),
        license: "test license".into(),
        generator: "test".into(),
        generation_command: "test".into(),
        validation_report: "docs/test.md".into(),
        calibration_status: "planning-proxy".into(),
        runtime_embedded: embedded,
        header: Default::default(),
    }
}

fn valid_required_set() -> Vec<Asset> {
    // Digests are placeholders; structure tests do not touch the filesystem.
    let zero = "0".repeat(64);
    types::REQUIRED_RUNTIME_ASSETS
        .iter()
        .map(|(path, schema)| sample_asset(path, schema, &zero, true))
        .collect()
}

#[test]
fn repository_manifest_is_structurally_valid() {
    let manifest = repository_manifest();
    validate_manifest_structure(&manifest).expect("repository manifest structure");
    assert_eq!(manifest.schema_version, EXPECTED_MANIFEST_SCHEMA_VERSION);
}

#[test]
fn repository_runtime_embedded_files_match_checksums() {
    let manifest = repository_manifest();
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    validate_runtime_embedded_files(&data_dir, &manifest).expect("embedded integrity");
}

#[test]
fn schema_version_mismatch_is_rejected() {
    let mut manifest = Manifest {
        schema_version: 99,
        assets: valid_required_set(),
    };
    // Keep starlight pair empty.
    let err = validate_manifest_structure(&manifest).unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::SchemaVersion {
            found: 99,
            expected: EXPECTED_MANIFEST_SCHEMA_VERSION
        }
    ));
    manifest.schema_version = EXPECTED_MANIFEST_SCHEMA_VERSION;
    validate_manifest_structure(&manifest).unwrap();
}

#[test]
fn duplicate_paths_are_rejected() {
    let mut assets = valid_required_set();
    assets.push(assets[0].clone());
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(err, ManifestValidationError::DuplicatePath(_)));
}

#[test]
fn missing_required_runtime_asset_is_rejected() {
    let mut assets = valid_required_set();
    assets.retain(|asset| asset.path != "airglow_cont.dat");
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::RequiredAsset { path, .. } if path == "airglow_cont.dat"
    ));
}

#[test]
fn invalid_required_schema_is_rejected() {
    let mut assets = valid_required_set();
    let airglow = assets
        .iter_mut()
        .find(|asset| asset.path == "airglow_cont.dat")
        .unwrap();
    airglow.schema = "wrong-schema".into();
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::RequiredAsset {
            path,
            expected_schema,
            ..
        } if path == "airglow_cont.dat" && expected_schema == "skycalc-airglow-continuum-v1"
    ));
}

#[test]
fn candidate_non_embedded_assets_do_not_block_structure_validation() {
    let mut assets = valid_required_set();
    assets.push(sample_asset(
        "candidate_only.dat",
        "nsb-test-candidate-v1",
        &"a".repeat(64),
        false,
    ));
    validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap();
}

#[test]
fn missing_runtime_embedded_file_is_rejected() {
    let zero = "0".repeat(64);
    let assets = vec![sample_asset(
        "missing_embedded.dat",
        "nsb-test-v1",
        &zero,
        true,
    )];
    // Bypass required-asset checks by validating files only.
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    };
    let tmp = tempfile_dir();
    let err = validate_runtime_embedded_files(&tmp, &manifest).unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::MissingEmbeddedFile(path) if path == "missing_embedded.dat"
    ));
}

#[test]
fn checksum_mismatch_is_rejected() {
    let tmp = tempfile_dir();
    let path = "bad_checksum.dat";
    fs::write(tmp.join(path), b"hello").unwrap();
    let actual = hex_sha256(b"hello");
    let wrong = "0".repeat(64);
    assert_ne!(actual, wrong);
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets: vec![sample_asset(path, "nsb-test-v1", &wrong, true)],
    };
    let err = validate_runtime_embedded_files(&tmp, &manifest).unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::ChecksumMismatch { path: p, .. } if p == path
    ));
}

#[test]
fn non_embedded_candidate_missing_on_disk_is_ignored() {
    let tmp = tempfile_dir();
    let zero = "0".repeat(64);
    // Create only required embedded assets with matching digests.
    let mut assets = Vec::new();
    for (path, schema) in types::REQUIRED_RUNTIME_ASSETS {
        let bytes = format!("fixture:{path}").into_bytes();
        let digest = hex_sha256(&bytes);
        fs::write(tmp.join(path), &bytes).unwrap();
        assets.push(sample_asset(path, schema, &digest, true));
    }
    assets.push(sample_asset(
        "absent_candidate.dat",
        "nsb-test-candidate-v1",
        &zero,
        false,
    ));
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    };
    validate_manifest_structure(&manifest).unwrap();
    validate_runtime_embedded_files(&tmp, &manifest).unwrap();
}

#[test]
fn starlight_pair_must_be_zero_or_one() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    assets.push(Asset {
        path: "a.release.csv".into(),
        schema: types::STARLIGHT_MAP_SCHEMA.into(),
        sha256: zero.clone(),
        source: "s".into(),
        license: "l".into(),
        generator: "g".into(),
        generation_command: "c".into(),
        validation_report: "v".into(),
        calibration_status: "production".into(),
        runtime_embedded: true,
        header: Default::default(),
    });
    // Map without matching sidecar.
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(
        err,
        ManifestValidationError::StarlightPair {
            maps: 1,
            manifests: 0
        }
    ));
}

#[test]
fn generated_output_is_deterministic() {
    let manifest = repository_manifest();
    let starlight = select_production_starlight(&manifest).unwrap();
    let once = generate_bundled_assets_rs(manifest.schema_version, &manifest.assets, starlight);
    let twice = generate_bundled_assets_rs(manifest.schema_version, &manifest.assets, starlight);
    assert_eq!(once, twice);
    assert!(once.contains("ASSET_MANIFEST_SCHEMA_VERSION"));
    assert!(once.contains("BUNDLED_ASSETS"));
    assert!(!once.contains("toml::"));
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nsb-manifest-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
