use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

use crate::paths::{repo_relative, workspace_crate};

/// Parsed `cargo llvm-cov --json` / llvm-cov export report.
#[derive(Debug, Clone)]
pub struct CoverageReport {
    pub lines: Metric,
    pub functions: Metric,
    pub regions: Metric,
    pub files: BTreeMap<String, FileCoverage>,
}

/// Line/function/region totals.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Metric {
    pub count: u64,
    pub covered: u64,
    pub percent: f64,
}

impl Metric {
    pub fn from_counts(covered: u64, count: u64) -> Self {
        let percent = if count == 0 {
            100.0
        } else {
            (covered as f64) * 100.0 / (count as f64)
        };
        Self {
            count,
            covered,
            percent,
        }
    }
}

/// Per-file llvm-cov data used by both overall and diff gates.
#[derive(Debug, Clone)]
pub struct FileCoverage {
    pub relative_path: String,
    pub crate_name: Option<String>,
    pub lines: Metric,
    pub functions: Metric,
    pub regions: Metric,
    /// Instrumented lines mapped to hit counts. Absent lines are non-executable.
    pub line_hits: BTreeMap<u32, u64>,
}

/// JSON load/parse failures.
#[derive(Debug, Error)]
pub enum LlvmError {
    #[error("failed to read coverage JSON {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid llvm-cov JSON: {0}")]
    Parse(String),
}

#[derive(Debug, Deserialize)]
struct Export {
    data: Vec<ExportData>,
}

#[derive(Debug, Deserialize)]
struct ExportData {
    #[serde(default)]
    files: Vec<ExportFile>,
    totals: ExportSummary,
}

#[derive(Debug, Deserialize)]
struct ExportFile {
    filename: String,
    summary: ExportSummary,
}

#[derive(Debug, Deserialize)]
struct ExportSummary {
    lines: ExportMetric,
    #[serde(default)]
    functions: Option<ExportMetric>,
    #[serde(default)]
    regions: Option<ExportMetric>,
}

#[derive(Debug, Deserialize)]
struct ExportMetric {
    count: u64,
    covered: u64,
    #[serde(default)]
    percent: f64,
}

fn metric_from_export(metric: &ExportMetric) -> Metric {
    let percent = if metric.percent == 0.0 && metric.count > 0 {
        Metric::from_counts(metric.covered, metric.count).percent
    } else {
        metric.percent
    };
    Metric {
        count: metric.count,
        covered: metric.covered,
        percent,
    }
}

/// Load llvm-cov JSON export from disk (function/region diagnostics).
pub fn load_report(path: &Path) -> Result<CoverageReport, LlvmError> {
    let bytes = std::fs::read(path).map_err(|error| LlvmError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    parse_report(&bytes)
}

pub fn parse_report(bytes: &[u8]) -> Result<CoverageReport, LlvmError> {
    let export: Export =
        serde_json::from_slice(bytes).map_err(|error| LlvmError::Parse(error.to_string()))?;
    let data = export
        .data
        .into_iter()
        .next()
        .ok_or_else(|| LlvmError::Parse("coverage export contains no data records".into()))?;
    let mut files = BTreeMap::new();
    for file in data.files {
        let relative = repo_relative(&file.filename);
        let coverage = FileCoverage {
            crate_name: workspace_crate(&relative).map(str::to_string),
            relative_path: relative.clone(),
            lines: metric_from_export(&file.summary.lines),
            functions: file
                .summary
                .functions
                .as_ref()
                .map(metric_from_export)
                .unwrap_or_default(),
            regions: file
                .summary
                .regions
                .as_ref()
                .map(metric_from_export)
                .unwrap_or_default(),
            line_hits: BTreeMap::new(),
        };
        files.insert(relative, coverage);
    }
    Ok(CoverageReport {
        lines: metric_from_export(&data.totals.lines),
        functions: data
            .totals
            .functions
            .as_ref()
            .map(metric_from_export)
            .unwrap_or_default(),
        regions: data
            .totals
            .regions
            .as_ref()
            .map(metric_from_export)
            .unwrap_or_default(),
        files,
    })
}

/// Workspace package totals from already-collected file summaries.
pub fn crate_metrics(report: &CoverageReport, crate_name: &str) -> CrateCoverage {
    let mut files = 0u64;
    let mut lines_covered = 0;
    let mut lines_count = 0;
    let mut fn_covered = 0;
    let mut fn_count = 0;
    let mut region_covered = 0;
    let mut region_count = 0;
    for file in report.files.values() {
        if file.crate_name.as_deref() != Some(crate_name) {
            continue;
        }
        files += 1;
        lines_covered += file.lines.covered;
        lines_count += file.lines.count;
        fn_covered += file.functions.covered;
        fn_count += file.functions.count;
        region_covered += file.regions.covered;
        region_count += file.regions.count;
    }
    CrateCoverage {
        files,
        lines: Metric::from_counts(lines_covered, lines_count),
        functions: Metric::from_counts(fn_covered, fn_count),
        regions: Metric::from_counts(region_covered, region_count),
    }
}

/// Aggregated coverage for one workspace crate.
#[derive(Debug, Clone, Copy)]
pub struct CrateCoverage {
    pub files: u64,
    pub lines: Metric,
    pub functions: Metric,
    pub regions: Metric,
}

impl CrateCoverage {
    /// Fail-closed: a crate with no files or no instrumented lines is missing.
    pub fn is_present(&self) -> bool {
        self.files > 0 && self.lines.count > 0
    }
}
