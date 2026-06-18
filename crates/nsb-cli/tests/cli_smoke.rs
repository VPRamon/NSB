use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn sites_list_prints_ctao_s() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["sites", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CTAO-S"));
}

#[test]
fn sites_show_json_is_valid() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args(["--format", "json", "sites", "show", "CTAO-S"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value[0]["canonical_alias"], "CTAO-S");
}

#[test]
fn invalid_site_errors() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--site",
        "NOPE",
        "--ra",
        "83.0",
        "--dec",
        "22.0",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("unknown site alias"));
}

#[test]
fn invalid_nsb_range_errors() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
        "window",
        "--start",
        "2026-06-18T20:00:00Z",
        "--end",
        "2026-06-19T06:00:00Z",
        "--site",
        "CTAO-S",
        "--ra",
        "83.0",
        "--dec",
        "22.0",
        "--min-nsb",
        "10.0",
        "--max-nsb",
        "1.0",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("--min-nsb must be less than or equal to --max-nsb"));
}
