// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Keep the published CLI getting-started commands executable as scientific defaults evolve.

use assert_cmd::Command;

#[test]
fn guide_point_evaluation_is_within_automatic_oxygen_domain() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format", "table", "point", "--time", "2026-06-19T03:00:00Z",
            "--site", "CTAO-S", "--site-profile", "cta-south",
            "--ra", "266.41683", "--dec", "-29.00781", "--components", "all",
        ])
        .assert()
        .success();
}

#[test]
fn guide_arbitrary_coordinates_evaluate_without_restricted_airglow() {
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format", "json", "point", "--time", "2026-06-19T03:00:00Z",
            "--lon", "12.5", "--lat", "41.9", "--height", "800",
            "--ra", "266.41683", "--dec", "-29.00781",
            "--components", "zodiacal",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["schema_version"], "nsb-cli-point-json-v1");
}

#[test]
fn guide_window_uses_explicit_continuum_for_broad_time_search() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "--format", "csv", "window",
            "--start", "2026-06-18T20:00:00Z",
            "--end", "2026-06-19T06:00:00Z",
            "--site", "CTAO-S", "--site-profile", "cta-south",
            "--ra", "266.41683", "--dec", "-29.00781",
            "--airglow-model", "paranal-palace-v1",
            "--max-nsb", "0.25", "--sun-altitude-max", "-18",
            "--target-altitude-min", "20",
        ])
        .assert()
        .success();
}
