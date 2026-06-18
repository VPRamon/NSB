use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "nsb")]
#[command(about = "Night Sky Background evaluator and planner")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Table)]
    pub format: OutputFormat,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Evaluate NSB at a single UTC instant.
    Point(PointArgs),
    /// Find UTC periods satisfying NSB bounds.
    Window(WindowArgs),
    /// Inspect CLI-owned site aliases.
    Sites(SitesArgs),
    /// Generate or validate CLI configuration files.
    Config(ConfigArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

#[derive(Debug, Args)]
pub struct ObserverArgs {
    /// Named site alias, for example CTAO-S or PARANAL.
    #[arg(long, conflicts_with_all = ["lon", "lat", "height"])]
    pub site: Option<String>,

    /// Geodetic longitude in degrees, east-positive.
    #[arg(long, requires_all = ["lat", "height"], allow_hyphen_values = true)]
    pub lon: Option<f64>,

    /// Geodetic latitude in degrees, north-positive.
    #[arg(long, requires_all = ["lon", "height"], allow_hyphen_values = true)]
    pub lat: Option<f64>,

    /// Geodetic height above the ellipsoid in meters.
    #[arg(long, requires_all = ["lon", "lat"], allow_hyphen_values = true)]
    pub height: Option<f64>,
}

#[derive(Debug, Args)]
pub struct TargetArgs {
    /// Target right ascension in degrees, ICRS/J2000.
    #[arg(long, allow_hyphen_values = true)]
    pub ra: f64,

    /// Target declination in degrees, ICRS/J2000.
    #[arg(long, allow_hyphen_values = true)]
    pub dec: f64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MoonlightModelArg {
    Jones2013,
    Ks1991,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ZodiacalExtinctionArg {
    Noll2012,
    None,
}

#[derive(Debug, Args)]
pub struct ModelArgs {
    /// Components to include: comma-separated zodiacal,starlight,airglow,moon or all.
    #[arg(long, default_value = "zodiacal,airglow")]
    pub components: String,

    /// Moonlight model.
    #[arg(long, value_enum, default_value_t = MoonlightModelArg::Jones2013)]
    pub moonlight_model: MoonlightModelArg,

    /// Solar radio flux F10.7 in solar flux units for airglow.
    #[arg(long)]
    pub solar_radio_flux_sfu: Option<f64>,

    /// Zodiacal atmospheric extinction model.
    #[arg(long, value_enum, default_value_t = ZodiacalExtinctionArg::Noll2012)]
    pub zodiacal_extinction: ZodiacalExtinctionArg,
}

#[derive(Debug, Args)]
pub struct PointArgs {
    /// Observation time in UTC RFC3339 format.
    #[arg(long)]
    pub time: String,

    #[command(flatten)]
    pub observer: ObserverArgs,

    #[command(flatten)]
    pub target: TargetArgs,

    #[command(flatten)]
    pub model: ModelArgs,
}

#[derive(Debug, Args)]
pub struct WindowArgs {
    /// Window start in UTC RFC3339 format.
    #[arg(long)]
    pub start: String,

    /// Window end in UTC RFC3339 format.
    #[arg(long)]
    pub end: String,

    #[command(flatten)]
    pub observer: ObserverArgs,

    #[command(flatten)]
    pub target: TargetArgs,

    /// Minimum allowed NSB radiance in ph cm^-2 ns^-1 sr^-1.
    #[arg(long)]
    pub min_nsb: Option<f64>,

    /// Maximum allowed NSB radiance in ph cm^-2 ns^-1 sr^-1.
    #[arg(long)]
    pub max_nsb: f64,

    /// Maximum Sun altitude in degrees for the pre-filter.
    #[arg(long, default_value_t = -18.0, allow_hyphen_values = true)]
    pub sun_altitude_max: f64,

    /// Minimum target altitude in degrees for the pre-filter.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    pub target_altitude_min: f64,

    /// Coarse threshold-search scan step in seconds.
    #[arg(long, default_value_t = 600.0)]
    pub step: f64,

    /// Disable Sun and target-altitude pre-filters.
    #[arg(long)]
    pub no_pre_filter: bool,

    #[command(flatten)]
    pub model: ModelArgs,
}

#[derive(Debug, Args)]
pub struct SitesArgs {
    #[command(subcommand)]
    pub command: SitesCommand,
}

#[derive(Debug, Subcommand)]
pub enum SitesCommand {
    /// List all known site aliases.
    List,
    /// Show one site alias.
    Show { alias: String },
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print a starter TOML configuration to stdout.
    Init,
    /// Validate a TOML configuration file.
    Validate { path: PathBuf },
}
