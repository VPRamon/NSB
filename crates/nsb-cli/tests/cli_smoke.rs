use assert_cmd::Command;
use predicates::prelude::*;
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use std::fs;

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
        let direction: Direction<Galactic> = grid.pixel_center(HealpixIndex::new(index)).unwrap();
        let latitude = direction.as_array()[2].asin().to_degrees().abs();
        let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
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
fn starlight_uncertainties_are_serialized_in_json_and_csv_v2() {
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
    assert_eq!(
        headers.get(18),
        Some("statistical_uncertainty_ph_cm2_ns_sr")
    );
    assert_eq!(headers.get(19), Some("systematic_uncertainty_ph_cm2_ns_sr"));
    assert_eq!(headers.get(20), Some("total_uncertainty_ph_cm2_ns_sr"));
    let row = reader.records().next().unwrap().unwrap();
    assert_eq!(row.get(0), Some("nsb-cli-point-csv-v2"));
    assert_eq!(row.get(8).unwrap().parse::<f64>().unwrap(), 0.25);
    assert_eq!(row.get(18).unwrap().parse::<f64>().unwrap(), statistical);
    assert_eq!(row.get(19).unwrap().parse::<f64>().unwrap(), systematic);
    assert_eq!(row.get(20).unwrap().parse::<f64>().unwrap(), total);
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
        "schema_version,record_type,component,integrated_ph_cm2_ns_sr,b_s10_diagnostic,v_s10_diagnostic,b_mag_arcsec2_diagnostic,v_mag_arcsec2_diagnostic,relative_uncertainty,calibration_status,provenance,validated_domain,band_convention,nsb_version,model_version,siderust_source,model_preset,asset_checksums",
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
