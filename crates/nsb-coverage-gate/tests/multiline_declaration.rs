use nsb_coverage_gate::{
    check_diff_with_sources, parse_lcov, parse_policy_str, parse_unified_diff, CheckStatus,
    GateOptions,
};

const POLICY: &str = r#"
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

fn report_without_target_file() -> nsb_coverage_gate::CoverageReport {
    parse_lcov("SF:/repo/crates/nsb/src/covered.rs\nDA:1,1\nend_of_record\n").unwrap()
}

#[test]
fn multiline_const_continuation_absent_from_lcov_is_not_instrumentable() {
    let path = "crates/nsb/src/provenance.rs";
    let source = "pub const SOME_SOURCE: &str =\n    \"git:https://example.com/repository?rev=abc123\";\n";
    let diff = "\
+++ b/crates/nsb/src/provenance.rs
@@ -1,0 +1,2 @@
+pub const SOME_SOURCE: &str =
+    \"git:https://example.com/repository?rev=abc123\";
";
    let outcome = check_diff_with_sources(
        &parse_policy_str(POLICY).unwrap(),
        &report_without_target_file(),
        &parse_unified_diff(diff).unwrap(),
        &GateOptions::default(),
        |requested| (requested == path).then(|| source.to_string()),
    );

    assert_eq!(outcome.status, CheckStatus::Pass, "{:?}", outcome.lines);
    assert!(outcome.missing_files.is_empty());
}

#[test]
fn multiline_runtime_assignment_absent_from_lcov_still_fails_closed() {
    let path = "crates/nsb/src/runtime.rs";
    let source = "fn runtime() {\n    let source =\n        \"runtime-value\";\n}\n";
    let diff = "\
+++ b/crates/nsb/src/runtime.rs
@@ -3,0 +3,1 @@
+        \"runtime-value\";
";
    let outcome = check_diff_with_sources(
        &parse_policy_str(POLICY).unwrap(),
        &report_without_target_file(),
        &parse_unified_diff(diff).unwrap(),
        &GateOptions::default(),
        |requested| (requested == path).then(|| source.to_string()),
    );

    assert_eq!(outcome.status, CheckStatus::Fail);
    assert_eq!(outcome.missing_files, vec![path.to_string()]);
}
