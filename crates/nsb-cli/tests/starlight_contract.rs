use assert_cmd::Command;
use predicates::prelude::*;

mod common;
use common::{csv_value, write_validated_fixture, write_validated_uncertainty_fixture};

#[test]
fn starlight_uses_bundled_production_or_reports_missing_asset() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let assertion = cmd
        .args([
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "PARANAL",
            "--site-profile",
            "cta-south",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "starlight",
        ])
        .assert();
    if nsb::Starlight::bundled_production_available() {
        assertion.success();
    } else {
        assertion.failure().stderr(predicate::str::contains(
            "bundled production starlight asset is not registered",
        ));
    }
}

#[test]
fn validated_external_starlight_is_labelled_in_window_json() {
    let dir = tempfile::tempdir().unwrap();
    let map_path = dir.path().join("starlight.csv");
    let manifest_path = dir.path().join("starlight.toml");
    write_validated_fixture(&map_path, &manifest_path);

    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "json",
            "window",
            "--start",
            "2026-06-18T23:00:00Z",
            "--end",
            "2026-06-19T00:00:00Z",
            "--site",
            "PARANAL",
            "--site-profile",
            "cta-south",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--max-nsb",
            "1000000",
            "--step",
            "3600",
            "--no-pre-filter",
            "--components",
            "starlight",
            "--starlight-map",
            map_path.to_str().unwrap(),
            "--starlight-manifest",
            manifest_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["model"]["starlight_model"], "validated-starlight");
    assert_eq!(value["selected_components"][0], "validated-starlight");
    assert_eq!(
        value["component_metadata"][0]["name"],
        "validated-starlight"
    );
    assert_eq!(
        value["component_metadata"][0]["metadata"]["calibration_status"],
        "production"
    );
}

#[test]
fn starlight_uncertainties_are_serialized_in_json_and_csv_v4() {
    let dir = tempfile::tempdir().unwrap();
    let map_path = dir.path().join("starlight-v2.csv");
    let manifest_path = dir.path().join("starlight-v2.toml");
    write_validated_uncertainty_fixture(&map_path, &manifest_path);
    let common_args = [
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--site",
        "PARANAL",
        "--site-profile",
        "cta-south",
        "--ra",
        "83.0",
        "--dec",
        "22.0",
        "--components",
        "starlight",
        "--starlight-map",
        map_path.to_str().unwrap(),
        "--starlight-manifest",
        manifest_path.to_str().unwrap(),
    ];

    let mut json_cmd = Command::cargo_bin("nsb").unwrap();
    let json_output = json_cmd
        .arg("--format")
        .arg("json")
        .args(common_args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&json_output).unwrap();
    let component = &value["components"][0];
    let statistical = component["statistical_uncertainty_ph_cm2_ns_sr"]
        .as_f64()
        .unwrap();
    let systematic = component["systematic_uncertainty_ph_cm2_ns_sr"]
        .as_f64()
        .unwrap();
    let total = component["total_uncertainty_ph_cm2_ns_sr"]
        .as_f64()
        .unwrap();
    assert!(statistical >= 0.0);
    assert!(systematic >= 0.0);
    assert!(total >= statistical && total >= systematic);
    assert_eq!(component["relative_uncertainty"].as_f64(), Some(0.25));

    let mut csv_cmd = Command::cargo_bin("nsb").unwrap();
    let csv_output = csv_cmd
        .arg("--format")
        .arg("csv")
        .args(common_args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(csv_output.as_slice());
    let headers = reader.headers().unwrap().clone();
    assert!(headers
        .iter()
        .any(|header| header == "statistical_uncertainty_ph_cm2_ns_sr"));
    assert!(headers
        .iter()
        .any(|header| header == "systematic_uncertainty_ph_cm2_ns_sr"));
    assert!(headers
        .iter()
        .any(|header| header == "total_uncertainty_ph_cm2_ns_sr"));
    let row = reader.records().next().unwrap().unwrap();
    assert_eq!(row.get(0), Some("nsb-cli-point-csv-v4"));
    assert_eq!(row.get(8).unwrap().parse::<f64>().unwrap(), 0.25);
    assert_eq!(
        csv_value(&headers, &row, "statistical_uncertainty_ph_cm2_ns_sr")
            .parse::<f64>()
            .unwrap(),
        statistical
    );
    assert_eq!(
        csv_value(&headers, &row, "systematic_uncertainty_ph_cm2_ns_sr")
            .parse::<f64>()
            .unwrap(),
        systematic
    );
    assert_eq!(
        csv_value(&headers, &row, "total_uncertainty_ph_cm2_ns_sr")
            .parse::<f64>()
            .unwrap(),
        total
    );
}

#[test]
fn unknown_experimental_starlight_alias_is_rejected() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
        "--format",
        "json",
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--site",
        "PARANAL",
        "--site-profile",
        "cta-south",
        "--ra",
        "83.0",
        "--dec",
        "22.0",
        "--components",
        "experimental-starlight",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("experimental-starlight"));
}
