//! Dataset-oriented command-line interface.

use crate::dataset::{self, DatasetName, Executor, Operation};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
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
    Starlight(StarlightArgs),
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
struct StarlightArgs {
    #[command(subcommand)]
    action: StarlightAction,
}

#[derive(Debug, Subcommand)]
enum StarlightAction {
    Update(CommonArgs),
    Build(CommonArgs),
    Validate(CommonArgs),
    Publish(CommonArgs),
    /// Verify a release-candidate manifest and both human decisions, then
    /// draft (but never apply) the production manifest change (#89).
    Promote(PromoteArgs),
}

#[derive(Debug, Args)]
struct PromoteArgs {
    /// Path to the `nsb-starlight-release-candidate-v1` manifest.
    #[arg(long)]
    release_candidate: PathBuf,
    /// Path to the recorded scientific review decision JSON.
    #[arg(long)]
    scientific_decision: PathBuf,
    /// Path to the recorded redistribution review decision JSON.
    #[arg(long)]
    redistribution_decision: PathBuf,
    /// Repository root used to resolve and checksum the candidate map and
    /// its asset registry entry.
    #[arg(long)]
    repository_root: PathBuf,
    /// Optional path to write the draft production manifest fragment.
    #[arg(long)]
    output: Option<PathBuf>,
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
            DatasetCommand::Starlight(args) => match args.action {
                StarlightAction::Update(common) => execute(
                    DatasetName::Starlight,
                    ActionArgs {
                        operation: Action::Update(common),
                    },
                ),
                StarlightAction::Build(common) => execute(
                    DatasetName::Starlight,
                    ActionArgs {
                        operation: Action::Build(common),
                    },
                ),
                StarlightAction::Validate(common) => execute(
                    DatasetName::Starlight,
                    ActionArgs {
                        operation: Action::Validate(common),
                    },
                ),
                StarlightAction::Publish(common) => execute(
                    DatasetName::Starlight,
                    ActionArgs {
                        operation: Action::Publish(common),
                    },
                ),
                StarlightAction::Promote(args) => promote(args),
            },
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

fn promote(args: PromoteArgs) -> Result<()> {
    let inputs = crate::starlight::promotion::PromotionInputs {
        release_candidate: args.release_candidate,
        scientific_decision: args.scientific_decision,
        redistribution_decision: args.redistribution_decision,
        repository_root: args.repository_root,
        output: args.output,
    };
    let outcome = crate::starlight::promotion::run_promotion(&inputs)?;
    match &outcome.written_to {
        Some(path) => println!(
            "promotion checks passed; draft production manifest written to {}",
            path.display()
        ),
        None => {
            println!("promotion checks passed; draft production manifest fragment:");
            println!("{}", outcome.draft_manifest_fragment);
        }
    }
    println!("no map bytes or repository manifest.toml were modified; a maintainer must apply this draft manually as part of the #47 promotion PR");
    Ok(())
}
