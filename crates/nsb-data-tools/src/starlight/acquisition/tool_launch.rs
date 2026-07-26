//! Launch hierarchical `nsb-data` actions from production orchestration.

use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve the supported `nsb-data` executable.
pub fn resolve_nsb_data_binary() -> PathBuf {
    if let Ok(path) = env::var("NSB_DATA_BINARY") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = env::current_exe() {
        if exe.file_name().and_then(|name| name.to_str()) == Some("nsb-data") {
            return exe;
        }
        if let Some(parent) = exe.parent() {
            let sibling = parent.join("nsb-data");
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("target/release/nsb-data")
}

fn command_for_action(action: &[&str]) -> Command {
    let binary = resolve_nsb_data_binary();
    let mut command = if binary.is_file() {
        Command::new(binary)
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
            "nsb-data",
            "--",
        ]);
        cargo
    };
    command.args(action);
    command
}

#[allow(clippy::too_many_arguments)]
fn append_partition_processor_args(
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
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--row-limit")
        .arg(row_limit.to_string())
        .arg("--batch-size")
        .arg(batch_size.to_string())
        .arg("--workers")
        .arg(workers.to_string())
        .arg("--checkpoint-interval")
        .arg(checkpoint_interval.to_string());
    if skip_normalized_output {
        command
            .arg("--skip-normalized-output")
            .arg("--light-checkpoint");
    }
    if let Some(policy) = frozen_policy {
        command.arg("--frozen-policy").arg(policy);
    }
    if let Some(environment) = gaiaxpy_environment {
        command.arg("--gaiaxpy-environment").arg(environment);
    }
    if resume {
        command.arg("--resume");
    }
}

/// Process one XP continuous partition through the hierarchical CLI.
#[allow(clippy::too_many_arguments)]
pub fn run_partition_processor_command(
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
    let mut command = command_for_action(&["starlight", "xp-continuous", "process-partition"]);
    append_partition_processor_args(
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
        .context("failed to launch XP partition processor")?;
    if !status.success() {
        anyhow::bail!("XP partition processor failed with status {status}");
    }
    Ok(())
}

/// Download one official partition through the hierarchical CLI.
pub fn run_download_bulk_command(
    filename: &str,
    usb_mountpoint: Option<&Path>,
    usb_cache_root: Option<&Path>,
    cache_subdir: &str,
    max_cache_bytes: u64,
    resume: bool,
) -> Result<()> {
    let mut command = command_for_action(&["starlight", "acquire", "xp-bulk", "download"]);
    if let Some(mount) = usb_mountpoint {
        command.arg("--usb-mountpoint").arg(mount);
    }
    if let Some(cache_root) = usb_cache_root {
        command.arg("--usb-cache-root").arg(cache_root);
    }
    command
        .arg("--cache-subdir")
        .arg(cache_subdir)
        .arg("--max-cache-bytes")
        .arg(max_cache_bytes.to_string())
        .arg("--only-filename")
        .arg(filename);
    if resume {
        command.arg("--resume");
    }
    let status = command
        .status()
        .context("failed to launch XP bulk download")?;
    if !status.success() {
        anyhow::bail!("XP bulk download failed for {filename} with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_targets_the_supported_binary() {
        assert!(resolve_nsb_data_binary().ends_with("nsb-data"));
    }
}
