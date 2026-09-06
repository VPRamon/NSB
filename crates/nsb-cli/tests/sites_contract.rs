use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn sites_list_is_composed_catalog_driven() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["sites", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("El Paranal Observatory"))
        .stdout(predicate::str::contains(
            "Roque de los Muchachos Observatory",
        ))
        .stdout(predicate::str::contains("CTAO South"))
        .stdout(predicate::str::contains("CTAO North"))
        .stdout(predicate::str::contains("H.E.S.S."))
        .stdout(predicate::str::contains("MAGIC Telescopes"))
        .stdout(predicate::str::contains("VERITAS"))
        .stdout(predicate::str::contains("Gran Telescopio Canarias"))
        .stdout(predicate::str::contains("PARANAL"))
        .stdout(predicate::str::contains("CTAO-S"))
        .stdout(predicate::str::contains("HESS"));
}

#[test]
fn sites_show_json_reports_catalog_name_and_cli_aliases() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args(["--format", "json", "sites", "show", "PARANAL"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value[0]["name"], "El Paranal Observatory");
    assert!(value[0]["aliases"]
        .as_array()
        .unwrap()
        .iter()
        .any(|alias| alias == "PARANAL"));
}

#[test]
fn sites_show_unknown_observatory_fails_usefully() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args(["sites", "show", "NOT-A-REAL-OBSERVATORY"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown observatory name or alias",
        ));
}

#[test]
fn sites_show_csv_is_parseable_and_preserves_observatory_fields() {
    let output = Command::cargo_bin("nsb")
        .unwrap()
        .args(["--format", "csv", "sites", "show", "CTAO-S"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut reader = csv::Reader::from_reader(output.as_slice());
    assert_eq!(
        reader.headers().unwrap(),
        &csv::StringRecord::from(vec![
            "name",
            "longitude_deg",
            "latitude_deg",
            "height_m",
            "aliases",
        ])
    );

    let records = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(&record[0], "CTAO South");
    assert!((record[1].parse::<f64>().unwrap() - (-70.31634444444444)).abs() < 1.0e-12);
    assert!((record[2].parse::<f64>().unwrap() - (-24.683427777777776)).abs() < 1.0e-12);
    assert!((record[3].parse::<f64>().unwrap() - 2184.6).abs() < 1.0e-9);
    assert!(record[4].split(';').any(|alias| alias == "CTAO-S"));
}

#[test]
fn sites_show_ctao_and_extension_aliases() {
    let cases = [
        (
            "CTAO-S",
            "CTAO South",
            -70.31634444444444,
            -24.683427777777776,
        ),
        ("CTAO-N", "CTAO North", -17.892005, 28.762164),
        ("HESS", "H.E.S.S.", 16.5, -23.271666666666665),
        ("MAGIC", "MAGIC Telescopes", -17.89, 28.761944),
        ("FACT", "First G-APD Cherenkov Telescope", -17.89, 28.761944),
        (
            "VERITAS",
            "VERITAS",
            -110.95215833333333,
            31.675058333333334,
        ),
        (
            "FAST",
            "Five-hundred-meter Aperture Spherical Telescope",
            106.856667,
            25.653056,
        ),
        (
            "GTC",
            "Gran Telescopio Canarias",
            -17.891944444444444,
            28.756666666666668,
        ),
    ];
    for (alias, name, lon, lat) in cases {
        let output = Command::cargo_bin("nsb")
            .unwrap()
            .args(["--format", "json", "sites", "show", alias])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value[0]["name"], name, "alias {alias}");
        let got_lon = value[0]["longitude_deg"].as_f64().unwrap();
        let got_lat = value[0]["latitude_deg"].as_f64().unwrap();
        assert!(
            (got_lon - lon).abs() < 1.0e-12,
            "alias {alias}: lon {got_lon} != {lon}"
        );
        assert!(
            (got_lat - lat).abs() < 1.0e-12,
            "alias {alias}: lat {got_lat} != {lat}"
        );
    }
}

#[test]
fn ctao_coordinates_differ_from_orm_and_paranal() {
    let show = |alias: &str| {
        let output = Command::cargo_bin("nsb")
            .unwrap()
            .args(["--format", "json", "sites", "show", alias])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice::<serde_json::Value>(&output).unwrap()
    };

    let ctao_n = &show("CTAO-N")[0];
    let orm = &show("ORM")[0];
    let ctao_s = &show("CTAO-S")[0];
    let paranal = &show("PARANAL")[0];

    assert_ne!(ctao_n["longitude_deg"], orm["longitude_deg"]);
    assert_ne!(ctao_n["latitude_deg"], orm["latitude_deg"]);
    assert_ne!(ctao_n["height_m"], orm["height_m"]);
    assert_ne!(ctao_s["longitude_deg"], paranal["longitude_deg"]);
    assert_ne!(ctao_s["latitude_deg"], paranal["latitude_deg"]);
    assert_ne!(ctao_s["height_m"], paranal["height_m"]);
}
