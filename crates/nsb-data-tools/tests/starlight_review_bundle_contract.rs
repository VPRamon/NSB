use nsb_data_tools::starlight::conditions::verify_review_bundle_evidence;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value as TomlValue;

const REVIEW_BUNDLE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/review-bundle-v1.toml";
const REVIEW_BUNDLE_SHA256: &str =
    "0932766ffe7a5ebd02ef72da35010e02497133569c1e63076b0bd576b3c473f2";
const REDISTRIBUTION_DECISION_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json";
const RELEASE_CANDIDATE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml";
const RUNTIME_ASSETS_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/runtime-assets-v1.toml";
const CANDIDATE_SHA256: &str = "7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d";
const RUNTIME_MAP_SHA256: &str = "d42e7d9c2583b089e6d12f20b2e2ad8693b1f41ec42b2d766b0d7462a2d0d485";
const RUNTIME_SIDECAR_SHA256: &str =
    "f91e8c7442dca03332f9b07236c262b7b997ed4d1ab19dbf98ca8a2bb06627ce";

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("write SHA-256 hex");
    }
    hex
}

fn decision_bundle_pin(path: &Path) -> (String, String) {
    let raw = fs::read_to_string(path).unwrap();
    let decision: JsonValue = serde_json::from_str(&raw).unwrap();
    let candidate = decision["candidate_sha256"]
        .as_str()
        .expect("decision candidate_sha256")
        .to_string();
    let conditions = decision["conditions"]
        .as_array()
        .expect("decision conditions array");
    let matching: Vec<&JsonValue> = conditions
        .iter()
        .filter(|condition| condition["id"].as_str() == Some("review-bundle-v1"))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "decision must pin exactly one review bundle"
    );
    let verifier = &matching[0]["verifier"];
    assert_eq!(verifier["type"].as_str(), Some("repository_file_sha256"));
    assert_eq!(verifier["path"].as_str(), Some(REVIEW_BUNDLE_PATH));
    let bundle_sha = verifier["sha256"]
        .as_str()
        .expect("review bundle sha256")
        .to_string();
    (candidate, bundle_sha)
}

#[test]
fn frozen_review_bundle_pins_exact_release_evidence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bundle_path = root.join(REVIEW_BUNDLE_PATH);
    assert_eq!(sha256_file(&bundle_path), REVIEW_BUNDLE_SHA256);
    verify_review_bundle_evidence(&root, Path::new(REVIEW_BUNDLE_PATH))
        .expect("review bundle and every transitively pinned validation artifact must verify");

    let raw = fs::read_to_string(&bundle_path).unwrap();
    let bundle: TomlValue = toml::from_str(&raw).unwrap();
    assert_eq!(bundle["schema_version"].as_integer(), Some(1));
    assert_eq!(
        bundle["schema"].as_str(),
        Some("nsb-starlight-review-bundle-v1")
    );

    let artifacts = bundle["artifacts"].as_array().expect("bundle artifacts");
    let mut by_id = BTreeMap::new();
    for artifact in artifacts {
        let table = artifact.as_table().expect("artifact table");
        let id = table["id"].as_str().expect("artifact id");
        let path = table["path"].as_str().expect("artifact path");
        let expected = table["sha256"].as_str().expect("artifact sha256");
        assert!(by_id.insert(id.to_string(), expected.to_string()).is_none());
        assert_eq!(
            sha256_file(&root.join(path)),
            expected,
            "review evidence {id} changed without repinning the human review bundle"
        );
    }

    for required in [
        "candidate_map",
        "merge_report",
        "release_candidate_gates",
        "redistribution_inventory",
        "validation_artifact_manifest",
        "runtime_assets_identity",
        "release_candidate_manifest",
        "redistribution_decision_contract_doc",
    ] {
        assert!(
            by_id.contains_key(required),
            "missing review evidence {required}"
        );
    }

    let (redistribution_candidate, redistribution_bundle) =
        decision_bundle_pin(&root.join(REDISTRIBUTION_DECISION_PATH));
    assert_eq!(redistribution_bundle, REVIEW_BUNDLE_SHA256);
    assert_eq!(redistribution_candidate, CANDIDATE_SHA256);
    assert_eq!(
        by_id.get("candidate_map").map(String::as_str),
        Some(CANDIDATE_SHA256)
    );

    let redistribution: JsonValue =
        serde_json::from_str(&fs::read_to_string(root.join(REDISTRIBUTION_DECISION_PATH)).unwrap())
            .unwrap();
    assert_eq!(
        redistribution["review_bundle_sha256"].as_str(),
        Some(REVIEW_BUNDLE_SHA256)
    );

    assert!(
        !root
            .join("docs/nsb_components/starlight/validation/scientific-review-decision-v1.json")
            .exists(),
        "obsolete validation scientific-decision template must not exist"
    );
    assert!(
        !root
            .join("docs/nsb_components/starlight/licensing/redistribution-review-decision-v1.json")
            .exists(),
        "obsolete licensing redistribution-decision template must not exist"
    );
}

#[test]
fn release_candidate_and_runtime_assets_agree_semantically() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let release_candidate: TomlValue =
        toml::from_str(&fs::read_to_string(root.join(RELEASE_CANDIDATE_PATH)).unwrap()).unwrap();
    let runtime_assets: TomlValue =
        toml::from_str(&fs::read_to_string(root.join(RUNTIME_ASSETS_PATH)).unwrap()).unwrap();

    let candidate = release_candidate["candidate"].as_table().unwrap();
    let review = release_candidate["review_artifacts"].as_table().unwrap();

    assert_eq!(
        candidate["map_path"].as_str(),
        runtime_assets["candidate_path"].as_str()
    );
    assert_eq!(
        candidate["candidate_sha256"].as_str(),
        runtime_assets["candidate_sha256"].as_str()
    );
    assert_eq!(
        candidate["candidate_sha256"].as_str(),
        Some(CANDIDATE_SHA256)
    );

    assert_eq!(
        review["runtime_map_path"].as_str(),
        runtime_assets["runtime_map_path"].as_str()
    );
    assert_eq!(
        review["runtime_map_sha256"].as_str(),
        runtime_assets["runtime_map_sha256"].as_str()
    );
    assert_eq!(
        review["runtime_map_sha256"].as_str(),
        Some(RUNTIME_MAP_SHA256)
    );
    assert_eq!(
        runtime_assets["runtime_map_schema"].as_str(),
        Some("nsb-healpix-starlight-v2")
    );

    assert_eq!(
        review["runtime_sidecar_path"].as_str(),
        runtime_assets["runtime_sidecar_path"].as_str()
    );
    assert_eq!(
        review["runtime_sidecar_sha256"].as_str(),
        runtime_assets["runtime_sidecar_sha256"].as_str()
    );
    assert_eq!(
        review["runtime_sidecar_sha256"].as_str(),
        Some(RUNTIME_SIDECAR_SHA256)
    );
    assert_eq!(
        runtime_assets["runtime_sidecar_schema"].as_str(),
        Some("nsb-starlight-runtime-manifest-v2")
    );

    assert_eq!(
        review["licensing_decision_path"].as_str(),
        Some(REDISTRIBUTION_DECISION_PATH)
    );
}

#[test]
fn stage_runtime_cli_emits_the_pinned_provenance_complete_assets() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temporary = tempfile::tempdir().unwrap();
    let runtime_map = temporary.path().join("starlight.release.csv");
    let runtime_sidecar = temporary.path().join("starlight.manifest.toml");
    let output = Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args([
            "dataset",
            "starlight",
            "stage-runtime",
            "--release-candidate",
        ])
        .arg(root.join(RELEASE_CANDIDATE_PATH))
        .arg("--repository-root")
        .arg(&root)
        .arg("--output-csv")
        .arg(&runtime_map)
        .arg("--output-sidecar")
        .arg(&runtime_sidecar)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stage-runtime failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(CANDIDATE_SHA256));
    assert!(stdout.contains(RUNTIME_MAP_SHA256));
    assert!(stdout.contains(RUNTIME_SIDECAR_SHA256));
    assert_eq!(sha256_file(&runtime_map), RUNTIME_MAP_SHA256);
    assert_eq!(sha256_file(&runtime_sidecar), RUNTIME_SIDECAR_SHA256);
    let sidecar = fs::read_to_string(runtime_sidecar).unwrap();
    assert!(sidecar.contains("schema_version = 2"));
    assert!(sidecar.contains("starlight-bright-stars-combined-v1"));
    assert!(sidecar.contains("ck04-hp-scaled-uv-300-336-v1"));
}

#[test]
fn final_promotion_is_main_only_and_verifies_review_bundle_first() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workflow = fs::read_to_string(root.join(".github/workflows/starlight-final-promotion.yml"))
        .expect("read final promotion workflow");

    assert!(workflow.contains("- name: Checkout approved main only"));
    assert!(workflow.contains("ref: main"));
    assert!(workflow.contains("${GITHUB_REF}"));
    assert!(workflow.contains("refs/heads/main"));
    assert!(workflow.contains("git rev-parse origin/main"));
    assert!(workflow.contains("- name: Require canonical promotion source and inputs"));
    assert!(workflow.contains("--test starlight_review_bundle_contract"));
    assert!(workflow.contains("frozen_review_bundle_pins_exact_release_evidence -- --exact"));
    assert!(!workflow.contains("verify_starlight_review_bundle.py"));
    assert!(!root
        .join(".github/scripts/verify_starlight_review_bundle.py")
        .exists());

    let verify_pos = workflow
        .find("Verify frozen release evidence bundle")
        .expect("review bundle verification step");
    let promote_pos = workflow
        .find("Pack runtime map and apply production registry")
        .expect("promotion step");
    assert!(
        verify_pos < promote_pos,
        "release evidence bundle must be verified before any runtime asset is packed/applied"
    );
}
