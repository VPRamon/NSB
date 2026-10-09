// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Black-box contracts for the nsb-data entry point in src/cli/mod.rs.
//! All workflows use local fixtures or temporary files; no network is required.

use nsb_data_tools::dataset::DatasetName;
use nsb_data_tools::platform::checksum_io;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args(args)
        .output()
        .expect("run nsb-data")
}

fn succeeds(args: &[&str]) -> String {
    let output = invoke(args);
    assert!(
        output.status.success(),
        "nsb-data {args:?} failed (exit {:?}): {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8 stdout")
}

fn fails(args: &[&str]) -> (Option<i32>, String) {
    let output = invoke(args);
    assert!(
        !output.status.success(),
        "nsb-data {args:?} unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    (
        output.status.code(),
        String::from_utf8(output.stderr).expect("UTF-8 stderr"),
    )
}

#[test]
fn dataset_list_matches_the_registered_datasets() {
    let expected = DatasetName::ALL
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(succeeds(&["dataset", "list"]), format!("{expected}\n"));
}

#[test]
fn nested_help_exposes_the_supported_cli_routes() {
    let routes: &[(&[&str], &[&str])] = &[
        (&["--help"], &["dataset", "run", "solar", "starlight-uv"]),
        (
            &["dataset", "--help"],
            &[
                "list",
                "airglow-continuum",
                "solar-spectrum",
                "moonlight-scattering",
                "starlight",
            ],
        ),
        (
            &["dataset", "solar-spectrum", "--help"],
            &["update", "build", "validate", "publish"],
        ),
        (
            &["dataset", "starlight", "--help"],
            &[
                "update",
                "build",
                "validate",
                "publish",
                "validation",
                "pack",
                "stage-runtime",
                "promote",
                "diagnose",
                "bright-stars",
            ],
        ),
        (
            &["solar", "f107", "--help"],
            &["update", "freeze", "status", "resolve", "import", "verify"],
        ),
        (&["run", "--help"], &["status", "resume"]),
        (&["starlight-uv", "--help"], &["validate"]),
    ];

    for (args, expected_commands) in routes {
        let help = succeeds(args);
        for command in *expected_commands {
            assert!(
                help.contains(command),
                "nsb-data {args:?} help is missing {command:?}: {help}"
            );
        }
    }
}

#[test]
fn clap_rejects_unknown_commands_missing_flags_and_worker_conflicts() {
    for (args, expected) in [
        (vec!["dataset", "not-a-dataset"], "not-a-dataset"),
        (vec!["dataset", "solar-spectrum", "build"], "--config"),
        (vec!["solar", "f107", "freeze"], "--snapshot-id"),
        (vec!["solar", "f107", "resolve"], "--time"),
        (
            vec![
                "_worker",
                "--config",
                "unused.toml",
                "--dataset",
                "starlight",
                "--operation",
                "build",
                "--partition",
                "one",
                "--partition-manifest",
                "two.json",
            ],
            "--partition",
        ),
    ] {
        let (exit, stderr) = fails(&args);
        assert_eq!(
            exit,
            Some(2),
            "unexpected parser exit for {args:?}: {stderr}"
        );
        assert!(
            stderr.contains(expected),
            "nsb-data {args:?} did not explain {expected:?}: {stderr}"
        );
    }
}

#[test]
fn lifecycle_and_run_commands_fail_closed_on_missing_files() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let missing = dir.path().join("missing.toml");
    let missing = missing.to_str().expect("UTF-8 temporary path");
    for dataset in [
        "airglow-continuum",
        "solar-spectrum",
        "moonlight-scattering",
        "starlight",
    ] {
        for operation in ["update", "build", "validate", "publish"] {
            let (exit, stderr) = fails(&["dataset", dataset, operation, "--config", missing]);
            assert_ne!(
                exit,
                Some(2),
                "{dataset} {operation} rejected valid arguments: {stderr}"
            );
            assert!(
                stderr.contains("missing.toml"),
                "{dataset} {operation} did not report missing config: {stderr}"
            );
        }
    }
    let run_path = dir.path().join("missing-run.json");
    let run_path = run_path.to_str().expect("UTF-8 temporary path");
    for operation in ["status", "resume"] {
        let (exit, stderr) = fails(&["run", operation, "--run", run_path]);
        assert_ne!(
            exit,
            Some(2),
            "run {operation} rejected valid arguments: {stderr}"
        );
        assert!(
            !stderr.trim().is_empty(),
            "run {operation} must explain its failure"
        );
    }
    assert!(!dir.path().join("missing.toml").exists());
    assert!(!dir.path().join("missing-run.json").exists());
    Ok(())
}

#[test]
fn offline_f107_freeze_verify_status_resolve_and_import_round_trip() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swpc");
    let fixture_dir = fixture_dir.to_str().expect("UTF-8 fixture path");
    let original = dir.path().join("f107.json");
    let original_path = original.to_str().expect("UTF-8 temporary path");
    let dataset = "nsb-f107-cli-contract";
    let snapshot = "cli-fixture-2026-08-27";

    let freeze = succeeds(&[
        "solar",
        "f107",
        "freeze",
        "--store",
        original_path,
        "--fixture-dir",
        fixture_dir,
        "--dataset-id",
        dataset,
        "--snapshot-id",
        snapshot,
        "--retrieved-at",
        "2026-08-27T08:00:00Z",
    ]);
    assert!(freeze.contains("status=frozen"), "{freeze}");
    assert!(freeze.contains(&format!("dataset={dataset}")), "{freeze}");
    assert!(original.is_file());
    let checksum = checksum_io::sha256_file(&original)?;
    assert!(freeze.contains(&format!("checksum={checksum}")), "{freeze}");

    let verify = succeeds(&[
        "solar",
        "f107",
        "verify",
        original_path,
        "--sha256",
        &checksum,
    ]);
    assert!(
        verify.contains(&format!("ok dataset={dataset}")),
        "{verify}"
    );
    assert!(verify.contains(&format!("snapshot={snapshot}")), "{verify}");

    // Freshness changes over time, but status must always identify the same store.
    let status = succeeds(&["solar", "f107", "status", "--store", original_path]);
    assert!(status.contains(&format!("dataset={dataset}")), "{status}");
    assert!(status.contains(&format!("checksum={checksum}")), "{status}");

    let resolved = succeeds(&[
        "solar",
        "f107",
        "resolve",
        "--time",
        "2026-08-20T12:00:00Z",
        "--store",
        original_path,
    ]);
    for field in [
        "value_sfu=",
        "kind=",
        "provider=",
        "requested_date=2026-08-20",
        "resolution_step=",
    ] {
        assert!(
            resolved.contains(field),
            "resolve omitted {field}: {resolved}"
        );
    }
    assert!(
        resolved.contains(&format!("dataset={dataset}")),
        "{resolved}"
    );

    let imported = dir.path().join("imported/store.json");
    let imported_path = imported.to_str().expect("UTF-8 temporary path");
    let import = succeeds(&[
        "solar",
        "f107",
        "import",
        original_path,
        "--store",
        imported_path,
    ]);
    assert!(import.contains("status=imported"), "{import}");
    assert_eq!(fs::read(&original)?, fs::read(&imported)?);
    succeeds(&[
        "solar",
        "f107",
        "verify",
        imported_path,
        "--sha256",
        &checksum,
    ]);
    Ok(())
}

#[test]
fn f107_rejects_invalid_time_checksum_and_store_without_importing() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let valid = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/f107_store.json");
    let valid = valid.to_str().expect("UTF-8 fixture path");
    let bad = dir.path().join("invalid.json");
    fs::write(&bad, b"{invalid-json")?;
    let bad_path = bad.to_str().expect("UTF-8 temporary path");
    let dest = dir.path().join("imported.json");
    let dest_path = dest.to_str().expect("UTF-8 temporary path");

    let (_, time_error) = fails(&[
        "solar", "f107", "resolve", "--time", "bad-date", "--store", valid,
    ]);
    assert!(time_error.contains("invalid --time"), "{time_error}");

    let (_, mismatch) = fails(&[
        "solar",
        "f107",
        "verify",
        valid,
        "--sha256",
        "0000000000000000000000000000000000000000000000000000000000000000",
    ]);
    assert!(mismatch.contains("checksum mismatch"), "{mismatch}");

    let (_, import_error) = fails(&["solar", "f107", "import", bad_path, "--store", dest_path]);
    assert!(!import_error.is_empty());
    assert!(
        !dest.exists(),
        "invalid input must not create an active store"
    );
    Ok(())
}

#[test]
fn fixture_only_solar_update_executes_the_cli_branch() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = temporary.path().join("local-f107.json");
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swpc");
    let output = succeeds(&[
        "solar",
        "f107",
        "update",
        "--store",
        store.to_str().expect("UTF-8 store path"),
        "--fixture-dir",
        fixtures.to_str().expect("UTF-8 fixture path"),
        "--dataset-id",
        "cli-update-fixture",
    ]);
    assert!(output.contains("dataset=cli-update-fixture"), "{output}");
    let checksum = checksum_io::sha256_file(&store)?;
    assert!(output.contains(&format!("checksum={checksum}")), "{output}");
    assert!(store.is_file());
    Ok(())
}

#[test]
fn uv_validation_and_internal_worker_reject_missing_inputs() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let missing = temporary.path().join("no-such-input.toml");
    let absent = missing.to_str().expect("UTF-8 input path");
    let output = temporary.path().join("report.json");
    let output = output.to_str().expect("UTF-8 report path");
    let (_, uv_error) = fails(&[
        "starlight-uv",
        "validate",
        "--reference-manifest",
        absent,
        "--partition-manifest",
        absent,
        "--artifact",
        absent,
        "--artifact-sha256",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "--holdout",
        absent,
        "--output",
        output,
    ]);
    assert!(!uv_error.is_empty());
    assert!(!temporary.path().join("report.json").exists());

    let (_, worker_error) = fails(&[
        "_worker",
        "--config",
        absent,
        "--dataset",
        "starlight",
        "--operation",
        "build",
        "--partition",
        "one",
    ]);
    assert!(!worker_error.is_empty());
    assert!(!missing.exists());
    Ok(())
}
