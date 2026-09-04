use super::WindowOutput;
use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::{
    assets::asset_registry, AirglowGeometryMetadata, ComponentMask, NsbModelConfig, NsbResult,
    StarlightModel, MODEL_VERSION, NSB_VERSION, SIDERUST_SOURCE,
};
use tempoch::{Period, UTC};

const POINT_SCHEMA_V3: &str = "nsb-cli-point-csv-v3";
const POINT_SCHEMA_V4: &str = "nsb-cli-point-csv-v4";
const WINDOW_SCHEMA_V3: &str = "nsb-cli-window-csv-v3";

const AIRGLOW_GEOMETRY_COLUMNS: [&str; 17] = [
    "airglow_geometry_model",
    "airglow_geometry_version",
    "airglow_geometry_emission_height_km",
    "airglow_profile_id",
    "airglow_profile_schema_version",
    "airglow_profile_checksum_sha256",
    "airglow_profile_normalization",
    "airglow_profile_altitude_min_km",
    "airglow_profile_altitude_max_km",
    "airglow_profile_wavelength_min_nm",
    "airglow_profile_wavelength_max_nm",
    "airglow_profile_wavelength_band",
    "airglow_geometry_assumptions",
    "airglow_profile_validated_zenith_min_deg",
    "airglow_profile_validated_zenith_max_deg",
    "airglow_geometry_provenance",
    "airglow_profile_license",
];

pub fn write_point(config: &NsbModelConfig, result: &NsbResult) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    write_point_to(&mut writer, config, result)?;
    writer.flush()?;
    Ok(())
}

fn write_point_to<W: std::io::Write>(
    writer: &mut csv::Writer<W>,
    config: &NsbModelConfig,
    result: &NsbResult,
) -> Result<()> {
    let has_absolute_uncertainty = result.components.iter().any(|component| {
        component.statistical_uncertainty.is_some()
            || component.systematic_uncertainty.is_some()
            || component.total_uncertainty.is_some()
    });
    let mut header = vec![
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
        "siderust_source",
        "model_preset",
        "asset_checksums",
    ];
    header.extend(AIRGLOW_GEOMETRY_COLUMNS);
    if has_absolute_uncertainty {
        header.extend([
            "statistical_uncertainty_ph_cm2_ns_sr",
            "systematic_uncertainty_ph_cm2_ns_sr",
            "total_uncertainty_ph_cm2_ns_sr",
        ]);
    }
    writer.write_record(header)?;
    let point_schema = if has_absolute_uncertainty {
        POINT_SCHEMA_V4
    } else {
        POINT_SCHEMA_V3
    };
    let assets = asset_checksums();
    for component in &result.components {
        let mut row = vec![
            point_schema.to_string(),
            "component".to_string(),
            component_label(component.name, config).to_string(),
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
            SIDERUST_SOURCE.to_string(),
            config.site_profile.as_str().to_string(),
            assets.clone(),
        ];
        row.extend(airglow_geometry_fields(
            component.metadata.airglow_geometry.as_ref(),
        ));
        if has_absolute_uncertainty {
            row.extend([
                component
                    .statistical_uncertainty
                    .map(|value| value.value().to_string())
                    .unwrap_or_default(),
                component
                    .systematic_uncertainty
                    .map(|value| value.value().to_string())
                    .unwrap_or_default(),
                component
                    .total_uncertainty
                    .map(|value| value.value().to_string())
                    .unwrap_or_default(),
            ]);
        }
        writer.write_record(row)?;
    }
    let mut total_row = vec![
        point_schema.to_string(),
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
        SIDERUST_SOURCE.to_string(),
        config.site_profile.as_str().to_string(),
        assets,
    ];
    total_row.extend(airglow_geometry_fields(None));
    if has_absolute_uncertainty {
        total_row.extend([String::new(), String::new(), String::new()]);
    }
    writer.write_record(total_row)?;
    Ok(())
}

pub fn write_window(output: &WindowOutput<'_>) -> Result<()> {
    let mut writer = csv::Writer::from_writer(std::io::stdout());
    let mut header = vec![
        "schema_version",
        "record_type",
        "start_utc",
        "end_utc",
        "duration_seconds",
        "components",
        "nsb_version",
        "model_version",
        "siderust_source",
        "model_preset",
        "asset_checksums",
    ];
    header.extend(AIRGLOW_GEOMETRY_COLUMNS);
    writer.write_record(header)?;
    let component_names = component_names(output.components, output.config).join(";");
    let assets = asset_checksums();
    let geometry = output
        .descriptions
        .iter()
        .find_map(|description| description.metadata.airglow_geometry.as_ref());
    let mut summary_row = vec![
        WINDOW_SCHEMA_V3.to_string(),
        "query_summary".to_string(),
        format_utc(output.start),
        format_utc(output.end),
        duration_seconds(Period::new(output.start, output.end))
            .map(|value| value.to_string())
            .unwrap_or_default(),
        component_names.clone(),
        NSB_VERSION.to_string(),
        MODEL_VERSION.to_string(),
        SIDERUST_SOURCE.to_string(),
        output.config.site_profile.as_str().to_string(),
        assets.clone(),
    ];
    summary_row.extend(airglow_geometry_fields(geometry));
    writer.write_record(summary_row)?;
    for period in output.periods {
        let mut row = vec![
            WINDOW_SCHEMA_V3.to_string(),
            "period".to_string(),
            format_utc(period.start),
            format_utc(period.end),
            duration_seconds(*period)
                .map(|value| value.to_string())
                .unwrap_or_default(),
            component_names.clone(),
            NSB_VERSION.to_string(),
            MODEL_VERSION.to_string(),
            SIDERUST_SOURCE.to_string(),
            output.config.site_profile.as_str().to_string(),
            assets.clone(),
        ];
        row.extend(airglow_geometry_fields(geometry));
        writer.write_record(row)?;
    }
    writer.flush()?;
    Ok(())
}

fn airglow_geometry_fields(metadata: Option<&AirglowGeometryMetadata>) -> [String; 17] {
    let Some(metadata) = metadata else {
        return std::array::from_fn(|_| String::new());
    };
    [
        metadata.model.to_string(),
        metadata.implementation_version.to_string(),
        metadata
            .emission_height_km
            .map(|value| value.value().to_string())
            .unwrap_or_default(),
        metadata.profile_id.clone().unwrap_or_default(),
        metadata
            .profile_schema_version
            .map(|value| value.to_string())
            .unwrap_or_default(),
        metadata.checksum_sha256.clone().unwrap_or_default(),
        metadata.normalization.unwrap_or_default().to_string(),
        metadata
            .altitude_min_km
            .map(|value| value.value().to_string())
            .unwrap_or_default(),
        metadata
            .altitude_max_km
            .map(|value| value.value().to_string())
            .unwrap_or_default(),
        metadata
            .wavelength_min_nm
            .map(|value| value.value().to_string())
            .unwrap_or_default(),
        metadata
            .wavelength_max_nm
            .map(|value| value.value().to_string())
            .unwrap_or_default(),
        metadata.wavelength_band.clone().unwrap_or_default(),
        metadata.assumptions.clone(),
        metadata.validated_zenith.min.value().to_string(),
        metadata.validated_zenith.max.value().to_string(),
        metadata.provenance.clone(),
        metadata.license.clone().unwrap_or_default(),
    ]
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

fn component_names(mask: ComponentMask, config: &NsbModelConfig) -> Vec<&'static str> {
    let mut names = Vec::new();
    if mask.contains(ComponentMask::ZODIACAL) {
        names.push("zodiacal");
    }
    if mask.contains(ComponentMask::STARLIGHT) {
        names.push(starlight_label(config));
    }
    if mask.contains(ComponentMask::AIRGLOW) {
        names.push("airglow");
    }
    if mask.contains(ComponentMask::MOON) {
        names.push("moon");
    }
    names
}

fn component_label(name: &'static str, config: &NsbModelConfig) -> &'static str {
    if name == "starlight" {
        starlight_label(config)
    } else {
        name
    }
}

fn starlight_label(config: &NsbModelConfig) -> &'static str {
    match config.starlight_model.as_ref() {
        Some(StarlightModel::BundledProductionGaiaDr3) => "starlight",
        Some(StarlightModel::ValidatedExternalMap(_)) => "validated-starlight",
        Some(StarlightModel::ExperimentalMap(_)) => "experimental-starlight",
        None => "starlight",
        _ => "unknown-starlight-model",
    }
}

fn duration_seconds(period: Period<UTC>) -> Option<f64> {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
        _ => None,
    }
}
