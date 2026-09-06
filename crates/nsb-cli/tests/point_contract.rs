use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn default_point_json_reports_schema_versions_and_components() {
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
    assert_eq!(value["version"]["siderust_version"], "0.11.1");
    assert_eq!(
        value["version"]["siderust_source"],
        "crates.io:siderust:0.11.1"
    );
    assert_eq!(value["model"]["preset"], "ctao-south-planning");
    assert_eq!(value["model"]["airglow_geometry"], "van_rhijn");
    assert!(value["version"]["data_assets"].as_array().unwrap().len() >= 4);
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
    assert_eq!(
        components
            .iter()
            .any(|component| component["name"] == "starlight"),
        nsb::Starlight::bundled_production_available()
    );
    let airglow = components
        .iter()
        .find(|component| component["name"] == "airglow")
        .unwrap();
    assert_eq!(
        airglow["metadata"]["airglow_geometry"]["model"],
        "van_rhijn"
    );
    assert_eq!(
        airglow["metadata"]["airglow_geometry"]["emission_height_km"],
        90.0
    );
}

#[test]
fn point_csv_v3_header_is_stable() {
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
        "schema_version,record_type,component,integrated_ph_cm2_ns_sr,b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,provenance,validated_domain,band_convention,nsb_version,model_version,siderust_source,model_preset,asset_checksums,airglow_geometry_model,airglow_geometry_version,airglow_geometry_emission_height_km,airglow_profile_id,airglow_profile_schema_version,airglow_profile_checksum_sha256",
    ));
}

#[test]
fn point_ks1991_moonlight_model_is_labelled_in_json() {
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2023-09-04T01:48:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--components",
            "moon",
            "--moonlight-model",
            "ks1991",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["model"]["moonlight_model"],
        "krisciunas-schaefer-1991"
    );
    let moon = value["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "moon")
        .expect("moon component");
    assert!(moon["metadata"]["provenance"]
        .as_str()
        .unwrap()
        .contains("Krisciunas & Schaefer 1991"));
}

#[test]
fn point_explicit_solar_radio_flux_is_accepted_and_labelled() {
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2023-09-04T01:48:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--components",
            "airglow",
            "--solar-radio-flux-sfu",
            "250",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["model"]["solar_radio_flux_sfu"], 250.0);
    let airglow = value["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "airglow")
        .expect("airglow component");
    assert_eq!(
        airglow["metadata"]["solar_activity"]["resolution_step"],
        "explicit-override"
    );
}

#[test]
fn point_rejects_non_positive_solar_radio_flux() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "point",
            "--time",
            "2023-09-04T01:48:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--components",
            "airglow",
            "--solar-radio-flux-sfu",
            "0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--solar-radio-flux-sfu must be finite and positive",
        ));
}
