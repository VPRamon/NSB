//! Local coverage floors and changed-production-code gates.
//!
//! The checker consumes `cargo llvm-cov report --json` output and the
//! repository-root `coverage-policy.toml`. It does not contact hosted coverage
//! services.

#![forbid(unsafe_code)]

mod check;
mod diff;
mod llvm;
mod paths;
mod policy;

pub use check::{check_diff, check_overall, CheckKind, CheckOutcome, CheckStatus};
pub use diff::{parse_unified_diff, ChangedLine, DiffError};
pub use llvm::{load_report, parse_report, CoverageReport, LlvmError};
pub use paths::{is_production_rust_file, repo_relative, workspace_crate};
pub use policy::{load_policy, parse_policy_str, CoveragePolicy, PolicyError};

use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// CLI options for a single gate invocation.
#[derive(Debug, Clone)]
pub struct GateOptions {
    /// Policy file. When `None`, search from the current directory.
    pub policy_path: Option<PathBuf>,
    /// `cargo llvm-cov report --json` output.
    pub report_path: PathBuf,
    /// Override workspace line floor (percent).
    pub workspace_lines_floor: Option<f64>,
    /// Override `nsb` line floor (percent).
    pub nsb_lines_floor: Option<f64>,
    /// Override changed-production line floor (percent).
    pub diff_lines_floor: Option<f64>,
    /// Git ref or SHA used as the diff base.
    pub base: Option<String>,
    /// Precomputed unified diff (`git diff -U0`) instead of invoking git.
    pub diff_file: Option<PathBuf>,
    /// GitHub Actions run URL or artifact hint appended to failures.
    pub artifact_hint: Option<String>,
}

impl Default for GateOptions {
    fn default() -> Self {
        Self {
            policy_path: None,
            report_path: PathBuf::from("coverage.json"),
            workspace_lines_floor: None,
            nsb_lines_floor: None,
            diff_lines_floor: None,
            base: None,
            diff_file: None,
            artifact_hint: None,
        }
    }
}

/// Failures while running a gate.
#[derive(Debug, Error)]
pub enum GateError {
    #[error(transparent)]
    Policy(#[from] PolicyError),
    #[error(transparent)]
    Llvm(#[from] LlvmError),
    #[error(transparent)]
    Diff(#[from] DiffError),
    #[error("failed to write output: {0}")]
    Io(#[from] std::io::Error),
}

/// Run the requested gate and write a human-readable report to `out`.
pub fn run(
    kind: CheckKind,
    options: &GateOptions,
    out: &mut impl Write,
) -> Result<CheckOutcome, GateError> {
    let policy = match &options.policy_path {
        Some(path) => load_policy(path)?,
        None => policy::load_policy_from_cwd()?,
    };
    let report = load_report(&options.report_path)?;
    match kind {
        CheckKind::Overall => {
            let outcome = check_overall(&policy, &report, options);
            write_outcome(out, &policy, &outcome, options.artifact_hint.as_deref())?;
            Ok(outcome)
        }
        CheckKind::Diff => {
            let changed = match &options.diff_file {
                Some(path) => {
                    let text = std::fs::read_to_string(path)?;
                    parse_unified_diff(&text)?
                }
                None => {
                    let base = options
                        .base
                        .clone()
                        .or_else(|| {
                            std::env::var("GITHUB_BASE_REF")
                                .ok()
                                .map(|r| format!("origin/{r}"))
                        })
                        .unwrap_or_else(|| policy.diff.base_ref.clone());
                    diff::changed_lines_from_git(&base)?
                }
            };
            let outcome = check_diff(&policy, &report, &changed, options);
            write_outcome(out, &policy, &outcome, options.artifact_hint.as_deref())?;
            Ok(outcome)
        }
    }
}

fn write_outcome(
    out: &mut impl Write,
    policy: &CoveragePolicy,
    outcome: &CheckOutcome,
    artifact_hint: Option<&str>,
) -> std::io::Result<()> {
    writeln!(out, "NSB coverage gate ({})", outcome.kind.as_str())?;
    writeln!(out, "baseline_kind: {}", policy.baseline_kind)?;
    writeln!(
        out,
        "baseline: {} ({})",
        policy.baseline.commit, policy.baseline.date
    )?;
    for line in &outcome.lines {
        writeln!(out, "{line}")?;
    }
    if !outcome.uncovered.is_empty() {
        writeln!(out, "uncovered changed production lines:")?;
        for item in &outcome.uncovered {
            writeln!(out, "  {item}")?;
        }
    }
    if !outcome.missing_files.is_empty() {
        writeln!(out, "changed production files with no coverage data:")?;
        for file in &outcome.missing_files {
            writeln!(out, "  {file}")?;
        }
    }
    let html = &policy.html_artifact_name;
    let json = &policy.json_artifact_name;
    match artifact_hint {
        Some(hint) => writeln!(
            out,
            "reports: HTML artifact `{html}`, JSON artifact `{json}` ({hint})"
        )?,
        None => writeln!(
            out,
            "reports: HTML artifact `{html}`, JSON artifact `{json}`"
        )?,
    }
    match outcome.status {
        CheckStatus::Pass => writeln!(out, "result: PASS")?,
        CheckStatus::Fail => writeln!(out, "result: FAIL")?,
    }
    Ok(())
}

/// Locate `coverage-policy.toml` starting at `start`.
pub fn find_policy_file(start: &Path) -> Option<PathBuf> {
    policy::find_policy_file(start)
}
