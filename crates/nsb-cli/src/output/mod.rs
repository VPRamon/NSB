pub mod csv;
pub mod json;
pub mod table;

use crate::cli::OutputFormat;
use crate::parsing::location::SitePreset;
use anyhow::Result;
use nsb::{ComponentMask, NsbResult, Target};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

pub fn write_point(
    format: OutputFormat,
    time: Time<UTC>,
    observer: Geodetic<ECEF>,
    target: Target,
    result: &NsbResult,
) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_point(time, observer, target, result),
        OutputFormat::Json => json::write_point(time, observer, target, result),
        OutputFormat::Csv => csv::write_point(result),
    }
}

pub fn write_window(
    format: OutputFormat,
    start: Time<UTC>,
    end: Time<UTC>,
    min: Option<BandPhotonRadiance>,
    max: BandPhotonRadiance,
    components: ComponentMask,
    periods: &[Period<UTC>],
) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_window(min, max, periods),
        OutputFormat::Json => json::write_window(start, end, min, max, components, periods),
        OutputFormat::Csv => csv::write_window(periods),
    }
}

pub fn write_sites(format: OutputFormat, sites: &[SitePreset]) -> Result<()> {
    match format {
        OutputFormat::Table => table::write_sites(sites),
        OutputFormat::Json => json::write_sites(sites),
        OutputFormat::Csv => csv::write_sites(sites),
    }
}
