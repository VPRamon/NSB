use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Version-controlled coverage policy loaded by CI and local checks.
#[derive(Debug, Clone, Deserialize)]
pub struct CoveragePolicy {
    pub schema_version: u32,
    pub baseline_kind: String,
    pub html_artifact_name: String,
    pub json_artifact_name: String,
    #[serde(default)]
    pub cobertura_artifact_name: String,
    #[serde(default = "default_lcov_artifact")]
    pub lcov_artifact_name: String,
    pub baseline: Baseline,
    pub measured: MeasuredCoverage,
    pub floors: Floors,
    pub diff: DiffPolicy,
    pub exclusions: Exclusions,
}

fn default_lcov_artifact() -> String {
    "coverage-lcov".to_string()
}

fn default_nightly_toolchain() -> String {
    "nightly-2026-09-02".to_string()
}

/// Recorded measurement identity for the approved baseline.
#[derive(Debug, Clone, Deserialize)]
pub struct Baseline {
    pub commit: String,
    pub date: String,
    pub rust_nightly: String,
    #[serde(default = "default_nightly_toolchain")]
    pub rust_nightly_toolchain: String,
    pub cargo_llvm_cov: String,
    pub command: String,
}

/// Observed coverage at the recorded baseline (diagnostic, not the gate).
#[derive(Debug, Clone, Deserialize)]
pub struct MeasuredCoverage {
    pub workspace_lines: f64,
    pub workspace_functions: f64,
    pub workspace_regions: f64,
    pub nsb_lines: f64,
    pub nsb_functions: f64,
    pub nsb_regions: f64,
    pub nsb_cli_lines: f64,
    pub nsb_cli_functions: f64,
    pub nsb_cli_regions: f64,
    pub nsb_data_tools_lines: f64,
    pub nsb_data_tools_functions: f64,
    pub nsb_data_tools_regions: f64,
}

/// Blocking line-coverage floors.
#[derive(Debug, Clone, Deserialize)]
pub struct Floors {
    pub workspace_lines: f64,
    pub nsb_lines: f64,
}

/// Changed-production-code gate.
#[derive(Debug, Clone, Deserialize)]
pub struct DiffPolicy {
    pub changed_production_lines: f64,
    pub base_ref: String,
}

/// Approved exclusions. Empty means none.
#[derive(Debug, Clone, Deserialize)]
pub struct Exclusions {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

/// Policy load failures.
#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("coverage-policy.toml not found from {}", .0.display())]
    NotFound(PathBuf),
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid coverage policy: {0}")]
    Parse(String),
}

/// Parse policy TOML from a string.
pub fn parse_policy_str(text: &str) -> Result<CoveragePolicy, PolicyError> {
    let policy: CoveragePolicy =
        toml::from_str(text).map_err(|error| PolicyError::Parse(error.to_string()))?;
    if policy.schema_version != 1 {
        return Err(PolicyError::Parse(format!(
            "unsupported coverage policy schema {}",
            policy.schema_version
        )));
    }
    if !policy.exclusions.files.is_empty() {
        return Err(PolicyError::Parse(
            "exclusions.files is not empty; nonempty exclusion lists are not supported and would be silently ignored. Leave the list empty until an exclusion is implemented and reviewed"
                .into(),
        ));
    }
    validate_policy_percents(&policy)?;
    Ok(policy)
}

fn validate_policy_percents(policy: &CoveragePolicy) -> Result<(), PolicyError> {
    let measured = &policy.measured;
    for (name, value) in [
        ("measured.workspace_lines", measured.workspace_lines),
        ("measured.workspace_functions", measured.workspace_functions),
        ("measured.workspace_regions", measured.workspace_regions),
        ("measured.nsb_lines", measured.nsb_lines),
        ("measured.nsb_functions", measured.nsb_functions),
        ("measured.nsb_regions", measured.nsb_regions),
        ("measured.nsb_cli_lines", measured.nsb_cli_lines),
        ("measured.nsb_cli_functions", measured.nsb_cli_functions),
        ("measured.nsb_cli_regions", measured.nsb_cli_regions),
        (
            "measured.nsb_data_tools_lines",
            measured.nsb_data_tools_lines,
        ),
        (
            "measured.nsb_data_tools_functions",
            measured.nsb_data_tools_functions,
        ),
        (
            "measured.nsb_data_tools_regions",
            measured.nsb_data_tools_regions,
        ),
        ("floors.workspace_lines", policy.floors.workspace_lines),
        ("floors.nsb_lines", policy.floors.nsb_lines),
        (
            "diff.changed_production_lines",
            policy.diff.changed_production_lines,
        ),
    ] {
        validate_percent(name, value)?;
    }
    Ok(())
}

/// Reject non-finite percentages outside `[0, 100]`.
pub fn validate_percent(name: &str, value: f64) -> Result<f64, PolicyError> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(PolicyError::Parse(format!(
            "{name} must be a finite percentage in [0, 100], got {value}"
        )));
    }
    Ok(value)
}

/// Load a policy file from an explicit path.
pub fn load_policy(path: &Path) -> Result<CoveragePolicy, PolicyError> {
    let text = std::fs::read_to_string(path).map_err(|error| PolicyError::Io {
        path: path.to_path_buf(),
        source: error,
    })?;
    parse_policy_str(&text)
}

pub(crate) fn load_policy_from_cwd() -> Result<CoveragePolicy, PolicyError> {
    let cwd = std::env::current_dir().map_err(|error| PolicyError::Io {
        path: PathBuf::from("."),
        source: error,
    })?;
    let path = find_policy_file(&cwd).ok_or_else(|| PolicyError::NotFound(cwd.clone()))?;
    load_policy(&path)
}

pub(crate) fn find_policy_file(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join("coverage-policy.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
