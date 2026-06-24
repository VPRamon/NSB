use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::{
    assets::asset_registry, ComponentMask, NsbModelConfig, NsbResult, MODEL_VERSION, NSB_VERSION,
    SIDERUST_REVISION,
};
use tempoch::{Period, UTC};

const POINT_SCHEMA: &str = "nsb-cli-point-csv-v1";
const WINDOW_SCHEMA: &str = "nsb-cli-window-csv-v1";

pub fn write_point(config: &NsbModelConfig, result: &NsbResult) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record([
        "schema_version",
        "record_type",
        "component",
        "integrated_ph_cm2_ns_sr",
        "b_s10_diagnostic",
        "v_s10_diagnostic",
        "b_mag_arcsec2_diagnostic",
        "v_mag_arcsec2_diagnostic",
        "relative_uncertainty",
        "calibration_status",
        "provenance",
        "validated_domain",
        "band_convention",
        "nsb_version",
        "model_version",
        "siderust_revision",
        "model_preset",
        "asset_checksums",
    ])?;
    let assets = asset_checksums();
    for component in &result.components {
        writer.write_record([
            POINT_SCHEMA.to_string(),
            "component".to_string(),
            component.name.to_string(),
            component.integrated.value().to_string(),
            component.b_flux_s10.value().to_string(),
            component.v_flux_s10.value().to_string(),
            String::new(),
            String::new(),
            component
                .relative_uncertainty
                .map(|value| value.to_string())
                .unwrap_or_default(),
            component.metadata.status.as_str().to_string(),
            component.metadata.provenance.to_string(),
            component.metadata.validated_domain.to_string(),
            component.metadata.band_diagnostic.convention.to_string(),
            NSB_VERSION.to_string(),
            MODEL_VERSION.to_string(),
            SIDERUST_REVISION.to_string(),
            config.site_profile.as_str().to_string(),
            assets.clone(),
        ])?;
    }
    writer.write_record([
        POINT_SCHEMA.to_string(),
        "total".to_string(),
        "total".to_string(),
        result.integrated.value().to_string(),
        String::new(),
        String::new(),
        result.b_mag.value().to_string(),
        result.v_mag.value().to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        result.band_diagnostic.convention.to_string(),
        NSB_VERSION.to_string(),
        MODEL_VERSION.to_string(),
        SIDERUST_REVISION.to_string(),
        config.site_profile.as_str().to_string(),
        assets,
    ])?;
    writer.flush()?;
    Ok(())
}

pub fn write_window(
    components: ComponentMask,
    config: &NsbModelConfig,
    periods: &[Period<UTC>],
) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record([
        "schema_version",
        "start_utc",
        "end_utc",
        "duration_seconds",
        "components",
        "nsb_version",
        "model_version",
        "siderust_revision",
        "model_preset",
        "asset_checksums",
    ])?;
    let component_names = component_names(components).join(";");
    let assets = asset_checksums();
    for period in periods {
        writer.write_record([
            WINDOW_SCHEMA.to_string(),
            format_utc(period.start),
            format_utc(period.end),
            duration_seconds(*period)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            component_names.clone(),
            NSB_VERSION.to_string(),
            MODEL_VERSION.to_string(),
            SIDERUST_REVISION.to_string(),
            config.site_profile.as_str().to_string(),
            assets.clone(),
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

fn asset_checksums() -> String {
    asset_registry()
        .assets
        .iter()
        .filter(|asset| asset.runtime_embedded)
        .map(|asset| format!("{}={}", asset.path, asset.sha256))
        .collect::<Vec<_>>()
        .join(";")
}

fn component_names(mask: ComponentMask) -> Vec<&'static str> {
    let mut names = Vec::new();
    if mask.contains(ComponentMask::ZODIACAL) {
        names.push("zodiacal");
    }
    if mask.contains(ComponentMask::STARLIGHT) {
        names.push("experimental-starlight");
    }
    if mask.contains(ComponentMask::AIRGLOW) {
        names.push("airglow");
    }
    if mask.contains(ComponentMask::MOON) {
        names.push("moon");
    }
    names
}

fn duration_seconds(period: Period<UTC>) -> Option<f64> {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
        _ => None,
    }
}
