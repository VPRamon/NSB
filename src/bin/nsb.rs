//! Command-line interface for the `nsb` crate.
//!
//! Scientific role:
//! this binary is the user-facing entry point for asking scientific questions
//! of the model without writing Rust code.
//!
//! Contribution to the science:
//! it exposes the same point-evaluation and threshold-window calculations as
//! the library API, making the NSB model easier to inspect, validate, and use
//! in observing-planning workflows. The CLI does not add new physics; it makes
//! the implemented science operational for end users.

use anyhow::{anyhow, Context};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::{Args, Parser, Subcommand, ValueEnum};
use nsb::{
    AirglowModel, ComponentMask, Location, MoonlightModel, NsbEvaluator, NsbModelConfig,
    PointQuery, Site, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use tempoch::{Period, Time, UTC};

#[derive(Debug, Parser)]
#[command(
    name = "nsb",
    about = "Night Sky Background queries (point evaluation and threshold-period search).",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compute the NSB at a single instant for a target.
    Point(PointArgs),
    /// Find UTC sub-periods within a window where the NSB is below a threshold.
    Window(WindowArgs),
}

#[derive(Debug, Args)]
struct LocationArgs {
    /// Named CTAO site (CTAO-S = Paranal, CTAO-N = La Palma).
    #[arg(long, conflicts_with_all = ["lat", "lon", "alt"])]
    site: Option<String>,

    /// Geodetic latitude in degrees (north-positive).
    #[arg(long, requires_all = ["lon", "alt"], allow_hyphen_values = true)]
    lat: Option<f64>,

    /// Geodetic longitude in degrees (east-positive).
    #[arg(long, requires_all = ["lat", "alt"], allow_hyphen_values = true)]
    lon: Option<f64>,

    /// Geodetic height above the ellipsoid, in metres.
    #[arg(long, requires_all = ["lat", "lon"], allow_hyphen_values = true)]
    alt: Option<f64>,
}

#[derive(Debug, Args)]
struct TargetArgs {
    /// Target right ascension (ICRS / J2000), in degrees.
    #[arg(long, allow_hyphen_values = true)]
    ra: f64,
    /// Target declination (ICRS / J2000), in degrees.
    #[arg(long, allow_hyphen_values = true)]
    dec: f64,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ComponentArg {
    Zodiacal,
    Starlight,
    Airglow,
    Moon,
}

impl ComponentArg {
    fn mask(self) -> ComponentMask {
        match self {
            Self::Zodiacal => ComponentMask::ZODIACAL,
            Self::Starlight => ComponentMask::STARLIGHT,
            Self::Airglow => ComponentMask::AIRGLOW,
            Self::Moon => ComponentMask::MOON,
        }
    }
}

#[derive(Debug, Args)]
struct ComponentArgs {
    /// Include all components (zodiacal + starlight + airglow + moon).
    #[arg(long, conflicts_with = "component")]
    all: bool,

    /// Components to include. May be repeated. Defaults to all components.
    #[arg(long = "component", value_enum)]
    component: Vec<ComponentArg>,
}

impl ComponentArgs {
    fn resolve(&self) -> ComponentMask {
        if self.all {
            return ComponentMask::ALL;
        }
        if self.component.is_empty() {
            return ComponentMask::ALL;
        }
        self.component
            .iter()
            .fold(ComponentMask::empty(), |acc, c| acc | c.mask())
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModelArg {
    Best,
    PythonParity,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AirglowModelArg {
    PythonPolynomial,
    SkycalcContinuum,
}

impl AirglowModelArg {
    fn model(self) -> AirglowModel {
        match self {
            Self::PythonPolynomial => AirglowModel::PythonPolynomial,
            Self::SkycalcContinuum => AirglowModel::SkyCalcContinuum,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum MoonModelArg {
    KrisciunasSchaefer,
    Jones2013Spectral,
}

impl MoonModelArg {
    fn model(self) -> MoonlightModel {
        match self {
            Self::KrisciunasSchaefer => MoonlightModel::KrisciunasSchaefer1991,
            Self::Jones2013Spectral => MoonlightModel::Jones2013Spectral,
        }
    }
}

#[derive(Debug, Args)]
struct ModelArgs {
    /// Overall model preset.
    #[arg(long, value_enum, default_value = "best")]
    model: ModelArg,

    /// Override the airglow model selected by --model.
    #[arg(long = "airglow-model", value_enum)]
    airglow_model: Option<AirglowModelArg>,

    /// Override the moonlight model selected by --model.
    #[arg(long = "moon-model", value_enum)]
    moon_model: Option<MoonModelArg>,

    /// Solar radio flux F10.7 in solar flux units for SkyCalc airglow.
    #[arg(long)]
    solar_radio_flux_sfu: Option<f64>,
}

impl ModelArgs {
    fn resolve(&self) -> NsbModelConfig {
        let mut config = match self.model {
            ModelArg::Best => NsbModelConfig::best_science(),
            ModelArg::PythonParity => NsbModelConfig::python_parity(),
        };
        if let Some(model) = self.airglow_model {
            config.airglow_model = model.model();
        }
        if let Some(model) = self.moon_model {
            config.moonlight_model = model.model();
        }
        if let Some(flux) = self.solar_radio_flux_sfu {
            config.solar_radio_flux_sfu = flux;
        }
        config
    }
}

#[derive(Debug, Args)]
struct PointArgs {
    /// Observation time, RFC3339 (e.g. `2023-09-04T01:48:00Z`) or
    /// `YYYY-MM-DD HH:MM:SS` (UTC).
    #[arg(long)]
    time: String,

    #[command(flatten)]
    location: LocationArgs,

    #[command(flatten)]
    target: TargetArgs,

    #[command(flatten)]
    components: ComponentArgs,

    #[command(flatten)]
    model: ModelArgs,
}

#[derive(Debug, Args)]
struct WindowArgs {
    /// Window start (UTC).
    #[arg(long)]
    start: String,
    /// Window end (UTC).
    #[arg(long)]
    end: String,
    /// Threshold integrated radiance, in `ph/(cm^2 ns sr)`.
    #[arg(long)]
    threshold: f64,
    /// Coarse-scan cadence in seconds.
    #[arg(long, default_value_t = 600.0)]
    step_seconds: f64,
    /// Pre-filter: drop sub-windows where the Sun is above this altitude
    /// (degrees). Defaults to −18° (astronomical twilight). Use a value
    /// of `90` to disable the Sun pre-filter.
    #[arg(long, default_value_t = -18.0, allow_hyphen_values = true)]
    sun_altitude_ceiling: f64,
    /// Pre-filter: drop sub-windows where the target is below this
    /// altitude (degrees). Defaults to 0° (geometric horizon). Use a
    /// value of `-90` to disable the target pre-filter.
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    target_altitude_floor: f64,
    /// Disable both pre-filters (legacy uniform-scan semantics).
    #[arg(long)]
    no_pre_filter: bool,

    #[command(flatten)]
    location: LocationArgs,

    #[command(flatten)]
    target: TargetArgs,

    #[command(flatten)]
    components: ComponentArgs,

    #[command(flatten)]
    model: ModelArgs,
}

fn parse_utc(s: &str) -> anyhow::Result<Time<UTC>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(Time::<UTC>::from_chrono(dt.with_timezone(&Utc)));
    }
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .with_context(|| format!("invalid UTC timestamp: {s:?}"))?;
    Ok(Time::<UTC>::from_chrono(
        DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc),
    ))
}

fn resolve_location(args: &LocationArgs) -> anyhow::Result<Location> {
    if let Some(name) = args.site.as_deref() {
        let site = Site::from_name(name).map_err(|e| anyhow!("{e}"))?;
        return Ok(Location::NamedSite(site));
    }
    match (args.lat, args.lon, args.alt) {
        (Some(lat), Some(lon), Some(alt)) => Ok(Location::Geodetic(Geodetic::<ECEF>::new(
            Degrees::new(lat),
            Degrees::new(lon),
            Meters::new(alt),
        ))),
        _ => Err(anyhow!(
            "must provide either --site or all of --lat, --lon and --alt"
        )),
    }
}

fn resolve_target(args: &TargetArgs) -> Target {
    Target::new(args.ra * DEG, args.dec * DEG)
}

fn run_point(args: PointArgs) -> anyhow::Result<()> {
    let query = PointQuery {
        location: resolve_location(&args.location)?,
        time: parse_utc(&args.time)?,
        target: resolve_target(&args.target),
        components: args.components.resolve(),
    };
    let evaluator = NsbEvaluator::with_config(args.model.resolve()).map_err(|e| anyhow!("{e}"))?;
    let r = evaluator.evaluate(&query).map_err(|e| anyhow!("{e}"))?;

    for c in &r.components {
        println!(
            "{:>10}: integrated = {:.6e}  ph/(cm² ns sr)  | B = {:.3} S10 | V = {:.3} S10",
            c.name,
            c.integrated.value(),
            c.b_flux_s10.value(),
            c.v_flux_s10.value()
        );
    }
    println!("--------------------");
    println!("    total: {:.6e} ph/(cm² ns sr)", r.integrated.value());
    println!("       B = {:.3} mag/arcsec²", r.b_mag.value());
    println!("       V = {:.3} mag/arcsec²", r.v_mag.value());
    Ok(())
}

fn run_window(args: WindowArgs) -> anyhow::Result<()> {
    let start = parse_utc(&args.start)?;
    let end = parse_utc(&args.end)?;
    let (sun_altitude_ceiling, target_altitude_floor) = if args.no_pre_filter {
        (None, None)
    } else {
        (
            Some(Degrees::new(args.sun_altitude_ceiling)),
            Some(Degrees::new(args.target_altitude_floor)),
        )
    };
    let query = ThresholdQuery {
        location: resolve_location(&args.location)?,
        target: resolve_target(&args.target),
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(args.threshold),
        components: args.components.resolve(),
        sample_step: Second::new(args.step_seconds),
        sun_altitude_ceiling,
        target_altitude_floor,
    };
    let evaluator = NsbEvaluator::with_config(args.model.resolve()).map_err(|e| anyhow!("{e}"))?;
    let result = evaluator
        .periods_below_threshold(&query)
        .map_err(|e| anyhow!("{e}"))?;

    println!(
        "threshold = {:.6e} ph/(cm² ns sr)",
        result.threshold.value()
    );
    if result.periods.is_empty() {
        println!("(no sub-periods in window are below threshold)");
        return Ok(());
    }
    println!("{} period(s) below threshold:", result.periods.len());
    for (i, p) in result.periods.iter().enumerate() {
        println!("  [{i:>3}] {} → {}", format_utc(p.start), format_utc(p.end));
    }
    Ok(())
}

fn format_utc(t: Time<UTC>) -> String {
    match t.to_chrono() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        None => "<unrepresentable UTC instant>".to_string(),
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Point(args) => run_point(args),
        Command::Window(args) => run_window(args),
    }
}
