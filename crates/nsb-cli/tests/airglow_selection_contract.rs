// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use assert_cmd::Command;

#[test]
fn point_json_exposes_automatic_airglow_fallback_structurally() {
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
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["kind"], "automatic");
    assert_eq!(selection["resolved_model"], "paranal-palace-oxygen-v1");
    assert_eq!(selection["used_fallback"], true);
    assert_eq!(
        selection["fallback_reason"],
        "global-planning-model-unavailable"
    );
    assert_eq!(selection["physical_outcome"], "evaluated");
    assert_eq!(
        value["components"][0]["metadata"]["airglow_selection"]["kind"],
        "automatic"
    );
}

#[test]
fn point_json_exposes_explicit_paranal_without_fallback_flag() {
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
            "--airglow-model",
            "paranal-palace-v1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["kind"], "explicit");
    assert_eq!(selection["requested_model"], "paranal-palace-v1");
    assert_eq!(selection["resolved_model"], "paranal-palace-v1");
    assert_eq!(selection["used_fallback"], false);
    assert!(selection["fallback_reason"].is_null());
    assert_eq!(selection["physical_outcome"], "evaluated");
}

#[test]
fn point_without_airglow_uses_config_only_selection_metadata() {
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
            "--airglow-model",
            "paranal-palace-v1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["kind"], "explicit");
    assert_eq!(selection["requested_model"], "paranal-palace-v1");
    assert_eq!(selection["resolved_model"], "paranal-palace-v1");
    assert!(selection["physical_outcome"].is_null());
}

#[test]
fn window_without_airglow_reports_not_selected_selection() {
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
            "1000000",
            "--no-pre-filter",
            "--components",
            "moon",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["model"]["airglow_selection"]["kind"], "not-selected");
    assert!(value["model"]["airglow_selection"]["physical_outcome"].is_null());
}

#[test]
fn starlight_map_without_manifest_is_rejected() {
    Command::cargo_bin("nsb")
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
            "starlight",
            "--starlight-map",
            "missing.csv",
        ])
        .assert()
        .failure();
}

#[test]
fn point_json_exposes_daytime_physical_zero() {
    // Use a target inside Oxygen's ZD <= 60 domain even during daylight.
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2023-09-04T16:00:00Z",
            "--site",
            "PARANAL",
            "--ra",
            "150",
            "--dec",
            "-24.6275",
            "--components",
            "airglow",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["physical_outcome"], "physical-zero");
    assert_eq!(
        selection["physical_zero_reason"],
        "outside-astronomical-night"
    );
    assert_eq!(value["components"][0]["integrated_ph_cm2_ns_sr"], 0.0);
}

#[test]
fn explicit_oxygen_selection_remains_experimental() {
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
            "--airglow-model",
            "paranal-palace-oxygen-v1",
            "--solar-radio-flux-sfu",
            "100",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["model"]["airglow_selection"]["resolved_model"],
        "paranal-palace-oxygen-v1"
    );
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "experimental"
    );
}

#[test]
fn registered_calibration_cli_matches_the_rust_fixture() {
    let artifact = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nsb/examples/airglow-calibration/calibration.toml");
    let mut command = Command::cargo_bin("nsb").unwrap();
    command
        .args([
            "--format",
            "json",
            "point",
            "--time",
            "2023-09-04T01:48:00Z",
            "--lon",
            "-70.4044",
            "--lat",
            "-24.6275",
            "--height",
            "2635",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--components",
            "airglow",
            "--solar-radio-flux-sfu",
            "100",
            "--airglow-calibration",
        ])
        .arg(artifact);
    let output = command.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["components"][0]["metadata"]["calibration_status"],
        "experimental"
    );
    use siderust::coordinates::{
        centers::Geodetic, spherical::direction, transform::TransformFrame,
    };
    use siderust::qtty::{Degrees, Meters};
    let raw = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nsb/examples/airglow-calibration/calibration.toml"),
    )
    .unwrap();
    let profile = nsb::site::AirglowCalibrationProfile::from_toml_str(&raw).unwrap();
    let config = nsb::NsbModelConfig::generic_clear_sky()
        .with_solar_radio_flux(nsb::SolarFluxUnits::new(100.))
        .with_airglow_calibration(profile)
        .unwrap();
    let location = Geodetic::new_raw(
        Degrees::new(-70.4044),
        Degrees::new(-24.6275),
        Meters::new(2635.),
    );
    // CLI angles are ICRS, while registered training angles are J2000. Apply
    // the same frame bias as CLI parsing before comparing the Rust result.
    let target = direction::ICRS::new(Degrees::new(266.41683), Degrees::new(-29.00781)).to_frame();
    let time = tempoch::Time::<tempoch::UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let expected = nsb::NsbEvaluator::with_config(config)
        .unwrap()
        .evaluate(
            &nsb::PointQuery::new(location, time, target)
                .with_components(nsb::ComponentMask::AIRGLOW),
        )
        .unwrap()
        .integrated
        .value();
    let actual = value["components"][0]["integrated_ph_cm2_ns_sr"]
        .as_f64()
        .unwrap();
    assert!((actual - expected).abs() < 1e-14);
}

#[test]
fn automatic_and_explicit_oxygen_share_values_and_domain_errors() {
    let run = |ra: &str, model: &str| {
        Command::cargo_bin("nsb")
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
                ra,
                "--dec",
                "-24.6275",
                "--components",
                "airglow",
                "--airglow-model",
                model,
                "--solar-radio-flux-sfu",
                "100",
            ])
            .output()
            .unwrap()
    };
    let automatic = run("0", "automatic");
    let explicit = run("0", "paranal-palace-oxygen-v1");
    assert!(automatic.status.success());
    assert!(explicit.status.success());
    let auto: serde_json::Value = serde_json::from_slice(&automatic.stdout).unwrap();
    let oxygen: serde_json::Value = serde_json::from_slice(&explicit.stdout).unwrap();
    assert_eq!(
        auto["components"][0]["integrated_ph_cm2_ns_sr"],
        oxygen["components"][0]["integrated_ph_cm2_ns_sr"]
    );
    for model in ["automatic", "paranal-palace-oxygen-v1"] {
        let outside = run("90", model);
        assert!(!outside.status.success());
        assert!(String::from_utf8_lossy(&outside.stderr).contains("zenith distance <= 60"));
    }
    assert!(run("90", "paranal-palace-v1").status.success());
}
