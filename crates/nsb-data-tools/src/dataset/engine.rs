use super::config::RunConfig;
use super::model::{
    Artifact, BuildPlan, DatasetName, Executor, Operation, RunManifest, RunStatus, ValidationGate,
    ValidationReport, RUN_SCHEMA_VERSION,
};
use super::slurm;
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
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
) -> Result<()> {
    let mut config = RunConfig::load(config_path)?;
    if config.dataset != dataset {
        bail!(
            "command dataset {dataset} does not match config dataset {}",
            config.dataset
        );
    }
    if let Some(executor) = executor {
        config.execution.executor = executor;
    }
    if let Some(concurrency) = concurrency {
        if concurrency == 0 {
            bail!("concurrency must be greater than zero");
        }
        config.execution.concurrency = concurrency;
    }
    let partitions = selected_partitions(config_path, &config, requested_partitions)?;
    let plan = BuildPlan {
        dataset,
        operation,
        executor: config.execution.executor,
        partitions,
    };
    let manifest_path = manifest_path(&config, operation);
    initialize_manifest(config_path, &config, &plan, &manifest_path)?;

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
    if let Some(job_id) = &manifest.slurm_job_id {
        let output = Command::new("squeue")
            .args(["--noheader", "--jobs", job_id, "--format", "%T"])
            .output()
            .context("failed to query Slurm with squeue")?;
        if !output.status.success() {
            bail!("squeue failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        let state = String::from_utf8(output.stdout)?;
        eprintln!("slurm_job_state={}", state.trim());
    }
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

pub fn resume(path: &Path) -> Result<()> {
    let manifest = read_manifest(path)?;
    execute(
        &manifest.config_path,
        manifest.dataset,
        manifest.operation,
        Some(manifest.executor),
        None,
        &manifest.partitions,
    )
}

pub fn run_worker(
    config_path: &Path,
    dataset: DatasetName,
    operation: Operation,
    partition: &str,
) -> Result<()> {
    let mut config = RunConfig::load(config_path)?;
    if dataset != DatasetName::Starlight || config.dataset != dataset {
        bail!("distributed workers are available only for starlight");
    }
    let plan = BuildPlan {
        dataset,
        operation,
        executor: Executor::Local,
        partitions: vec![partition.to_string()],
    };
    let worker_root = config.workspace.root.join("workers").join(partition);
    fs::create_dir_all(&worker_root)?;
    config.workspace.root = worker_root.clone();
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
    let _lease = Lease::acquire(&worker_root.join("lease"))?;
    initialize_manifest(config_path, &config, &plan, &worker_manifest)?;
    if operation == Operation::Build {
        update_sources(&config, &plan.partitions)?;
    }
    run_local(config_path, &config, &plan, &worker_manifest)
}

fn run_local(
    _config_path: &Path,
    config: &RunConfig,
    plan: &BuildPlan,
    manifest_path: &Path,
) -> Result<()> {
    update_status(manifest_path, RunStatus::Running)?;
    let artifacts = match plan.operation {
        Operation::Update => update_sources(config, &plan.partitions)?,
        Operation::Build => build(config, &plan.partitions)?,
        Operation::Validate => {
            if config.dataset == DatasetName::Starlight {
                reconcile_workers(config)?;
            }
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
    if plan.operation == Operation::Validate {
        manifest.validation_report = Some(validation_path(config));
    }
    write_json(manifest_path, &manifest)?;
    println!("{}", manifest_path.display());
    Ok(())
}

fn selected_partitions(
    _config_path: &Path,
    config: &RunConfig,
    selected: &[String],
) -> Result<Vec<String>> {
    let mut available: Vec<String> = config
        .sources
        .iter()
        .filter_map(|source| source.partition.clone())
        .collect();
    available.sort();
    available.dedup();
    if config.dataset != DatasetName::Starlight {
        if !selected.is_empty() {
            bail!("partition selection is supported only for starlight");
        }
        return Ok(Vec::new());
    }
    if !selected.is_empty() {
        for partition in selected {
            if !available.contains(partition) {
                bail!("unknown partition {partition:?}");
            }
        }
        return Ok(selected.to_vec());
    }
    Ok(available)
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
    let sources_root = config.workspace.root.join("sources");
    let output_root = config.workspace.root.join("outputs");
    fs::create_dir_all(&output_root)?;
    let expected = expected_outputs(config.dataset);
    let sources = filtered_sources(config, partitions);
    if sources.len() != expected.len() && config.dataset != DatasetName::Starlight {
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
                    let name = output_name(config.dataset, &source.name)?;
                    let destination = output_root.join(name);
                    normalize(config.dataset, &source.name, &input, &destination)?;
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
        let format = validate_format(config.dataset, &artifact.name, &artifact.path);
        gates.push(ValidationGate {
            name: format!("format:{}", artifact.name),
            passed: format.is_ok(),
            detail: format
                .err()
                .map_or_else(|| "valid".to_string(), |e| e.to_string()),
        });
    }
    let expected = expected_outputs(config.dataset);
    gates.push(ValidationGate {
        name: "complete-artifact-set".to_string(),
        passed: config.dataset == DatasetName::Starlight
            || expected
                .iter()
                .all(|name| artifacts.iter().any(|a| a.name == *name)),
        detail: format!("{} artifacts", artifacts.len()),
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
    let mut reconciled = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&workers)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let artifacts_path = entry.path().join("outputs/artifacts.json");
        if !artifacts_path.is_file() {
            bail!(
                "partition {} has no completed build artifacts",
                entry.file_name().to_string_lossy()
            );
        }
        let artifacts: Vec<Artifact> = read_json(&artifacts_path)?;
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
        update_manifest_checksum(&mut document, &artifact.name, &artifact.sha256)?;
    }
    atomic_write(&manifest_path, document.to_string().as_bytes())?;
    Ok(report.artifacts)
}

fn normalize(dataset: DatasetName, name: &str, input: &Path, output: &Path) -> Result<()> {
    let bytes = fs::read(input)?;
    if bytes.contains(&0) {
        bail!("source {name:?} contains NUL bytes");
    }
    validate_format(dataset, name, input)?;
    atomic_write(output, &bytes)
}

fn validate_format(dataset: DatasetName, name: &str, path: &Path) -> Result<()> {
    let file = fs::File::open(path)?;
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .collect::<std::io::Result<_>>()?;
    let data: Vec<&str> = lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect();
    if data.is_empty() {
        bail!("{name} contains no data rows");
    }
    match dataset {
        DatasetName::SolarSpectrum => {
            if data.iter().any(|line| {
                let mut fields = line.split(',');
                fields
                    .next()
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .is_none()
                    || fields
                        .next()
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .is_none()
            }) {
                bail!("solar spectrum requires two numeric CSV columns");
            }
        }
        DatasetName::Starlight => {
            let text = lines.join("\n");
            for header in [
                "# map_type=healpix",
                "# coordinate_frame=galactic",
                "# nside=",
            ] {
                if !text.contains(header) {
                    bail!("starlight map is missing header {header}");
                }
            }
        }
        DatasetName::AirglowContinuum | DatasetName::MoonlightScattering => {
            if data.len() < 2 {
                bail!("{name} contains too few data rows");
            }
        }
    }
    Ok(())
}

fn expected_outputs(dataset: DatasetName) -> &'static [&'static str] {
    match dataset {
        DatasetName::AirglowContinuum => &["airglow_cont.dat"],
        DatasetName::SolarSpectrum => &["solar_spectrum.dat"],
        DatasetName::MoonlightScattering => &["mie_m15s1.dat", "sscatcor_m15s1.dat"],
        DatasetName::Starlight => &["starlight_manual_seed_v1.csv"],
    }
}

fn output_name(dataset: DatasetName, source_name: &str) -> Result<&str> {
    if dataset == DatasetName::Starlight {
        return Ok(source_name);
    }
    expected_outputs(dataset)
        .iter()
        .copied()
        .find(|expected| *expected == source_name)
        .with_context(|| format!("unexpected source name {source_name:?} for {dataset}"))
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
) -> Result<()> {
    let manifest = RunManifest {
        schema_version: RUN_SCHEMA_VERSION,
        dataset: plan.dataset,
        operation: plan.operation,
        executor: plan.executor,
        status: RunStatus::Planned,
        config_path: fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf()),
        config_sha256: sha256_file(config_path)?,
        software_commit: software_commit(),
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

fn manifest_path(config: &RunConfig, operation: Operation) -> PathBuf {
    config
        .workspace
        .root
        .join("runs")
        .join(config.dataset.slug())
        .join(operation.to_string())
        .join(RUN_MANIFEST)
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

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn update_manifest_checksum(
    document: &mut toml_edit::DocumentMut,
    name: &str,
    checksum: &str,
) -> Result<()> {
    let assets = document["assets"]
        .as_array_of_tables_mut()
        .context("manifest is missing [[assets]]")?;
    let asset = assets
        .iter_mut()
        .find(|asset| asset["path"].as_str() == Some(name))
        .with_context(|| format!("asset {name:?} is not registered"))?;
    asset["sha256"] = toml_edit::value(checksum);
    asset["generator"] = toml_edit::value("nsb-data dataset pipeline");
    asset["generation_command"] =
        toml_edit::value("nsb-data dataset <dataset> publish --config <run.toml>");
    Ok(())
}

struct Lease {
    path: PathBuf,
}

impl Lease {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("partition lease {} is already held", path.display()))?;
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
