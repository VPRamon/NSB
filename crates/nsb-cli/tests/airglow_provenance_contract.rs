use assert_cmd::Command;
use nsb::VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION;
use std::fs;

mod common;
use common::{csv_value, synthetic_vertical_profile, AIRGLOW_CSV_PROVENANCE_COLUMNS};

#[test]
fn checksum_pinned_vertical_profile_is_selected_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let profile_path = dir.path().join("airglow-profile.toml");
    let profile = synthetic_vertical_profile("cli-synthetic-profile-v1", 1.0);
    fs::write(&profile_path, profile.to_toml_string().unwrap()).unwrap();

    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--lon",
            "12.3",
            "--lat",
            "-31.2",
            "--height",
            "1234",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "airglow",
            "--airglow-vertical-profile",
            profile_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["model"]["airglow_geometry"], "vertical_profile");
    let geometry = &value["components"][0]["metadata"]["airglow_geometry"];
    assert_eq!(geometry["model"], "vertical_profile");
    assert_eq!(geometry["profile_id"], "cli-synthetic-profile-v1");
    assert_eq!(geometry["checksum_sha256"], profile.checksum_sha256());
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "generic-clear-sky"
    );
}

#[test]
fn default_van_rhijn_csv_reports_geometry_identity() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "csv",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--lon",
            "12.3",
            "--lat",
            "-31.2",
            "--height",
            "1234",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "airglow",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let row = reader.records().next().unwrap().unwrap();
    assert_eq!(row.get(0), Some("nsb-cli-point-csv-v3"));
    assert_eq!(
        csv_value(&headers, &row, "airglow_geometry_model"),
        "van_rhijn"
    );
    assert!(!csv_value(&headers, &row, "airglow_geometry_version").is_empty());
    assert_eq!(
        csv_value(&headers, &row, "airglow_geometry_emission_height_km"),
        "90"
    );
}

#[test]
fn vertical_profile_csv_reports_exact_checksum_and_distinguishes_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let mut identities = Vec::new();
    for (filename, id, peak) in [
        ("profile-a.toml", "csv-profile-a", 1.0),
        ("profile-b.toml", "csv-profile-b", 0.75),
    ] {
        let profile = synthetic_vertical_profile(id, peak);
        let path = dir.path().join(filename);
        fs::write(&path, profile.to_toml_string().unwrap()).unwrap();
        let mut cmd = Command::cargo_bin("nsb").unwrap();
        let output = cmd
            .args([
                "--format",
                "csv",
                "point",
                "--time",
                "2026-06-18T23:00:00Z",
                "--lon",
                "12.3",
                "--lat",
                "-31.2",
                "--height",
                "1234",
                "--ra",
                "83.0",
                "--dec",
                "22.0",
                "--components",
                "airglow",
                "--airglow-vertical-profile",
                path.to_str().unwrap(),
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let mut reader = csv::Reader::from_reader(output.as_slice());
        let headers = reader.headers().unwrap().clone();
        let row = reader.records().next().unwrap().unwrap();
        assert_eq!(
            csv_value(&headers, &row, "airglow_geometry_model"),
            "vertical_profile"
        );
        assert_eq!(csv_value(&headers, &row, "airglow_profile_id"), id);
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_schema_version"),
            VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION.to_string()
        );
        let csv_checksum = csv_value(&headers, &row, "airglow_profile_checksum_sha256");
        assert_eq!(csv_checksum, profile.checksum_sha256());
        assert_eq!(
            csv_value(&headers, &row, "airglow_geometry_provenance"),
            profile.provenance()
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_normalization"),
            profile.normalization().as_str()
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_altitude_min_km"),
            "80"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_altitude_max_km"),
            "100"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_wavelength_min_nm"),
            "300"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_wavelength_max_nm"),
            "650"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_wavelength_band"),
            profile.wavelength_applicability().band
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_validated_zenith_min_deg"),
            "0"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_validated_zenith_max_deg"),
            "90"
        );
        assert_eq!(
            csv_value(&headers, &row, "airglow_profile_license"),
            profile.license()
        );
        identities.push((id.to_string(), csv_checksum));
    }
    assert_ne!(identities[0], identities[1]);
}

#[test]
fn non_airglow_csv_rows_do_not_inherit_geometry_metadata() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "csv",
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
            "zodiacal",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let row = reader.records().next().unwrap().unwrap();
    for column in [
        "airglow_geometry_model",
        "airglow_geometry_version",
        "airglow_profile_id",
        "airglow_profile_checksum_sha256",
        "airglow_geometry_provenance",
    ] {
        assert_eq!(csv_value(&headers, &row, column), "");
    }
}

#[test]
fn window_csv_preserves_selected_vertical_profile_identity() {
    let dir = tempfile::tempdir().unwrap();
    let profile = synthetic_vertical_profile("window-csv-profile", 1.0);
    let path = dir.path().join("window-profile.toml");
    fs::write(&path, profile.to_toml_string().unwrap()).unwrap();
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "csv",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--site-profile",
            "cta-south",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "1000000",
            "--step",
            "3600",
            "--no-pre-filter",
            "--components",
            "airglow",
            "--airglow-vertical-profile",
            path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert!(rows.len() >= 2);
    assert_eq!(rows[0].get(0), Some("nsb-cli-window-csv-v3"));
    assert_eq!(
        csv_value(&headers, &rows[0], "record_type"),
        "query_summary"
    );
    assert!(rows
        .iter()
        .skip(1)
        .all(|row| csv_value(&headers, row, "record_type") == "period"));
    for row in &rows {
        assert_eq!(row.get(0), Some("nsb-cli-window-csv-v3"));
        assert_eq!(
            csv_value(&headers, row, "airglow_geometry_model"),
            "vertical_profile"
        );
        assert_eq!(
            csv_value(&headers, row, "airglow_profile_id"),
            profile.profile_id()
        );
        assert_eq!(
            csv_value(&headers, row, "airglow_profile_checksum_sha256"),
            profile.checksum_sha256()
        );
    }
}

#[test]
fn empty_window_csv_preserves_selected_vertical_profile_identity() {
    let dir = tempfile::tempdir().unwrap();
    let profile = synthetic_vertical_profile("empty-window-csv-profile", 1.0);
    let path = dir.path().join("empty-window-profile.toml");
    fs::write(&path, profile.to_toml_string().unwrap()).unwrap();
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "csv",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--site-profile",
            "cta-south",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "0",
            "--step",
            "3600",
            "--no-pre-filter",
            "--components",
            "airglow",
            "--airglow-vertical-profile",
            path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "an empty result must contain only its summary"
    );
    let row = &rows[0];
    assert_eq!(row.get(0), Some("nsb-cli-window-csv-v3"));
    assert_eq!(csv_value(&headers, row, "record_type"), "query_summary");
    assert_eq!(
        csv_value(&headers, row, "airglow_geometry_model"),
        "vertical_profile"
    );
    assert_eq!(
        csv_value(&headers, row, "airglow_profile_id"),
        profile.profile_id()
    );
    assert_eq!(
        csv_value(&headers, row, "airglow_profile_checksum_sha256"),
        profile.checksum_sha256()
    );
}

#[test]
fn non_airglow_empty_window_csv_does_not_expose_airglow_metadata() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args([
            "--format",
            "csv",
            "window",
            "--start",
            "2023-09-04T01:00:00Z",
            "--end",
            "2023-09-04T02:00:00Z",
            "--site",
            "PARANAL",
            "--site-profile",
            "cta-south",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--max-nsb",
            "0",
            "--step",
            "3600",
            "--no-pre-filter",
            "--components",
            "zodiacal",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mut reader = csv::Reader::from_reader(output.as_slice());
    let headers = reader.headers().unwrap().clone();
    let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "an empty result must contain only its summary"
    );
    let row = &rows[0];
    assert_eq!(csv_value(&headers, row, "record_type"), "query_summary");
    for column in AIRGLOW_CSV_PROVENANCE_COLUMNS {
        assert_eq!(csv_value(&headers, row, column), "");
    }
}
