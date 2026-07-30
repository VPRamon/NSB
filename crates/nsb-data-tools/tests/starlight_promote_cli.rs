//! CLI-level fail-closed checks for `dataset starlight promote` (issue #90).
//!
//! These exercise the actual built binary end to end (argument parsing plus
//! the promotion precondition check), complementing the unit tests in
//! `nsb_data_tools::starlight::promote`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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

fn sha256_hex(path: &Path) -> String {
    nsb_data_tools::platform::checksum_io::sha256_file(path).unwrap()
}

fn promote(gates: &Path, scientific: &Path, redistribution: &Path, map: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args([
            "dataset",
            "starlight",
            "promote",
            "--release-candidate-gates",
            gates.to_str().unwrap(),
            "--scientific-decision",
            scientific.to_str().unwrap(),
            "--redistribution-decision",
            redistribution.to_str().unwrap(),
            "--map",
            map.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn approved_decisions_and_matching_checksums_succeed() {
    let dir = TempDir::new().unwrap();
    let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
    let sha256 = sha256_hex(&map);
    let gates = write(&dir, "gates.json", &gates_json(true, "gates_passed", &sha256));
    let scientific = write(&dir, "scientific.json", &approved_decision_json(&sha256));
    let redistribution = write(&dir, "redistribution.json", &approved_decision_json(&sha256));

    let output = promote(&gates, &scientific, &redistribution, &map);
    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("promotion preconditions satisfied"));
    assert!(stdout.contains(&sha256));
}

#[test]
fn pending_scientific_decision_fails_closed_via_cli() {
    let dir = TempDir::new().unwrap();
    let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
    let sha256 = sha256_hex(&map);
    let gates = write(&dir, "gates.json", &gates_json(true, "gates_passed", &sha256));
    let scientific = write(&dir, "pending.json", pending_decision_json());
    let redistribution = write(&dir, "redistribution.json", &approved_decision_json(&sha256));

    let output = promote(&gates, &scientific, &redistribution, &map);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fail-closed"));
    assert!(stderr.contains("scientific"));
}

#[test]
fn gates_not_passed_fails_closed_via_cli() {
    let dir = TempDir::new().unwrap();
    let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
    let sha256 = sha256_hex(&map);
    let gates = write(
        &dir,
        "gates.json",
        &gates_json(false, "awaiting_regeneration", &sha256),
    );
    let scientific = write(&dir, "scientific.json", &approved_decision_json(&sha256));
    let redistribution = write(&dir, "redistribution.json", &approved_decision_json(&sha256));

    let output = promote(&gates, &scientific, &redistribution, &map);
    assert!(!output.status.success());
    assert!(map.exists(), "map itself must never be deleted or moved");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fail-closed"));
}

#[test]
fn missing_decision_file_fails_closed_via_cli() {
    let dir = TempDir::new().unwrap();
    let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
    let sha256 = sha256_hex(&map);
    let gates = write(&dir, "gates.json", &gates_json(true, "gates_passed", &sha256));
    let redistribution = write(&dir, "redistribution.json", &approved_decision_json(&sha256));
    let missing_scientific = dir.path().join("does-not-exist.json");

    let output = promote(&gates, &missing_scientific, &redistribution, &map);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fail-closed"));
}

#[test]
fn promote_never_mutates_the_map() {
    let dir = TempDir::new().unwrap();
    let map = write(&dir, "map.csv", "pixel,flux\n0,1.0\n");
    let before = fs::read(&map).unwrap();
    let sha256 = sha256_hex(&map);
    let gates = write(&dir, "gates.json", &gates_json(true, "gates_passed", &sha256));
    let scientific = write(&dir, "scientific.json", &approved_decision_json(&sha256));
    let redistribution = write(&dir, "redistribution.json", &approved_decision_json(&sha256));

    let output = promote(&gates, &scientific, &redistribution, &map);
    assert!(output.status.success());
    assert_eq!(before, fs::read(&map).unwrap());
}
