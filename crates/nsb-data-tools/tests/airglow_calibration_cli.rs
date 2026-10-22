// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Independent-process fit/export integrity, including original reference bytes.
use nsb::site::AirglowCalibrationProfile;
use std::process::Command;

#[test]
fn fit_export_rejects_reference_tampering_and_preserves_existing_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("request.toml");
    let measurements = directory.path().join("measurements.txt");
    let output = directory.path().join("calibration.toml");
    std::fs::write(
        &input,
        include_str!("../../nsb/examples/airglow-calibration/request.toml"),
    )
    .unwrap();
    std::fs::write(
        &measurements,
        include_bytes!("../../nsb/examples/airglow-calibration/measurements.txt"),
    )
    .unwrap();
    let run = |output: &std::path::Path| {
        Command::new(env!("CARGO_BIN_EXE_nsb-airglow-calibrate"))
            .arg("--input")
            .arg(&input)
            .arg("--output")
            .arg(output)
            .output()
            .unwrap()
    };
    let result = run(&output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout)
        .contains("registered-fit-not-independently-validated"));
    let bytes = std::fs::read_to_string(&output).unwrap();
    let artifact = AirglowCalibrationProfile::from_toml_str(&bytes).unwrap();
    assert!((artifact.scale().value() - 1.2).abs() < 1e-12);
    assert!(artifact.variance() > 0.);
    assert_eq!(
        artifact.request().definition.calibration_id(),
        "synthetic-paranal-continuum-v1"
    );
    assert_eq!(artifact.request().definition.site_id(), "synthetic-paranal");
    assert!(!run(&output).status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), bytes);
    std::fs::write(&measurements, b"modified observations").unwrap();
    let rejected = directory.path().join("must-not-exist.toml");
    let result = run(&rejected);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("reference checksum mismatch"));
    assert!(!rejected.exists());
}
