//! Compatibility binary for the scientific-validation workflow pin.
//!
//! Broader starlight CLI contracts live in `starlight_contract.rs`. Keep this
//! file focused on the exact test name invoked by
//! `.github/workflows/scientific-validation.yml`.

use assert_cmd::Command;

mod common;
use common::{csv_value, write_validated_fixture};

#[test]
fn validated_external_starlight_is_production_labelled_in_json() {
    let dir = tempfile::tempdir().unwrap();
    let map_path = dir.path().join("starlight.csv");
    let manifest_path = dir.path().join("starlight.toml");
    write_validated_fixture(&map_path, &manifest_path);

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
    assert_eq!(value["components"][0]["name"], "validated-starlight");
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "production"
    );
    let provenance = value["components"][0]["metadata"]["provenance"]
        .as_str()
        .unwrap();
    assert!(provenance.contains(
        "source checksum sha256:1111111111111111111111111111111111111111111111111111111111111111"
    ));
    assert!(provenance.contains("validation report test admission report"));

    let mut csv_cmd = Command::cargo_bin("nsb").unwrap();
    let csv_output = csv_cmd
        .args([
            "--format",
            "csv",
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
    let mut reader = csv::Reader::from_reader(csv_output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let row = reader.records().next().unwrap().unwrap();
    assert_eq!(
        csv_value(&headers, &row, "component"),
        "validated-starlight"
    );
}
