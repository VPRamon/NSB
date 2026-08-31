//! Ties the frozen `docs/nsb_components/starlight/validation/*` documents to
//! the schemas that read them, so the documentation cannot silently drift
//! out of sync with the Rust types that parse it.

use anyhow::{bail, Context, Result};
use nsb_data_tools::starlight::validation::preregistration::Preregistration;
use nsb_data_tools::starlight::validation::references::ReferencesDocument;
use nsb_data_tools::starlight::validation::regions::RegionsDocument;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/nsb_components/starlight/validation")
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn preregistration_document_parses_and_validates() -> Result<()> {
    let path = docs_dir().join("preregistration-v1.toml");
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let document: Preregistration =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    document.validate()
}

#[test]
fn references_document_parses_and_validates_acquired_checksums() -> Result<()> {
    let path = docs_dir().join("references-v1.toml");
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let document: ReferencesDocument =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    document.validate()?;
    if document.acquisition_required {
        bail!(
            "checked-in references-v1.toml must not require acquisition after checksums are pinned"
        );
    }
    if document.acquired().count() != 3 {
        bail!(
            "checked-in references-v1.toml must declare three acquired references, found {}",
            document.acquired().count()
        );
    }
    Ok(())
}

#[test]
fn regions_document_parses_and_validates_at_the_candidate_map_nside() -> Result<()> {
    let path = docs_dir().join("regions-v1.json");
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let document: RegionsDocument =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    document.validate()?;
    if document.nside != 128 {
        bail!(
            "regions-v1.json must use nside=128 to match the candidate map, found {}",
            document.nside
        );
    }
    let required_ids = [
        "all-sky",
        "galactic-plane",
        "galactic-center",
        "anticenter",
        "poles",
        "dark-fields",
        "seam-0-360",
        "dense",
        "high-extinction",
        "bright-star",
        "high-crowding",
    ];
    for id in required_ids {
        if !document.regions.iter().any(|region| region.id == id) {
            bail!("regions-v1.json is missing required region {id}");
        }
    }
    Ok(())
}

#[test]
fn only_release_candidate_scientific_decision_is_authoritative() -> Result<()> {
    let obsolete = docs_dir().join("scientific-review-decision-v1.json");
    if obsolete.exists() {
        bail!(
            "obsolete validation/scientific-review-decision-v1.json must not exist; \
             the only authoritative scientific decision is under release-candidate/"
        );
    }

    let path = repository_root()
        .join("docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json");
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    let object = value
        .as_object()
        .context("scientific-review-decision-v1.json must be a JSON object")?;
    if object.get("decision").and_then(Value::as_str) != Some("pending") {
        bail!("canonical scientific-review-decision-v1.json must have decision = \"pending\"");
    }
    for field in ["reviewer_name", "reviewer_role", "reviewed_at_utc"] {
        if !object.get(field).is_some_and(Value::is_null) {
            bail!("canonical scientific-review-decision-v1.json must leave {field} null");
        }
    }
    if object.get("candidate_sha256").and_then(Value::as_str)
        != Some("b17124d057faad2445575239c04928514d2846ec36a2f5df7137566058d85154")
    {
        bail!("canonical scientific decision must pin the frozen fix-116 candidate SHA");
    }
    Ok(())
}
