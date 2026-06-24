pub mod csv;
pub mod json;
pub mod table;

use crate::cli::OutputFormat;
use crate::parsing::location::SitePreset;
use anyhow::Result;
use nsb::{ComponentMask, NsbComponentDescriptor, NsbModelConfig, NsbResult, Target};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

pub struct WindowOutput<'a> {
    pub start: Time<UTC>,
    pub end: Time<UTC>,
    pub min: Option<BandPhotonRadiance>,
    pub max: BandPhotonRadiance,
    pub components: ComponentMask,
    pub config: &'a NsbModelConfig,
    pub descriptions: &'a [NsbComponentDescriptor],
    pub periods: &'a [Period<UTC>],
}

pub fn write_point(
    format: OutputFormat,
    time: Time<UTC>,
    observer: Geodetic<ECEF>,
    target: Target,
    config: &NsbModelConfig,
    result: &NsbResult,
) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_point(time, observer, target, result),
        OutputFormat::Json => json::write_point(time, observer, target, config, result),
        OutputFormat::Csv => csv::write_point(config, result),
    }
}

pub fn write_window(format: OutputFormat, output: &WindowOutput<'_>) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_window(output.min, output.max, output.periods),
        OutputFormat::Json => json::write_window(output),
        OutputFormat::Csv => csv::write_window(output.components, output.config, output.periods),
    }
}

pub fn write_sites(format: OutputFormat, sites: &[SitePreset]) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_sites(sites),
        OutputFormat::Json => json::write_sites(sites),
        OutputFormat::Csv => csv::write_sites(sites),
    }
}
