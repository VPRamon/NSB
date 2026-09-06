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

fn explicit_coordinate_args<'a>(lon: &'a str, lat: &'a str, height: &'a str) -> Vec<&'a str> {
    vec![
        "point",
        "--time",
        "2026-06-18T23:00:00Z",
        "--lon",
        lon,
        "--lat",
        lat,
        "--height",
        height,
        "--ra",
        "83",
        "--dec",
        "22",
        "--components",
        "zodiacal",
    ]
}

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
    .stdout(predicate::str::contains("El Paranal Observatory").not())
    .stdout(predicate::str::contains("CTAO South").not());

    let mut show_missing_builtin = Command::cargo_bin("nsb").unwrap();
    show_missing_builtin
        .args([
            "sites",
            "--observatory-catalog",
            path.to_str().unwrap(),
            "show",
            "PARANAL",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown observatory name or alias",
        ));

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
fn explicit_coordinate_boundaries_are_valid() {
    for (lon, lat, height) in [
        ("10", "20", "30"),
        ("-180", "-90", "-500"),
        ("180", "90", "10000"),
    ] {
        Command::cargo_bin("nsb")
            .unwrap()
            .args(explicit_coordinate_args(lon, lat, height))
            .assert()
            .success();
    }
}

#[test]
fn explicit_coordinates_reject_each_invalid_dimension() {
    let cases = [
        ("-180.0001", "20", "30", "--lon must be finite"),
        ("180.0001", "20", "30", "--lon must be finite"),
        ("NaN", "20", "30", "--lon must be finite"),
        ("inf", "20", "30", "--lon must be finite"),
        ("10", "-90.0001", "30", "--lat must be finite"),
        ("10", "90.0001", "30", "--lat must be finite"),
        ("10", "NaN", "30", "--lat must be finite"),
        ("10", "inf", "30", "--lat must be finite"),
        ("10", "20", "-500.0001", "--height must be finite"),
        ("10", "20", "10000.0001", "--height must be finite"),
        ("10", "20", "NaN", "--height must be finite"),
        ("10", "20", "inf", "--height must be finite"),
    ];

    for (lon, lat, height, expected_error) in cases {
        Command::cargo_bin("nsb")
            .unwrap()
            .args(explicit_coordinate_args(lon, lat, height))
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected_error));
    }
}

#[test]
fn profile_selection_is_independent_and_custom_sites_default_to_generic() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("observatories.toml");
    fs::write(&path, CUSTOM_CATALOG).unwrap();

    let run = |site: &str, catalog: Option<&str>, profile: Option<&str>| {
        let mut args = vec![
            "--format",
            "json",
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            site,
            "--ra",
            "83",
            "--dec",
            "22",
            "--components",
            "zodiacal",
        ];
        if let Some(catalog) = catalog {
            args.extend(["--observatory-catalog", catalog]);
        }
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

    assert_eq!(
        run(
            "Fictional Test Observatory",
            Some(path.to_str().unwrap()),
            None
        )["model"]["preset"],
        "generic-clear-sky"
    );
    assert_eq!(
        run(
            "Fictional Test Observatory",
            Some(path.to_str().unwrap()),
            Some("cta-north")
        )["model"]["preset"],
        "ctao-north-planning"
    );

    assert_eq!(
        run("CTAO-N", None, None)["model"]["preset"],
        "generic-clear-sky"
    );
    assert_eq!(
        run("CTAO-N", None, Some("cta-north"))["model"]["preset"],
        "ctao-north-planning"
    );
    assert_eq!(
        run("CTAO-S", None, None)["model"]["preset"],
        "generic-clear-sky"
    );
    assert_eq!(
        run("CTAO-S", None, Some("cta-south"))["model"]["preset"],
        "ctao-south-planning"
    );
}

#[test]
fn point_and_window_accept_composed_site_aliases() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "point",
            "--time",
            "2026-06-18T23:00:00Z",
            "--site",
            "CTAO-S",
            "--site-profile",
            "cta-south",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--components",
            "zodiacal",
        ])
        .assert()
        .success();

    Command::cargo_bin("nsb")
        .unwrap()
        .args([
            "window",
            "--start",
            "2026-06-18T20:00:00Z",
            "--end",
            "2026-06-18T22:00:00Z",
            "--site",
            "HESS",
            "--ra",
            "83.0",
            "--dec",
            "22.0",
            "--max-nsb",
            "10",
            "--components",
            "zodiacal",
            "--step",
            "1800",
            "--no-pre-filter",
        ])
        .assert()
        .success();
}
