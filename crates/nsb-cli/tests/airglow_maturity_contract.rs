use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

const CUSTOM_CATALOG: &str = r#"
[[observatory]]
name = "Airglow Test Observatory"
longitude_deg = 12.5
latitude_deg = 41.9
height_m = 800.0
reference_pressure_hpa = 920.0
"#;

fn run_airglow(site_args: &[&str], profile: Option<&str>) -> serde_json::Value {
    let mut args = vec![
        "--format",
        "json",
        "point",
        "--time",
        "2023-09-04T01:48:00Z",
    ];
    args.extend_from_slice(site_args);
    args.extend([
        "--ra",
        "266.41683",
        "--dec",
        "-29.00781",
        "--components",
        "airglow",
    ]);
    if let Some(profile) = profile {
        args.extend(["--site-profile", profile]);
    }

    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

fn assert_generic_airglow(value: &serde_json::Value) {
    assert_eq!(value["model"]["preset"], "generic-clear-sky");
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "generic-clear-sky"
    );
    let provenance = value["components"][0]["metadata"]["provenance"]
        .as_str()
        .unwrap();
    assert!(provenance.contains("Paranal-derived"));
    assert!(provenance.contains("site_calibrated false"));
}

#[test]
fn paranal_observatory_alone_remains_generic_airglow() {
    assert_generic_airglow(&run_airglow(&["--site", "PARANAL"], None));
}

#[test]
fn custom_observatory_defaults_to_generic_airglow() {
    let directory = tempdir().unwrap();
    let catalog = directory.path().join("observatories.toml");
    fs::write(&catalog, CUSTOM_CATALOG).unwrap();

    assert_generic_airglow(&run_airglow(
        &[
            "--site",
            "Airglow Test Observatory",
            "--observatory-catalog",
            catalog.to_str().unwrap(),
        ],
        None,
    ));
}

#[test]
fn ctao_observatory_identity_does_not_select_planning_profile() {
    assert_generic_airglow(&run_airglow(&["--site", "CTAO-N"], None));
    assert_generic_airglow(&run_airglow(&["--site", "CTAO-S"], None));
}

#[test]
fn explicit_ctao_profiles_are_planning_only() {
    for (site, profile, preset) in [
        ("CTAO-N", "cta-north", "ctao-north-planning"),
        ("CTAO-S", "cta-south", "ctao-south-planning"),
    ] {
        let value = run_airglow(&["--site", site], Some(profile));
        assert_eq!(value["model"]["preset"], preset);
        assert_eq!(
            value["components"][0]["metadata"]["calibration_status"],
            "planning-preset"
        );
        assert!(value["components"][0]["metadata"]["provenance"]
            .as_str()
            .unwrap()
            .contains("site_calibrated false"));
    }
}
