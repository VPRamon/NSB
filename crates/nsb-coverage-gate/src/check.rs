use crate::cfg_test::cfg_test_line_numbers;
use crate::diff::{
    group_by_path, is_likely_non_instrumentable_rust_line,
    is_likely_non_instrumentable_rust_line_with_context, ChangedLine,
};
use crate::llvm::{crate_metrics, CoverageReport};
use crate::paths::is_production_rust_file;
use crate::policy::CoveragePolicy;
use crate::GateOptions;
use std::path::Path;

/// Which gate to evaluate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    Overall,
    Diff,
}

impl CheckKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overall => "overall",
            Self::Diff => "diff",
        }
    }
}

/// Pass/fail result of a gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Fail,
}

/// Structured gate result for CI and tests.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub kind: CheckKind,
    pub status: CheckStatus,
    pub lines: Vec<String>,
    pub uncovered: Vec<String>,
    pub missing_files: Vec<String>,
}

/// Enforce workspace and `nsb` line floors against an already-collected report.
pub fn check_overall(
    policy: &CoveragePolicy,
    report: &CoverageReport,
    options: &GateOptions,
) -> CheckOutcome {
    let workspace_floor = options
        .workspace_lines_floor
        .unwrap_or(policy.floors.workspace_lines);
    let nsb_floor = options.nsb_lines_floor.unwrap_or(policy.floors.nsb_lines);
    let workspace_lines = report.lines;
    let nsb = crate_metrics(report, "nsb");
    let nsb_cli = crate_metrics(report, "nsb-cli");
    let tools = crate_metrics(report, "nsb-data-tools");
    let nsb_lines = nsb.lines;
    let nsb_functions = nsb.functions;
    let nsb_regions = nsb.regions;
    let nsb_cli_lines = nsb_cli.lines;
    let nsb_cli_functions = nsb_cli.functions;
    let nsb_cli_regions = nsb_cli.regions;
    let tools_lines = tools.lines;
    let tools_functions = tools.functions;
    let tools_regions = tools.regions;
    let workspace_functions = report.functions;
    let workspace_regions = report.regions;

    let mut lines = vec![
        format!(
            "workspace lines: {:.2}% (floor {:.2}%; measured baseline {:.2}%)",
            workspace_lines.percent, workspace_floor, policy.measured.workspace_lines
        ),
        format!(
            "nsb lines: {:.2}% (floor {:.2}%; measured baseline {:.2}%)",
            nsb_lines.percent, nsb_floor, policy.measured.nsb_lines
        ),
        format!(
            "workspace functions: {:.2}% (diagnostic; not blocking)",
            workspace_functions.percent
        ),
        format!(
            "workspace regions: {:.2}% (diagnostic; not blocking)",
            workspace_regions.percent
        ),
        format!(
            "nsb functions: {:.2}%  nsb regions: {:.2}% (diagnostic)",
            nsb_functions.percent, nsb_regions.percent
        ),
        format!(
            "nsb-cli lines: {:.2}% functions: {:.2}% regions: {:.2}% (recorded, not a separate floor)",
            nsb_cli_lines.percent, nsb_cli_functions.percent, nsb_cli_regions.percent
        ),
        format!(
            "nsb-data-tools lines: {:.2}% functions: {:.2}% regions: {:.2}% (recorded, not a separate floor)",
            tools_lines.percent, tools_functions.percent, tools_regions.percent
        ),
    ];

    let mut failed = false;
    if workspace_lines.count == 0 {
        failed = true;
        lines.push("FAIL: coverage report contains no instrumented lines".to_string());
    }
    if !nsb.is_present() {
        failed = true;
        lines.push(
            "FAIL: no nsb coverage data in the report (fail-closed; missing crates/nsb files or instrumented lines)"
                .to_string(),
        );
    }
    if workspace_lines.count > 0 && workspace_lines.percent + 1e-9 < workspace_floor {
        failed = true;
        lines.push(format!(
            "FAIL: workspace line coverage {:.2}% is below the floor {:.2}%",
            workspace_lines.percent, workspace_floor
        ));
    }
    if nsb.is_present() && nsb_lines.percent + 1e-9 < nsb_floor {
        failed = true;
        lines.push(format!(
            "FAIL: nsb line coverage {:.2}% is below the floor {:.2}%",
            nsb_lines.percent, nsb_floor
        ));
    }

    CheckOutcome {
        kind: CheckKind::Overall,
        status: if failed {
            CheckStatus::Fail
        } else {
            CheckStatus::Pass
        },
        lines,
        uncovered: Vec::new(),
        missing_files: Vec::new(),
    }
}

/// Enforce changed-production-line coverage against git/diff input.
pub fn check_diff(
    policy: &CoveragePolicy,
    report: &CoverageReport,
    changed: &[ChangedLine],
    options: &GateOptions,
) -> CheckOutcome {
    check_diff_with_sources(policy, report, changed, options, load_workspace_source)
}

/// Like [`check_diff`], but loads file text through `load_source` so tests can
/// supply in-memory fixtures without touching the workspace tree.
pub fn check_diff_with_sources<F>(
    policy: &CoveragePolicy,
    report: &CoverageReport,
    changed: &[ChangedLine],
    options: &GateOptions,
    mut load_source: F,
) -> CheckOutcome
where
    F: FnMut(&str) -> Option<String>,
{
    let floor = options
        .diff_lines_floor
        .unwrap_or(policy.diff.changed_production_lines);
    let grouped = group_by_path(changed);
    let mut executable = 0u64;
    let mut covered = 0u64;
    let mut uncovered = Vec::new();
    let mut missing_files = Vec::new();
    let mut ignored_test_files = 0usize;
    let mut ignored_inline_test_lines = 0usize;

    for (path, lines) in grouped {
        if !path.ends_with(".rs") {
            continue;
        }
        if !is_production_rust_file(&path) {
            ignored_test_files += 1;
            continue;
        }

        // Each grouped path is visited once, so keep the exact source alongside
        // the cfg(test) scan. Missing source remains fail-closed for ambiguous
        // lines, while self-evident declarations can still be classified from
        // their diff text alone.
        let source = load_source(&path);
        let test_only = source
            .as_deref()
            .map(cfg_test_line_numbers)
            .unwrap_or_default();
        let production_lines: Vec<&&ChangedLine> = lines
            .iter()
            .filter(|line| {
                if test_only.contains(&line.line) {
                    ignored_inline_test_lines += 1;
                    false
                } else {
                    true
                }
            })
            .collect();
        if production_lines.is_empty() {
            continue;
        }
        let Some(file) = report.files.get(&path) else {
            // Absent from LCOV: declaration syntax that is unambiguous in the
            // changed line itself remains non-instrumentable without source.
            // Only ambiguous continuation lines require HEAD source context.
            let declaration_only = production_lines.iter().all(|line| {
                is_likely_non_instrumentable_rust_line(&line.text)
                    || source.as_deref().is_some_and(|source| {
                        is_likely_non_instrumentable_rust_line_with_context(
                            source, line.line, &line.text,
                        )
                    })
            });
            if !declaration_only {
                missing_files.push(path);
            }
            continue;
        };
        for line in production_lines {
            match file.line_hits.get(&line.line).copied() {
                None => {}
                Some(0) => {
                    executable += 1;
                    uncovered.push(format!("{path}:{}", line.line));
                }
                Some(_) => {
                    executable += 1;
                    covered += 1;
                }
            }
        }
    }

    let percent = if executable == 0 {
        100.0
    } else {
        (covered as f64) * 100.0 / (executable as f64)
    };

    let mut lines = vec![
        format!(
            "diff production lines: {:.2}% ({covered}/{executable} executable changed lines; floor {:.2}%)",
            percent, floor
        ),
        format!("ignored non-production/test Rust files: {ignored_test_files}"),
        format!("ignored inline #[cfg(test)] lines: {ignored_inline_test_lines}"),
    ];
    let mut failed = false;
    if !missing_files.is_empty() {
        failed = true;
        lines.push(format!(
            "FAIL: {} changed production file(s) have no coverage information",
            missing_files.len()
        ));
    }
    if percent + 1e-9 < floor {
        failed = true;
        lines.push(format!(
            "FAIL: changed production line coverage {:.2}% is below the floor {:.2}%",
            percent, floor
        ));
    }
    if executable == 0 && missing_files.is_empty() {
        lines.push("no executable changed production lines; diff gate passes".to_string());
    }

    CheckOutcome {
        kind: CheckKind::Diff,
        status: if failed {
            CheckStatus::Fail
        } else {
            CheckStatus::Pass
        },
        lines,
        uncovered,
        missing_files,
    }
}

fn load_workspace_source(path: &str) -> Option<String> {
    let candidate = Path::new(path);
    if candidate.is_file() {
        return std::fs::read_to_string(candidate).ok();
    }
    None
}
