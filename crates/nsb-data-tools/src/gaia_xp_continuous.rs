//! Gaia DR3 XP continuous coefficient products and reconstructed-spectrum metadata.
//!
//! Coefficient CSV files are retrieved via Gaia DataLink (`RETRIEVAL_TYPE=XP_CONTINUOUS`).
//! Spectrum calibration uses pinned GaiaXPy offline; NSB integrates the resulting
//! normalized grids with the same 336–650 nm photon-flux contract as sampled XP.

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use crate::gaia_xp::{
    integrate_photon_flux, parse_gaia_tuple_array, parse_normalized_record, parse_series,
    PhotonFluxIntegral, XpProduct, BAND_MAX_NM, BAND_MIN_NM, NORMALIZED_FLUX_COLUMN,
    NORMALIZED_FLUX_ERROR_COLUMN, NORMALIZED_WAVELENGTH_COLUMN,
};

/// Stable identifier for GaiaXPy-reconstructed continuous XP integrated in 336–650 nm.
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1";

/// Pinned GaiaXPy version used for offline reconstruction (see `tools/starlight-xp-continuous`).
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";

pub const CANONICAL_COEFFICIENT_SCHEMA: u32 = 1;

const REQUIRED_COEFFICIENT_COLUMNS: [&str; 5] = [
    "source_id",
    "bp_coefficients",
    "rp_coefficients",
    "bp_coefficient_errors",
    "rp_coefficient_errors",
];

/// Parsed XP continuous coefficient row from Gaia DataLink.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContinuousCoefficients {
    pub schema_version: u32,
    pub source_id: String,
    pub bp_n_parameters: usize,
    pub rp_n_parameters: usize,
    pub bp_coefficients: Vec<f64>,
    pub rp_coefficients: Vec<f64>,
    pub bp_coefficient_errors: Vec<f64>,
    pub rp_coefficient_errors: Vec<f64>,
    pub input_checksum: Option<String>,
    pub retrieval_batch: Option<String>,
}

/// Integrated 336–650 nm reconstruction for one source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReconstructedContribution {
    pub source_id: String,
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: Option<f64>,
    pub systematic_uncertainty_336_650_ph_m2_s: f64,
    pub positive_integral_ph_m2_s: f64,
    pub negative_integral_ph_m2_s: f64,
    pub negative_sample_count: usize,
    pub finite_sample_count: usize,
    pub valid_wavelength_count: usize,
    pub quality_flags: String,
    pub extrapolated: bool,
    pub reconstruction_status: String,
    pub input_checksum: String,
    pub calibration_checksum: String,
    pub branch: String,
}

fn required_header(headers: &csv::StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|entry| entry == name)
        .ok_or_else(|| anyhow::anyhow!("XP continuous coefficient CSV missing column {name}"))
}

fn field<'a>(row: &'a csv::StringRecord, index: usize, name: &str) -> Result<&'a str> {
    row.get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing Gaia XP continuous field {name}"))
}

fn parse_coefficient_array(raw: &str, field: &str, source_id: Option<u64>) -> Result<Vec<f64>> {
    parse_gaia_tuple_array(raw, field, source_id, None)
}

/// Validate a raw Gaia DataLink `XP_CONTINUOUS` coefficient CSV for one source.
pub fn validate_continuous_coefficient_csv(bytes: &[u8], expected_source_id: &str) -> Result<()> {
    parse_continuous_coefficient_csv(bytes, expected_source_id).map(|_| ())
}

/// Parse coefficient arrays from a DataLink XP_CONTINUOUS CSV payload.
pub fn parse_continuous_coefficient_csv(
    bytes: &[u8],
    expected_source_id: &str,
) -> Result<ContinuousCoefficients> {
    if bytes.is_empty() {
        bail!("empty XP continuous coefficient response");
    }
    if crate::gaia_xp::contains_service_error(bytes) {
        bail!("XP continuous coefficient response contains SERVICE ERROR");
    }
    let text = String::from_utf8_lossy(bytes);
    if text.trim_start().starts_with('<') {
        bail!("XP continuous coefficient response looks like HTML/XML, not CSV");
    }
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = reader.headers()?.clone();
    for column in REQUIRED_COEFFICIENT_COLUMNS {
        if !headers.iter().any(|entry| entry == column) {
            bail!("XP continuous coefficient CSV missing column {column}");
        }
    }
    let mut rows = reader.records();
    let record = rows
        .next()
        .transpose()
        .context("failed to read XP continuous coefficient row")?
        .ok_or_else(|| anyhow::anyhow!("XP continuous coefficient CSV has no data rows"))?;
    if rows.next().transpose()?.is_some() {
        bail!("XP continuous coefficient CSV must contain exactly one row");
    }
    let source_idx = required_header(&headers, "source_id")?;
    let source_id = field(&record, source_idx, "source_id")?;
    if source_id != expected_source_id {
        bail!(
            "XP continuous coefficient source_id mismatch: expected {expected_source_id}, found {source_id}"
        );
    }
    let sid = source_id.parse::<u64>().ok();
    let bp_idx = required_header(&headers, "bp_coefficients")?;
    let rp_idx = required_header(&headers, "rp_coefficients")?;
    let bp_err_idx = required_header(&headers, "bp_coefficient_errors")?;
    let rp_err_idx = required_header(&headers, "rp_coefficient_errors")?;
    let bp_coefficients = parse_coefficient_array(
        field(&record, bp_idx, "bp_coefficients")?,
        "bp_coefficients",
        sid,
    )?;
    let rp_coefficients = parse_coefficient_array(
        field(&record, rp_idx, "rp_coefficients")?,
        "rp_coefficients",
        sid,
    )?;
    let bp_coefficient_errors = parse_coefficient_array(
        field(&record, bp_err_idx, "bp_coefficient_errors")?,
        "bp_coefficient_errors",
        sid,
    )?;
    let rp_coefficient_errors = parse_coefficient_array(
        field(&record, rp_err_idx, "rp_coefficient_errors")?,
        "rp_coefficient_errors",
        sid,
    )?;
    if bp_coefficients.len() != bp_coefficient_errors.len() {
        bail!("BP coefficient/error length mismatch");
    }
    if rp_coefficients.len() != rp_coefficient_errors.len() {
        bail!("RP coefficient/error length mismatch");
    }
    if bp_coefficients.is_empty() || rp_coefficients.is_empty() {
        bail!("XP continuous coefficients must be non-empty");
    }
    validate_finite_arrays(
        &bp_coefficients,
        &rp_coefficients,
        &bp_coefficient_errors,
        &rp_coefficient_errors,
    )?;
    Ok(ContinuousCoefficients {
        schema_version: CANONICAL_COEFFICIENT_SCHEMA,
        source_id: source_id.to_string(),
        bp_n_parameters: bp_coefficients.len(),
        rp_n_parameters: rp_coefficients.len(),
        bp_coefficients,
        rp_coefficients,
        bp_coefficient_errors,
        rp_coefficient_errors,
        input_checksum: None,
        retrieval_batch: None,
    })
}

fn validate_finite_arrays(bp: &[f64], rp: &[f64], bp_err: &[f64], rp_err: &[f64]) -> Result<()> {
    for (label, values) in [
        ("bp_coefficients", bp),
        ("rp_coefficients", rp),
        ("bp_coefficient_errors", bp_err),
        ("rp_coefficient_errors", rp_err),
    ] {
        if values.iter().any(|value| !value.is_finite()) {
            bail!("non-finite values in {label}");
        }
    }
    Ok(())
}

pub fn format_series(values: &[f64]) -> String {
    values
        .iter()
        .map(|value| format!("{value:.8e}"))
        .collect::<Vec<_>>()
        .join(";")
}

pub fn write_canonical_coefficient_csv(path: &Path, coeffs: &ContinuousCoefficients) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let part = path.with_extension("csv.part");
    let mut writer = WriterBuilder::new().from_path(&part)?;
    writer.write_record([
        "schema_version",
        "source_id",
        "bp_n_parameters",
        "rp_n_parameters",
        "bp_coefficients",
        "rp_coefficients",
        "bp_coefficient_errors",
        "rp_coefficient_errors",
        "input_checksum",
        "retrieval_batch",
    ])?;
    writer.write_record([
        coeffs.schema_version.to_string(),
        coeffs.source_id.clone(),
        coeffs.bp_n_parameters.to_string(),
        coeffs.rp_n_parameters.to_string(),
        format_series(&coeffs.bp_coefficients),
        format_series(&coeffs.rp_coefficients),
        format_series(&coeffs.bp_coefficient_errors),
        format_series(&coeffs.rp_coefficient_errors),
        coeffs.input_checksum.clone().unwrap_or_default(),
        coeffs.retrieval_batch.clone().unwrap_or_default(),
    ])?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(part, path)?;
    Ok(())
}

pub fn read_canonical_coefficient_csv(path: &Path) -> Result<ContinuousCoefficients> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    let mut records = reader.records();
    let row = records
        .next()
        .transpose()
        .context("canonical coefficient row")?
        .ok_or_else(|| anyhow::anyhow!("empty canonical coefficient file"))?;
    let _sid = row.get(1).context("source_id")?.parse::<u64>().ok();
    Ok(ContinuousCoefficients {
        schema_version: row.get(0).context("schema")?.parse()?,
        source_id: row.get(1).context("source_id")?.to_string(),
        bp_n_parameters: row.get(2).context("bp_n")?.parse()?,
        rp_n_parameters: row.get(3).context("rp_n")?.parse()?,
        bp_coefficients: parse_series(row.get(4).context("bp")?)?,
        rp_coefficients: parse_series(row.get(5).context("rp")?)?,
        bp_coefficient_errors: parse_series(row.get(6).context("bp_err")?)?,
        rp_coefficient_errors: parse_series(row.get(7).context("rp_err")?)?,
        input_checksum: row.get(8).filter(|v| !v.is_empty()).map(str::to_string),
        retrieval_batch: row.get(9).filter(|v| !v.is_empty()).map(str::to_string),
    })
}

/// Integrate a normalized reconstructed continuous spectrum CSV (GaiaXPy output).
pub fn integrate_reconstructed_csv(path: &Path) -> Result<(String, PhotonFluxIntegral)> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(std::fs::File::open(path).with_context(|| {
            format!("failed to open reconstructed spectrum {}", path.display())
        })?);
    let headers = reader.headers()?.clone();
    let record = reader
        .records()
        .next()
        .transpose()
        .context("failed to read reconstructed spectrum row")?
        .ok_or_else(|| anyhow::anyhow!("reconstructed spectrum CSV is empty"))?;
    let product = parse_normalized_record(&headers, &record)?;
    let integral = integrate_photon_flux(&product)?;
    Ok((product.source_id, integral))
}

pub fn integral_to_contribution(
    source_id: &str,
    integral: &PhotonFluxIntegral,
    input_checksum: &str,
    calibration_checksum: &str,
) -> ReconstructedContribution {
    ReconstructedContribution {
        source_id: source_id.to_string(),
        flux_336_650_ph_m2_s: integral.total_ph_m2_s,
        statistical_uncertainty_336_650_ph_m2_s: integral.uncertainty_ph_m2_s,
        systematic_uncertainty_336_650_ph_m2_s: 0.0,
        positive_integral_ph_m2_s: integral.positive_ph_m2_s,
        negative_integral_ph_m2_s: integral.negative_ph_m2_s,
        negative_sample_count: integral.negative_samples,
        finite_sample_count: integral.band_samples,
        valid_wavelength_count: integral.band_samples,
        quality_flags: String::new(),
        extrapolated: false,
        reconstruction_status: "valid".to_string(),
        input_checksum: input_checksum.to_string(),
        calibration_checksum: calibration_checksum.to_string(),
        branch: "xp_continuous_reconstructed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_html_coefficient_payload() {
        let err = validate_continuous_coefficient_csv(b"<html>error</html>", "1")
            .expect_err("html must fail");
        assert!(err.to_string().contains("HTML"));
    }

    #[test]
    fn rejects_mismatched_bp_error_lengths() {
        let raw = concat!(
            "source_id,bp_coefficients,rp_coefficients,bp_coefficient_errors,rp_coefficient_errors\n",
            "1,\"(1.0)\",\"(2.0)\",\"(0.1,0.2)\",\"(0.2)\"\n",
        );
        let err = parse_continuous_coefficient_csv(raw.as_bytes(), "1").expect_err("length");
        assert!(err.to_string().contains("mismatch"));
    }

    #[test]
    fn rejects_duplicate_rows() {
        let raw = concat!(
            "source_id,bp_coefficients,rp_coefficients,bp_coefficient_errors,rp_coefficient_errors\n",
            "42,\"(1.0)\",\"(2.0)\",\"(0.1)\",\"(0.2)\"\n",
            "42,\"(1.0)\",\"(2.0)\",\"(0.1)\",\"(0.2)\"\n",
        );
        let err = parse_continuous_coefficient_csv(raw.as_bytes(), "42").expect_err("dup");
        assert!(err.to_string().contains("exactly one row"));
    }

    #[test]
    fn canonical_roundtrip() {
        let coeffs = ContinuousCoefficients {
            schema_version: CANONICAL_COEFFICIENT_SCHEMA,
            source_id: "99".to_string(),
            bp_n_parameters: 2,
            rp_n_parameters: 2,
            bp_coefficients: vec![1.0, 2.0],
            rp_coefficients: vec![3.0, 4.0],
            bp_coefficient_errors: vec![0.1, 0.2],
            rp_coefficient_errors: vec![0.3, 0.4],
            input_checksum: Some("abc".to_string()),
            retrieval_batch: Some("batch".to_string()),
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("99.csv");
        write_canonical_coefficient_csv(&path, &coeffs).unwrap();
        let read = read_canonical_coefficient_csv(&path).unwrap();
        assert_eq!(read, coeffs);
    }
}
