//! Guard against deliberately removed compatibility-only symbols.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

const FORBIDDEN_PATTERNS: &[&str] = &[
    "ALL_SUPPORTED",
    "python_parity",
    "periods_below_threshold_legacy",
    "#[deprecated]",
];

#[derive(Debug, Error)]
pub enum CompatError {
    #[error("removed or compatibility-only API found in production source:\n{0}")]
    Found(String),
    #[error("failed to scan production sources: {0}")]
    Io(String),
}

/// Fail if forbidden compatibility symbols reappear under production crate sources.
pub fn reject_removed_compat_apis(repo: &Path) -> Result<(), CompatError> {
    let crates_dir = repo.join("crates");
    let mut hits = Vec::new();
    let entries = fs::read_dir(&crates_dir)
        .map_err(|error| CompatError::Io(format!("read {}: {error}", crates_dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| CompatError::Io(error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Gate/tooling crates may mention forbidden tokens in policy source.
        if matches!(name.as_ref(), "nsb-public-api-gate" | "nsb-coverage-gate") {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            visit(&src, &mut hits)?;
        }
    }
    if hits.is_empty() {
        Ok(())
    } else {
        Err(CompatError::Found(hits.join("\n")))
    }
}

fn visit(path: &Path, hits: &mut Vec<String>) -> Result<(), CompatError> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| CompatError::Io(error.to_string()))? {
            let entry = entry.map_err(|error| CompatError::Io(error.to_string()))?;
            visit(&entry.path(), hits)?;
        }
        return Ok(());
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(|error| CompatError::Io(error.to_string()))?;
    for (index, line) in text.lines().enumerate() {
        for pattern in FORBIDDEN_PATTERNS {
            if line.contains(pattern) {
                hits.push(format!("{}:{}:{line}", display_repo_path(path), index + 1));
            }
        }
    }
    Ok(())
}

fn display_repo_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<PathBuf>()
        .display()
        .to_string()
}
