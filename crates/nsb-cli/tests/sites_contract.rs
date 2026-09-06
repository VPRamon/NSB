use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn sites_list_is_siderust_catalog_driven() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["sites", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("El Paranal Observatory"))
        .stdout(predicate::str::contains(
            "Roque de los Muchachos Observatory",
        ))
        .stdout(predicate::str::contains("PARANAL"))
        .stdout(predicate::str::contains("CTAO-S").not());
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
fn ctao_is_not_substituted_with_paranal_or_orm() {
    for alias in ["CTAO-N", "CTAO-S"] {
        let mut cmd = Command::cargo_bin("nsb").unwrap();
        cmd.args(["sites", "show", alias])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "unknown observatory name or alias",
            ));
    }
}
