use super::config::RunConfig;
use super::execution::scheduler::{aggregate_states, Scheduler, SchedulerState, SlurmScheduler};
use super::model::{
    Artifact, BuildPlan, DatasetName, Executor, Operation, RunManifest, RunStatus, ValidationGate,
    ValidationReport, RUN_SCHEMA_VERSION,
};
use super::pipeline::pipeline_for;
use super::slurm;
use crate::platform::artifact_store;
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUN_MANIFEST: &str = "run.json";
const VALIDATION_REPORT: &str = "validation.json";

pub fn execute(
    config_path: &Path,
    dataset: DatasetName,
    operation: Operation,
    executor: Option<Executor>,
    concurrency: Option<usize>,
    requested_partitions: &[String],
    skip_completed_from: Option<&Path>,
) -> Result<()> {
    let mut config = RunConfig::load(config_path)?;
    if config.dataset != dataset {
        bail!(
            "command dataset {dataset} does not match config dataset {}",
            config.dataset
        );
    }
    let pipeline = pipeline_for(dataset);
    pipeline.validate_config(&config)?;
    if let Some(executor) = executor {
        config.execution.executor = executor;
    }
    if let Some(concurrency) = concurrency {
        if concurrency == 0 {
            bail!("concurrency must be greater than zero");
        }
        config.execution.concurrency = concurrency;
    }
    let partitions = selected_partitions(
        &config,
        operation,
        requested_partitions,
        skip_completed_from,
    )?;
    let plan = BuildPlan {
        dataset,
        operation,
        executor: config.execution.executor,
        partitions,
    };
    let config_sha256 = sha256_file(config_path)?;
    let software_commit = software_commit();
    let run_id = run_id(&plan, &config_sha256, &software_commit)?;
    let manifest_path = manifest_path(&config, &run_id);
    initialize_manifest(
        config_path,
        &config,
        &plan,
        &manifest_path,
        &run_id,
        &config_sha256,
        &software_commit,
    )?;
    if completed_manifest_is_valid(&manifest_path)? {
        println!("{}", manifest_path.display());
        return Ok(());
    }

    let result = if config.execution.executor == Executor::Slurm {
        if dataset != DatasetName::Starlight {
            bail!("the Slurm executor is supported only for starlight");
        }
        if matches!(operation, Operation::Validate | Operation::Publish) {
            bail!("validate and publish run locally after distributed build reconciliation");
        }
        slurm::submit(config_path, &config, &plan, &manifest_path)
    } else {
        run_local(config_path, &config, &plan, &manifest_path)
    };
    if let Err(error) = &result {
        mark_failed(&manifest_path, error);
    }
    result
}

pub fn status(path: &Path) -> Result<()> {
    let manifest = read_manifest(path)?;
    let scheduler_state = manifest
        .slurm_job_id
        .as_deref()
        .map(super::slurm::scheduler_state)
        .transpose()?
        .map(|state| format!("{state:?}").to_ascii_lowercase());
    let (partitions_complete, partitions_pending) = partition_progress(&manifest)?;
    #[derive(serde::Serialize)]
    struct StatusReport<'a> {
        schema_version: u32,
        manifest: &'a RunManifest,
        scheduler_state: Option<String>,
        partitions_complete: usize,
        partitions_pending: usize,
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&StatusReport {
            schema_version: 1,
            manifest: &manifest,
            scheduler_state,
            partitions_complete,
            partitions_pending,
        })?
    );
    Ok(())
}

pub fn resume(path: &Path) -> Result<()> {
    let manifest = read_manifest(path)?;
    let partitions = if manifest.executor == Executor::Slurm {
        manifest
            .partitions
            .iter()
            .filter(|partition| {
                !worker_is_complete(&manifest.resolved_workspace, partition, manifest.operation)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    } else {
        manifest.partitions.clone()
    };
    if manifest.executor == Executor::Slurm && partitions.is_empty() {
        println!("all partitions are complete");
        return Ok(());
    }
    execute(
        &manifest.config_path,
        manifest.dataset,
        manifest.operation,
        Some(manifest.executor),
        None,
        &partitions,
        None,
    )
}

pub fn run_worker(
    config_path: &Path,
    dataset: DatasetName,
    operation: Operation,
    partition: Option<&str>,
    partition_manifest: Option<&Path>,
) -> Result<()> {
    let partition = match (partition, partition_manifest) {
        (Some(partition), None) => partition.to_string(),
        (None, Some(manifest)) => super::slurm::partition_from_array(manifest)?,
        _ => bail!("worker requires exactly one of --partition or --partition-manifest"),
    };
    let config = RunConfig::load(config_path)?;
    if dataset != DatasetName::Starlight || config.dataset != dataset {
        bail!("distributed workers are available only for starlight");
    }
    let pipeline = pipeline_for(dataset);
    pipeline.validate_config(&config)?;
    let plan = BuildPlan {
        dataset,
        operation,
        executor: Executor::Local,
        partitions: vec![partition.clone()],
    };
    let worker_root = config.workspace.root.join("workers").join(&partition);
    fs::create_dir_all(&worker_root)?;
    let mut worker_config = config.clone();
    worker_config.workspace.root = worker_root.clone();
    let worker_manifest = worker_root.join("runs").join(format!("{operation}.json"));
    if worker_manifest.is_file() {
        let existing = read_manifest(&worker_manifest)?;
        if matches!(existing.status, RunStatus::Complete)
            && existing.artifacts.iter().all(|artifact| {
                artifact.path.is_file()
                    && sha256_file(&artifact.path).is_ok_and(|checksum| checksum == artifact.sha256)
            })
        {
            println!("{}", worker_manifest.display());
            return Ok(());
        }
    }
    let _lease = Lease::acquire(
        &worker_root.join("lease"),
        config.execution.lease_timeout_seconds,
    )?;
    let config_sha256 = sha256_file(config_path)?;
    let software_commit = software_commit();
    let run_id = run_id(&plan, &config_sha256, &software_commit)?;
    initialize_manifest(
        config_path,
        &worker_config,
        &plan,
        &worker_manifest,
        &run_id,
        &config_sha256,
        &software_commit,
    )?;
    if operation == Operation::Update {
        let artifacts = match pipeline.update(&config, &plan.partitions)? {
            Some(artifacts) => artifacts,
            None => update_sources(&worker_config, &plan.partitions)?,
        };
        let mut manifest = read_manifest(&worker_manifest)?;
        manifest.status = RunStatus::Complete;
        manifest.artifacts = artifacts;
        manifest.error = None;
        write_json(&worker_manifest, &manifest)?;
        println!("{}", worker_manifest.display());
        return Ok(());
    }
    if operation == Operation::Build && pipeline.update(&config, &plan.partitions)?.is_none() {
        update_sources(&worker_config, &plan.partitions)?;
    }
    run_local(config_path, &worker_config, &plan, &worker_manifest)
}

fn run_local(
    _config_path: &Path,
    config: &RunConfig,
    plan: &BuildPlan,
    manifest_path: &Path,
) -> Result<()> {
    update_status(manifest_path, RunStatus::Running)?;
    let artifacts = match plan.operation {
        Operation::Update => {
            match pipeline_for(config.dataset).update(config, &plan.partitions)? {
                Some(artifacts) => artifacts,
                None => update_sources(config, &plan.partitions)?,
            }
        }
        Operation::Build => build(config, &plan.partitions)?,
        Operation::Validate => {
            if config.dataset == DatasetName::Starlight {
                reconcile_workers(config)?;
            }
            pipeline_for(config.dataset).finalize(config)?;
            let report = validate(config)?;
            if !report.passed {
                bail!("dataset validation failed");
            }
            report.artifacts
        }
        Operation::Publish => publish(config)?,
    };
    let mut manifest = read_manifest(manifest_path)?;
    manifest.status = RunStatus::Complete;
    manifest.artifacts = artifacts;
    manifest.error = None;
    if plan.operation == Operation::Validate {
        manifest.validation_report = Some(validation_path(config));
    }
    write_json(manifest_path, &manifest)?;
    println!("{}", manifest_path.display());
    Ok(())
}

fn selected_partitions(
    config: &RunConfig,
    operation: Operation,
    selected: &[String],
    skip_completed_from: Option<&Path>,
) -> Result<Vec<String>> {
    let pipeline = pipeline_for(config.dataset);
    if !pipeline.supports_partitions() {
        if !selected.is_empty() {
            bail!("partition selection is supported only for starlight");
        }
        return Ok(Vec::new());
    }
    if operation == Operation::Update
        && selected.is_empty()
        && config.execution.executor == Executor::Local
    {
        return Ok(Vec::new());
    }
    let available = match pipeline.available_partitions(config)? {
        Some(partitions) => partitions,
        None if operation == Operation::Update && selected.is_empty() => return Ok(Vec::new()),
        None => bail!(
            "{} source inventory is missing; run dataset {} update first",
            config.dataset,
            config.dataset
        ),
    };
    let mut selected_partitions = if !selected.is_empty() {
        for partition in selected {
            if !available.contains(partition) {
                bail!("unknown partition {partition:?}");
            }
        }
        selected.to_vec()
    } else {
        available
    };
    if let Some(checkpoints_dir) = skip_completed_from {
        if config.dataset != DatasetName::Starlight || operation != Operation::Build {
            bail!("--skip-completed-from is supported only by dataset starlight build");
        }
        let starlight = config
            .starlight
            .as_ref()
            .context("Starlight production configuration is missing")?;
        let completed = crate::starlight::migration::load_completed_partition_ids(checkpoints_dir)?;
        let mut retained = Vec::with_capacity(selected_partitions.len());
        for partition in selected_partitions {
            let legacy_name = format!("XpContinuousMeanSpectrum_{partition}.csv.gz");
            if !completed.contains(&legacy_name) {
                retained.push(partition);
                continue;
            }
            let receipt_is_valid = crate::starlight::sources::acquisition::has_valid_receipt(
                &config.workspace.root,
                &starlight.gaia_products,
                "xp-continuous",
                &partition,
            )?;
            if !receipt_is_valid {
                retained.push(partition);
            }
        }
        selected_partitions = retained;
    }
    Ok(selected_partitions)
}

fn update_sources(config: &RunConfig, partitions: &[String]) -> Result<Vec<Artifact>> {
    let root = config.workspace.root.join("sources");
    fs::create_dir_all(&root)?;
    let mut artifacts = Vec::new();
    for source in filtered_sources(config, partitions) {
        let destination = root.join(&source.name);
        if let Some(path) = &source.path {
            verify_source(path, &source.sha256)?;
            copy_atomic(path, &destination)?;
        } else {
            let url = source.url.as_deref().context("source URL is missing")?;
            let response = reqwest::blocking::get(url)
                .with_context(|| format!("failed to download {url}"))?
                .error_for_status()
                .with_context(|| format!("source request failed for {url}"))?;
            atomic_write(&destination, &response.bytes()?)?;
            verify_source(&destination, &source.sha256)?;
        }
        artifacts.push(artifact(&source.name, &destination)?);
    }
    write_json(&root.join("artifacts.json"), &artifacts)?;
    Ok(artifacts)
}

fn build(config: &RunConfig, partitions: &[String]) -> Result<Vec<Artifact>> {
    let pipeline = pipeline_for(config.dataset);
    if let Some(artifacts) = pipeline.build(config, partitions)? {
        return Ok(artifacts);
    }
    let sources_root = config.workspace.root.join("sources");
    let output_root = config.workspace.root.join("outputs");
    fs::create_dir_all(&output_root)?;
    let expected = pipeline.expected_outputs_for(config);
    let sources = filtered_sources(config, partitions);
    if sources.len() != expected.len() && !pipeline.supports_partitions() {
        bail!(
            "{} requires sources named {}",
            config.dataset,
            expected.join(", ")
        );
    }
    let chunk_size = sources.len().div_ceil(config.execution.concurrency).max(1);
    let artifacts = std::thread::scope(|scope| -> Result<Vec<Artifact>> {
        let mut handles = Vec::new();
        for chunk in sources.chunks(chunk_size) {
            let sources_root = &sources_root;
            let output_root = &output_root;
            handles.push(scope.spawn(move || -> Result<Vec<Artifact>> {
                let mut artifacts = Vec::new();
                for source in chunk {
                    let input = sources_root.join(&source.name);
                    if !input.is_file() {
                        bail!(
                            "updated source {} is missing; run update first",
                            input.display()
                        );
                    }
                    let name = pipeline.output_name(&source.name)?;
                    let destination = output_root.join(name);
                    pipeline.transform(&source.name, &input, &destination)?;
                    artifacts.push(artifact(name, &destination)?);
                }
                Ok(artifacts)
            }));
        }
        let mut artifacts = Vec::new();
        for handle in handles {
            artifacts.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("dataset worker panicked"))??,
            );
        }
        Ok(artifacts)
    })?;
    let mut artifacts = artifacts;
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    write_json(&output_root.join("artifacts.json"), &artifacts)?;
    Ok(artifacts)
}

fn validate(config: &RunConfig) -> Result<ValidationReport> {
    let pipeline = pipeline_for(config.dataset);
    let output_root = config.workspace.root.join("outputs");
    let artifacts: Vec<Artifact> =
        read_json(&output_root.join("artifacts.json")).context("run build before validate")?;
    let mut gates = Vec::new();
    for artifact in &artifacts {
        let actual = self::artifact(&artifact.name, &artifact.path)?;
        gates.push(ValidationGate {
            name: format!("checksum:{}", artifact.name),
            passed: actual.sha256 == artifact.sha256,
            detail: actual.sha256,
        });
        let format = pipeline.validate_artifact(&artifact.name, &artifact.path);
        gates.push(ValidationGate {
            name: format!("format:{}", artifact.name),
            passed: format.is_ok(),
            detail: format
                .err()
                .map_or_else(|| "valid".to_string(), |e| e.to_string()),
        });
    }
    gates.extend(pipeline.validation_gates(config, &artifacts)?);
    let expected = pipeline.expected_outputs_for(config);
    let expected_set = expected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let actual_set = artifacts
        .iter()
        .map(|artifact| artifact.name.as_str())
        .collect::<BTreeSet<_>>();
    gates.push(ValidationGate {
        name: "complete-artifact-set".to_string(),
        passed: actual_set == expected_set && artifacts.len() == expected.len(),
        detail: format!("expected {:?}; found {:?}", expected_set, actual_set),
    });
    let report = ValidationReport {
        schema_version: RUN_SCHEMA_VERSION,
        dataset: config.dataset,
        passed: gates.iter().all(|gate| gate.passed),
        gates,
        artifacts,
    };
    write_json(&validation_path(config), &report)?;
    Ok(report)
}

fn reconcile_workers(config: &RunConfig) -> Result<()> {
    let workers = config.workspace.root.join("workers");
    if !workers.is_dir() {
        return Ok(());
    }
    let output_root = config.workspace.root.join("outputs");
    fs::create_dir_all(&output_root)?;
    let shard_root = output_root.join("shards");
    if shard_root.is_dir() {
        for entry in fs::read_dir(&shard_root)? {
            let path = entry?.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                fs::remove_file(path)?;
            }
        }
    }
    let mut reconciled = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&workers)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let partition = entry.file_name().to_string_lossy().to_string();
        let expected_name = format!("shards/{partition}.json");
        let artifacts_path = entry.path().join("outputs/artifacts.json");
        let artifacts: Vec<Artifact> = if artifacts_path.is_file() {
            read_json(&artifacts_path)?
        } else if entry.path().join("shard.json").is_file() {
            let shard_path = entry.path().join("shard.json");
            vec![self::artifact(&expected_name, &shard_path)?]
        } else {
            bail!(
                "partition {} has no completed build artifacts",
                entry.file_name().to_string_lossy()
            );
        };
        if artifacts.len() != 1 || artifacts[0].name != expected_name {
            bail!("partition {partition} did not emit exactly {expected_name}");
        }
        for artifact in artifacts {
            let verified = self::artifact(&artifact.name, &artifact.path)?;
            if verified.sha256 != artifact.sha256 {
                bail!("partition artifact {} failed reconciliation", artifact.name);
            }
            if reconciled
                .iter()
                .any(|current: &Artifact| current.name == artifact.name)
            {
                bail!("duplicate partition artifact {}", artifact.name);
            }
            let destination = output_root.join(&artifact.name);
            copy_atomic(&artifact.path, &destination)?;
            reconciled.push(self::artifact(&artifact.name, &destination)?);
        }
    }
    reconciled.sort_by(|left, right| left.name.cmp(&right.name));
    write_json(&output_root.join("artifacts.json"), &reconciled)
}

fn publish(config: &RunConfig) -> Result<Vec<Artifact>> {
    let report: ValidationReport =
        read_json(&validation_path(config)).context("run validate before publish")?;
    if !report.passed || report.dataset != config.dataset {
        bail!("publish requires a passing validation report for the same dataset");
    }
    for registered in &report.artifacts {
        if artifact(&registered.name, &registered.path)?.sha256 != registered.sha256 {
            bail!(
                "validated artifact {} changed after validation",
                registered.name
            );
        }
    }
    let publish = config
        .publish
        .as_ref()
        .context("publish.repository_root is required")?;
    let data_root = publish.repository_root.join("crates/nsb/data");
    let manifest_path = data_root.join("manifest.toml");
    let mut document = fs::read_to_string(&manifest_path)?.parse::<toml_edit::DocumentMut>()?;
    for artifact in &report.artifacts {
        let destination = data_root.join(&artifact.name);
        copy_atomic(&artifact.path, &destination)?;
        update_manifest_checksum(
            &mut document,
            config.dataset,
            &artifact.name,
            &artifact.sha256,
        )?;
    }
    atomic_write(&manifest_path, document.to_string().as_bytes())?;
    Ok(report.artifacts)
}

fn filtered_sources<'a>(
    config: &'a RunConfig,
    partitions: &'a [String],
) -> Vec<&'a super::SourceConfig> {
    config
        .sources
        .iter()
        .filter(|source| {
            partitions.is_empty()
                || source
                    .partition
                    .as_ref()
                    .is_some_and(|partition| partitions.contains(partition))
        })
        .collect()
}

fn verify_source(path: &Path, expected: &str) -> Result<()> {
    let actual = sha256_file(path)?;
    if actual != expected {
        bail!(
            "source {} checksum mismatch: expected {}, found {}",
            path.display(),
            expected,
            actual
        );
    }
    Ok(())
}

fn artifact(name: &str, path: &Path) -> Result<Artifact> {
    Ok(Artifact {
        name: name.to_string(),
        path: path.to_path_buf(),
        sha256: sha256_file(path)?,
        bytes: fs::metadata(path)?.len(),
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    checksum_io::sha256_file(path)
}

fn initialize_manifest(
    config_path: &Path,
    config: &RunConfig,
    plan: &BuildPlan,
    path: &Path,
    run_id: &str,
    config_sha256: &str,
    software_commit: &str,
) -> Result<()> {
    if path.is_file() {
        let existing = read_manifest(path)?;
        if existing.run_id != run_id
            || existing.config_sha256 != config_sha256
            || existing.software_commit != software_commit
            || existing.dataset != plan.dataset
            || existing.operation != plan.operation
            || existing.partitions != plan.partitions
        {
            bail!("persisted run identity does not match requested execution");
        }
        return Ok(());
    }
    let manifest = RunManifest {
        schema_version: RUN_SCHEMA_VERSION,
        run_id: run_id.to_string(),
        dataset: plan.dataset,
        operation: plan.operation,
        executor: plan.executor,
        status: RunStatus::Planned,
        config_path: fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf()),
        config_sha256: config_sha256.to_string(),
        software_commit: software_commit.to_string(),
        resolved_workspace: config.workspace.root.clone(),
        partitions: plan.partitions.clone(),
        artifacts: Vec::new(),
        validation_report: None,
        slurm_job_id: None,
        error: None,
    };
    write_json(path, &manifest)
}

fn software_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|commit| commit.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn manifest_path(config: &RunConfig, run_id: &str) -> PathBuf {
    config
        .workspace
        .root
        .join("runs")
        .join(config.dataset.slug())
        .join(run_id)
        .join(RUN_MANIFEST)
}

fn run_id(plan: &BuildPlan, config_sha256: &str, software_commit: &str) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Identity<'a> {
        schema_version: u32,
        plan: &'a BuildPlan,
        config_sha256: &'a str,
        software_commit: &'a str,
    }
    let bytes = serde_json::to_vec(&Identity {
        schema_version: RUN_SCHEMA_VERSION,
        plan,
        config_sha256,
        software_commit,
    })?;
    Ok(checksum_io::sha256_bytes(&bytes))
}

fn completed_manifest_is_valid(path: &Path) -> Result<bool> {
    let manifest = read_manifest(path)?;
    if !matches!(manifest.status, RunStatus::Complete) {
        return Ok(false);
    }
    Ok(manifest.artifacts.iter().all(|artifact| {
        artifact.path.is_file()
            && sha256_file(&artifact.path).is_ok_and(|checksum| checksum == artifact.sha256)
    }))
}

fn partition_progress(manifest: &RunManifest) -> Result<(usize, usize)> {
    if manifest.partitions.is_empty() {
        return Ok((0, 0));
    }
    let mut complete = 0;
    for partition in &manifest.partitions {
        if worker_is_complete(&manifest.resolved_workspace, partition, manifest.operation)? {
            complete += 1;
        }
    }
    Ok((complete, manifest.partitions.len() - complete))
}

fn worker_is_complete(workspace: &Path, partition: &str, operation: Operation) -> Result<bool> {
    let path = workspace
        .join("workers")
        .join(partition)
        .join("runs")
        .join(format!("{operation}.json"));
    if !path.is_file() {
        return Ok(false);
    }
    completed_manifest_is_valid(&path)
}

fn validation_path(config: &RunConfig) -> PathBuf {
    config.workspace.root.join(VALIDATION_REPORT)
}

fn update_status(path: &Path, status: RunStatus) -> Result<()> {
    let mut manifest = read_manifest(path)?;
    manifest.status = status;
    write_json(path, &manifest)
}

fn mark_failed(path: &Path, error: &anyhow::Error) {
    if let Ok(mut manifest) = read_manifest(path) {
        manifest.status = RunStatus::Failed;
        manifest.error = Some(format!("{error:#}"));
        let _ = write_json(path, &manifest);
    }
}

pub(super) fn read_manifest(path: &Path) -> Result<RunManifest> {
    let manifest: RunManifest = read_json(path)?;
    if manifest.schema_version != RUN_SCHEMA_VERSION {
        bail!("unsupported run manifest schema");
    }
    Ok(manifest)
}

pub(super) fn write_manifest(path: &Path, manifest: &RunManifest) -> Result<()> {
    write_json(path, manifest)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value)?)
}

fn copy_atomic(source: &Path, destination: &Path) -> Result<()> {
    atomic_write(destination, &fs::read(source)?)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    artifact_store::atomic_write(path, bytes)
}

fn update_manifest_checksum(
    document: &mut toml_edit::DocumentMut,
    dataset: DatasetName,
    name: &str,
    checksum: &str,
) -> Result<()> {
    let assets = document["assets"]
        .as_array_of_tables_mut()
        .context("manifest is missing [[assets]]")?;
    if !assets
        .iter()
        .any(|asset| asset["path"].as_str() == Some(name))
    {
        if dataset != DatasetName::Starlight {
            bail!("asset {name:?} is not registered");
        }
        let mut asset = toml_edit::Table::new();
        asset["path"] = toml_edit::value(name);
        asset["schema"] = toml_edit::value(if name == "merge_report.json" {
            "nsb-starlight-merge-report-v3"
        } else {
            "nsb-healpix-starlight-candidate-v2"
        });
        asset["sha256"] = toml_edit::value(checksum);
        asset["source"] =
            toml_edit::value("Gaia DR3 GaiaSource and XP continuous bulk distributions");
        asset["license"] = toml_edit::value("Gaia data licence: CC BY-NC 3.0 IGO");
        asset["generator"] = toml_edit::value("nsb-data dataset pipeline");
        asset["generation_command"] =
            toml_edit::value("nsb-data dataset starlight publish --config <run.toml>");
        asset["validation_report"] =
            toml_edit::value("external validation.json pinned by the dataset run manifest");
        asset["calibration_status"] = toml_edit::value("candidate");
        asset["runtime_embedded"] = toml_edit::value(false);
        assets.push(asset);
    }
    let asset = assets
        .iter_mut()
        .find(|asset| asset["path"].as_str() == Some(name))
        .context("newly registered Starlight asset is missing")?;
    if dataset == DatasetName::Starlight
        && (asset["calibration_status"].as_str() == Some("production")
            || asset["runtime_embedded"].as_bool() == Some(true))
    {
        bail!("candidate Starlight publish cannot replace a production runtime asset");
    }
    asset["sha256"] = toml_edit::value(checksum);
    asset["generator"] = toml_edit::value("nsb-data dataset pipeline");
    asset["generation_command"] =
        toml_edit::value("nsb-data dataset <dataset> publish --config <run.toml>");
    Ok(())
}

struct Lease {
    path: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseRecord {
    schema_version: u32,
    hostname: String,
    pid: u32,
    created_unix_seconds: u64,
    slurm_job_id: Option<String>,
}

impl Lease {
    fn acquire(path: &Path, timeout_seconds: u64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if path.is_file() {
            let existing: LeaseRecord = read_json(path)
                .with_context(|| format!("invalid existing lease {}", path.display()))?;
            if lease_can_be_recovered(&existing, timeout_seconds)? {
                fs::remove_file(path)?;
            } else {
                bail!("partition lease {} is already held", path.display());
            }
        }
        let record = LeaseRecord {
            schema_version: 1,
            hostname: current_hostname(),
            pid: std::process::id(),
            created_unix_seconds: unix_seconds()?,
            slurm_job_id: std::env::var("SLURM_ARRAY_JOB_ID")
                .ok()
                .or_else(|| std::env::var("SLURM_JOB_ID").ok()),
        };
        let bytes = serde_json::to_vec_pretty(&record)?;
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

fn lease_can_be_recovered(record: &LeaseRecord, timeout_seconds: u64) -> Result<bool> {
    if record.schema_version != 1 {
        bail!("unsupported lease schema {}", record.schema_version);
    }
    let age = unix_seconds()?.saturating_sub(record.created_unix_seconds);
    if age < timeout_seconds {
        return Ok(false);
    }
    if let Some(job_ids) = &record.slurm_job_id {
        let states: Vec<SchedulerState> = job_ids
            .split(',')
            .map(str::trim)
            .filter(|job_id| !job_id.is_empty())
            .map(|job_id| SlurmScheduler::default().state(job_id))
            .collect::<Result<_>>()?;
        return Ok(matches!(
            aggregate_states(&states),
            SchedulerState::Succeeded | SchedulerState::Failed | SchedulerState::Cancelled
        ));
    }
    Ok(record.hostname == current_hostname()
        && !Path::new("/proc").join(record.pid.to_string()).exists())
}

fn current_hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string())
}

fn unix_seconds() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starlight_publish_registers_new_outputs_as_non_runtime_candidates() {
        let mut document = "schema_version = 1\n\n[[assets]]\npath = \"existing.dat\"\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        update_manifest_checksum(
            &mut document,
            DatasetName::Starlight,
            "starlight_nside128.csv",
            &"a".repeat(64),
        )
        .unwrap();
        let assets = document["assets"].as_array_of_tables().unwrap();
        let candidate = assets
            .iter()
            .find(|asset| asset["path"].as_str() == Some("starlight_nside128.csv"))
            .unwrap();
        assert_eq!(
            candidate["schema"].as_str(),
            Some("nsb-healpix-starlight-candidate-v2")
        );
        assert_eq!(candidate["calibration_status"].as_str(), Some("candidate"));
        assert_eq!(candidate["runtime_embedded"].as_bool(), Some(false));
    }
}
