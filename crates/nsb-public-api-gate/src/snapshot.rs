//! Snapshot generation and integrity checks via `cargo-public-api`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

pub const DEFAULT_SNAPSHOT_PATH: &str = "crates/nsb/api/public-api.txt";
pub const DEFAULT_PUBLIC_API_VERSION: &str = "0.50.1";
pub const DEFAULT_NIGHTLY: &str = "nightly-2026-09-02";

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("{0}")]
    Message(String),
}

pub struct ToolConfig {
    pub nightly: String,
    pub public_api_version: String,
    pub snapshot_path: PathBuf,
}

impl ToolConfig {
    pub fn from_env(repo: &Path) -> Self {
        Self {
            nightly: std::env::var("NSB_PUBLIC_API_RUSTDOC_TOOLCHAIN")
                .unwrap_or_else(|_| DEFAULT_NIGHTLY.to_string()),
            public_api_version: std::env::var("NSB_PUBLIC_API_TOOL_VERSION")
                .unwrap_or_else(|_| DEFAULT_PUBLIC_API_VERSION.to_string()),
            snapshot_path: repo.join(DEFAULT_SNAPSHOT_PATH),
        }
    }
}

/// Ensure `cargo-public-api` is on PATH at the pinned version.
pub fn ensure_cargo_public_api(version: &str) -> Result<(), SnapshotError> {
    let installed = Command::new("cargo")
        .args(["public-api", "--version"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.split_whitespace().nth(1).map(str::to_string)
        });
    if installed.as_deref() == Some(version) {
        return Ok(());
    }
    let status = Command::new("cargo")
        .args([
            "install",
            "cargo-public-api",
            "--locked",
            "--version",
            version,
            "--force",
        ])
        .status()
        .map_err(|error| SnapshotError::Message(format!("cargo install failed: {error}")))?;
    if !status.success() {
        return Err(SnapshotError::Message(format!(
            "failed to install cargo-public-api {version}"
        )));
    }
    Ok(())
}

/// Generate the public API listing for crate `nsb`.
pub fn generate_snapshot(repo: &Path, nightly: &str) -> Result<String, SnapshotError> {
    let output = Command::new("cargo")
        .args([
            &format!("+{nightly}"),
            "public-api",
            "-p",
            "nsb",
            "-sss",
            "--all-features",
            "--color=never",
        ])
        .current_dir(repo)
        .env(
            "RUSTDOCFLAGS",
            std::env::var_os("RUSTDOCFLAGS").unwrap_or_default(),
        )
        .output()
        .map_err(|error| {
            SnapshotError::Message(format!("failed to run cargo public-api: {error}"))
        })?;
    if !output.status.success() {
        return Err(SnapshotError::Message(format!(
            "cargo public-api failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Fail closed if the committed snapshot is missing, empty, or malformed.
pub fn validate_snapshot_file(path: &Path) -> Result<String, SnapshotError> {
    let text = fs::read_to_string(path).map_err(|_| {
        SnapshotError::Message(format!(
            "missing or empty public API snapshot: {}",
            path.display()
        ))
    })?;
    if text.trim().is_empty() {
        return Err(SnapshotError::Message(format!(
            "missing or empty public API snapshot: {}",
            path.display()
        )));
    }
    if !text.contains("pub ") {
        return Err(SnapshotError::Message(format!(
            "malformed public API snapshot (no exported items): {}",
            path.display()
        )));
    }
    Ok(text)
}

/// Compare committed snapshot to the API generated from HEAD.
pub fn check_snapshot_matches_head(repo: &Path, config: &ToolConfig) -> Result<(), SnapshotError> {
    let committed = validate_snapshot_file(&config.snapshot_path)?;
    let generated = generate_snapshot(repo, &config.nightly)?;
    if committed == generated {
        return Ok(());
    }
    let diff = unified_diff(&committed, &generated, DEFAULT_SNAPSHOT_PATH, "generated");
    Err(SnapshotError::Message(format!(
        "{diff}\npublic API snapshot drift: run `cargo run -p nsb-public-api-gate -- --write`"
    )))
}

/// Run historical `cargo public-api diff $base..HEAD`.
pub fn check_historical_diff(repo: &Path, nightly: &str, base: &str) -> Result<(), SnapshotError> {
    eprintln!("SemVer gate: cargo public-api diff {base}..HEAD (deny removed/changed)");
    let status = Command::new("cargo")
        .args([
            &format!("+{nightly}"),
            "public-api",
            "diff",
            &format!("{base}..HEAD"),
            "-p",
            "nsb",
            "-sss",
            "--all-features",
            "--deny=removed",
            "--deny=changed",
            "--color=never",
        ])
        .current_dir(repo)
        .env(
            "RUSTDOCFLAGS",
            std::env::var_os("RUSTDOCFLAGS").unwrap_or_default(),
        )
        .status()
        .map_err(|error| {
            SnapshotError::Message(format!("failed to run cargo public-api diff: {error}"))
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(SnapshotError::Message(format!(
            "SemVer gate failed for {base}..HEAD (deny removed/changed)"
        )))
    }
}

fn unified_diff(left: &str, right: &str, left_name: &str, right_name: &str) -> String {
    let left_lines: Vec<&str> = left.lines().collect();
    let right_lines: Vec<&str> = right.lines().collect();
    let mut out = format!("--- {left_name}\n+++ {right_name}\n");
    let max = left_lines.len().max(right_lines.len());
    for index in 0..max {
        match (left_lines.get(index), right_lines.get(index)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                out.push_str(&format!("-{a}\n+{b}\n"));
            }
            (Some(a), None) => out.push_str(&format!("-{a}\n")),
            (None, Some(b)) => out.push_str(&format!("+{b}\n")),
            (None, None) => {}
        }
    }
    out
}
