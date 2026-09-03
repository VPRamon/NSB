use assert_cmd::Command;
use nsb::{
    AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
use predicates::prelude::*;
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use std::fs;

const AIRGLOW_CSV_PROVENANCE_COLUMNS: [&str; 17] = [
    "airglow_geometry_model",
    "airglow_geometry_version",
    "airglow_geometry_emission_height_km",
    "airglow_profile_id",
    "airglow_profile_schema_version",
    "airglow_profile_checksum_sha256",
    "airglow_profile_normalization",
    "airglow_profile_altitude_min_km",
    "airglow_profile_altitude_max_km",
    "airglow_profile_wavelength_min_nm",
    "airglow_profile_wavelength_max_nm",
    "airglow_profile_wavelength_band",
    "airglow_geometry_assumptions",
    "airglow_profile_validated_zenith_min_deg",
    "airglow_profile_validated_zenith_max_deg",
    "airglow_geometry_provenance",
    "airglow_profile_license",
];

fn synthetic_vertical_profile(id: &str, peak_emissivity: f64) -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: id.into(),
        altitude_km: vec![
            Kilometers::new(80.0),
            Kilometers::new(90.0),
            Kilometers::new(100.0),
        ],
        relative_emissivity: vec![0.0, peak_emissivity, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic NSB optical validation band".into(),
        },
        assumptions: "Synthetic triangular layer for CLI transport validation only".into(),
        provenance: "Generated in crates/nsb-cli/tests/cli_smoke.rs".into(),
        license: "Synthetic test data; AGPL-3.0-only repository fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .unwrap()
}

fn csv_value(headers: &csv::StringRecord, row: &csv::StringRecord, column: &str) -> String {
    let index = headers
        .iter()
        .position(|header| header == column)
        .unwrap_or_else(|| panic!("missing CSV column {column}"));
    row.get(index).unwrap().to_string()
}

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
    assert_eq!(value["version"]["siderust_version"], "0.11.0");
    assert_eq!(
        value["version"]["siderust_source"],
        "crates.io:siderust:0.11.0"
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
fn checksum_pinned_vertical_profile_is_selected_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let profile_path = dir.path().join("airglow-profile.toml");
    let profile = VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: "cli-synthetic-profile-v1".into(),
        altitude_km: vec![
            Kilometers::new(80.0),
            Kilometers::new(90.0),
            Kilometers::new(100.0),
        ],
        relative_emissivity: vec![0.0, 1.0, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic NSB optical validation band".into(),
        },
        assumptions: "Synthetic triangular layer for CLI transport validation only".into(),
        provenance: "Generated in crates/nsb-cli/tests/cli_smoke.rs".into(),
        license: "Synthetic test data; AGPL-3.0-only repository fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .unwrap();
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
            "CTAO-S",
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
            "CTAO-S",
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
            "CTAO-S",
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
fn starlight_uses_bundled_production_or_reports_missing_asset() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let assertion = cmd
        .args([
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
            "CTAO-S",
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

fn write_validated_fixture(map_path: &std::path::Path, manifest_path: &std::path::Path) {
    write_validated_fixture_schema(map_path, manifest_path, false);
}

fn write_validated_uncertainty_fixture(
    map_path: &std::path::Path,
    manifest_path: &std::path::Path,
) {
    write_validated_fixture_schema(map_path, manifest_path, true);
}

fn write_validated_fixture_schema(
    map_path: &std::path::Path,
    manifest_path: &std::path::Path,
    with_uncertainty: bool,
) {
    let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
    let mut map = String::from(concat!(
        "# map_type=healpix\n",
        "# coordinate_frame=galactic\n",
        "# nside=8\n",
        "# ordering=ring\n",
        "# dataset_name=CLI validated fixture\n",
        "# version=fixture-v1\n",
        "# generation_date_utc=2026-06-24T00:00:00Z\n",
        "# source_catalogue=synthetic fixture catalogue\n",
        "# source_catalogue_release=fixture-release\n",
        "# source_catalogue_license=CC0-1.0\n",
        "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
        "# source_selection=complete synthetic plane-enhanced fixture\n",
        "# magnitude_limit=not applicable\n",
        "# map_resolution=HEALPix nside=8 ordering=ring\n",
        "# calibration_status=production\n",
        "# photometry_model=synthetic-passband-integrated-v1\n",
        "# band_definition=synthetic integrated 300-650 nm test band\n",
        "# smoothing=none\n",
        "# generated_by=CLI integration test\n",
        "# generation_command=synthetic fixture builder\n",
        "# validation_report=test admission report\n",
        "# independent_comparison=synthetic trusted reference fixture\n",
        "# s10_diagnostics=not_provided\n",
    ));
    map.push_str(concat!(
        "healpix_index,integrated_ph_cm2_ns_sr,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    let mut source_flux = 0.0;
    for index in 0..grid.npix() {
        let latitude = grid
            .pixel_center_spherical::<Galactic>(HealpixIndex::new(index))
            .unwrap()
            .b()
            .abs();
        let value = if latitude <= Degrees::new(10.0) {
            2.0
        } else {
            1.0
        };
        source_flux += value * grid.pixel_area_sr();
        if with_uncertainty {
            map.push_str(&format!(
                "{index},{value},{},{},{}\n",
                value * 0.1,
                value * 0.2,
                value * 0.25,
            ));
        } else {
            // Packed schema always carries an uncertainty triplet; zeros mark
            // “no published uncertainty” only when all three are zero together.
            map.push_str(&format!("{index},{value},0.0,0.0,0.0\n"));
        }
    }
    let checksum = format!("sha256:{}", to_hex(&sha256(map.as_bytes())));
    fs::write(map_path, &map).unwrap();
    let manifest = format!(
        r#"schema_version = 1
calibration_status = "production"
dataset_name = "CLI validated fixture"
version = "fixture-v1"
generation_date = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable"
map_resolution = "HEALPix nside=8 ordering=ring"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "CLI integration test"
generation_command = "synthetic fixture builder"
map_sha256 = "{checksum}"
validation_report = "test admission report"
independent_comparison = "synthetic trusted reference fixture"
flux_conservation_validated = true
input_integrated_flux_sum = {source_flux:.17}
integrated_flux_conservation_tolerance = 0.000000001

[header]
map_type = "healpix"
coordinate_frame = "galactic"
nside = "8"
ordering = "ring"
dataset_name = "CLI validated fixture"
version = "fixture-v1"
generation_date_utc = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable"
map_resolution = "HEALPix nside=8 ordering=ring"
calibration_status = "production"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "CLI integration test"
generation_command = "synthetic fixture builder"
validation_report = "test admission report"
independent_comparison = "synthetic trusted reference fixture"
"#,
    );
    fs::write(manifest_path, manifest).unwrap();
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
fn starlight_csv_uses_validated_component_label() {
    let dir = tempfile::tempdir().unwrap();
    let map_path = dir.path().join("starlight.csv");
    let manifest_path = dir.path().join("starlight.toml");
    write_validated_fixture(&map_path, &manifest_path);

    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args([
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
    .stdout(predicate::str::contains("validated-starlight"));
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
        "CTAO-S",
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
fn config_init_contract_is_stable() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("site = \"CTAO-S\""))
        .stdout(predicate::str::contains("starlight = false"))
        .stdout(predicate::str::contains("sample_step_seconds = 600.0"));
}

#[test]
fn config_validate_accepts_init_template() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nsb.toml");
    let template = Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "init"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    std::fs::write(&path, template).unwrap();

    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok:"));
}

#[test]
fn config_validate_rejects_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is not = toml [").unwrap();

    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config file"));
}

#[test]
fn config_validate_rejects_missing_file() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", "/no/such/nsb-config.toml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read config file"));
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
