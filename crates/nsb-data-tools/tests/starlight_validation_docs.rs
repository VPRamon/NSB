// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
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
fn external_validation_is_the_authoritative_scientific_gate() -> Result<()> {
    let obsolete = docs_dir().join("scientific-review-decision-v1.json");
    if obsolete.exists() {
        bail!(
            "obsolete validation/scientific-review-decision-v1.json must not exist; \
             scientific readiness is established by checksum-pinned external validation"
        );
    }

    let obsolete_release = repository_root()
        .join("docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json");
    if obsolete_release.exists() {
        bail!("manual scientific-decision ceremony must not remain authoritative");
    }

    let path = repository_root().join(
        "docs/nsb_components/starlight/validation/results/issue-207-external-cross-validation-v1.json",
    );
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value: Value =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    let object = value
        .as_object()
        .context("external validation must be a JSON object")?;
    if object.get("status").and_then(Value::as_str) != Some("passed")
        || object.get("passed").and_then(Value::as_bool) != Some(true)
    {
        bail!("external scientific validation must pass");
    }
    if object.get("candidate_sha256").and_then(Value::as_str)
        != Some("7e903ff289e76d07c018933b8f97fcf264cead73999912ff63f34b9d1e01b37d")
    {
        bail!("external validation must pin the final issue #207 candidate SHA");
    }
    Ok(())
}
