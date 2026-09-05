//! Unit tests for build-time scientific asset validation helpers.
//!
//! These tests exercise the same pure functions used by `crates/nsb/build.rs`
//! without spawning Cargo recursively.

#[path = "../build/generate.rs"]
mod generate;
#[path = "../build/types.rs"]
mod types;
#[path = "../build/validate.rs"]
mod validate;

use generate::generate_bundled_assets_rs;
use std::fs;
use std::path::PathBuf;
use types::{
    is_safe_data_relative_path, Asset, Manifest, EXPECTED_MANIFEST_SCHEMA_VERSION,
    STARLIGHT_MANIFEST_SCHEMA, STARLIGHT_MAP_SCHEMA,
};
use validate::{
    hex_sha256, parse_manifest, select_production_starlight, validate_manifest_structure,
    validate_path_confinement, validate_runtime_embedded_files, verified_runtime_embedded_assets,
    ManifestValidationError,
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

fn production_map(path: &str, sha: &str) -> Asset {
    let mut asset = sample_asset(path, STARLIGHT_MAP_SCHEMA, sha, true);
    asset.calibration_status = "production".into();
    asset
}

fn production_sidecar(path: &str, sha: &str) -> Asset {
    let mut asset = sample_asset(path, STARLIGHT_MANIFEST_SCHEMA, sha, true);
    asset.calibration_status = "production".into();
    asset
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
fn unsafe_manifest_paths_are_rejected() {
    for path in [
        "../Cargo.toml",
        "../../some-file",
        "/absolute/path",
        "\\windows\\style",
        "C:\\abs\\path",
        "nested/../../escape.dat",
    ] {
        assert!(
            !is_safe_data_relative_path(path),
            "expected unsafe path {path:?}"
        );
        assert!(matches!(
            validate_path_confinement(path),
            Err(ManifestValidationError::UnsafePath(_))
        ));
    }
    assert!(is_safe_data_relative_path("airglow_cont.dat"));
    assert!(is_safe_data_relative_path("nested/ok.dat"));
    assert!(validate_path_confinement("nested/ok.dat").is_ok());

    let mut assets = valid_required_set();
    assets.push(sample_asset(
        "../Cargo.toml",
        "nsb-test-v1",
        &"a".repeat(64),
        false,
    ));
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(err, ManifestValidationError::UnsafePath(_)));
}

#[test]
fn candidate_non_embedded_assets_do_not_block_or_enter_verified_codegen() {
    let zero = "0".repeat(64);
    let tmp = tempfile_dir();
    let mut rewritten = Vec::new();
    for (path, schema) in types::REQUIRED_RUNTIME_ASSETS {
        let bytes = format!("fixture:{path}").into_bytes();
        let digest = hex_sha256(&bytes);
        fs::write(tmp.join(path), &bytes).unwrap();
        rewritten.push(sample_asset(path, schema, &digest, true));
    }
    rewritten.push(sample_asset(
        "absent_candidate.dat",
        "nsb-test-candidate-v1",
        &zero,
        false,
    ));
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets: rewritten,
    };
    validate_manifest_structure(&manifest).unwrap();
    validate_runtime_embedded_files(&tmp, &manifest).unwrap();

    let verified = verified_runtime_embedded_assets(&manifest);
    assert!(verified.iter().all(|asset| asset.runtime_embedded));
    assert!(verified
        .iter()
        .all(|asset| asset.path != "absent_candidate.dat"));
    let generated = generate_bundled_assets_rs(manifest.schema_version, &verified, None);
    assert!(generated.contains("airglow_cont.dat"));
    assert!(!generated.contains("absent_candidate.dat"));
}

#[test]
fn missing_runtime_embedded_file_is_rejected() {
    let zero = "0".repeat(64);
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets: vec![sample_asset(
            "missing_embedded.dat",
            "nsb-test-v1",
            &zero,
            true,
        )],
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
    let wrong = "0".repeat(64);
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
fn legitimate_absence_of_production_starlight_is_allowed() {
    let assets = valid_required_set();
    // Candidate-shaped entry must not be treated as a production claim.
    let mut assets = assets;
    assets.push(sample_asset(
        "starlight_nside128.csv",
        "nsb-healpix-starlight-candidate-v5",
        &"b".repeat(64),
        false,
    ));
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    };
    validate_manifest_structure(&manifest).unwrap();
    assert!(select_production_starlight(&manifest).unwrap().is_none());
}

#[test]
fn release_map_without_sidecar_fails_build_policy() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    assets.push(production_map("starlight_nside128.release.csv", &zero));
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(err, ManifestValidationError::StarlightPolicy(_)));
}

#[test]
fn release_sidecar_without_map_fails_build_policy() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    assets.push(production_sidecar(
        "starlight_nside128.manifest.toml",
        &zero,
    ));
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(err, ManifestValidationError::StarlightPolicy(_)));
}

#[test]
fn demoted_release_map_cannot_silently_disable_starlight() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    let mut map = production_map("starlight_nside128.release.csv", &zero);
    map.calibration_status = "candidate".into();
    map.runtime_embedded = false;
    assets.push(map);
    assets.push(production_sidecar(
        "starlight_nside128.manifest.toml",
        &zero,
    ));
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    match err {
        ManifestValidationError::StarlightPolicy(message) => {
            assert!(
                message.contains("not a valid production registration")
                    || message.contains("runtime_embedded"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected StarlightPolicy, got {other:?}"),
    }
}

#[test]
fn incompatible_release_schema_fails_build_policy() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    let mut map = production_map("starlight_nside128.release.csv", &zero);
    map.schema = "nsb-healpix-starlight-candidate-v5".into();
    assets.push(map);
    assets.push(production_sidecar(
        "starlight_nside128.manifest.toml",
        &zero,
    ));
    let err = validate_manifest_structure(&Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    })
    .unwrap_err();
    assert!(matches!(err, ManifestValidationError::StarlightPolicy(_)));
}

#[test]
fn valid_production_starlight_pair_is_selected() {
    let mut assets = valid_required_set();
    let zero = "0".repeat(64);
    assets.push(production_map("starlight_nside128.release.csv", &zero));
    assets.push(production_sidecar(
        "starlight_nside128.manifest.toml",
        &zero,
    ));
    let manifest = Manifest {
        schema_version: EXPECTED_MANIFEST_SCHEMA_VERSION,
        assets,
    };
    validate_manifest_structure(&manifest).unwrap();
    let pair = select_production_starlight(&manifest).unwrap().unwrap();
    assert_eq!(pair.0.path, "starlight_nside128.release.csv");
    assert_eq!(pair.1.path, "starlight_nside128.manifest.toml");
}

#[test]
fn generated_output_is_deterministic_and_verified_only() {
    let manifest = repository_manifest();
    let starlight = select_production_starlight(&manifest).unwrap();
    let verified = verified_runtime_embedded_assets(&manifest);
    let once = generate_bundled_assets_rs(manifest.schema_version, &verified, starlight);
    let twice = generate_bundled_assets_rs(manifest.schema_version, &verified, starlight);
    assert_eq!(once, twice);
    assert!(once.contains("ASSET_MANIFEST_SCHEMA_VERSION"));
    assert!(once.contains("BUNDLED_ASSETS"));
    assert!(once.contains("airglow_cont.dat"));
    assert!(once.contains("f107_store.json"));
    // Candidate registry entries must not appear in verified runtime metadata.
    assert!(!once.contains("merge_report.json"));
    assert!(!once.contains("starlight_nside128.csv"));
    assert!(!once.contains("nsb-healpix-starlight-candidate-v5"));
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
