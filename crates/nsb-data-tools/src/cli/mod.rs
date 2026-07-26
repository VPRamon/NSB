//! Discoverable hierarchical command-line interface for NSB data products.

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::ffi::OsString;

#[derive(Debug, Parser)]
#[command(name = "nsb-data", about = "NSB scientific data-product tools")]
pub struct Cli {
    #[command(subcommand)]
    command: RootCommand,
}

#[derive(Debug, Subcommand)]
enum RootCommand {
    /// Verify runtime scientific assets.
    Assets(AssetsArgs),
    /// Acquire, construct, validate, and release integrated starlight products.
    Starlight(StarlightArgs),
    /// Maintain generated documentation for this tool suite.
    Maintenance(MaintenanceArgs),
}

#[derive(Debug, Args)]
struct AssetsArgs {
    #[command(subcommand)]
    command: AssetsCommand,
}

#[derive(Debug, Subcommand)]
enum AssetsCommand {
    /// Verify scientific-asset schemas, checksums, and headers.
    Verify(ForwardArgs),
}

#[derive(Debug, Args)]
struct StarlightArgs {
    #[command(subcommand)]
    command: StarlightCommand,
}

#[derive(Debug, Subcommand)]
enum StarlightCommand {
    /// Acquire Gaia inputs and official XP bulk products.
    Acquire(AcquireArgs),
    /// Prepare canonical starlight catalogues.
    Catalogue(CatalogueArgs),
    /// Normalize, reconstruct, validate, and process Gaia XP continuous products.
    XpContinuous(XpContinuousArgs),
    /// Generate and consolidate model-development samples.
    Sampling(SamplingArgs),
    /// Build and assess Galactic HEALPix maps.
    Map(MapArgs),
    /// Audit source-accounting and scientific-exclusion evidence.
    Quality(QualityArgs),
    /// Build or export integrated-product inputs.
    Product(ProductArgs),
    /// Package an admitted runtime asset.
    Release(ReleaseArgs),
}

#[derive(Debug, Args)]
struct AcquireArgs {
    #[command(subcommand)]
    command: AcquireCommand,
}

#[derive(Debug, Subcommand)]
enum AcquireCommand {
    /// Execute a reproducible Gaia TAP query.
    Tap(ForwardArgs),
    /// Generate Gaia release inputs and provenance evidence.
    ReleaseInputs(ForwardArgs),
    /// Download or index official XP continuous bulk partitions.
    XpBulk(XpBulkArgs),
}

#[derive(Debug, Args)]
struct XpBulkArgs {
    #[command(subcommand)]
    command: XpBulkCommand,
}

#[derive(Debug, Subcommand)]
enum XpBulkCommand {
    /// Download and checksum-verify official partitions.
    Download(ForwardArgs),
    /// Build deterministic partition and source indexes.
    Index(ForwardArgs),
}

#[derive(Debug, Args)]
struct CatalogueArgs {
    #[command(subcommand)]
    command: CatalogueCommand,
}

#[derive(Debug, Subcommand)]
enum CatalogueCommand {
    /// Prepare canonical source rows from Gaia XP sampled data.
    PrepareGaia(ForwardArgs),
}

#[derive(Debug, Args)]
struct XpContinuousArgs {
    #[command(subcommand)]
    command: XpContinuousCommand,
}

#[derive(Debug, Subcommand)]
enum XpContinuousCommand {
    /// Normalize official coefficient records.
    Normalize(ForwardArgs),
    /// Reconstruct calibrated spectra and photon flux in Rust.
    Reconstruct(ForwardArgs),
    /// Validate reconstruction against the scientific contract.
    Validate(ForwardArgs),
    /// Process one verified partition into checkpointed HEALPix state.
    ProcessPartition(ForwardArgs),
    /// Run the resumable bulk production pipeline.
    RunBulk(ForwardArgs),
}

#[derive(Debug, Args)]
struct SamplingArgs {
    #[command(subcommand)]
    command: SamplingCommand,
}

#[derive(Debug, Subcommand)]
enum SamplingCommand {
    /// Generate deterministic stratified Gaia queries.
    GenerateQueries(ForwardArgs),
    /// Consolidate and spatially split completed samples.
    Consolidate(ForwardArgs),
}

#[derive(Debug, Args)]
struct MapArgs {
    #[command(subcommand)]
    command: MapCommand,
}

#[derive(Debug, Subcommand)]
enum MapCommand {
    /// Build one deterministic HEALPix map.
    Build(ForwardArgs),
    /// Assess candidate resolutions.
    Sweep(ForwardArgs),
    /// Validate a map and its independent-reference evidence.
    Validate(ForwardArgs),
}

#[derive(Debug, Args)]
struct QualityArgs {
    #[command(subcommand)]
    command: QualityCommand,
}

#[derive(Debug, Subcommand)]
enum QualityCommand {
    /// Audit scientific exclusions and source accounting.
    AuditExclusions(ForwardArgs),
}

#[derive(Debug, Args)]
struct ProductArgs {
    #[command(subcommand)]
    command: ProductCommand,
}

#[derive(Debug, Subcommand)]
enum ProductCommand {
    /// Build an integrated starlight candidate.
    BuildIntegrated(ForwardArgs),
    /// Export a runtime HEALPix map into normalized contribution rows.
    ExportContributions(ForwardArgs),
}

#[derive(Debug, Args)]
struct ReleaseArgs {
    #[command(subcommand)]
    command: ReleaseCommand,
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    /// Package a validated map and runtime manifest.
    PackAsset(ForwardArgs),
}

#[derive(Debug, Args)]
struct MaintenanceArgs {
    #[command(subcommand)]
    command: MaintenanceCommand,
}

#[derive(Debug, Subcommand)]
enum MaintenanceCommand {
    /// Render the tool reference from the normative registry.
    RenderToolReference(RenderToolReferenceArgs),
}

#[derive(Debug, Args)]
struct RenderToolReferenceArgs {
    /// Rewrite the checked-in reference. Without this flag, fail when stale.
    #[arg(long)]
    write: bool,
    /// Check that the checked-in reference is current (the default behaviour).
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct ForwardArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<OsString>,
}

fn arguments(name: &str, forwarded: ForwardArgs) -> Vec<OsString> {
    let mut arguments = vec![OsString::from(name)];
    arguments.extend(forwarded.args);
    arguments
}

fn action(name: &str, forwarded: ForwardArgs, run: impl FnOnce() -> Result<()>) -> Result<()> {
    log::info!(target: "nsb_data_tools::cli", "starting {name}");
    let result = crate::with_command_args(arguments(name, forwarded), run);
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::cli", "{name} failed: {error:#}");
    }
    result
}

/// Parse and execute the `nsb-data` command tree.
pub fn run() -> Result<()> {
    crate::platform::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    match Cli::parse().command {
        RootCommand::Assets(assets) => match assets.command {
            AssetsCommand::Verify(args) => action(
                "assets verify",
                args,
                crate::platform::verify_assets::run_cli,
            ),
        },
        RootCommand::Starlight(starlight) => dispatch_starlight(starlight),
        RootCommand::Maintenance(maintenance) => match maintenance.command {
            MaintenanceCommand::RenderToolReference(args) => {
                if args.write && args.check {
                    anyhow::bail!("--write and --check are mutually exclusive");
                }
                crate::platform::tool_catalog::render_reference(args.write)
            }
        },
    }
}

fn dispatch_starlight(starlight: StarlightArgs) -> Result<()> {
    match starlight.command {
        StarlightCommand::Acquire(acquire) => match acquire.command {
            AcquireCommand::Tap(args) => action(
                "starlight acquire tap",
                args,
                crate::starlight::acquisition::query_gaia_tap::run_cli,
            ),
            AcquireCommand::ReleaseInputs(args) => action(
                "starlight acquire release-inputs",
                args,
                crate::starlight::acquisition::generate_gaia_starlight_release_inputs::run_cli,
            ),
            AcquireCommand::XpBulk(bulk) => match bulk.command {
                XpBulkCommand::Download(args) => action(
                    "starlight acquire xp-bulk download",
                    args,
                    crate::starlight::acquisition::download_gaia_xp_continuous_bulk::run_cli,
                ),
                XpBulkCommand::Index(args) => action(
                    "starlight acquire xp-bulk index",
                    args,
                    crate::starlight::acquisition::index_gaia_xp_continuous_bulk::run_cli,
                ),
            },
        },
        StarlightCommand::Catalogue(catalogue) => match catalogue.command {
            CatalogueCommand::PrepareGaia(args) => action(
                "starlight catalogue prepare-gaia",
                args,
                crate::starlight::catalogue::prepare_gaia_starlight_catalogue::run_cli,
            ),
        },
        StarlightCommand::XpContinuous(xp) => match xp.command {
            XpContinuousCommand::Normalize(args) => action(
                "starlight xp-continuous normalize",
                args,
                crate::starlight::xp_continuous::normalize_xp_continuous_coefficients::run_cli,
            ),
            XpContinuousCommand::Reconstruct(args) => action(
                "starlight xp-continuous reconstruct",
                args,
                crate::starlight::xp_continuous::reconstruct_canonical_coefficients::run_cli,
            ),
            XpContinuousCommand::Validate(args) => action(
                "starlight xp-continuous validate",
                args,
                crate::starlight::xp_continuous::validate_xp_continuous_reconstruction::run_cli,
            ),
            XpContinuousCommand::ProcessPartition(args) => action(
                "starlight xp-continuous process-partition",
                args,
                crate::starlight::xp_continuous::process_partition::run_cli,
            ),
            XpContinuousCommand::RunBulk(args) => action(
                "starlight xp-continuous run-bulk",
                args,
                crate::starlight::xp_continuous::run_bulk_pipeline::run_cli,
            ),
        },
        StarlightCommand::Sampling(sampling) => match sampling.command {
            SamplingCommand::GenerateQueries(args) => action(
                "starlight sampling generate-queries",
                args,
                crate::starlight::sampling::generate_starlight_sample_queries::run_cli,
            ),
            SamplingCommand::Consolidate(args) => action(
                "starlight sampling consolidate",
                args,
                crate::starlight::sampling::consolidate_gaia_starlight_samples::run_cli,
            ),
        },
        StarlightCommand::Map(map) => match map.command {
            MapCommand::Build(args) => action(
                "starlight map build",
                args,
                crate::starlight::map::build_starlight_map::run_cli,
            ),
            MapCommand::Sweep(args) => action(
                "starlight map sweep",
                args,
                crate::starlight::map::sweep_starlight_nside::run_cli,
            ),
            MapCommand::Validate(args) => action(
                "starlight map validate",
                args,
                crate::starlight::map::validate_starlight_map::run_cli,
            ),
        },
        StarlightCommand::Quality(quality) => match quality.command {
            QualityCommand::AuditExclusions(args) => action(
                "starlight quality audit-exclusions",
                args,
                crate::starlight::quality::audit_gaia_starlight_exclusions::run_cli,
            ),
        },
        StarlightCommand::Product(product) => match product.command {
            ProductCommand::BuildIntegrated(args) => action(
                "starlight product build-integrated",
                args,
                crate::starlight::product::build_integrated_starlight_product::run_cli,
            ),
            ProductCommand::ExportContributions(args) => action(
                "starlight product export-contributions",
                args,
                crate::starlight::product::export_starlight_contributions::run_cli,
            ),
        },
        StarlightCommand::Release(release) => match release.command {
            ReleaseCommand::PackAsset(args) => action(
                "starlight release pack-asset",
                args,
                crate::starlight::release::pack_starlight_asset::run_cli,
            ),
        },
    }
}
