use nsb_coverage_gate::{
    check_diff, check_overall, find_policy_file, load_policy, parse_lcov, parse_policy_str,
    parse_unified_diff, run, validate_percent, CheckKind, CheckStatus, CoveragePolicy, GateOptions,
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
lcov_artifact_name = "coverage-lcov"

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

fn sample_lcov() -> String {
    // LLVM LCOV `DA:line,hits`: hits>0 covered, hits=0 uncovered, absent = non-executable.
    let mut lcov = String::from("SF:/repo/crates/nsb/src/lib.rs\n");
    lcov.push_str("DA:10,3\nDA:11,0\nDA:12,1\n");
    for line in 20..=97 {
        lcov.push_str(&format!("DA:{line},1\n"));
    }
    lcov.push_str("end_of_record\n");
    lcov.push_str("SF:/repo/crates/nsb/src/solar_activity/tests.rs\nDA:1,1\nend_of_record\n");
    lcov
}

fn sample_policy() -> CoveragePolicy {
    parse_policy_str(POLICY_TOML).expect("test policy")
}

fn sample_report() -> nsb_coverage_gate::CoverageReport {
    parse_lcov(&sample_lcov()).expect("test lcov")
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
    std::fs::write(dir.join("coverage.lcov"), sample_lcov()).unwrap();
    dir
}

fn options_with_lcov(dir: &std::path::Path) -> GateOptions {
    GateOptions {
        policy_path: Some(dir.join("coverage-policy.toml")),
        lcov_path: dir.join("coverage.lcov"),
        report_path: None,
        ..GateOptions::default()
    }
}

#[test]
fn real_llvm_cov_lcov_fixture_matches_da_hit_semantics() {
    let text = include_str!("fixtures/llvm-cov-0.9.0.lcov");
    let report = parse_lcov(text).expect("llvm-cov LCOV fixture");
    let file = &report.files["crates/nsb-coverage-gate/src/check.rs"];
    assert_eq!(
        file.line_hits.get(&15),
        Some(&5),
        "executed DA hits are covered"
    );
    assert_eq!(file.line_hits.get(&99), Some(&0), "DA hits=0 is uncovered");
    assert_eq!(file.line_hits.get(&156), Some(&0));
    assert_eq!(file.line_hits.get(&157), Some(&9));
    assert_eq!(
        file.line_hits.get(&1),
        None,
        "lines without DA records are non-executable"
    );
}

#[test]
fn llvm_lcov_da_records_classify_changed_lines() {
    let report = sample_report();
    let file = &report.files["crates/nsb/src/lib.rs"];
    assert_eq!(
        file.line_hits.get(&10),
        Some(&3),
        "executed line is covered"
    );
    assert_eq!(
        file.line_hits.get(&11),
        Some(&0),
        "instrumented miss is uncovered"
    );
    assert_eq!(file.line_hits.get(&12), Some(&1));
    assert_eq!(
        file.line_hits.get(&99),
        None,
        "lines without DA records are non-executable"
    );
}

#[test]
fn overall_floor_passes_measured_baseline() {
    let outcome = check_overall(&sample_policy(), &sample_report(), &GateOptions::default());
    assert_eq!(outcome.status, CheckStatus::Pass, "{:?}", outcome.lines);
    assert!(outcome
        .lines
        .iter()
        .any(|line| line.contains("workspace lines: 98.78%")));
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
        .any(|line| line.contains("is below the floor 100.00%")));
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
fn missing_nsb_coverage_fails_closed_instead_of_100_percent() {
    let report = parse_lcov("SF:/repo/crates/nsb-cli/src/cli.rs\nDA:1,1\nend_of_record\n").unwrap();
    let outcome = check_overall(&sample_policy(), &report, &GateOptions::default());
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert!(outcome
        .lines
        .iter()
        .any(|line| line.contains("no nsb coverage data")));
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
fn coverage_gate_crate_file_is_not_a_diff_target() {
    let diff = "\
+++ b/crates/nsb-coverage-gate/src/check.rs
@@ -1,0 +1,1 @@
+tooling only
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
fn declaration_only_production_file_absent_from_lcov_passes() {
    let diff = "\
+++ b/crates/nsb/src/components/airglow/mod.rs
@@ -1,0 +1,4 @@
+//! Airglow component.
+pub mod continuum;
+pub use continuum::Airglow;
+#[cfg(test)]
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome.missing_files.is_empty());
    assert!(outcome.uncovered.is_empty());
}

#[test]
fn multiline_use_reexport_absent_from_lcov_passes() {
    let diff = "\
+++ b/crates/nsb/src/lib.rs
@@ -1,0 +1,5 @@
+pub use components::moonlight::{
+    AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991, DEFAULT_K_EXT,
+};
+    DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
+pub(crate) use extinction::NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG;
";
    let changed = parse_unified_diff(diff).unwrap();
    let outcome = check_diff(
        &sample_policy(),
        &sample_report(),
        &changed,
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome.missing_files.is_empty());
}

#[test]
fn changed_executable_line_hit_zero_fails() {
    let lcov = "SF:/repo/crates/nsb/src/lib.rs\nDA:40,0\nend_of_record\n";
    let report = parse_lcov(lcov).unwrap();
    let diff = "+++ b/crates/nsb/src/lib.rs\n@@ -40,0 +40,1 @@\n+return value;\n";
    let outcome = check_diff(
        &sample_policy(),
        &report,
        &parse_unified_diff(diff).unwrap(),
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert_eq!(
        outcome.uncovered,
        vec!["crates/nsb/src/lib.rs:40".to_string()]
    );
    assert!(outcome.missing_files.is_empty());
}

#[test]
fn changed_executable_line_above_threshold_passes() {
    let lcov = "SF:/repo/crates/nsb/src/lib.rs\nDA:40,2\nend_of_record\n";
    let report = parse_lcov(lcov).unwrap();
    let diff = "+++ b/crates/nsb/src/lib.rs\n@@ -40,0 +40,1 @@\n+return value;\n";
    let outcome = check_diff(
        &sample_policy(),
        &report,
        &parse_unified_diff(diff).unwrap(),
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Pass);
    assert!(outcome.uncovered.is_empty());
    assert!(outcome.missing_files.is_empty());
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
fn lcov_hit_count_zero_is_uncovered_even_when_json_summary_would_pass() {
    let lcov = "SF:/repo/crates/nsb/src/lib.rs\nDA:40,0\nend_of_record\n";
    let report = parse_lcov(lcov).unwrap();
    let diff = "+++ b/crates/nsb/src/lib.rs\n@@ -40,0 +40,1 @@\n+changed\n";
    let outcome = check_diff(
        &sample_policy(),
        &report,
        &parse_unified_diff(diff).unwrap(),
        &GateOptions::default(),
    );
    assert_eq!(outcome.status, CheckStatus::Fail);
    assert_eq!(
        outcome.uncovered,
        vec!["crates/nsb/src/lib.rs:40".to_string()]
    );
}

#[test]
fn run_overall_and_diff_write_actionable_output() {
    let dir = fixture_dir();
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
            artifact_hint: Some("https://example.test/run".into()),
            ..options_with_lcov(&dir)
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
            diff_file: Some(diff_path),
            ..options_with_lcov(&dir)
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
fn load_policy_rejects_nonempty_exclusions() {
    let mut text = POLICY_TOML.to_string();
    text = text.replace("files = []", "files = [\"crates/nsb/src/lib.rs\"]");
    let error = parse_policy_str(&text).unwrap_err();
    assert!(error.to_string().contains("exclusions.files is not empty"));
}

#[test]
fn percent_validation_rejects_nan_inf_and_out_of_range() {
    assert!(validate_percent("x", 0.0).is_ok());
    assert!(validate_percent("x", 100.0).is_ok());
    assert!(validate_percent("x", f64::NAN).is_err());
    assert!(validate_percent("x", f64::INFINITY).is_err());
    assert!(validate_percent("x", -0.1).is_err());
    assert!(validate_percent("x", 100.1).is_err());

    for (needle, replacement) in [
        ("workspace_lines = 78.0", "workspace_lines = nan"),
        ("workspace_lines = 78.0", "workspace_lines = inf"),
        ("workspace_lines = 78.0", "workspace_lines = -inf"),
        ("workspace_lines = 78.0", "workspace_lines = -1.0"),
        ("workspace_lines = 78.0", "workspace_lines = 100.1"),
        ("nsb_lines = 85.0", "nsb_lines = 101.0"),
        (
            "changed_production_lines = 90.0",
            "changed_production_lines = inf",
        ),
    ] {
        let text = POLICY_TOML.replace(needle, replacement);
        assert!(
            parse_policy_str(&text).is_err(),
            "expected {replacement} to be rejected"
        );
    }

    let repo_policy = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../coverage-policy.toml");
    load_policy(&repo_policy).expect("repository coverage-policy.toml must parse");
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

    let lcov = dir.join("coverage.lcov");
    let policy = dir.join("coverage-policy.toml");

    let pass = Command::new(bin)
        .args([
            "overall",
            "--policy",
            policy.to_str().unwrap(),
            "--lcov",
            lcov.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        pass.status.success(),
        "{}",
        String::from_utf8_lossy(&pass.stderr)
    );

    let fail = Command::new(bin)
        .args([
            "overall",
            "--policy",
            policy.to_str().unwrap(),
            "--lcov",
            lcov.to_str().unwrap(),
            "--workspace-lines-floor",
            "100",
        ])
        .output()
        .unwrap();
    assert_eq!(fail.status.code(), Some(1));

    let invalid = Command::new(bin)
        .args([
            "overall",
            "--policy",
            policy.to_str().unwrap(),
            "--lcov",
            lcov.to_str().unwrap(),
            "--nsb-lines-floor",
            "nan",
        ])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));

    for bad in ["inf", "-inf", "-1", "100.1"] {
        let invalid = Command::new(bin)
            .args([
                "overall",
                "--policy",
                policy.to_str().unwrap(),
                "--lcov",
                lcov.to_str().unwrap(),
                "--diff-lines-floor",
                bad,
            ])
            .output()
            .unwrap();
        assert_eq!(invalid.status.code(), Some(2), "{bad}");
    }

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
            policy.to_str().unwrap(),
            "--lcov",
            lcov.to_str().unwrap(),
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
            policy.to_str().unwrap(),
            "--lcov",
            dir.join("missing.lcov").to_str().unwrap(),
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
            base: Some("nsb-coverage-gate-missing-base".into()),
            ..options_with_lcov(&dir)
        },
        &mut Cursor::new(Vec::new()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("git"));
}
