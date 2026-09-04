//! Top-level check / write orchestration.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::base::{decide_historical_mode, BaseDecision, HistoricalMode};
use crate::compat::{reject_removed_compat_apis, CompatError};
use crate::git::{path_exists_at, resolve_base_decision, GitError};
use crate::snapshot::{
    check_historical_diff, check_snapshot_matches_head, ensure_cargo_public_api, generate_snapshot,
    validate_snapshot_file, SnapshotError, ToolConfig, DEFAULT_SNAPSHOT_PATH,
};

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    pub repo: PathBuf,
    pub write: bool,
    pub base: Option<String>,
    pub base_explicit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateOutcome {
    pub status: GateStatus,
    pub message: String,
    pub historical: Option<HistoricalMode>,
}

#[derive(Debug, Error)]
pub enum GateError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Compat(#[from] CompatError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error("{0}")]
    Message(String),
}

/// Regenerate the committed public API snapshot.
pub fn run_write(options: &CheckOptions) -> Result<GateOutcome, GateError> {
    let config = ToolConfig::from_env(&options.repo);
    ensure_cargo_public_api(&config.public_api_version)?;
    if let Some(parent) = config.snapshot_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            GateError::Message(format!("failed to create {}: {error}", parent.display()))
        })?;
    }
    let generated = generate_snapshot(&options.repo, &config.nightly)?;
    std::fs::write(&config.snapshot_path, &generated).map_err(|error| {
        GateError::Message(format!(
            "failed to write {}: {error}",
            config.snapshot_path.display()
        ))
    })?;
    Ok(GateOutcome {
        status: GateStatus::Pass,
        message: format!("wrote {}", config.snapshot_path.display()),
        historical: None,
    })
}

/// Run snapshot integrity + historical SemVer checks.
pub fn run_check(options: &CheckOptions) -> Result<GateOutcome, GateError> {
    reject_removed_compat_apis(&options.repo)?;
    let config = ToolConfig::from_env(&options.repo);
    ensure_cargo_public_api(&config.public_api_version)?;
    validate_snapshot_file(&config.snapshot_path)?;
    check_snapshot_matches_head(&options.repo, &config)?;

    let decision =
        resolve_base_decision(&options.repo, options.base.clone(), options.base_explicit)?;
    let snapshot_at_base = match &decision {
        BaseDecision::BootstrapNoBase => false,
        BaseDecision::UseBase { rev } => path_exists_at(&options.repo, rev, DEFAULT_SNAPSHOT_PATH),
    };
    let mode = decide_historical_mode(decision, snapshot_at_base);
    match &mode {
        HistoricalMode::Bootstrap { reason } => {
            eprintln!("SemVer gate: {reason}");
        }
        HistoricalMode::Diff { base } => {
            check_historical_diff(&options.repo, &config.nightly, base)?;
        }
    }

    Ok(GateOutcome {
        status: GateStatus::Pass,
        message: "public API policy: OK".into(),
        historical: Some(mode),
    })
}

/// Discover the workspace root containing `crates/nsb/api/public-api.txt`.
pub fn discover_repo(start: &Path) -> Result<PathBuf, GateError> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(DEFAULT_SNAPSHOT_PATH).exists()
            || current.join("crates/nsb/Cargo.toml").exists()
        {
            return Ok(current);
        }
        if !current.pop() {
            return Err(GateError::Message(
                "could not locate NSB repository root".into(),
            ));
        }
    }
}
