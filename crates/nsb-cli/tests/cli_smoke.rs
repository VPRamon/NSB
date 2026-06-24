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
fn default_components_include_moonlight() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "CTAO-S",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["schema_version"], "nsb-cli-point-json-v1");
    assert_eq!(value["version"]["model_version"], "nsb-model-2026.1");
    assert_eq!(
        value["version"]["siderust_revision"],
        "8d94b8375ae23c26d00346f74951e52cd1b595cc"
    );
    assert_eq!(value["model"]["preset"], "ctao-south-planning");
    assert!(value["version"]["data_assets"].as_array().unwrap().len() >= 5);
    let components = value["components"].as_array().unwrap();
    assert!(components
        .iter()
        .any(|component| component["name"] == "zodiacal"));
    assert!(components
        .iter()
        .any(|component| component["name"] == "airglow"));
    assert!(components
        .iter()
        .any(|component| component["name"] == "moon"));
    assert!(!components
        .iter()
        .any(|component| component["name"] == "starlight"));
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
    .stderr(predicate::str::contains(
        "--min-nsb must be less than or equal to --max-nsb",
    ));
}

#[test]
fn starlight_requires_explicit_experimental_name() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--site",
        "CTAO-S",
        "--ra",
        "83.0",
        "--dec",
        "22.0",
        "--components",
        "starlight",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("unknown component \"starlight\""));
}

#[test]
fn explicit_experimental_starlight_is_labelled_in_json() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "CTAO-S",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "experimental-starlight",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["components"][0]["name"], "starlight");
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "experimental"
    );
}

#[test]
fn point_csv_v1_header_is_stable() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
        "--format",
        "csv",
        "point",
        "--time",
        "2023-09-04T01:48:00Z",
        "--site",
        "CTAO-S",
        "--ra",
        "266.41683",
        "--dec",
        "-29.00781",
    ])
    .assert()
    .success()
    .stdout(predicate::str::starts_with(
        "schema_version,record_type,component,integrated_ph_cm2_ns_sr,b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,provenance,validated_domain,band_convention,nsb_version,model_version,siderust_revision,model_preset,asset_checksums",
    ));
}

#[test]
fn window_json_v1_contains_audit_metadata() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "json",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "CTAO-S",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "1000000",
            "--step",
            "3600",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["schema_version"], "nsb-cli-window-json-v1");
    assert_eq!(value["model"]["preset"], "ctao-south-planning");
    assert_eq!(value["component_metadata"].as_array().unwrap().len(), 3);
}

#[test]
fn config_init_contract_is_stable() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("site = \"CTAO-S\""))
        .stdout(predicate::str::contains("starlight = false"))
        .stdout(predicate::str::contains("sample_step_seconds = 600.0"));
}
