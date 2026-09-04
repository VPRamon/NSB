//! Git helpers used by the public API gate.

use std::path::Path;
use std::process::{Command, Output};

use thiserror::Error;

use crate::base::{resolve_local_base_candidate, BaseDecision, BaseError, BaseInput};

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git command failed: {0}")]
    Command(String),
    #[error(transparent)]
    Base(#[from] BaseError),
}

fn run_git(repo: &Path, args: &[&str]) -> Result<Output, GitError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|error| GitError::Command(format!("failed to spawn git: {error}")))?;
    Ok(output)
}

fn git_stdout(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = run_git(repo, args)?;
    if !output.status.success() {
        return Err(GitError::Command(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolve `rev` to a full commit SHA, or `None` if it is not a commit.
pub fn resolve_commit(repo: &Path, rev: &str) -> Option<String> {
    git_stdout(
        repo,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )
    .ok()
}

/// Current `HEAD` commit SHA.
pub fn head_sha(repo: &Path) -> Result<String, GitError> {
    git_stdout(repo, &["rev-parse", "HEAD"])
}

/// Whether `path` exists in tree `rev`.
pub fn path_exists_at(repo: &Path, rev: &str, path: &str) -> bool {
    run_git(repo, &["cat-file", "-e", &format!("{rev}:{path}")])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Resolve the SemVer base for this repository checkout.
pub fn resolve_base_decision(
    repo: &Path,
    explicit_base: Option<String>,
    explicit: bool,
) -> Result<BaseDecision, GitError> {
    let head = head_sha(repo)?;
    let requested = if explicit {
        explicit_base
    } else {
        let origin_main = resolve_commit(repo, "origin/main");
        let merge_base = origin_main
            .as_ref()
            .and_then(|_| git_stdout(repo, &["merge-base", "HEAD", "origin/main"]).ok());
        let parent = resolve_commit(repo, "HEAD~1");
        resolve_local_base_candidate(
            &head,
            origin_main.as_deref(),
            merge_base.as_deref(),
            parent.as_deref(),
        )
    };

    let input = BaseInput {
        explicit_base: requested,
        head_sha: head,
        explicit,
    };
    Ok(crate::base::decide_base(&input, |rev| {
        resolve_commit(repo, rev)
    })?)
}
