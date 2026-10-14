// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Actual frozen evidence and CLI promotion, with authorization confined to a
//! temporary repository. These fixture identities grant no real permission.
use anyhow::{Context, Result};
use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::licensing::{ArtifactInventory, ArtifactLifecycle};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RC: &str = "docs/nsb_components/starlight/release-candidate";
const LIC: &str = "docs/nsb_components/starlight/licensing";
const MAP: &str = "crates/nsb/data/starlight_nside128.compact.release.bin";
const SIDECAR: &str = "crates/nsb/data/starlight_nside128.compact.manifest.toml";

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let dest = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn fixture() -> Result<tempfile::TempDir> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = tempfile::tempdir()?;
    copy_tree(
        &root.join("docs/nsb_components/starlight"),
        &fixture.path().join("docs/nsb_components/starlight"),
    )?;
    for name in [
        "manifest.toml",
        "starlight_nside128.csv",
        "merge_report.json",
    ] {
        let path = fixture.path().join("crates/nsb/data").join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::copy(root.join("crates/nsb/data").join(name), path)?;
    }
    // Start this negative/transition fixture from the immutable preapproval
    // state even when the real checkout already contains the promoted pair.
    let registry = fixture.path().join("crates/nsb/data/manifest.toml");
    let mut baseline: toml_edit::DocumentMut = fs::read_to_string(&registry)?.parse()?;
    baseline["assets"]
        .as_array_of_tables_mut()
        .context("asset tables")?
        .retain(|table| {
            !matches!(
                table.get("path").and_then(|item| item.as_str()),
                Some("starlight_nside128.compact.release.bin")
                    | Some("starlight_nside128.compact.manifest.toml")
            )
        });
    fs::write(&registry, baseline.to_string())?;
    fs::copy(
        fixture.path().join(LIC).join("approval-inventory-v1.toml"),
        fixture.path().join(LIC).join("artifact-inventory-v1.toml"),
    )?;
    fs::create_dir_all(fixture.path().join("crates/nsb"))?;
    fs::copy(
        root.join("crates/nsb/THIRD_PARTY_NOTICES.md"),
        fixture.path().join("crates/nsb/THIRD_PARTY_NOTICES.md"),
    )?;
    Ok(fixture)
}

fn invoke(root: &Path, extra: &[&str]) -> Result<std::process::Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args(["dataset", "starlight", "promote", "--repository-root"])
        .arg(root)
        .arg("--release-candidate")
        .arg(root.join(RC).join("release-candidate-v1.toml"))
        .arg("--redistribution-decision")
        .arg(root.join(RC).join("redistribution-review-decision-v1.json"))
        .args(extra)
        .output()?)
}

fn succeeds(root: &Path, extra: &[&str]) -> Result<()> {
    let output = invoke(root, extra)?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn real_frozen_evidence_promotes_with_isolated_authorization_and_records_publication() -> Result<()>
{
    let temp = fixture()?;
    let root = temp.path();
    let decision_path = root.join(RC).join("redistribution-review-decision-v1.json");
    // The real decision is untouched. A pending temporary decision must fail.
    let mut pending: Value = serde_json::from_slice(&fs::read(&decision_path)?)?;
    pending["decision"] = "pending".into();
    fs::write(&decision_path, serde_json::to_vec_pretty(&pending)?)?;
    assert!(!invoke(root, &["--apply"])?.status.success());
    assert!(!root.join(MAP).exists());
    let mut approved: Value = serde_json::from_slice(&fs::read(
        root.join(RC)
            .join("redistribution-review-decision-template-v1.json"),
    )?)?;
    fs::write(&decision_path, serde_json::to_vec_pretty(&approved)?)?;
    assert!(
        !invoke(root, &["--apply"])?.status.success(),
        "template null reviewer must never authorize"
    );
    approved["reviewer_name"] = "Synthetic integration fixture; no real authorization".into();
    approved["reviewer_role"] = "Temporary test only".into();
    approved["reviewed_at_utc"] = "2026-01-01T00:00:00Z".into();
    fs::write(&decision_path, serde_json::to_vec_pretty(&approved)?)?;
    let immutable = [
        root.join(LIC).join("approval-inventory-v1.toml"),
        root.join(RC).join("review-bundle-v1.toml"),
        root.join("crates/nsb/data/starlight_nside128.csv"),
        decision_path.clone(),
    ];
    let pins: Vec<_> = immutable
        .iter()
        .map(|p| checksum_io::sha256_file(p))
        .collect::<Result<_>>()?;
    assert!(
        !invoke(root, &["--apply", "--published-channel", "git_repository"])?
            .status
            .success()
    );
    assert!(!root.join(MAP).exists());
    succeeds(root, &["--apply"])?;
    succeeds(root, &["--apply"])?;
    succeeds(root, &[])?;
    let map = nsb::components::starlight::ValidatedStarlightMap::from_files(
        root.join(MAP),
        root.join(SIDECAR),
    )?;
    assert_eq!(map.map().pixels().len(), 196_608);
    assert!(map.diagnostics().flux_conservation_recomputed);
    let inventory_path = root.join(LIC).join("artifact-inventory-v1.toml");
    let registered_bytes = fs::read(&inventory_path)?;
    let inventory: ArtifactInventory = toml::from_str(std::str::from_utf8(&registered_bytes)?)?;
    for id in ["compact-runtime-map-v1", "compact-runtime-sidecar-v1"] {
        let artifact = inventory
            .find(id)
            .context("missing registered inventory artifact")?;
        assert_eq!(
            artifact.lifecycle,
            Some(ArtifactLifecycle::ProductionRegistered)
        );
        assert!(!artifact.distributed && artifact.channels.is_empty());
        assert!(artifact.planned_channels.iter().any(|c| c == "crates_io"));
    }
    // Both approved dispositions verify the complete evidence, even without a
    // bundle condition. Every pinned file is tested against tampering.
    let mut unconditional = approved.clone();
    unconditional["decision"] = "approved".into();
    unconditional["conditions"] = serde_json::json!([]);
    fs::write(&decision_path, serde_json::to_vec_pretty(&unconditional)?)?;
    succeeds(root, &[])?;
    let bundle: toml::Value = toml::from_str(&fs::read_to_string(
        root.join(RC).join("review-bundle-v1.toml"),
    )?)?;
    for artifact in bundle["artifacts"].as_array().unwrap() {
        let path = root.join(artifact["path"].as_str().unwrap());
        let original = fs::read(&path)?;
        fs::write(&path, b"tampered evidence")?;
        assert!(!invoke(root, &[])?.status.success(), "{}", path.display());
        fs::write(&path, original)?;
    }
    fs::write(&decision_path, serde_json::to_vec_pretty(&approved)?)?;
    for path in [
        root.join(MAP),
        root.join(SIDECAR),
        inventory_path.clone(),
        root.join("crates/nsb/data/manifest.toml"),
    ] {
        let original = fs::read(&path)?;
        fs::write(&path, b"tampered installed state")?;
        assert!(!invoke(root, &["--apply"])?.status.success());
        fs::write(&path, original)?;
    }
    let registry_path = root.join("crates/nsb/data/manifest.toml");
    let registry_bytes = fs::read(&registry_path)?;
    let mut partial: toml::Value = toml::from_str(std::str::from_utf8(&registry_bytes)?)?;
    partial["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["path"].as_str() != Some("starlight_nside128.compact.manifest.toml"));
    fs::write(&registry_path, toml::to_string(&partial)?)?;
    assert!(!invoke(root, &["--apply"])?.status.success());
    fs::write(&registry_path, &registry_bytes)?;
    let mut incomplete = approved.clone();
    incomplete["pinned_artifacts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["id"] == "compact-runtime-sidecar-v1")
        .unwrap()["approved_channels"] = serde_json::json!(["git_repository"]);
    fs::write(&decision_path, serde_json::to_vec_pretty(&incomplete)?)?;
    assert!(!invoke(root, &["--apply"])?.status.success());
    fs::write(&decision_path, serde_json::to_vec_pretty(&approved)?)?;
    assert!(!invoke(
        root,
        &["--apply", "--published-channel", "unapproved_channel"]
    )?
    .status
    .success());
    assert_eq!(registered_bytes, fs::read(&inventory_path)?);
    succeeds(
        root,
        &[
            "--apply",
            "--published-channel",
            "git_repository",
            "--published-channel",
            "crates_io",
        ],
    )?;
    let published_bytes = fs::read(&inventory_path)?;
    succeeds(root, &["--apply", "--published-channel", "crates_io"])?;
    assert_eq!(published_bytes, fs::read(&inventory_path)?);
    let inventory: ArtifactInventory = toml::from_str(std::str::from_utf8(&published_bytes)?)?;
    for id in ["compact-runtime-map-v1", "compact-runtime-sidecar-v1"] {
        let artifact = inventory.find(id).unwrap();
        assert_eq!(artifact.lifecycle, Some(ArtifactLifecycle::Published));
        assert!(artifact.distributed);
        assert_eq!(artifact.channels, ["crates_io", "git_repository"]);
    }
    for (path, sha) in immutable.iter().zip(&pins) {
        assert_eq!(&checksum_io::sha256_file(path)?, sha);
    }
    Ok(())
}
