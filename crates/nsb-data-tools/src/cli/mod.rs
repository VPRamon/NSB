//! Dataset-oriented command-line interface.

use crate::dataset::{self, DatasetName, Executor, Operation};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "nsb-data",
    about = "Build and maintain NSB scientific datasets"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build, update, validate, or publish a dataset.
    Dataset(DatasetArgs),
    /// Inspect or resume a persisted run.
    Run(RunArgs),
    /// Validate a checksum-pinned Starlight ultraviolet calibration.
    StarlightUv(StarlightUvArgs),
    #[command(name = "_worker", hide = true)]
    Worker(WorkerArgs),
}

#[derive(Debug, Args)]
struct StarlightUvArgs {
    #[command(subcommand)]
    command: StarlightUvCommand,
}

#[derive(Debug, Subcommand)]
enum StarlightUvCommand {
    /// Validate manifests/artifact, evaluate a holdout, and emit deterministic JSON.
    Validate(StarlightUvValidateArgs),
}

#[derive(Debug, Args)]
struct StarlightUvValidateArgs {
    #[arg(long)]
    reference_manifest: PathBuf,
    #[arg(long)]
    partition_manifest: PathBuf,
    #[arg(long)]
    artifact: PathBuf,
    #[arg(long)]
    artifact_sha256: String,
    #[arg(long)]
    holdout: PathBuf,
    #[arg(long)]
    materialize_partitions: Option<PathBuf>,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct DatasetArgs {
    #[command(subcommand)]
    command: DatasetCommand,
}

#[derive(Debug, Subcommand)]
enum DatasetCommand {
    /// List supported datasets.
    List,
    AirglowContinuum(ActionArgs),
    SolarSpectrum(ActionArgs),
    MoonlightScattering(ActionArgs),
    Starlight(StarlightActionArgs),
}

#[derive(Debug, Args)]
struct ActionArgs {
    #[command(subcommand)]
    operation: Action,
}

#[derive(Debug, Clone, Subcommand)]
enum Action {
    Update(CommonArgs),
    Build(CommonArgs),
    Validate(CommonArgs),
    Publish(CommonArgs),
}

#[derive(Debug, Args)]
struct StarlightActionArgs {
    #[command(subcommand)]
    operation: StarlightAction,
}

#[derive(Debug, Subcommand)]
enum StarlightAction {
    Update(CommonArgs),
    Build(CommonArgs),
    Validate(CommonArgs),
    Publish(CommonArgs),
    /// Independent validation pipeline for issue #87 (acquire references, run comparisons).
    Validation(StarlightValidationArgs),
}

#[derive(Debug, Args)]
struct StarlightValidationArgs {
    #[command(subcommand)]
    command: StarlightValidationCommand,
}

#[derive(Debug, Subcommand)]
enum StarlightValidationCommand {
    /// Download or locate registered reference files and write verified receipts.
    Acquire(StarlightValidationAcquireArgs),
    /// Compare the candidate map against every acquired-and-transformed reference.
    Run(StarlightValidationRunArgs),
}

#[derive(Debug, Args)]
struct StarlightValidationAcquireArgs {
    /// Path to `references-v1.toml`.
    #[arg(long)]
    references: PathBuf,
    /// Directory holding receipts and content-addressed objects.
    #[arg(long)]
    workspace: PathBuf,
    /// Override a reference's source, formatted as `id=url-or-local-path`. May repeat.
    #[arg(long = "source", value_parser = parse_source_override)]
    sources: Vec<(String, String)>,
}

#[derive(Debug, Args)]
struct StarlightValidationRunArgs {
    /// Path to `preregistration-v1.toml`.
    #[arg(long)]
    preregistration: PathBuf,
    /// Path to `references-v1.toml`.
    #[arg(long)]
    references: PathBuf,
    /// Path to `regions-v1.json`.
    #[arg(long)]
    regions: PathBuf,
    /// Path to the candidate HEALPix map CSV.
    #[arg(long)]
    candidate_map: PathBuf,
    /// Optional pinned SHA-256 to cross-check against the candidate map.
    #[arg(long)]
    candidate_map_sha256: Option<String>,
    /// Directory containing per-reference acquisition receipts and transformed grids.
    #[arg(long)]
    references_workspace: PathBuf,
    /// Output directory for results, report, and artifact manifest.
    #[arg(long)]
    output: PathBuf,
}

fn parse_source_override(raw: &str) -> Result<(String, String), String> {
    raw.split_once('=')
        .map(|(id, source)| (id.to_string(), source.to_string()))
        .ok_or_else(|| format!("expected id=url-or-path, found {raw:?}"))
}

#[derive(Debug, Clone, Args)]
struct CommonArgs {
    /// Versioned run configuration.
    #[arg(long)]
    config: PathBuf,
    /// Override the configured executor.
    #[arg(long, value_enum)]
    executor: Option<Executor>,
    /// Override local worker concurrency.
    #[arg(long)]
    concurrency: Option<usize>,
    /// Process only a comma-separated set of partition identifiers.
    #[arg(long, value_delimiter = ',')]
    partitions: Vec<String>,
    /// Skip legacy-completed Starlight partitions that have a valid new CAS receipt.
    #[arg(long)]
    skip_completed_from: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[command(subcommand)]
    command: RunCommand,
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    Status {
        #[arg(long)]
        run: PathBuf,
    },
    Resume {
        #[arg(long)]
        run: PathBuf,
    },
}

#[derive(Debug, Args)]
struct WorkerArgs {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    dataset: DatasetName,
    #[arg(long)]
    operation: Operation,
    #[arg(long, conflicts_with = "partition_manifest")]
    partition: Option<String>,
    #[arg(long, conflicts_with = "partition")]
    partition_manifest: Option<PathBuf>,
}

pub fn run() -> Result<()> {
    crate::platform::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    match Cli::parse().command {
        Command::Dataset(args) => match args.command {
            DatasetCommand::List => {
                for dataset in DatasetName::ALL {
                    println!("{dataset}");
                }
                Ok(())
            }
            DatasetCommand::AirglowContinuum(args) => execute(DatasetName::AirglowContinuum, args),
            DatasetCommand::SolarSpectrum(args) => execute(DatasetName::SolarSpectrum, args),
            DatasetCommand::MoonlightScattering(args) => {
                execute(DatasetName::MoonlightScattering, args)
            }
            DatasetCommand::Starlight(args) => execute_starlight(args),
        },
        Command::Run(args) => match args.command {
            RunCommand::Status { run } => dataset::status(&run),
            RunCommand::Resume { run } => dataset::resume(&run),
        },
        Command::StarlightUv(args) => match args.command {
            StarlightUvCommand::Validate(args) => {
                let inputs = crate::starlight::uv::ReproducibilityInputs {
                    reference_manifest: args.reference_manifest,
                    partition_manifest: args.partition_manifest,
                    artifact: args.artifact,
                    artifact_sha256: args.artifact_sha256,
                    holdout: args.holdout,
                    materialize_partitions: args.materialize_partitions,
                    output: args.output,
                };
                crate::starlight::uv::run_reproducibility_validation(&inputs)?;
                Ok(())
            }
        },
        Command::Worker(args) => dataset::run_worker(
            &args.config,
            args.dataset,
            args.operation,
            args.partition.as_deref(),
            args.partition_manifest.as_deref(),
        ),
    }
}

fn execute(dataset: DatasetName, args: ActionArgs) -> Result<()> {
    let (operation, common) = match args.operation {
        Action::Update(args) => (Operation::Update, args),
        Action::Build(args) => (Operation::Build, args),
        Action::Validate(args) => (Operation::Validate, args),
        Action::Publish(args) => (Operation::Publish, args),
    };
    dataset::execute(
        &common.config,
        dataset,
        operation,
        common.executor,
        common.concurrency,
        &common.partitions,
        common.skip_completed_from.as_deref(),
    )
}

fn execute_starlight(args: StarlightActionArgs) -> Result<()> {
    let (operation, common) = match args.operation {
        StarlightAction::Update(args) => (Operation::Update, args),
        StarlightAction::Build(args) => (Operation::Build, args),
        StarlightAction::Validate(args) => (Operation::Validate, args),
        StarlightAction::Publish(args) => (Operation::Publish, args),
        StarlightAction::Validation(args) => return execute_starlight_validation(args),
    };
    dataset::execute(
        &common.config,
        DatasetName::Starlight,
        operation,
        common.executor,
        common.concurrency,
        &common.partitions,
        common.skip_completed_from.as_deref(),
    )
}

fn execute_starlight_validation(args: StarlightValidationArgs) -> Result<()> {
    match args.command {
        StarlightValidationCommand::Acquire(args) => {
            let bytes = std::fs::read(&args.references)
                .map_err(|error| anyhow::anyhow!("read {}: {error}", args.references.display()))?;
            let document: crate::starlight::validation::references::ReferencesDocument =
                toml::from_str(std::str::from_utf8(&bytes)?).map_err(|error| {
                    anyhow::anyhow!("parse {}: {error}", args.references.display())
                })?;
            document.validate()?;
            let overrides = args.sources.into_iter().collect::<BTreeMap<_, _>>();
            let results = crate::starlight::validation::acquire::acquire_references(
                &document,
                &args.workspace,
                &overrides,
            )?;
            let mut manual = 0;
            for result in &results {
                println!(
                    "{}: {:?} — {}",
                    result.reference_id, result.outcome, result.detail
                );
                if result.outcome
                    == crate::starlight::validation::acquire::AcquisitionOutcome::ManualAcquisitionRequired
                {
                    manual += 1;
                }
            }
            if manual > 0 {
                println!(
                    "{manual} reference(s) require manual acquisition; supply --source id=path once obtained."
                );
            }
            Ok(())
        }
        StarlightValidationCommand::Run(args) => {
            let inputs = crate::starlight::validation::run::RunInputs {
                preregistration: args.preregistration,
                references: args.references,
                regions: args.regions,
                candidate_map: args.candidate_map,
                candidate_map_sha256: args.candidate_map_sha256,
                references_workspace: args.references_workspace,
                output: args.output,
            };
            let results = crate::starlight::validation::run::run(&inputs)?;
            println!(
                "technical_gates_passed={} scientific_review_status={} scientifically_validated={}",
                results.technical_gates_passed,
                results.scientific_review_status,
                results.scientifically_validated
            );
            if !results.technical_gates_passed {
                for failure in &results.technical_gate_failures {
                    println!("gate failure: {failure}");
                }
            }
            Ok(())
        }
    }
}
