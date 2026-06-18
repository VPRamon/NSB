use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::{ComponentMask, NsbResult, Target};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use serde::Serialize;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

#[derive(Serialize)]
struct PointJson {
    time_utc: String,
    observer: ObserverJson,
    target: TargetJson,
    components: Vec<ComponentJson>,
    total: TotalJson,
}

#[derive(Serialize)]
struct ObserverJson {
    longitude_deg: f64,
    latitude_deg: f64,
    height_m: f64,
}

#[derive(Serialize)]
struct TargetJson {
    ra_deg: f64,
    dec_deg: f64,
}

#[derive(Serialize)]
struct ComponentJson {
    name: &'static str,
    integrated_ph_cm2_ns_sr: f64,
    b_s10: f64,
    v_s10: f64,
}

#[derive(Serialize)]
struct TotalJson {
    integrated_ph_cm2_ns_sr: f64,
    b_mag_arcsec2: f64,
    v_mag_arcsec2: f64,
}

#[derive(Serialize)]
struct WindowJson {
    start_utc: String,
    end_utc: String,
    min_nsb_ph_cm2_ns_sr: Option<f64>,
    max_nsb_ph_cm2_ns_sr: f64,
    components: Vec<&'static str>,
    periods: Vec<PeriodJson>,
}

#[derive(Serialize)]
struct PeriodJson {
    start_utc: String,
    end_utc: String,
    duration_seconds: Option<f64>,
}

pub fn write_point(
    time: Time<UTC>,
    observer: Geodetic<ECEF>,
    target: Target,
    result: &NsbResult,
) -> Result<()> {
    let payload = PointJson {
        time_utc: format_utc(time),
        observer: ObserverJson {
            longitude_deg: observer.lon.value(),
            latitude_deg: observer.lat.value(),
            height_m: observer.height.value(),
        },
        target: TargetJson {
            ra_deg: target.ra().value(),
            dec_deg: target.dec().value(),
        },
        components: result
            .components
            .iter()
            .map(|c| ComponentJson {
                name: c.name,
                integrated_ph_cm2_ns_sr: c.integrated.value(),
                b_s10: c.b_flux_s10.value(),
                v_s10: c.v_flux_s10.value(),
            })
            .collect(),
        total: TotalJson {
            integrated_ph_cm2_ns_sr: result.integrated.value(),
            b_mag_arcsec2: result.b_mag.value(),
            v_mag_arcsec2: result.v_mag.value(),
        },
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

pub fn write_window(
    start: Time<UTC>,
    end: Time<UTC>,
    min: Option<BandPhotonRadiance>,
    max: BandPhotonRadiance,
    components: ComponentMask,
    periods: &[Period<UTC>],
) -> Result<()> {
    let payload = WindowJson {
        start_utc: format_utc(start),
        end_utc: format_utc(end),
        min_nsb_ph_cm2_ns_sr: min.map(|v| v.value()),
        max_nsb_ph_cm2_ns_sr: max.value(),
        components: component_names(components),
        periods: periods
            .iter()
            .map(|p| PeriodJson {
                start_utc: format_utc(p.start),
                end_utc: format_utc(p.end),
                duration_seconds: duration_seconds(*p),
            })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

pub fn write_sites(sites: &[SitePreset]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(sites)?);
    Ok(())
}

fn component_names(mask: ComponentMask) -> Vec<&'static str> {
    let mut out = Vec::new();
    if mask.contains(ComponentMask::ZODIACAL) {
        out.push("zodiacal");
    }
    if mask.contains(ComponentMask::STARLIGHT) {
        out.push("starlight");
    }
    if mask.contains(ComponentMask::AIRGLOW) {
        out.push("airglow");
    }
    if mask.contains(ComponentMask::MOON) {
        out.push("moon");
    }
    out
}

fn duration_seconds(period: Period<UTC>) -> Option<f64> {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
        _ => None,
    }
}
