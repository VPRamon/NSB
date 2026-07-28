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
    /// Repair conservative Starlight derivatives from the canonical nside=128 map.
    #[command(name = "_repair-starlight-resolution-sweep", hide = true)]
    RepairStarlightResolutionSweep {
        #[arg(long)]
        data_dir: PathBuf,
    },
    #[command(name = "_worker", hide = true)]
    Worker(WorkerArgs),
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
    Starlight(ActionArgs),
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
            DatasetCommand::Starlight(args) => execute(DatasetName::Starlight, args),
        },
        Command::Run(args) => match args.command {
            RunCommand::Status { run } => dataset::status(&run),
            RunCommand::Resume { run } => dataset::resume(&run),
        },
        Command::RepairStarlightResolutionSweep { data_dir } => {
            crate::starlight::map::product::repair_resolution_sweep(&data_dir)
        }
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
