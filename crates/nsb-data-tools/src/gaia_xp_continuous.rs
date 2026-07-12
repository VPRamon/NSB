//! Gaia DR3 XP continuous coefficient products and reconstructed-spectrum metadata.
//!
//! Coefficient CSV files are retrieved via Gaia DataLink (`RETRIEVAL_TYPE=XP_CONTINUOUS`).
//! Production calibration is in-process Rust (`gaia_xp_continuous_calibrate`); GaiaXPy
//! Python is retained only for oracle fixtures and environment audit.

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::gaia_xp::{
    integrate_photon_flux, parse_normalized_record, PhotonFluxIntegral, XpProduct,
};
pub use crate::gaia_xp_continuous_canonical::{
    parse_bulk_ecsv_record, parse_datalink_gaiaxpy_csv, stream_bulk_ecsv_gz,
    write_gaiaxpy_datalink_csv, CanonicalXpContinuousRecord, FieldDiffSummary,
    XpContinuousSourceFormat, CANONICAL_XP_CONTINUOUS_SCHEMA, CORRELATION_PACKING,
};

/// Stable identifier for GaiaXPy-reconstructed continuous XP integrated in 336–650 nm.
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1";

/// Pinned GaiaXPy version used for offline reconstruction (see `tools/starlight-xp-continuous`).
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";

pub const CANONICAL_COEFFICIENT_SCHEMA: u32 = 1;

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

/// Validate a raw Gaia DataLink `XP_CONTINUOUS` coefficient CSV for one source.
pub fn validate_continuous_coefficient_csv(bytes: &[u8], expected_source_id: &str) -> Result<()> {
    parse_continuous_coefficient_csv(bytes, expected_source_id).map(|_| ())
}

/// Parse coefficient arrays from a DataLink XP_CONTINUOUS CSV payload.
pub fn parse_continuous_coefficient_csv(
    bytes: &[u8],
    expected_source_id: &str,
) -> Result<ContinuousCoefficients> {
    canonical_to_legacy(&parse_datalink_gaiaxpy_csv(bytes, expected_source_id)?)
}

fn canonical_to_legacy(record: &CanonicalXpContinuousRecord) -> Result<ContinuousCoefficients> {
    Ok(ContinuousCoefficients {
        schema_version: CANONICAL_COEFFICIENT_SCHEMA,
        source_id: record.source_id.clone(),
        bp_n_parameters: record.bp_n_parameters,
        rp_n_parameters: record.rp_n_parameters,
        bp_coefficients: record.bp_coefficients.clone(),
        rp_coefficients: record.rp_coefficients.clone(),
        bp_coefficient_errors: record.bp_coefficient_errors.clone(),
        rp_coefficient_errors: record.rp_coefficient_errors.clone(),
        input_checksum: record.source_checksum.clone(),
        retrieval_batch: None,
    })
}

pub fn write_canonical_coefficient_csv(
    path: &Path,
    record: &CanonicalXpContinuousRecord,
) -> Result<()> {
    write_gaiaxpy_datalink_csv(path, record)
}

pub fn read_canonical_coefficient_csv(path: &Path) -> Result<ContinuousCoefficients> {
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes.as_slice());
    let headers = reader.headers()?.clone();
    let source_idx = headers
        .iter()
        .position(|h| h == "source_id")
        .context("source_id")?;
    let row = reader
        .records()
        .next()
        .transpose()
        .context("canonical coefficient row")?
        .ok_or_else(|| anyhow::anyhow!("empty canonical coefficient file"))?;
    let source_id = row.get(source_idx).context("source_id")?;
    canonical_to_legacy(&parse_datalink_gaiaxpy_csv(
        std::fs::read(path)?.as_slice(),
        source_id,
    )?)
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

/// Write one NSB normalized continuous spectrum CSV (GaiaXPy-compatible layout).
pub fn write_normalized_spectrum_csv(path: &Path, product: &XpProduct) -> Result<()> {
    let errors = product
        .flux_error_w_m2_nm
        .as_ref()
        .context("normalized spectrum requires per-sample flux errors")?;
    if product.wavelengths_nm.len() != product.flux_w_m2_nm.len()
        || product.flux_w_m2_nm.len() != errors.len()
    {
        bail!("normalized spectrum sample length mismatch");
    }
    for (label, values) in [
        ("wavelength", &product.wavelengths_nm),
        ("flux", &product.flux_w_m2_nm),
        ("flux_error", errors),
    ] {
        for value in values {
            if !value.is_finite() {
                bail!(
                    "non-finite {label} in normalized spectrum for {}",
                    product.source_id
                );
            }
        }
    }
    let part = path.with_extension(format!(
        "{}.part",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("csv")
    ));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = WriterBuilder::new().has_headers(true).from_path(&part)?;
    writer.write_record([
        "source_id",
        "xp_wavelength_nm",
        "xp_flux_w_m2_nm",
        "xp_flux_error_w_m2_nm",
    ])?;
    writer.write_record([
        product.source_id.as_str(),
        &format_series(&product.wavelengths_nm, false),
        &format_series(&product.flux_w_m2_nm, true),
        &format_series(errors, true),
    ])?;
    writer.flush()?;
    fs::rename(&part, path)?;
    Ok(())
}

fn format_series(values: &[f64], scientific: bool) -> String {
    values
        .iter()
        .map(|value| {
            if scientific {
                format!("{value:.8e}")
            } else {
                format!("{value:.8}")
            }
        })
        .collect::<Vec<_>>()
        .join(";")
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

    fn minimal_datalink_csv(source_id: &str, bp_errors: &str, rp_errors: &str) -> String {
        format!(
            concat!(
                "source_id,bp_n_parameters,bp_standard_deviation,rp_n_parameters,rp_standard_deviation,",
                "bp_coefficients,bp_coefficient_errors,bp_coefficient_correlations,",
                "rp_coefficients,rp_coefficient_errors,rp_coefficient_correlations\n",
                "{source_id},2,1.00000000e0,2,1.00000000e0,",
                "\"(1.0,2.0)\",\"{bp_errors}\",\"(0.2)\",",
                "\"(3.0,4.0)\",\"{rp_errors}\",\"(0.1)\"\n",
            ),
            source_id = source_id,
            bp_errors = bp_errors,
            rp_errors = rp_errors
        )
    }

    #[test]
    fn rejects_mismatched_bp_error_lengths() {
        let raw = minimal_datalink_csv("1", "(0.1)", "(0.3,0.4)");
        assert!(parse_datalink_gaiaxpy_csv(raw.as_bytes(), "1").is_err());
    }

    #[test]
    fn rejects_duplicate_rows() {
        let row = minimal_datalink_csv("42", "(0.1,0.2)", "(0.3,0.4)");
        let raw = format!("{row}{row}");
        let err = parse_continuous_coefficient_csv(raw.as_bytes(), "42").expect_err("dup");
        assert!(err.to_string().contains("exactly one row"));
    }

    #[test]
    fn canonical_roundtrip() {
        let raw = minimal_datalink_csv("99", "(0.1,0.2)", "(0.3,0.4)");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("99.csv");
        let record = parse_datalink_gaiaxpy_csv(raw.as_bytes(), "99").unwrap();
        write_canonical_coefficient_csv(&path, &record).unwrap();
        let read = read_canonical_coefficient_csv(&path).unwrap();
        assert_eq!(read.source_id, "99");
        assert_eq!(read.bp_coefficients, vec![1.0, 2.0]);
    }
}
