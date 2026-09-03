use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

use crate::llvm::{CoverageReport, FileCoverage, Metric};
use crate::paths::{repo_relative, workspace_crate};

/// LCOV load/parse failures.
#[derive(Debug, Error)]
pub enum LcovError {
    #[error("failed to read coverage LCOV {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid LCOV: {0}")]
    Parse(String),
}

/// Load LLVM/cargo-llvm-cov LCOV, using `DA:line,hits` as the line-coverage source of truth.
pub fn load_lcov(path: &Path) -> Result<CoverageReport, LcovError> {
    let text = std::fs::read_to_string(path).map_err(|error| LcovError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    parse_lcov(&text)
}

/// Parse LCOV text exported by llvm-cov (`SF:` + `DA:line,hits`).
pub fn parse_lcov(text: &str) -> Result<CoverageReport, LcovError> {
    let mut files = BTreeMap::new();
    let mut current_path: Option<String> = None;
    let mut current_hits: BTreeMap<u32, u64> = BTreeMap::new();
    let mut current_fn_count = 0u64;
    let mut current_fn_covered = 0u64;

    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("TN:") {
            continue;
        }
        if let Some(filename) = line.strip_prefix("SF:") {
            flush_file(
                &mut files,
                current_path.take(),
                std::mem::take(&mut current_hits),
                current_fn_covered,
                current_fn_count,
            );
            current_fn_count = 0;
            current_fn_covered = 0;
            current_path = Some(repo_relative(filename.trim()));
            continue;
        }
        if line.starts_with("DA:") {
            let rest = line.strip_prefix("DA:").unwrap();
            let (line_str, hits_str) = rest.split_once(',').ok_or_else(|| {
                LcovError::Parse(format!("line {line_no}: DA record missing comma: {line}"))
            })?;
            let source_line: u32 = line_str.parse().map_err(|_| {
                LcovError::Parse(format!("line {line_no}: invalid DA line number {line_str}"))
            })?;
            let hits_field = hits_str.split(',').next().unwrap_or(hits_str);
            let hits: u64 = hits_field.parse().map_err(|_| {
                LcovError::Parse(format!("line {line_no}: invalid DA hit count {hits_field}"))
            })?;
            let entry = current_hits.entry(source_line).or_insert(0);
            *entry = (*entry).max(hits);
            continue;
        }
        if line.starts_with("FNDA:") {
            let rest = line.strip_prefix("FNDA:").unwrap();
            let (hits_str, _) = rest.split_once(',').ok_or_else(|| {
                LcovError::Parse(format!("line {line_no}: FNDA record missing comma: {line}"))
            })?;
            let hits: u64 = hits_str.parse().map_err(|_| {
                LcovError::Parse(format!("line {line_no}: invalid FNDA hit count {hits_str}"))
            })?;
            current_fn_count += 1;
            if hits > 0 {
                current_fn_covered += 1;
            }
            continue;
        }
        if line == "end_of_record" {
            flush_file(
                &mut files,
                current_path.take(),
                std::mem::take(&mut current_hits),
                current_fn_covered,
                current_fn_count,
            );
            current_fn_count = 0;
            current_fn_covered = 0;
        }
    }
    flush_file(
        &mut files,
        current_path.take(),
        current_hits,
        current_fn_covered,
        current_fn_count,
    );

    let mut lines_covered = 0;
    let mut lines_count = 0;
    let mut fn_covered = 0;
    let mut fn_count = 0;
    for file in files.values() {
        lines_covered += file.lines.covered;
        lines_count += file.lines.count;
        fn_covered += file.functions.covered;
        fn_count += file.functions.count;
    }

    Ok(CoverageReport {
        lines: Metric::from_counts(lines_covered, lines_count),
        functions: Metric::from_counts(fn_covered, fn_count),
        regions: Metric::default(),
        files,
    })
}

fn flush_file(
    files: &mut BTreeMap<String, FileCoverage>,
    path: Option<String>,
    line_hits: BTreeMap<u32, u64>,
    fn_covered: u64,
    fn_count: u64,
) {
    let Some(relative) = path else {
        return;
    };
    if relative.is_empty() {
        return;
    }
    let covered = line_hits.values().filter(|hits| **hits > 0).count() as u64;
    let count = line_hits.len() as u64;
    let coverage = FileCoverage {
        crate_name: workspace_crate(&relative).map(str::to_string),
        relative_path: relative.clone(),
        lines: Metric::from_counts(covered, count),
        functions: Metric::from_counts(fn_covered, fn_count),
        regions: Metric::default(),
        line_hits,
    };
    files.insert(relative, coverage);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn da_records_are_the_executable_line_source_of_truth() {
        let lcov = "\
SF:/repo/crates/nsb/src/lib.rs
DA:10,3
DA:11,0
DA:12,1
DA:10,1
end_of_record
";
        let report = parse_lcov(lcov).unwrap();
        let file = &report.files["crates/nsb/src/lib.rs"];
        assert_eq!(file.line_hits.get(&10), Some(&3));
        assert_eq!(file.line_hits.get(&11), Some(&0));
        assert_eq!(file.line_hits.get(&12), Some(&1));
        assert_eq!(file.line_hits.get(&99), None);
        assert_eq!(file.lines.covered, 2);
        assert_eq!(file.lines.count, 3);
    }
}
