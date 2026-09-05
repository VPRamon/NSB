use assert_cmd::Command;
use predicates::prelude::*;

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
    let expected_component_count = if nsb::Starlight::bundled_production_available() {
        4
    } else {
        3
    };
    assert_eq!(
        value["component_metadata"].as_array().unwrap().len(),
        expected_component_count
    );
}

#[test]
fn window_zero_max_reports_no_matching_periods() {
    let assertion = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "table",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "0",
            "--components",
            "zodiacal,airglow",
            "--no-pre-filter",
        ])
        .assert()
        .success();
    assertion.stdout(predicate::str::contains("(no matching periods)"));
}

#[test]
fn window_json_empty_periods_keep_schema() {
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "0",
            "--components",
            "zodiacal,airglow",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["schema_version"], "nsb-cli-window-json-v1");
    assert_eq!(value["periods"].as_array().unwrap().len(), 0);
}

#[test]
fn window_min_nsb_band_excludes_periods_below_minimum() {
    let max_only = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "1000000",
            "--components",
            "zodiacal,airglow",
            "--step",
            "3600",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let max_value: serde_json::Value = serde_json::from_slice(&max_only).unwrap();
    let max_periods = max_value["periods"].as_array().unwrap();
    assert_eq!(
        max_periods.len(),
        1,
        "large max-nsb without min should keep the full window"
    );

    let band = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--min-nsb",
            "1000000",
            "--max-nsb",
            "1000000",
            "--components",
            "zodiacal,airglow",
            "--step",
            "3600",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let band_value: serde_json::Value = serde_json::from_slice(&band).unwrap();
    assert_eq!(band_value["min_nsb_ph_cm2_ns_sr"], 1_000_000.0);
    assert_eq!(band_value["max_nsb_ph_cm2_ns_sr"], 1_000_000.0);
    assert!(
        band_value["periods"].as_array().unwrap().is_empty(),
        "equal min/max band must exclude every below-max period via subtract_periods"
    );
}

#[test]
fn window_rejects_non_finite_max_nsb() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "nan",
            "--no-pre-filter",
        ])
        .assert()
        .failure();
}
