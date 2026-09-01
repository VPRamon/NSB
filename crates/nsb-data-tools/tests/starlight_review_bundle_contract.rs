use nsb_data_tools::starlight::conditions::verify_review_bundle_evidence;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

const REVIEW_BUNDLE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/review-bundle-v1.toml";
const REVIEW_BUNDLE_SHA256: &str =
    "68fecf5d5635597d9c7038f2ad57454ccb241abb424f76a58b886f277c96e403";
const SCIENTIFIC_DECISION_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json";
const REDISTRIBUTION_DECISION_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json";
const RELEASE_CANDIDATE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml";
const RUNTIME_ASSETS_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/runtime-assets-v1.toml";
const CANDIDATE_SHA256: &str = "b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154";
const RUNTIME_MAP_SHA256: &str = "a458debfd4665b590d27f952352a0d3f69b33d88635ed08c587202ff8a30bab3";
const RUNTIME_SIDECAR_SHA256: &str =
    "3fde80dc418ecc71865e04a792c0fb3cc7dbc6f62456827883f0e538863d7afb";

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
fn frozen_review_bundle_pins_exact_human_evidence() {
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

    let (scientific_candidate, scientific_bundle) =
        decision_bundle_pin(&root.join(SCIENTIFIC_DECISION_PATH));
    let (redistribution_candidate, redistribution_bundle) =
        decision_bundle_pin(&root.join(REDISTRIBUTION_DECISION_PATH));
    assert_eq!(scientific_bundle, REVIEW_BUNDLE_SHA256);
    assert_eq!(redistribution_bundle, REVIEW_BUNDLE_SHA256);
    assert_eq!(scientific_candidate, redistribution_candidate);
    assert_eq!(scientific_candidate, CANDIDATE_SHA256);
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
        Some("nsb-starlight-runtime-manifest-v1")
    );

    assert_eq!(
        review["licensing_decision_path"].as_str(),
        Some(REDISTRIBUTION_DECISION_PATH)
    );
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
    assert!(workflow.contains("frozen_review_bundle_pins_exact_human_evidence -- --exact"));
    assert!(!workflow.contains("verify_starlight_review_bundle.py"));
    assert!(!root
        .join(".github/scripts/verify_starlight_review_bundle.py")
        .exists());

    let verify_pos = workflow
        .find("Verify frozen human review bundle")
        .expect("review bundle verification step");
    let promote_pos = workflow
        .find("Pack runtime map and apply production registry")
        .expect("promotion step");
    assert!(
        verify_pos < promote_pos,
        "human evidence bundle must be verified before any runtime asset is packed/applied"
    );
}
