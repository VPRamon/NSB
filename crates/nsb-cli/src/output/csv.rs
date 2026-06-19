use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::NsbResult;
use tempoch::{Period, UTC};

pub fn write_point(result: &NsbResult) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record([
        "component",
        "integrated_ph_cm2_ns_sr",
        "b_s10",
        "v_s10",
        "b_mag_arcsec2",
        "v_mag_arcsec2",
    ])?;
    for c in &result.components {
        writer.write_record([
            c.name.to_string(),
            c.integrated.value().to_string(),
            c.b_flux_s10.value().to_string(),
            c.v_flux_s10.value().to_string(),
            String::new(),
            String::new(),
        ])?;
    }
    writer.write_record([
        "total".to_string(),
        result.integrated.value().to_string(),
        String::new(),
        String::new(),
        result.b_mag.value().to_string(),
        result.v_mag.value().to_string(),
    ])?;
    writer.flush()?;
    Ok(())
}

pub fn write_window(periods: &[Period<UTC>]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record(["start_utc", "end_utc", "duration_seconds"])?;
    for p in periods {
        writer.write_record([
            format_utc(p.start),
            format_utc(p.end),
            duration_seconds(*p)
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

pub fn write_sites(sites: &[SitePreset]) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record(["alias", "name", "lon_deg", "lat_deg", "height_m", "aliases"])?;
    for site in sites {
        writer.write_record([
            site.canonical_alias.to_string(),
            site.display_name.to_string(),
            site.lon_deg.to_string(),
            site.lat_deg.to_string(),
            site.height_m.to_string(),
            site.aliases.join(";"),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn duration_seconds(period: Period<UTC>) -> Option<f64> {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
        _ => None,
    }
}
