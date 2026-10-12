// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use assert_cmd::Command;
use predicates::prelude::*;

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
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["schema_version"], "nsb-cli-window-json-v1");
    assert_eq!(value["model"]["preset"], "ctao-south-planning");
    assert_eq!(value["model"]["airglow_selection"]["kind"], "automatic");
    assert_eq!(
        value["model"]["airglow_selection"]["resolved_model"],
        "paranal-palace-v1"
    );
    assert_eq!(value["model"]["airglow_selection"]["used_fallback"], true);
    assert!(
        value["model"]["airglow_selection"]["physical_outcome"].is_null(),
        "window descriptors must not fabricate evaluation outcomes"
    );
    assert_eq!(value["model"]["zodiacal_model"], "leinert-1998");
    assert_eq!(
        value["model"]["zodiacal_extinction"],
        "noll-2012-approximation"
    );
    let expected_component_count = if nsb::StarlightProduct::bundled_production_available() {
        4
    } else {
        3
    };
    assert_eq!(
        value["component_metadata"].as_array().unwrap().len(),
        expected_component_count
    );
    let airglow = value["component_metadata"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "airglow")
        .expect("airglow component metadata");
    assert_eq!(
        airglow["metadata"]["airglow_selection"]["resolved_model"],
        "paranal-palace-v1"
    );
    assert!(
        airglow["metadata"]["airglow_selection"]["physical_outcome"].is_null(),
        "window descriptors must not fabricate evaluation outcomes"
    );
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

#[test]
fn window_min_nsb_band_excludes_periods_below_minimum() {
    let max_only = Command::cargo_bin("nsb")
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
            "--components",
            "zodiacal,airglow",
            "--step",
            "3600",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let max_value: serde_json::Value = serde_json::from_slice(&max_only).unwrap();
    let max_periods = max_value["periods"].as_array().unwrap();
    assert_eq!(
        max_periods.len(),
        1,
        "large max-nsb without min should keep the full window"
    );

    let band = Command::cargo_bin("nsb")
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
            "--min-nsb",
            "1000000",
            "--max-nsb",
            "1000000",
            "--components",
            "zodiacal,airglow",
            "--step",
            "3600",
            "--no-pre-filter",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let band_value: serde_json::Value = serde_json::from_slice(&band).unwrap();
    assert_eq!(band_value["min_nsb_ph_cm2_ns_sr"], 1_000_000.0);
    assert_eq!(band_value["max_nsb_ph_cm2_ns_sr"], 1_000_000.0);
    assert!(
        band_value["periods"].as_array().unwrap().is_empty(),
        "equal min/max band must exclude every below-max period"
    );
}

#[test]
fn window_rejects_non_finite_max_nsb() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
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
            "nan",
            "--no-pre-filter",
        ])
        .assert()
        .failure();
}

#[test]
fn certified_window_fails_explicitly_without_a_dynamic_envelope() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
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
            "--components",
            "all",
            "--no-pre-filter",
            "--certified",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "incomplete certified threshold search",
        ));
}

#[test]
fn certified_window_returns_a_complete_constant_sum_without_a_step() {
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
            "",
            "--no-pre-filter",
            "--certified",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["periods"].as_array().unwrap().len(), 1);
}

#[test]
fn adaptive_window_supports_active_models_without_step_and_rejects_conflicting_modes() {
    let base = [
        "--format",
        "json",
        "window",
        "--start",
        "2026-01-03T01:00:00.000000999Z",
        "--end",
        "2026-01-03T06:00:00.000000999Z",
        "--site",
        "PARANAL",
        "--ra",
        "83.6331",
        "--dec",
        "22.0145",
        "--max-nsb",
        "1000000",
        "--components",
        "all",
        "--no-pre-filter",
        "--adaptive",
    ];
    for moon in ["ks1991", "jones2013"] {
        let output = Command::cargo_bin("nsb")
            .unwrap()
            .args(base)
            .args(["--moonlight-model", moon])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let periods = value["periods"].as_array().unwrap();
        assert_eq!(periods.len(), 1);
        let start = chrono::DateTime::parse_from_rfc3339(periods[0]["start_utc"].as_str().unwrap())
            .unwrap();
        let end =
            chrono::DateTime::parse_from_rfc3339(periods[0]["end_utc"].as_str().unwrap()).unwrap();
        assert!(start >= chrono::DateTime::parse_from_rfc3339(base[4]).unwrap());
        assert!(end <= chrono::DateTime::parse_from_rfc3339(base[6]).unwrap());
    }
    for extra in [vec!["--step", "600"], vec!["--certified"]] {
        Command::cargo_bin("nsb")
            .unwrap()
            .args(base)
            .args(extra)
            .assert()
            .failure()
            .stdout(predicate::str::is_empty());
    }
}

#[test]
fn joint_band_cli_millisecond_boundaries_satisfy_both_constraints() {
    use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
    use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
    use tempoch::{Time, UTC};
    let evaluator = NsbEvaluator::new().unwrap();
    let observer = siderust::catalogs::observatories::EL_PARANAL.geodetic();
    let target = Target::new(266.41683 * DEG, -29.00781 * DEG);
    let mask = ComponentMask::ZODIACAL | ComponentMask::AIRGLOW;
    let start = "2023-09-04T00:00:00Z"
        .parse::<chrono::DateTime<chrono::Utc>>()
        .unwrap();
    let mut values: Vec<_> = (0..=36)
        .map(|i| {
            evaluator
                .evaluate(
                    &PointQuery::new(
                        observer,
                        Time::<UTC>::from_chrono(start + chrono::Duration::minutes(i * 10)),
                        target,
                    )
                    .with_components(mask),
                )
                .unwrap()
                .integrated
                .value()
        })
        .collect();
    values.sort_by(f64::total_cmp);
    let minimum = values[values.len() / 3];
    let maximum = values[values.len() * 2 / 3];
    for adaptive in [false, true] {
        let mut cmd = Command::cargo_bin("nsb").unwrap();
        cmd.args([
            "--format",
            "json",
            "window",
            "--start",
            "2023-09-04T00:00:00.000000999Z",
            "--end",
            "2023-09-04T06:00:00.000000999Z",
            "--site",
            "PARANAL",
            "--ra",
            "266.41683",
            "--dec",
            "-29.00781",
            "--components",
            "zodiacal,airglow",
            "--no-pre-filter",
            "--min-nsb",
            &minimum.to_string(),
            "--max-nsb",
            &maximum.to_string(),
        ]);
        if adaptive {
            cmd.arg("--adaptive");
        }
        let output = cmd.assert().success().get_output().stdout.clone();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let periods = value["periods"].as_array().unwrap();
        assert!(!periods.is_empty());
        for period in periods {
            let a = chrono::DateTime::parse_from_rfc3339(period["start_utc"].as_str().unwrap())
                .unwrap()
                .with_timezone(&chrono::Utc);
            let b = chrono::DateTime::parse_from_rfc3339(period["end_utc"].as_str().unwrap())
                .unwrap()
                .with_timezone(&chrono::Utc);
            for i in 0..=100 {
                let time = Time::<UTC>::from_chrono(a + (b - a) * i / 100);
                let y = evaluator
                    .evaluate(&PointQuery::new(observer, time, target).with_components(mask))
                    .unwrap()
                    .integrated;
                assert!(
                    y >= Radiance::new(minimum) && y <= Radiance::new(maximum),
                    "displayed band violates constraint: {y}"
                );
            }
        }
    }
}
