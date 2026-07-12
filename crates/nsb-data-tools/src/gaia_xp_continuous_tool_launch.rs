//! Resolve release `nsb-data-tools` binaries for production orchestration.
//!
//! Prefer a sibling executable next to the current process image (typical under
//! `target/release/`). Fall back to `cargo run --release` when not built yet.

use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve `run_phase5b_mini_pilot` for production invocation.
pub fn resolve_mini_pilot_binary() -> PathBuf {
    resolve_tool_binary("run_phase5b_mini_pilot")
}

fn resolve_tool_binary(name: &str) -> PathBuf {
    let env_key = format!("NSB_{}", name.to_ascii_uppercase().replace('-', "_"));
    if let Ok(path) = env::var(&env_key) {
        return PathBuf::from(path);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sibling = parent.join(name);
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("target/release").join(name)
}

/// Return true when the resolved release binary exists and is executable.
pub fn release_binary_available(path: &Path) -> bool {
    path.is_file()
}

/// Append common mini-pilot arguments to `command`.
#[allow(clippy::too_many_arguments)]
pub fn append_mini_pilot_args(
    command: &mut Command,
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    workers: usize,
    skip_normalized_output: bool,
    resume: bool,
    frozen_policy: Option<&Path>,
    gaiaxpy_environment: Option<&Path>,
    checkpoint_interval: usize,
) {
    command
        .arg("--bulk-gz")
        .arg(bulk_gz)
        .args(["--output-dir"])
        .arg(output_dir)
        .args(["--row-limit"])
        .arg(row_limit.to_string())
        .args(["--batch-size"])
        .arg(batch_size.to_string())
        .args(["--workers"])
        .arg(workers.to_string())
        .args(["--checkpoint-interval"])
        .arg(checkpoint_interval.to_string());
    if skip_normalized_output {
        command.arg("--skip-normalized-output");
        command.arg("--light-checkpoint");
    }
    if let Some(policy) = frozen_policy {
        command.args(["--frozen-policy"]).arg(policy);
    }
    if let Some(env) = gaiaxpy_environment {
        command.args(["--gaiaxpy-environment"]).arg(env);
    }
    if resume {
        command.arg("--resume");
    }
}

/// Launch mini-pilot using the release binary when available.
#[allow(clippy::too_many_arguments)]
pub fn run_mini_pilot_command(
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    workers: usize,
    skip_normalized_output: bool,
    resume: bool,
    frozen_policy: Option<&Path>,
    gaiaxpy_environment: Option<&Path>,
    checkpoint_interval: usize,
) -> Result<()> {
    let binary = resolve_mini_pilot_binary();
    let mut command = if release_binary_available(&binary) {
        Command::new(&binary)
    } else {
        let mut cargo = Command::new("cargo");
        cargo.args([
            "run",
            "--release",
            "--locked",
            "-q",
            "-p",
            "nsb-data-tools",
            "--bin",
            "run_phase5b_mini_pilot",
            "--",
        ]);
        cargo
    };
    append_mini_pilot_args(
        &mut command,
        bulk_gz,
        output_dir,
        row_limit,
        batch_size,
        workers,
        skip_normalized_output,
        resume,
        frozen_policy,
        gaiaxpy_environment,
        checkpoint_interval,
    );
    let status = command
        .status()
        .with_context(|| format!("failed to launch mini-pilot ({binary:?})"))?;
    if !status.success() {
        anyhow::bail!("run_phase5b_mini_pilot failed with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mini_pilot_returns_path() {
        let path = resolve_mini_pilot_binary();
        assert!(
            path.ends_with("run_phase5b_mini_pilot"),
            "unexpected path {}",
            path.display()
        );
    }
}
