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
    /// Independent validation pipeline for issue #102 (acquire references, run comparisons).
    Validation(StarlightValidationArgs),
    /// Pack a frozen candidate-v5 map into a runtime-loadable HEALPix CSV (#102).
    Pack(PackArgs),
    /// Verify a release-candidate manifest and both human decisions, pack a
    /// runtime map, and draft (or `--apply`) the production registry change (#102).
    Promote(PromoteArgs),
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
    /// Convert acquired references onto the candidate map's photon-flux grid.
    Transform(StarlightValidationTransformArgs),
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
struct StarlightValidationTransformArgs {
    /// Path to `references-v1.toml`.
    #[arg(long)]
    references: PathBuf,
    /// Directory holding receipts, objects, and per-reference transform outputs.
    #[arg(long)]
    workspace: PathBuf,
    /// HEALPix nside of the comparison grid (must match the candidate map).
    #[arg(long, default_value_t = 128)]
    nside: u32,
    /// Optional `id=/path` overrides for the acquired source bytes.
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
    /// Write packed runtime assets and production registry entries.
    #[arg(long, default_value_t = false)]
    apply: bool,
}

#[derive(Debug, Args)]
struct PackArgs {
    /// Sparse candidate-v5 HEALPix CSV. Bytes are never rewritten.
    #[arg(long)]
    candidate_map: PathBuf,
    /// Expected SHA-256 of the candidate file.
    #[arg(long)]
    expected_sha256: String,
    /// HEALPix nside of the candidate.
    #[arg(long)]
    nside: u32,
    /// Output packed runtime CSV path.
    #[arg(long)]
    output_csv: PathBuf,
    /// Output pack sidecar TOML path.
    #[arg(long)]
    output_sidecar: PathBuf,
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
        StarlightAction::Pack(args) => return pack_starlight(args),
        StarlightAction::Promote(args) => return promote(args),
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
        StarlightValidationCommand::Transform(args) => {
            let bytes = std::fs::read(&args.references)
                .map_err(|error| anyhow::anyhow!("read {}: {error}", args.references.display()))?;
            let document: crate::starlight::validation::references::ReferencesDocument =
                toml::from_str(std::str::from_utf8(&bytes)?).map_err(|error| {
                    anyhow::anyhow!("parse {}: {error}", args.references.display())
                })?;
            document.validate()?;
            let overrides = args.sources.into_iter().collect::<BTreeMap<_, _>>();
            for reference in &document.references {
                if reference.status
                    != crate::starlight::validation::references::ReferenceStatus::Acquired
                {
                    println!("{}: skipped (not acquired)", reference.id);
                    continue;
                }
                let acquired = if let Some(path) = overrides.get(&reference.id) {
                    PathBuf::from(path)
                } else {
                    let receipt_path = args
                        .workspace
                        .join("receipts")
                        .join(format!("{}.json", reference.id));
                    if receipt_path.is_file() {
                        let receipt: crate::starlight::validation::acquire::ReferenceAcquisitionReceipt =
                            serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
                        receipt.object_path
                    } else {
                        anyhow::bail!(
                            "no acquired bytes for {}; pass --source {}=/path or run acquire first",
                            reference.id,
                            reference.id
                        );
                    }
                };
                let output_dir = args.workspace.join(&reference.id);
                let record =
                    crate::starlight::validation::transforms::transform_acquired_reference(
                        &reference.id,
                        &acquired,
                        &output_dir,
                        args.nside,
                    )?;
                println!(
                    "{}: {:?} — {}",
                    record.reference_id, record.admissibility, record.detail
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

fn pack_starlight(args: PackArgs) -> Result<()> {
    let outcome =
        crate::starlight::pack::pack_candidate_map(&crate::starlight::pack::PackInputs {
            candidate_map: args.candidate_map,
            expected_candidate_sha256: args.expected_sha256,
            expected_nside: args.nside,
            output_csv: args.output_csv,
            output_sidecar: args.output_sidecar,
            provenance_headers: BTreeMap::new(),
        })?;
    println!(
        "packed runtime map sha256={} sidecar sha256={} occupied={} omitted={}",
        outcome.runtime_map_sha256,
        outcome.runtime_sidecar_sha256,
        outcome.occupied_pixels,
        outcome.omitted_pixels
    );
    Ok(())
}

fn promote(args: PromoteArgs) -> Result<()> {
    let inputs = crate::starlight::promotion::PromotionInputs {
        release_candidate: args.release_candidate,
        scientific_decision: args.scientific_decision,
        redistribution_decision: args.redistribution_decision,
        repository_root: args.repository_root,
        output: args.output,
        apply: args.apply,
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
    println!(
        "packed runtime map sha256={} sidecar sha256={}",
        outcome.runtime_map_sha256, outcome.runtime_sidecar_sha256
    );
    if outcome.applied {
        println!(
            "applied production registry entries for {} and {}",
            outcome.runtime_map_path.display(),
            outcome.runtime_sidecar_path.display()
        );
    } else {
        println!(
            "candidate map bytes were not modified; pass --apply after #103 signatures to write production registry entries"
        );
    }
    Ok(())
}
