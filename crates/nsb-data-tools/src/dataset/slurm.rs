use super::config::RunConfig;
use super::engine::{read_manifest, write_manifest};
use super::execution::scheduler::{ArrayRequest, Scheduler, SlurmScheduler};
use super::model::{BuildPlan, RunStatus};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

const ARRAY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartitionArray {
    schema_version: u32,
    partitions: Vec<String>,
}

pub fn submit(
    config_path: &Path,
    config: &RunConfig,
    plan: &BuildPlan,
    manifest_path: &Path,
) -> Result<()> {
    submit_with(
        &SlurmScheduler::default(),
        config_path,
        config,
        plan,
        manifest_path,
    )
}

fn submit_with(
    scheduler: &dyn Scheduler,
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
    let partition_file = config.workspace.root.join("slurm-partitions.json");
    super::engine::atomic_write(
        &partition_file,
        &serde_json::to_vec_pretty(&PartitionArray {
            schema_version: ARRAY_SCHEMA_VERSION,
            partitions: plan.partitions.clone(),
        })?,
    )?;
    let wrapped = format!(
        "exec {} _worker --config {} --dataset {} --operation {} --partition-manifest {}",
        shell_quote(&executable),
        shell_quote(config_path),
        plan.dataset,
        plan.operation,
        shell_quote(&partition_file),
    );
    let indices = (0..plan.partitions.len())
        .map(u32::try_from)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("too many partitions for Slurm array indices")?;
    let submission = scheduler.submit_array(&ArrayRequest {
        job_name: format!("nsb-{}-{}", plan.dataset, plan.operation),
        indices,
        max_parallel: slurm.array_parallelism.max(1),
        wrapped_command: wrapped,
        partition: slurm.partition.clone(),
        account: slurm.account.clone(),
        time_limit: slurm.time_limit.clone(),
        memory: slurm.memory.clone(),
    })?;

    let mut manifest = read_manifest(manifest_path)?;
    manifest.status = RunStatus::Submitted;
    manifest.slurm_job_id = Some(submission.job_id.clone());
    write_manifest(manifest_path, &manifest)?;
    println!("{}", submission.job_id);
    Ok(())
}

pub(crate) fn partition_from_array(path: &Path) -> Result<String> {
    let raw = std::fs::read(path)?;
    let manifest: PartitionArray = serde_json::from_slice(&raw)?;
    if manifest.schema_version != ARRAY_SCHEMA_VERSION {
        bail!(
            "unsupported partition-array schema {}",
            manifest.schema_version
        );
    }
    let index = std::env::var("SLURM_ARRAY_TASK_ID")
        .context("SLURM_ARRAY_TASK_ID is required for array worker")?
        .parse::<usize>()
        .context("SLURM_ARRAY_TASK_ID is not a valid index")?;
    manifest
        .partitions
        .get(index)
        .cloned()
        .with_context(|| format!("array task index {index} is outside partition manifest"))
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_handles_spaces_and_single_quotes() {
        assert_eq!(
            shell_quote(Path::new("/data/a b/c'd")),
            "'/data/a b/c'\"'\"'d'"
        );
    }
}
