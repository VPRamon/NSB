// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::config::RunConfig;
use super::engine::{read_manifest, write_manifest};
use super::execution::scheduler::{
    aggregate_states, ArrayRequest, Scheduler, SchedulerState, SlurmScheduler,
};
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

pub(crate) fn scheduler_state(job_ids: &str) -> Result<SchedulerState> {
    let scheduler = SlurmScheduler::default();
    let states = job_ids
        .split(',')
        .map(str::trim)
        .filter(|job_id| !job_id.is_empty())
        .map(|job_id| scheduler.state(job_id))
        .collect::<Result<Vec<_>>>()?;
    Ok(aggregate_states(&states))
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
    let chunk_size = slurm.max_array_size.max(1);
    let mut job_ids = Vec::new();
    for (chunk_index, chunk) in plan.partitions.chunks(chunk_size).enumerate() {
        let partition_file = config
            .workspace
            .root
            .join(format!("slurm-partitions-{chunk_index:03}.json"));
        super::engine::atomic_write(
            &partition_file,
            &serde_json::to_vec_pretty(&PartitionArray {
                schema_version: ARRAY_SCHEMA_VERSION,
                partitions: chunk.to_vec(),
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
        let indices = (0..chunk.len())
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
        job_ids.push(submission.job_id);
    }

    let mut manifest = read_manifest(manifest_path)?;
    manifest.status = RunStatus::Submitted;
    manifest.slurm_job_id = Some(job_ids.join(","));
    write_manifest(manifest_path, &manifest)?;
    println!("{}", manifest.slurm_job_id.as_deref().unwrap_or_default());
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


    use super::super::config::{ExecutionConfig, SlurmConfig, WorkspaceConfig};
    use super::super::model::{DatasetName, Executor, Operation, RunManifest, RUN_SCHEMA_VERSION};
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingScheduler {
        requests: Mutex<Vec<ArrayRequest>>,
    }

    impl Scheduler for RecordingScheduler {
        fn submit_array(&self, request: &ArrayRequest) -> Result<super::super::execution::scheduler::JobSubmission> {
            let mut requests = self.requests.lock().unwrap();
            requests.push(request.clone());
            Ok(super::super::execution::scheduler::JobSubmission {
                job_id: (4000 + requests.len()).to_string(),
            })
        }

        fn state(&self, _job_id: &str) -> Result<SchedulerState> {
            Ok(SchedulerState::Unknown)
        }
    }

    fn fixture() -> (tempfile::TempDir, RunConfig, BuildPlan, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let config = RunConfig {
            schema_version: 1,
            dataset: DatasetName::Starlight,
            workspace: WorkspaceConfig { root: temp.path().join("work") },
            execution: ExecutionConfig {
                slurm: Some(SlurmConfig {
                    partition: Some("compute".into()),
                    account: Some("astronomy".into()),
                    time_limit: Some("00:30:00".into()),
                    memory: Some("4G".into()),
                    array_parallelism: 2,
                    max_array_size: 2,
                }),
                ..ExecutionConfig::default()
            },
            sources: vec![],
            publish: None,
            starlight: None,
        };
        std::fs::create_dir_all(&config.workspace.root).unwrap();
        let plan = BuildPlan {
            dataset: DatasetName::Starlight,
            operation: Operation::Build,
            executor: Executor::Slurm,
            partitions: ["p1", "p2", "p3", "p4", "p5"]
                .into_iter().map(str::to_string).collect(),
        };
        let manifest_path = temp.path().join("manifest.json");
        write_manifest(&manifest_path, &RunManifest {
            schema_version: RUN_SCHEMA_VERSION,
            run_id: "test-run".into(),
            dataset: plan.dataset,
            operation: plan.operation,
            executor: plan.executor,
            status: RunStatus::Planned,
            config_path: temp.path().join("config.toml"),
            config_sha256: "0".repeat(64),
            software_commit: "test".into(),
            resolved_workspace: config.workspace.root.clone(),
            partitions: plan.partitions.clone(),
            artifacts: vec![],
            validation_report: None,
            slurm_job_id: None,
            error: None,
        }).unwrap();
        (temp, config, plan, manifest_path)
    }

    #[test]
    fn slurm_submission_chunks_partitions_and_persists_job_ids() {
        let (temp, config, plan, manifest_path) = fixture();
        let scheduler = RecordingScheduler::default();
        let config_path = temp.path().join("config 'quoted'.toml");
        submit_with(&scheduler, &config_path, &config, &plan, &manifest_path).unwrap();

        let requests = scheduler.requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests.iter().map(|request| request.indices.clone())
            .collect::<Vec<_>>(), vec![vec![0, 1], vec![0, 1], vec![0]]);
        for (index, request) in requests.iter().enumerate() {
            assert_eq!(request.job_name, "nsb-starlight-build");
            assert_eq!(request.max_parallel, 2);
            assert_eq!(request.partition.as_deref(), Some("compute"));
            assert_eq!(request.account.as_deref(), Some("astronomy"));
            assert_eq!(request.time_limit.as_deref(), Some("00:30:00"));
            assert_eq!(request.memory.as_deref(), Some("4G"));
            let path = config.workspace.root.join(format!("slurm-partitions-{index:03}.json"));
            let array: PartitionArray = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(array.schema_version, ARRAY_SCHEMA_VERSION);
            assert_eq!(array.partitions, plan.partitions[index * 2..(index * 2 + 2).min(5)]);
            assert!(request.wrapped_command.contains("_worker --config "));
            assert!(request.wrapped_command.contains("config '\"'\"'quoted'\"'\"'.toml"));
            assert!(request.wrapped_command.contains(&shell_quote(&path)));
        }
        let manifest = read_manifest(&manifest_path).unwrap();
        assert!(matches!(manifest.status, RunStatus::Submitted));
        assert_eq!(manifest.slurm_job_id.as_deref(), Some("4001,4002,4003"));
    }

    #[test]
    fn slurm_submission_requires_partitions_and_slurm_configuration() {
        let (temp, mut config, mut plan, manifest_path) = fixture();
        let scheduler = RecordingScheduler::default();
        let config_path = temp.path().join("run.toml");
        plan.partitions.clear();
        assert!(submit_with(&scheduler, &config_path, &config, &plan, &manifest_path)
            .unwrap_err().to_string().contains("requires partitioned"));
        plan.partitions.push("p1".into());
        config.execution.slurm = None;
        assert!(submit_with(&scheduler, &config_path, &config, &plan, &manifest_path)
            .unwrap_err().to_string().contains("execution.slurm is required"));
        assert!(scheduler.requests.lock().unwrap().is_empty());
        assert!(matches!(read_manifest(&manifest_path).unwrap().status, RunStatus::Planned));
    }

    #[test]
    fn empty_scheduler_job_list_is_unknown() {
        assert_eq!(scheduler_state(" , ,, ").unwrap(), SchedulerState::Unknown);
    }

    #[test]
    fn partition_array_rejects_unknown_schema_and_missing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("array.json");
        assert!(partition_from_array(&path).is_err());
        std::fs::write(&path, r#"{"schema_version":99,"partitions":["p1"]}"#).unwrap();
        assert!(partition_from_array(&path).unwrap_err().to_string().contains("unsupported partition-array schema"));
        std::fs::write(&path, r#"{"schema_version":1,"partitions":["p1"],"surprise":true}"#).unwrap();
        assert!(partition_from_array(&path).is_err());
    }

    #[test]
    fn partition_array_selects_worker_task_and_rejects_invalid_indices() {
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("array.json");
        std::fs::write(&path, r#"{"schema_version":1,"partitions":["p1","p2"]}"#).unwrap();
        let original = std::env::var_os("SLURM_ARRAY_TASK_ID");
        std::env::set_var("SLURM_ARRAY_TASK_ID", "1");
        assert_eq!(partition_from_array(&path).unwrap(), "p2");
        std::env::set_var("SLURM_ARRAY_TASK_ID", "2");
        assert!(partition_from_array(&path).unwrap_err().to_string().contains("outside partition manifest"));
        std::env::set_var("SLURM_ARRAY_TASK_ID", "invalid");
        assert!(partition_from_array(&path).unwrap_err().to_string().contains("not a valid index"));
        std::env::remove_var("SLURM_ARRAY_TASK_ID");
        assert!(partition_from_array(&path).unwrap_err().to_string().contains("SLURM_ARRAY_TASK_ID is required"));
        if let Some(value) = original {
            std::env::set_var("SLURM_ARRAY_TASK_ID", value);
        }
    }

    #[test]
    fn shell_quote_handles_spaces_and_single_quotes() {
        assert_eq!(
            shell_quote(Path::new("/data/a b/c'd")),
            "'/data/a b/c'\"'\"'d'"
        );
    }
}
