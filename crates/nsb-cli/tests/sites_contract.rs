use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn sites_list_prints_bundled_observatories() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["sites", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CTAO-S"))
        .stdout(predicate::str::contains("CTAO-N"))
        .stdout(predicate::str::contains("HESS"))
        .stdout(predicate::str::contains("MAGIC"))
        .stdout(predicate::str::contains("FACT"))
        .stdout(predicate::str::contains("VERITAS"))
        .stdout(predicate::str::contains("FAST"))
        .stdout(predicate::str::contains("GTC"))
        .stdout(predicate::str::contains("PARANAL"));
}

#[test]
fn sites_show_json_is_valid() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    let output = cmd
        .args(["--format", "json", "sites", "show", "CTAO-S"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value[0]["canonical_alias"], "CTAO-S");
    assert_eq!(value[0]["lon_deg"], -70.31634444444444);
    assert_eq!(value[0]["lat_deg"], -24.683427777777776);
}

#[test]
fn sites_show_resolves_new_aliases() {
    for alias in ["HESS", "MAGIC", "FACT", "VERITAS", "FAST", "GTC"] {
        let mut cmd = Command::cargo_bin("nsb").unwrap();
        cmd.args(["sites", "show", alias]).assert().success();
    }
}
