//! Fail-closed pre-promotion gate for the frozen Starlight release candidate.
//!
//! Scope note (issue #90): this is intentionally a thin, minimal
//! implementation of the checksum/schema/decision half of `dataset starlight
//! promote` — enough for the `starlight-final-promotion` workflow to have a
//! real, fail-closed command to run instead of a shell placeholder. It does
//! **not** implement the `nsb-starlight-release-candidate-v1` bundle, the
//! `StarlightModel::BundledProductionGaiaDr3` runtime path, or manifest
//! mutation (`calibration_status = "production"`, `runtime_embedded = true`).
//! Those remain issue #89's scope; this command should be extended, not
//! replaced, once #89 lands. It never writes to the map, the manifest, or any
//! decision file.
//!
//! Every failure mode below is fail-closed: a missing file, an unparsable
//! file, a non-`"approved"` decision, a missing reviewer, or a checksum
//! mismatch all abort with a non-zero exit and an explanation, and none of
//! them promote anything.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::checksum_io::sha256_file;

const SUPPORTED_GATES_SCHEMA: u32 = 1;

/// Inputs for the fail-closed promotion precondition check.
#[derive(Debug, Clone)]
pub struct PromoteArgs {
    /// `release-candidate-gates-v1` report (see issue #90).
    pub release_candidate_gates: PathBuf,
    /// Human scientific-review decision file (owned by issue #47).
    pub scientific_decision: PathBuf,
    /// Human redistribution-review decision file (owned by issue #47).
    pub redistribution_decision: PathBuf,
    /// Canonical Starlight map to checksum; never mutated.
    pub map: PathBuf,
}

#[derive(Debug, Deserialize)]
struct GatesReport {
    schema_version: u32,
    #[serde(default)]
    passed: bool,
    #[serde(default)]
    status: String,
    #[serde(default)]
    candidate_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DecisionFile {
    decision: String,
    #[serde(default)]
    reviewer_name: Option<String>,
    #[serde(default)]
    reviewer_role: Option<String>,
    #[serde(default)]
    reviewed_at_utc: Option<String>,
    #[serde(default)]
    candidate_map_sha256: Option<String>,
}

/// Verify every fail-closed promotion precondition. Never mutates any input.
pub fn run(args: &PromoteArgs) -> Result<()> {
    let gates = read_json::<GatesReport>(&args.release_candidate_gates, "release-candidate gates")?;
    let scientific = read_json::<DecisionFile>(&args.scientific_decision, "scientific decision")?;
    let redistribution =
        read_json::<DecisionFile>(&args.redistribution_decision, "redistribution decision")?;

    if gates.schema_version != SUPPORTED_GATES_SCHEMA {
        bail!(
            "fail-closed: unsupported release-candidate gates schema {} (expected {SUPPORTED_GATES_SCHEMA})",
            gates.schema_version
        );
    }
    if !gates.passed || gates.status != "gates_passed" {
        bail!(
            "fail-closed: release-candidate gates have not passed (status={:?}, passed={}); see {}",
            gates.status,
            gates.passed,
            args.release_candidate_gates.display()
        );
    }
    let gates_sha256 = gates.candidate_sha256.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "fail-closed: {} does not pin candidate_sha256",
            args.release_candidate_gates.display()
        )
    })?;

    require_approved(&scientific, "scientific")?;
    require_approved(&redistribution, "redistribution")?;

    let map_sha256 = sha256_file(&args.map)
        .with_context(|| format!("failed to checksum {}", args.map.display()))?;

    if gates_sha256 != map_sha256 {
        bail!(
            "fail-closed: {} has candidate_sha256 {gates_sha256}, but {} actually hashes to {map_sha256}",
            args.release_candidate_gates.display(),
            args.map.display()
        );
    }
    let scientific_sha256 = scientific.candidate_map_sha256.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "fail-closed: {} does not pin candidate_map_sha256",
            args.scientific_decision.display()
        )
    })?;
    if scientific_sha256 != map_sha256 {
        bail!(
            "fail-closed: {} approved candidate_map_sha256 {scientific_sha256}, but {} actually hashes to {map_sha256}",
            args.scientific_decision.display(),
            args.map.display()
        );
    }

    println!("promotion preconditions satisfied");
    println!("map={}", args.map.display());
    println!("map_sha256={map_sha256}");
    println!(
        "note: fail-closed checksum/schema/decision verification only; no map, manifest, or \
         decision file was modified. Manifest promotion (calibration_status=production, \
         runtime_embedded=true) and the release-candidate bundle remain issue #89's scope."
    );
    Ok(())
}

fn require_approved(decision: &DecisionFile, label: &str) -> Result<()> {
    if decision.decision != "approved" {
        bail!(
            "fail-closed: {label} decision is {:?}, not \"approved\"",
            decision.decision
        );
    }
    if is_blank(decision.reviewer_name.as_deref()) {
        bail!("fail-closed: {label} decision has no reviewer_name");
    }
    if is_blank(decision.reviewer_role.as_deref()) {
        bail!("fail-closed: {label} decision has no reviewer_role");
    }
    if is_blank(decision.reviewed_at_utc.as_deref()) {
        bail!("fail-closed: {label} decision has no reviewed_at_utc");
    }
    Ok(())
}

fn is_blank(value: Option<&str>) -> bool {
    value.map(str::trim).unwrap_or("").is_empty()
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("fail-closed: failed to read {label} at {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("fail-closed: failed to parse {label} at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn gates_json(passed: bool, status: &str, sha256: &str) -> String {
        format!(
            r#"{{"schema_version": 1, "passed": {passed}, "status": "{status}", "candidate_sha256": "{sha256}"}}"#
        )
    }

    fn approved_decision_json(sha256: &str) -> String {
        format!(
            r#"{{"decision": "approved", "reviewer_name": "Dr. Reviewer", "reviewer_role": "scientific-lead", "reviewed_at_utc": "2026-07-30T00:00:00Z", "candidate_map_sha256": "{sha256}"}}"#
        )
    }

    fn pending_decision_json() -> &'static str {
        r#"{"decision": "pending", "reviewer_name": null, "reviewer_role": null, "reviewed_at_utc": null, "candidate_map_sha256": null}"#
    }

    fn args_with(dir: &TempDir, map: PathBuf, sha256: &str, gates_passed: bool) -> PromoteArgs {
        let gates = write(
            dir,
            "gates.json",
            &gates_json(gates_passed, "gates_passed", sha256),
        );
        let scientific = write(dir, "scientific.json", &approved_decision_json(sha256));
        let redistribution = write(dir, "redistribution.json", &approved_decision_json(sha256));
        PromoteArgs {
            release_candidate_gates: gates,
            scientific_decision: scientific,
            redistribution_decision: redistribution,
            map,
        }
    }

    #[test]
    fn approved_matching_checksums_succeed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let args = args_with(&dir, map, &sha256, true);
        run(&args).expect("fail-closed checks should pass when everything matches");
    }

    #[test]
    fn gates_not_passed_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.release_candidate_gates = write(
            &dir,
            "gates_failed.json",
            &gates_json(false, "awaiting_regeneration", &sha256),
        );
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
        assert!(error.contains("gates"));
    }

    #[test]
    fn pending_scientific_decision_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.scientific_decision = write(&dir, "pending.json", pending_decision_json());
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
        assert!(error.contains("scientific"));
        assert!(error.contains("approved"));
    }

    #[test]
    fn pending_redistribution_decision_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.redistribution_decision = write(&dir, "pending.json", pending_decision_json());
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
        assert!(error.contains("redistribution"));
    }

    #[test]
    fn wrong_checksum_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let wrong_sha256 = "0".repeat(64);
        let args = args_with(&dir, map, &wrong_sha256, true);
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
        assert!(error.contains("actually hashes to"));
    }

    #[test]
    fn missing_reviewer_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.scientific_decision = write(
            &dir,
            "no_reviewer.json",
            &format!(
                r#"{{"decision": "approved", "reviewer_name": "", "reviewer_role": "lead", "reviewed_at_utc": "2026-07-30T00:00:00Z", "candidate_map_sha256": "{sha256}"}}"#
            ),
        );
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
        assert!(error.contains("reviewer_name"));
    }

    #[test]
    fn missing_gates_file_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.release_candidate_gates = dir.path().join("does-not-exist.json");
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
    }

    #[test]
    fn malformed_decision_json_fails_closed() {
        let dir = TempDir::new().unwrap();
        let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
        let sha256 = sha256_file(&map).unwrap();
        let mut args = args_with(&dir, map, &sha256, true);
        args.scientific_decision = write(&dir, "malformed.json", "{not valid json");
        let error = run(&args).unwrap_err().to_string();
        assert!(error.contains("fail-closed"));
    }
}
