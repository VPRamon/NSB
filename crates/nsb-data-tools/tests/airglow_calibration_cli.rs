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
    let evaluator = nsb::NsbEvaluator::with_config(
        nsb::NsbModelConfig::generic_clear_sky()
            .with_solar_radio_flux(nsb::SolarFluxUnits::new(100.))
            .with_airglow_calibration(artifact.clone())
            .unwrap(),
    )
    .unwrap();
    let query = nsb::PointQuery::new(
        siderust::coordinates::centers::Geodetic::new_raw(
            siderust::qtty::Degrees::new(-70.4044),
            siderust::qtty::Degrees::new(-24.6275),
            siderust::qtty::Meters::new(2635.),
        ),
        tempoch::Time::<tempoch::UTC>::from_chrono(
            chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        ),
        nsb::Target::new(
            siderust::qtty::Degrees::new(266.41683),
            siderust::qtty::Degrees::new(-29.00781),
        ),
    )
    .with_components(nsb::ComponentMask::AIRGLOW);
    assert!(evaluator.evaluate(&query).is_ok());

    assert_eq!(
        artifact.request().definition.calibration_id(),
        "synthetic-paranal-continuum-v1"
    );
    assert_eq!(artifact.request().definition.site_id(), "synthetic-paranal");
    assert!(!run(&output).status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), bytes);
    // Every scientifically significant field is checked against the source,
    // independently of whether the altered row would still produce a valid fit.
    let original_request = std::fs::read_to_string(&input).unwrap();
    let original: toml::Value = toml::from_str(&original_request).unwrap();
    for field in [
        "continuum_radiance",
        "uncertainty",
        "solar_flux_sfu",
        "ra_deg",
        "dec_deg",
        "utc",
        "wavelength_nm",
        "id",
    ] {
        let mut changed = original.clone();
        let row = &mut changed["observations"][0];
        row[field] = match field {
            "utc" => "2023-09-04T01:49:00Z".into(),
            "id" => "other-record".into(),
            "wavelength_nm" => toml::Value::Array(vec![301.0.into(), 650.0.into()]),
            _ => (row[field].as_float().unwrap() * 1.01).into(),
        };
        std::fs::write(&input, toml::to_string(&changed).unwrap()).unwrap();
        let rejected = directory.path().join(format!("rejected-{field}.toml"));
        let result = run(&rejected);
        assert!(!result.status.success(), "{field}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("does not match verified measurement"),
            "{field}: {:?}",
            result
        );
        assert!(!rejected.exists());
    }
    assert_eq!(
        artifact.request().measurement_sources[0].reference_id,
        "synthetic-measurements"
    );
    assert_eq!(artifact.request().observations[0].id, "synthetic-1");
    assert_eq!(
        artifact
            .request()
            .definition
            .reference_checksums()
            .next()
            .unwrap()
            .2,
        siderust::checksum::to_hex(&siderust::checksum::sha256(
            &std::fs::read(&measurements).unwrap()
        ))
    );

    // TOML tables also support real reduction exports and extra held-out records.
    let mut mapped = original.clone();
    mapped["measurement_sources"][0]["format"] = "observations-toml-v1".into();
    let mut table = toml::map::Map::new();
    table.insert("observations".into(), original["observations"].clone());
    let source = toml::to_string(&table).unwrap();
    std::fs::write(&measurements, &source).unwrap();
    mapped["definition"]["evidence"]["references"][0]["sha256"] =
        siderust::checksum::to_hex(&siderust::checksum::sha256(source.as_bytes())).into();
    std::fs::write(&input, toml::to_string(&mapped).unwrap()).unwrap();
    let toml_output = directory.path().join("toml-source.toml");
    let result = run(&toml_output);
    assert!(result.status.success(), "{:?}", result);
    let loaded =
        AirglowCalibrationProfile::from_toml_str(&std::fs::read_to_string(toml_output).unwrap())
            .unwrap();
    assert_eq!(
        loaded.request().measurement_sources[0].format,
        "observations-toml-v1"
    );

    // Missing, unsupported and ambiguous mappings fail without publishing output.
    for case in 0..4 {
        let mut invalid = mapped.clone();
        match case {
            0 => {
                invalid
                    .as_table_mut()
                    .unwrap()
                    .remove("measurement_sources");
            }
            1 => invalid["measurement_sources"][0]["reference_id"] = "missing".into(),
            2 => invalid["measurement_sources"][0]["format"] = "observations-toml-v2".into(),
            _ => {
                let duplicate = invalid["measurement_sources"][0].clone();
                invalid["measurement_sources"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
        }
        std::fs::write(&input, toml::to_string(&invalid).unwrap()).unwrap();
        let rejected = directory.path().join(format!("mapping-{case}.toml"));
        assert!(!run(&rejected).status.success());
        assert!(!rejected.exists());
    }
    std::fs::write(&input, original_request).unwrap();
    std::fs::write(&measurements, b"modified observations").unwrap();
    let rejected = directory.path().join("must-not-exist.toml");
    let result = run(&rejected);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("reference checksum mismatch"));
    assert!(!rejected.exists());
}
