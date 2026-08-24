use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value as TomlValue;

const REVIEW_BUNDLE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/review-bundle-v1.toml";
const REVIEW_BUNDLE_SHA256: &str =
    "408bc87e26f6e4588d541f93a2226805667290895acf7e48f2261e3ddf3a9163";
const SCIENTIFIC_DECISION_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json";
const REDISTRIBUTION_DECISION_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json";

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    format!("{:x}", Sha256::digest(bytes))
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
    assert_eq!(matching.len(), 1, "decision must pin exactly one review bundle");
    let verifier = &matching[0]["verifier"];
    assert_eq!(
        verifier["type"].as_str(),
        Some("repository_file_sha256")
    );
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
    ] {
        assert!(by_id.contains_key(required), "missing review evidence {required}");
    }

    let (scientific_candidate, scientific_bundle) =
        decision_bundle_pin(&root.join(SCIENTIFIC_DECISION_PATH));
    let (redistribution_candidate, redistribution_bundle) =
        decision_bundle_pin(&root.join(REDISTRIBUTION_DECISION_PATH));
    assert_eq!(scientific_bundle, REVIEW_BUNDLE_SHA256);
    assert_eq!(redistribution_bundle, REVIEW_BUNDLE_SHA256);
    assert_eq!(scientific_candidate, redistribution_candidate);
    assert_eq!(
        by_id.get("candidate_map").map(String::as_str),
        Some(scientific_candidate.as_str())
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
    assert!(workflow.contains("verify_starlight_review_bundle.py"));

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

#[test]
fn python_review_bundle_verifier_accepts_the_frozen_pending_templates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join(".github/scripts/verify_starlight_review_bundle.py");
    let status = match Command::new("python3")
        .arg(&script)
        .arg("--repository-root")
        .arg(&root)
        .arg("--bundle")
        .arg(REVIEW_BUNDLE_PATH)
        .arg("--scientific-decision")
        .arg(SCIENTIFIC_DECISION_PATH)
        .arg("--redistribution-decision")
        .arg(REDISTRIBUTION_DECISION_PATH)
        .status()
    {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("run review-bundle verifier: {error}"),
    };
    assert!(status.success(), "review-bundle verifier rejected frozen templates");
}
