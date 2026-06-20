use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::{NsbResult, Target};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

pub fn write_point(
    time: Time<UTC>,
    observer: Geodetic<ECEF>,
    target: Target,
    result: &NsbResult,
) -> Result<()> {
    println!("time_utc: {}", format_utc(time));
    println!(
        "observer: lon={:.6} deg lat={:.6} deg height={:.1} m",
        observer.lon.value(),
        observer.lat.value(),
        observer.height.value()
    );
    println!(
        "target: ra={:.6} deg dec={:.6} deg",
        target.ra().value(),
        target.dec().value()
    );
    println!("component    integrated_ph_cm2_ns_sr        B_s10        V_s10");
    for c in &result.components {
        println!(
            "{:<10} {:>24.8e} {:>12.4} {:>12.4}",
            c.name,
            c.integrated.value(),
            c.b_flux_s10.value(),
            c.v_flux_s10.value()
        );
    }
    println!("{:-<64}", "");
    println!("{:<10} {:>24.8e}", "total", result.integrated.value());
    println!("B_mag_arcsec2 = {:.4}", result.b_mag.value());
    println!("V_mag_arcsec2 = {:.4}", result.v_mag.value());
    Ok(())
}

pub fn write_window(
    min: Option<BandPhotonRadiance>,
    max: BandPhotonRadiance,
    periods: &[Period<UTC>],
) -> Result<()> {
    println!(
        "min_nsb = {}",
        min.map(|v| v.value().to_string())
            .unwrap_or_else(|| "none".into())
    );
    println!("max_nsb = {:.8e} ph cm^-2 ns^-1 sr^-1", max.value());
    if periods.is_empty() {
        println!("(no matching periods)");
        return Ok(());
    }
    println!("start_utc                    end_utc                      duration_s");
    for p in periods {
        let duration = duration_seconds(*p);
        println!(
            "{:<28} {:<28} {:>10.1}",
            format_utc(p.start),
            format_utc(p.end),
            duration
        );
    }
    Ok(())
}

pub fn write_sites(sites: &[SitePreset]) -> Result<()> {
    println!("alias                      name                                  lon_deg      lat_deg     height_m");
    for site in sites {
        println!(
            "{:<26} {:<36} {:>10.6} {:>10.6} {:>10.1}",
            site.canonical_alias, site.display_name, site.lon_deg, site.lat_deg, site.height_m
        );
    }
    Ok(())
}

fn duration_seconds(period: Period<UTC>) -> f64 {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => (end - start).num_milliseconds() as f64 / 1000.0,
        _ => f64::NAN,
    }
}
