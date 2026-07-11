use super::WindowOutput;
use crate::parsing::location::SitePreset;
use crate::parsing::time::format_utc;
use anyhow::Result;
use nsb::{
    assets::asset_registry, BandDiagnostic, ComponentMask, NsbComponentMetadata, NsbModelConfig,
    NsbResult, StarlightModel, Target, ZodiacalExtinction, MODEL_VERSION, NSB_VERSION,
    SIDERUST_SOURCE, SIDERUST_VERSION,
};
use serde::Serialize;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

#[derive(Serialize)]
struct PointJson {
    schema_version: &'static str,
    version: VersionJson,
    model: ModelJson,
    time_utc: String,
    observer: ObserverJson,
    target: TargetJson,
    components: Vec<ComponentJson>,
    total: TotalJson,
    band_diagnostic: BandDiagnosticJson,
}

#[derive(Serialize)]
struct WindowJson {
    schema_version: &'static str,
    version: VersionJson,
    model: ModelJson,
    start_utc: String,
    end_utc: String,
    min_nsb_ph_cm2_ns_sr: Option<f64>,
    max_nsb_ph_cm2_ns_sr: f64,
    selected_components: Vec<&'static str>,
    component_metadata: Vec<ComponentDescriptorJson>,
    periods: Vec<PeriodJson>,
}

#[derive(Serialize)]
struct VersionJson {
    nsb_version: &'static str,
    model_version: &'static str,
    siderust_version: &'static str,
    siderust_source: &'static str,
    asset_manifest_schema: u32,
    data_assets: Vec<AssetJson>,
}

#[derive(Serialize)]
struct AssetJson {
    path: String,
    schema: String,
    sha256: String,
    calibration_status: String,
}

#[derive(Serialize)]
struct ModelJson {
    preset: &'static str,
    moonlight_model: &'static str,
    starlight_model: &'static str,
    solar_radio_flux_sfu: f64,
    zodiacal_extinction: &'static str,
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
    b_s10_diagnostic: f64,
    v_s10_diagnostic: f64,
    relative_uncertainty: Option<f64>,
    metadata: ComponentMetadataJson,
}

#[derive(Serialize)]
struct ComponentDescriptorJson {
    name: &'static str,
    metadata: ComponentMetadataJson,
}

#[derive(Serialize)]
struct ComponentMetadataJson {
    calibration_status: &'static str,
    provenance: String,
    validated_domain: String,
    band_diagnostic: BandDiagnosticJson,
}

#[derive(Serialize)]
struct BandDiagnosticJson {
    convention: &'static str,
    b_reference_nm: f64,
    v_reference_nm: f64,
    zero_point: f64,
    interpretation: &'static str,
}

#[derive(Serialize)]
struct TotalJson {
    integrated_ph_cm2_ns_sr: f64,
    b_mag_arcsec2_diagnostic: f64,
    v_mag_arcsec2_diagnostic: f64,
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
    config: &NsbModelConfig,
    result: &NsbResult,
) -> Result<()> {
    let payload = PointJson {
        schema_version: "nsb-cli-point-json-v1",
        version: version_json(),
        model: model_json(config),
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
            .map(|component| ComponentJson {
                name: component_label(component.name, config),
                integrated_ph_cm2_ns_sr: component.integrated.value(),
                b_s10_diagnostic: component.b_flux_s10.value(),
                v_s10_diagnostic: component.v_flux_s10.value(),
                relative_uncertainty: component.relative_uncertainty,
                metadata: component_metadata_json(&component.metadata),
            })
            .collect(),
        total: TotalJson {
            integrated_ph_cm2_ns_sr: result.integrated.value(),
            b_mag_arcsec2_diagnostic: result.b_mag.value(),
            v_mag_arcsec2_diagnostic: result.v_mag.value(),
        },
        band_diagnostic: band_diagnostic_json(result.band_diagnostic),
    };
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

pub fn write_window(output: &WindowOutput<'_>) -> Result<()> {
    let payload = WindowJson {
        schema_version: "nsb-cli-window-json-v1",
        version: version_json(),
        model: model_json(output.config),
        start_utc: format_utc(output.start),
        end_utc: format_utc(output.end),
        min_nsb_ph_cm2_ns_sr: output.min.map(|value| value.value()),
        max_nsb_ph_cm2_ns_sr: output.max.value(),
        selected_components: component_names(output.components, output.config),
        component_metadata: output
            .descriptions
            .iter()
            .map(|description| ComponentDescriptorJson {
                name: component_label(description.name, output.config),
                metadata: component_metadata_json(&description.metadata),
            })
            .collect(),
        periods: output
            .periods
            .iter()
            .map(|period| PeriodJson {
                start_utc: format_utc(period.start),
                end_utc: format_utc(period.end),
                duration_seconds: duration_seconds(*period),
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

fn version_json() -> VersionJson {
    let registry = asset_registry();
    VersionJson {
        nsb_version: NSB_VERSION,
        model_version: MODEL_VERSION,
        siderust_version: SIDERUST_VERSION,
        siderust_source: SIDERUST_SOURCE,
        asset_manifest_schema: registry.schema_version,
        data_assets: registry
            .assets
            .iter()
            .filter(|asset| asset.runtime_embedded)
            .map(|asset| AssetJson {
                path: asset.path.clone(),
                schema: asset.schema.clone(),
                sha256: asset.sha256.clone(),
                calibration_status: asset.calibration_status.clone(),
            })
            .collect(),
    }
}

fn model_json(config: &NsbModelConfig) -> ModelJson {
    ModelJson {
        preset: config.site_profile.as_str(),
        moonlight_model: config.moonlight_model.as_str(),
        starlight_model: match config.starlight_model.as_ref() {
            None => "not-configured-non-production-component",
            Some(StarlightModel::BundledProductionGaiaDr3) => "starlight",
            Some(StarlightModel::BundledExperimentalSeed) => "experimental-starlight",
            Some(StarlightModel::ExperimentalMap(_)) => "experimental-starlight",
            Some(StarlightModel::ValidatedExternalMap(_)) => "validated-starlight",
        },
        solar_radio_flux_sfu: config.solar_radio_flux.value(),
        zodiacal_extinction: match config.zodiacal_extinction {
            ZodiacalExtinction::None => "none",
            ZodiacalExtinction::Noll2012Approx => "noll-2012-approximation",
        },
    }
}

fn component_metadata_json(metadata: &NsbComponentMetadata) -> ComponentMetadataJson {
    ComponentMetadataJson {
        calibration_status: metadata.status.as_str(),
        provenance: metadata.provenance.to_string(),
        validated_domain: metadata.validated_domain.to_string(),
        band_diagnostic: band_diagnostic_json(metadata.band_diagnostic),
    }
}

fn band_diagnostic_json(diagnostic: BandDiagnostic) -> BandDiagnosticJson {
    BandDiagnosticJson {
        convention: diagnostic.convention,
        b_reference_nm: diagnostic.b_reference.value(),
        v_reference_nm: diagnostic.v_reference.value(),
        zero_point: diagnostic.zero_point.value(),
        interpretation: "diagnostic monochromatic S10 proxy; not a validated passband integration",
    }
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
        Some(StarlightModel::BundledExperimentalSeed)
        | Some(StarlightModel::ExperimentalMap(_)) => "experimental-starlight",
        None => "starlight",
    }
}

fn duration_seconds(period: Period<UTC>) -> Option<f64> {
    match (period.start.to_chrono(), period.end.to_chrono()) {
        (Some(start), Some(end)) => Some((end - start).num_milliseconds() as f64 / 1000.0),
        _ => None,
    }
}
