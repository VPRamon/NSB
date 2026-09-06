use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

const CUSTOM_CATALOG: &str = r#"
[[observatory]]
name = "Fictional Test Observatory"
longitude_deg = 12.5
latitude_deg = 41.9
height_m = 800.0
reference_pressure_hpa = 920.0
"#;

#[test]
fn external_catalog_replaces_bundled_scope_and_selects_unknown_site() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("observatories.toml");
    fs::write(&path, CUSTOM_CATALOG).unwrap();

    let mut list = Command::cargo_bin("nsb").unwrap();
    list.args([
        "sites",
        "--observatory-catalog",
        path.to_str().unwrap(),
        "list",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Fictional Test Observatory"))
    .stdout(predicate::str::contains("El Paranal Observatory").not());

    let mut point = Command::cargo_bin("nsb").unwrap();
    point
        .args([
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "Fictional Test Observatory",
            "--observatory-catalog",
            path.to_str().unwrap(),
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "zodiacal",
        ])
        .assert()
        .success();
}

#[test]
fn malformed_missing_and_duplicate_catalogs_report_useful_errors() {
    let directory = tempdir().unwrap();
    let malformed = directory.path().join("malformed.toml");
    fs::write(&malformed, "[[observatory]\n").unwrap();

    let mut malformed_command = Command::cargo_bin("nsb").unwrap();
    malformed_command
        .args([
            "sites",
            "--observatory-catalog",
            malformed.to_str().unwrap(),
            "list",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid observatory catalog TOML"));

    let mut missing_command = Command::cargo_bin("nsb").unwrap();
    missing_command
        .args([
            "sites",
            "--observatory-catalog",
            directory.path().join("missing.toml").to_str().unwrap(),
            "list",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "failed to read observatory catalog",
        ));

    let duplicate = directory.path().join("duplicate.toml");
    fs::write(&duplicate, format!("{CUSTOM_CATALOG}{CUSTOM_CATALOG}")).unwrap();
    let mut duplicate_command = Command::cargo_bin("nsb").unwrap();
    duplicate_command
        .args([
            "sites",
            "--observatory-catalog",
            duplicate.to_str().unwrap(),
            "list",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate observatory name"));
}

#[test]
fn explicit_coordinates_are_validated() {
    let base = [
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--lon",
        "10",
        "--lat",
        "20",
        "--height",
        "30",
        "--ra",
        "83",
        "--dec",
        "22",
        "--components",
        "zodiacal",
    ];
    Command::cargo_bin("nsb")
        .unwrap()
        .args(base)
        .assert()
        .success();

    let mut invalid = base;
    invalid[4] = "181";
    Command::cargo_bin("nsb")
        .unwrap()
        .args(invalid)
        .assert()
        .failure()
        .stderr(predicate::str::contains("--lon must be finite"));
}

#[test]
fn profile_selection_is_independent_and_custom_sites_default_to_generic() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("observatories.toml");
    fs::write(&path, CUSTOM_CATALOG).unwrap();

    let run = |profile: Option<&str>| {
        let mut args = vec![
            "--format",
            "json",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "Fictional Test Observatory",
            "--observatory-catalog",
            path.to_str().unwrap(),
            "--ra",
            "83",
            "--dec",
            "22",
            "--components",
            "zodiacal",
        ];
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
        serde_json::from_slice::<serde_json::Value>(&output).unwrap()
    };

    assert_eq!(run(None)["model"]["preset"], "generic-clear-sky");
    assert_eq!(
        run(Some("cta-north"))["model"]["preset"],
        "ctao-north-planning"
    );
}
