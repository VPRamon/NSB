// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
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

fn run_airglow(
    site_args: &[&str],
    profile: Option<&str>,
    // Near-zenith ICRS Dec matching the site latitude so Automatic Oxygen (ZD≤60°) evaluates.
    dec_deg: &str,
) -> serde_json::Value {
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
        "0.0",
        "--dec",
        dec_deg,
        "--components",
        "airglow",
        "--solar-radio-flux-sfu",
        "100",
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

fn assert_automatic_oxygen_airglow(value: &serde_json::Value, preset: &str) {
    assert_eq!(value["model"]["preset"], preset);
    assert_eq!(
        value["model"]["airglow_selection"]["resolved_model"],
        "paranal-palace-oxygen-v1"
    );
    // Automatic Oxygen keeps Experimental scientific maturity (not a calibration upgrade).
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "experimental"
    );
    let provenance = value["components"][0]["metadata"]["provenance"]
        .as_str()
        .unwrap();
    assert!(provenance.contains("PALACE"));
    assert!(provenance.contains("site_calibrated false"));
}

#[test]
fn paranal_observatory_alone_uses_automatic_oxygen() {
    assert_automatic_oxygen_airglow(
        &run_airglow(&["--site", "PARANAL"], None, "-24.6275"),
        "generic-clear-sky",
    );
}

#[test]
fn custom_observatory_defaults_to_automatic_oxygen() {
    let directory = tempdir().unwrap();
    let catalog = directory.path().join("observatories.toml");
    fs::write(&catalog, CUSTOM_CATALOG).unwrap();

    assert_automatic_oxygen_airglow(
        &run_airglow(
            &[
                "--site",
                "Airglow Test Observatory",
                "--observatory-catalog",
                catalog.to_str().unwrap(),
            ],
            None,
            "41.9",
        ),
        "generic-clear-sky",
    );
}

#[test]
fn ctao_observatory_identity_does_not_select_planning_profile() {
    assert_automatic_oxygen_airglow(
        &run_airglow(&["--site", "CTAO-N"], None, "28.7619"),
        "generic-clear-sky",
    );
    assert_automatic_oxygen_airglow(
        &run_airglow(&["--site", "CTAO-S"], None, "-24.6834"),
        "generic-clear-sky",
    );
}

#[test]
fn explicit_ctao_profiles_remain_planning_site_with_experimental_oxygen() {
    for (site, profile, preset, dec) in [
        ("CTAO-N", "cta-north", "ctao-north-planning", "28.7619"),
        ("CTAO-S", "cta-south", "ctao-south-planning", "-24.6834"),
    ] {
        let value = run_airglow(&["--site", site], Some(profile), dec);
        assert_eq!(value["model"]["preset"], preset);
        // Site profile is planning; Automatic Oxygen component maturity stays Experimental.
        assert_eq!(
            value["components"][0]["metadata"]["calibration_status"],
            "experimental"
        );
        assert_eq!(
            value["model"]["airglow_selection"]["resolved_model"],
            "paranal-palace-oxygen-v1"
        );
        assert!(value["components"][0]["metadata"]["provenance"]
            .as_str()
            .unwrap()
            .contains("site_calibrated false"));
    }
}
