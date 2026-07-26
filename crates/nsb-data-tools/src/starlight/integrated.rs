//! Shared helpers for the integrated 300–650 nm Starlight product contract.
//!
//! The integrated product aggregates measured XP, reconstructed continuous XP,
//! photometric inference, completeness correction, and selection-function
//! weights into one exoatmospheric photon-radiance map. XP-only 336–650 nm
//! maps are explicitly rejected when presented as the integrated product.

use crate::starlight::approval::STARLIGHT_PRODUCTION_BAND_NM;
use crate::starlight::science::{STARLIGHT_BAND_MAX_NM, STARLIGHT_BAND_MIN_NM};
use anyhow::{bail, Context, Result};
use csv::ReaderBuilder;
use std::collections::BTreeMap;

/// Gaia DR3 XP sampled-only photometry model identifier (candidate, not integrated).
pub const XP_ONLY_PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_photon_radiance_336_650nm_v1";
/// Integrated product identifier recorded in diagnostics and manifests.
pub const INTEGRATED_PRODUCT_ID: &str = "nsb.integrated_starlight_300_650nm";
/// CSV schema tag for the mean radiance sidecar.
pub const INTEGRATED_MEAN_SCHEMA: &str = "nsb.starlight.mean";
/// Normative band definition for production validation.
pub const INTEGRATED_BAND_DEFINITION: &str =
    "exoatmospheric integrated 300-650 nm galactic direct starlight photon radiance";
/// Default integrated photometry model version suffix.
pub const INTEGRATED_PHOTOMETRY_MODEL: &str = "nsb_integrated_starlight_300_650nm_v1";
/// Runtime radiance column used by `StarlightMap`.
pub const RUNTIME_RADIANCE_FIELD: &str = "integrated_ph_cm2_ns_sr";

/// Detected starlight map input format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapInputFormat {
    /// Standard runtime HEALPix CSV (`integrated_ph_cm2_ns_sr`).
    RuntimeHealpix,
    /// Integrated-product mean sidecar (`mean_radiance_300_650_ph_cm2_ns_sr`).
    IntegratedMean,
}

/// Return true when the photometry model denotes XP sampled-only coverage.
pub fn is_xp_only_photometry_model(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    normalized == XP_ONLY_PHOTOMETRY_MODEL.to_ascii_lowercase()
        || (normalized.contains("336") && normalized.contains("xp") && normalized.contains("650"))
}

/// Return true when the band definition matches the 300–650 nm production contract.
pub fn band_definition_matches_production(band_definition: Option<&str>) -> bool {
    let Some(band) = band_definition else {
        return false;
    };
    let normalized = band.replace(['–', '—'], "-").to_ascii_lowercase();
    normalized.contains("300-650")
        || normalized.contains("300–650")
        || (normalized.contains("300") && normalized.contains("650"))
}

/// Return true when header metadata declares the integrated 300–650 nm contract.
pub fn integrated_spectral_contract_pass(
    photometry_model: Option<&str>,
    band_definition: Option<&str>,
    header: &BTreeMap<String, String>,
) -> bool {
    if photometry_model.is_some_and(is_xp_only_photometry_model) {
        return false;
    }
    if header
        .get("schema")
        .is_some_and(|schema| schema == INTEGRATED_MEAN_SCHEMA)
        && !header_band_is_production(header)
    {
        return false;
    }
    let model_ok = photometry_model.is_some_and(|model| {
        !is_xp_only_photometry_model(model)
            && (model.contains("integrated") || model.contains("300_650"))
    }) || header
        .get("schema")
        .is_some_and(|schema| schema == INTEGRATED_MEAN_SCHEMA);
    let band_ok =
        band_definition_matches_production(band_definition) || header_band_is_production(header);
    model_ok && band_ok
}

fn header_band_is_production(header: &BTreeMap<String, String>) -> bool {
    header.get("band_nm").is_some_and(|band| {
        let normalized = band.replace(['–', '—'], "-");
        normalized.contains(&format!(
            "{}-{}",
            STARLIGHT_PRODUCTION_BAND_NM[0] as u16, STARLIGHT_PRODUCTION_BAND_NM[1] as u16
        )) || (normalized.contains("300") && normalized.contains("650"))
    })
}

/// Detect whether raw CSV text is an integrated mean map or runtime HEALPix map.
pub fn detect_map_format(raw: &str, header: &BTreeMap<String, String>) -> Result<MapInputFormat> {
    if header
        .get("schema")
        .is_some_and(|schema| schema == INTEGRATED_MEAN_SCHEMA)
    {
        return Ok(MapInputFormat::IntegratedMean);
    }
    let data_header = first_data_header(raw)?;
    if data_header.starts_with("healpix_index,mean_radiance_300_650") {
        Ok(MapInputFormat::IntegratedMean)
    } else if data_header.starts_with("healpix_index,")
        || data_header.starts_with("galactic_lon_deg,")
    {
        Ok(MapInputFormat::RuntimeHealpix)
    } else {
        bail!("unsupported starlight map data header: {data_header}");
    }
}

/// Convert an integrated mean sidecar CSV into runtime HEALPix v2 format.
pub fn convert_integrated_mean_to_runtime(
    raw: &str,
    header: &BTreeMap<String, String>,
) -> Result<String> {
    let data_header = first_data_header(raw)?;
    if !data_header.starts_with("healpix_index,mean_radiance_300_650") {
        bail!("integrated mean conversion requires mean_radiance_300_650 columns");
    }

    let nside = required_header(header, "nside")?
        .parse::<u32>()
        .context("invalid nside in integrated mean header")?;
    let ordering = required_header(header, "ordering")?;
    let release_id = header
        .get("release_id")
        .cloned()
        .unwrap_or_else(|| "integrated-candidate".to_string());
    let model_sha256 = header.get("model_sha256").cloned().unwrap_or_default();
    let input_manifest_sha256 = header
        .get("input_manifest_sha256")
        .cloned()
        .unwrap_or_default();

    let mut out = String::new();
    out.push_str("# map_type=healpix\n");
    out.push_str("# coordinate_frame=galactic\n");
    out.push_str(&format!("# nside={nside}\n"));
    out.push_str(&format!("# ordering={ordering}\n"));
    out.push_str(&format!("# release_id={release_id}\n"));
    out.push_str("# calibration_status=candidate\n");
    out.push_str(&format!("# band_definition={INTEGRATED_BAND_DEFINITION}\n"));
    out.push_str(&format!(
        "# photometry_model={INTEGRATED_PHOTOMETRY_MODEL}\n"
    ));
    out.push_str(&format!(
        "# band_nm={STARLIGHT_BAND_MIN_NM}-{STARLIGHT_BAND_MAX_NM}\n"
    ));
    if !model_sha256.is_empty() {
        out.push_str(&format!("# model_sha256={model_sha256}\n"));
    }
    if !input_manifest_sha256.is_empty() {
        out.push_str(&format!(
            "# input_manifest_sha256={input_manifest_sha256}\n"
        ));
    }
    for (key, value) in header {
        if [
            "source_catalogue",
            "source_catalogue_release",
            "source_catalogue_license",
            "source_catalogue_checksum",
            "generation_date_utc",
            "generated_by",
            "generation_command",
        ]
        .contains(&key.as_str())
        {
            out.push_str(&format!("# {key}={value}\n"));
        }
    }
    out.push_str(
        "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10,statistical_uncertainty_ph_cm2_ns_sr,systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr\n",
    );

    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(raw.as_bytes());
    let headers = reader.headers()?.clone();
    let col = |name: &str| -> Result<usize> {
        headers
            .iter()
            .position(|field| field.trim() == name)
            .with_context(|| format!("integrated mean CSV missing column {name}"))
    };
    let idx_healpix = col("healpix_index")?;
    let idx_mean = col("mean_radiance_300_650_ph_cm2_ns_sr")?;
    let idx_stat = col("statistical_uncertainty_300_650_ph_cm2_ns_sr")?;
    let idx_sys = col("systematic_uncertainty_300_650_ph_cm2_ns_sr")?;
    let idx_total = col("total_uncertainty_300_650_ph_cm2_ns_sr")?;

    for (row_idx, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("integrated mean row {}", row_idx + 1))?;
        let row = row_idx + 1;
        let field = |idx: usize, name: &str| -> Result<&str> {
            record
                .get(idx)
                .with_context(|| format!("row {row} missing {name}"))
        };
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            field(idx_healpix, "healpix_index")?,
            field(idx_mean, "mean_radiance")?,
            0.0,
            0.0,
            field(idx_stat, "statistical_uncertainty")?,
            field(idx_sys, "systematic_uncertainty")?,
            field(idx_total, "total_uncertainty")?,
        ));
    }
    Ok(out)
}

fn first_data_header(raw: &str) -> Result<&str> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .context("map CSV has no data header")
}

fn required_header<'a>(header: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    header
        .get(key)
        .map(String::as_str)
        .with_context(|| format!("integrated mean header missing required key {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_xp_only_model() {
        assert!(is_xp_only_photometry_model(XP_ONLY_PHOTOMETRY_MODEL));
        assert!(!is_xp_only_photometry_model(INTEGRATED_PHOTOMETRY_MODEL));
    }

    #[test]
    fn integrated_contract_rejects_xp_only() {
        let header = BTreeMap::new();
        assert!(!integrated_spectral_contract_pass(
            Some(XP_ONLY_PHOTOMETRY_MODEL),
            Some(INTEGRATED_BAND_DEFINITION),
            &header,
        ));
    }

    #[test]
    fn integrated_contract_accepts_production_band() {
        let mut header = BTreeMap::new();
        header.insert("schema".to_string(), INTEGRATED_MEAN_SCHEMA.to_string());
        header.insert("band_nm".to_string(), "300-650".to_string());
        assert!(integrated_spectral_contract_pass(
            Some(INTEGRATED_PHOTOMETRY_MODEL),
            Some(INTEGRATED_BAND_DEFINITION),
            &header,
        ));
    }

    #[test]
    fn converts_integrated_mean_to_runtime_v2() -> Result<()> {
        let raw = concat!(
            "# schema=nsb.starlight.mean\n",
            "# nside=1\n",
            "# ordering=ring\n",
            "# release_id=fixture\n",
            "healpix_index,mean_radiance_300_650_ph_cm2_ns_sr,",
            "statistical_uncertainty_300_650_ph_cm2_ns_sr,",
            "systematic_uncertainty_300_650_ph_cm2_ns_sr,",
            "total_uncertainty_300_650_ph_cm2_ns_sr,",
            "inferred_fraction,flags_extrapolation,flags_crowding,",
            "contribution_rows,represented_multiplicity\n",
            "0,1.0,0.1,0.2,0.22,0.0,false,false,1,1\n",
        );
        let header = parse_test_header(raw);
        let runtime = convert_integrated_mean_to_runtime(raw, &header)?;
        assert!(runtime.contains("integrated_ph_cm2_ns_sr"));
        assert!(runtime.contains("statistical_uncertainty_ph_cm2_ns_sr"));
        assert!(runtime.contains("0,1"));
        assert!(runtime.contains("0.1,0.2,0.22"));
        Ok(())
    }

    fn parse_test_header(raw: &str) -> BTreeMap<String, String> {
        raw.lines()
            .filter_map(|line| line.strip_prefix('#'))
            .filter_map(|line| line.trim().split_once('='))
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect()
    }
}
