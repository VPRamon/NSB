use nsb_coverage_gate::{
    check_diff, check_overall, find_policy_file, load_policy, parse_report, parse_unified_diff,
    run, CheckKind, CheckStatus, CoveragePolicy, GateOptions,
};
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;

const POLICY_TOML: &str = r#"
schema_version = 1
baseline_kind = "test"
html_artifact_name = "coverage-html"
json_artifact_name = "coverage-json"
cobertura_artifact_name = "coverage-cobertura"

[baseline]
commit = "deadbeef"
date = "2026-09-03"
rust_nightly = "nightly-test"
cargo_llvm_cov = "0.0.0"
command = "cargo llvm-cov"

[measured]
workspace_lines = 80.0
workspace_functions = 75.0
workspace_regions = 78.0
nsb_lines = 85.0
nsb_functions = 80.0
nsb_regions = 82.0
nsb_cli_lines = 75.0
nsb_cli_functions = 70.0
nsb_cli_regions = 72.0
nsb_data_tools_lines = 77.0
nsb_data_tools_functions = 70.0
nsb_data_tools_regions = 72.0

[floors]
workspace_lines = 78.0
nsb_lines = 84.0

[diff]
changed_production_lines = 90.0
base_ref = "origin/main"

[exclusions]
files = []
notes = "none"
"#;

const REPORT_JSON: &str = r#"
{
  "data": [
    {
      "files": [
        {
          "filename": "/repo/crates/nsb/src/lib.rs",
          "segments": [
            [10, 1, 3, true, true, false],
            [11, 1, 0, true, true, false],
            [12, 1, 1, true, true, false],
            [13, 1, 0, false, false, false]
          ],
          "summary": {
            "lines": {"count": 90, "covered": 88, "percent": 97.7778},
            "functions": {"count": 2, "covered": 2, "percent": 100.0},
            "regions": {"count": 3, "covered": 2, "percent": 66.6667}
          }
        },
        {
          "filename": "/repo/crates/nsb/src/solar_activity/tests.rs",
          "segments": [
            [1, 1, 1, true, true, false],
            [2, 1, 0, false, false, false]
          ],
          "summary": {
            "lines": {"count": 1, "covered": 1, "percent": 100.0},
            "functions": {"count": 1, "covered": 1, "percent": 100.0},
            "regions": {"count": 1, "covered": 1, "percent": 100.0}
          }
        }
      ],
      "totals": {
        "lines": {"count": 100, "covered": 80, "percent": 80.0},
        "functions": {"count": 10, "covered": 8, "percent": 80.0},
        "regions": {"count": 20, "covered": 16, "percent": 80.0}
      }
    }
  ]
}
"#;

fn sample_policy() -> CoveragePolicy {
    nsb_coverage_gate::parse_policy_str(POLICY_TOML).expect("test policy")
}

fn sample_report() -> nsb_coverage_gate::CoverageReport {
    parse_report(REPORT_JSON.as_bytes()).expect("test report")
}

fn fixture_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nsb-coverage-gate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("coverage-policy.toml"), POLICY_TOML).unwrap();
    std::fs::write(dir.join("coverage.json"), REPORT_JSON).unwrap();
    dir
}

#[test]
fn overall_floor_passes_measured_baseline() {
    let outcome = check_overall(&sample_policy(), &sample_report(), &GateOptions::default());
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome
        .lines
        .iter()
        .any(|line| line.contains("workspace lines: 80.00%")));
}

#[test]
fn overall_floor_fails_impossible_threshold() {
    let options = GateOptions {
        workspace_lines_floor: Some(100.0),
        ..GateOptions::default()
    };
    let outcome = check_overall(&sample_policy(), &sample_report(), &options);
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert!(outcome
        .lines
        .iter()
        .any(|line| line.contains("workspace line coverage 80.00% is below the floor 100.00%")));
}

#[test]
fn nsb_floor_uses_package_files_not_workspace_totals() {
    let options = GateOptions {
        nsb_lines_floor: Some(99.0),
        ..GateOptions::default()
    };
    let outcome = check_overall(&sample_policy(), &sample_report(), &options);
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert!(outcome
        .lines
        .iter()
        .any(|line| line.contains("nsb line coverage")));
}

#[test]
fn uncovered_changed_production_line_fails_diff_gate() {
    let diff = "\
+++ b/crates/nsb/src/lib.rs
@@ -11,0 +11,1 @@
+uncovered
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert_eq!(
        outcome.uncovered,
        vec!["crates/nsb/src/lib.rs:11".to_string()]
    );
}

#[test]
fn covered_changed_production_line_passes_diff_gate() {
    let diff = "\
+++ b/crates/nsb/src/lib.rs
@@ -10,0 +10,1 @@
+covered
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome.uncovered.is_empty());
}

#[test]
fn changed_test_file_is_not_a_diff_target() {
    let diff = "\
+++ b/crates/nsb/src/solar_activity/tests.rs
@@ -1,0 +1,1 @@
+test only
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome.uncovered.is_empty());
}

#[test]
fn changed_production_file_without_coverage_data_fails() {
    let diff = "\
+++ b/crates/nsb/src/missing.rs
@@ -1,0 +1,1 @@
+fn missing() {}
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert_eq!(
        outcome.missing_files,
        vec!["crates/nsb/src/missing.rs".to_string()]
    );
}

#[test]
fn comment_only_changed_line_is_ignored() {
    let diff = "\
+++ b/crates/nsb/src/lib.rs
@@ -99,0 +99,1 @@
+// comment, not instrumented
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
}

#[test]
fn run_overall_and_diff_write_actionable_output() {
    let dir = fixture_dir();
    let policy = dir.join("coverage-policy.toml");
    let report = dir.join("coverage.json");
    let diff_path = dir.join("change.diff");
    std::fs::write(
        &diff_path,
        "+++ b/crates/nsb/src/lib.rs\n@@ -11,0 +11,1 @@\n+uncovered\n",
    )
    .unwrap();

    let mut out = Cursor::new(Vec::new());
    let outcome = run(
        CheckKind::Overall,
        &GateOptions {
            policy_path: Some(policy.clone()),
            report_path: report.clone(),
            artifact_hint: Some("https://example.test/run".into()),
            ..GateOptions::default()
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(outcome.status, CheckStatus::Pass);
    let text = String::from_utf8(out.into_inner()).unwrap();
    assert!(text.contains("result: PASS"));
    assert!(text.contains("https://example.test/run"));

    let mut out = Cursor::new(Vec::new());
    let outcome = run(
        CheckKind::Diff,
        &GateOptions {
            policy_path: Some(policy),
            report_path: report,
            diff_file: Some(diff_path),
            ..GateOptions::default()
        },
        &mut out,
    )
    .unwrap();
    assert_eq!(outcome.status, CheckStatus::Fail);
    let text = String::from_utf8(out.into_inner()).unwrap();
    assert!(text.contains("uncovered changed production lines"));
    assert!(text.contains("crates/nsb/src/lib.rs:11"));
}

#[test]
fn load_policy_rejects_unsupported_schema() {
    let dir = fixture_dir();
    let path = dir.join("bad.toml");
    std::fs::write(&path, "schema_version = 99\nbaseline_kind = \"x\"\n").unwrap();
    assert!(load_policy(&path).is_err());
}

#[test]
fn finds_policy_file_from_nested_directory() {
    let dir = fixture_dir();
    let nested = dir.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let found = find_policy_file(&nested).unwrap();
    assert_eq!(found, dir.join("coverage-policy.toml"));
}

#[test]
fn binary_help_and_overall_contracts() {
    let dir = fixture_dir();
    let bin = env!("CARGO_BIN_EXE_nsb-coverage-gate");
    let help = Command::new(bin).arg("--help").output().unwrap();
    assert!(help.status.success());

    let unknown = Command::new(bin).arg("nope").output().unwrap();
    assert_eq!(unknown.status.code(), Some(2));

    let pass = Command::new(bin)
        .args([
            "overall",
            "--policy",
            dir.join("coverage-policy.toml").to_str().unwrap(),
            "--report",
            dir.join("coverage.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(pass.status.success());

    let fail = Command::new(bin)
        .args([
            "overall",
            "--policy",
            dir.join("coverage-policy.toml").to_str().unwrap(),
            "--report",
            dir.join("coverage.json").to_str().unwrap(),
            "--workspace-lines-floor",
            "100",
        ])
        .output()
        .unwrap();
    assert_eq!(fail.status.code(), Some(1));

    let diff = dir.join("ok.diff");
    std::fs::write(
        &diff,
        "+++ b/crates/nsb/src/lib.rs\n@@ -10,0 +10,1 @@\n+covered\n",
    )
    .unwrap();
    let diff_ok = Command::new(bin)
        .args([
            "diff",
            "--policy",
            dir.join("coverage-policy.toml").to_str().unwrap(),
            "--report",
            dir.join("coverage.json").to_str().unwrap(),
            "--diff-file",
            diff.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(diff_ok.status.success());

    let missing = Command::new(bin)
        .args([
            "overall",
            "--policy",
            dir.join("coverage-policy.toml").to_str().unwrap(),
            "--report",
            dir.join("missing.json").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn git_diff_unknown_base_is_an_error() {
    let dir = fixture_dir();
    let err = run(
        CheckKind::Diff,
        &GateOptions {
            policy_path: Some(dir.join("coverage-policy.toml")),
            report_path: dir.join("coverage.json"),
            base: Some("nsb-coverage-gate-missing-base".into()),
            ..GateOptions::default()
        },
        &mut Cursor::new(Vec::new()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("git"));
}
