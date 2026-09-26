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
    assert_eq!(selection["resolved_model"], "paranal-noll-skycalc-fors1");
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
            "paranal-noll-skycalc-fors1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["kind"], "explicit");
    assert_eq!(selection["requested_model"], "paranal-noll-skycalc-fors1");
    assert_eq!(selection["resolved_model"], "paranal-noll-skycalc-fors1");
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
            "paranal-noll-skycalc-fors1",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let selection = &value["model"]["airglow_selection"];
    assert_eq!(selection["kind"], "explicit");
    assert_eq!(selection["requested_model"], "paranal-noll-skycalc-fors1");
    assert_eq!(selection["resolved_model"], "paranal-noll-skycalc-fors1");
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
            "--step",
            "3600",
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
    assert_eq!(selection["physical_outcome"], "physical-zero");
    assert_eq!(
        selection["physical_zero_reason"],
        "outside-astronomical-night"
    );
    assert_eq!(value["components"][0]["integrated_ph_cm2_ns_sr"], 0.0);
}
