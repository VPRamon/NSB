use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn config_init_contract_is_stable() {
    let mut cmd = Command::cargo_bin("nsb").unwrap();
    cmd.args(["config", "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("site = \"CTAO-S\""))
        .stdout(predicate::str::contains("starlight = false"))
        .stdout(predicate::str::contains("sample_step_seconds = 600.0"));
}

#[test]
fn config_validate_accepts_init_template() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nsb.toml");
    let template = Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "init"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    std::fs::write(&path, template).unwrap();

    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok:"));
}

#[test]
fn config_validate_rejects_invalid_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "this is not = toml [").unwrap();

    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid config file"));
}

#[test]
fn config_validate_rejects_missing_file() {
    Command::cargo_bin("nsb")
        .unwrap()
        .args(["config", "validate", "/no/such/nsb-config.toml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to read config file"));
}
