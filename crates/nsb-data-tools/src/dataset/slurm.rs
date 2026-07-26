use super::config::RunConfig;
use super::engine::{read_manifest, write_manifest};
use super::model::{BuildPlan, RunStatus};
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn submit(
    config_path: &Path,
    config: &RunConfig,
    plan: &BuildPlan,
    manifest_path: &Path,
) -> Result<()> {
    if plan.partitions.is_empty() {
        bail!("Slurm execution requires partitioned starlight sources");
    }
    let slurm = config
        .execution
        .slurm
        .as_ref()
        .context("execution.slurm is required for the Slurm executor")?;
    let executable = std::env::current_exe()?;
    let partition_file = config.workspace.root.join("slurm-partitions.txt");
    std::fs::create_dir_all(
        partition_file
            .parent()
            .context("partition file has no parent")?,
    )?;
    std::fs::write(&partition_file, plan.partitions.join("\n"))?;
    let array = format!(
        "0-{}%{}",
        plan.partitions.len() - 1,
        slurm.array_parallelism.max(1)
    );
    let wrapped = format!(
        "PARTITION=$(sed -n \"$((SLURM_ARRAY_TASK_ID+1))p\" '{}'); '{}' _worker --config '{}' --dataset {} --operation {} --partition \"$PARTITION\"",
        partition_file.display(),
        executable.display(),
        config_path.display(),
        plan.dataset,
        plan.operation
    );
    let mut command = Command::new("sbatch");
    command.args(["--parsable", "--array", &array, "--wrap", &wrapped]);
    if let Some(value) = &slurm.partition {
        command.args(["--partition", value]);
    }
    if let Some(value) = &slurm.account {
        command.args(["--account", value]);
    }
    if let Some(value) = &slurm.time_limit {
        command.args(["--time", value]);
    }
    if let Some(value) = &slurm.memory {
        command.args(["--mem", value]);
    }
    let output = command.output().context("failed to launch sbatch")?;
    if !output.status.success() {
        bail!("sbatch failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    let job_id = String::from_utf8(output.stdout)?.trim().to_string();
    let mut manifest = read_manifest(manifest_path)?;
    manifest.status = RunStatus::Submitted;
    manifest.slurm_job_id = Some(job_id.clone());
    write_manifest(manifest_path, &manifest)?;
    println!("{job_id}");
    Ok(())
}
