//! Gaia DR3 XP continuous coefficient products and reconstructed-spectrum metadata.
//!
//! Coefficient CSV files are retrieved via Gaia DataLink (`RETRIEVAL_TYPE=XP_CONTINUOUS`).
//! Spectrum calibration uses pinned GaiaXPy offline during the #61 migration; NSB
//! integrates normalized grids in Rust with the same 336–650 nm photon-flux
//! contract as sampled XP.

use anyhow::{Context, Result};
use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use crate::gaia::xp::canonical::{
    parse_bulk_ecsv_record, parse_datalink_gaiaxpy_csv, stream_bulk_ecsv_gz,
    write_gaiaxpy_datalink_csv, CanonicalXpContinuousRecord, FieldDiffSummary,
    XpContinuousSourceFormat, CANONICAL_XP_CONTINUOUS_SCHEMA, CORRELATION_PACKING,
};
use crate::gaia::xp::sampled::{
    integrate_photon_flux, parse_normalized_record, PhotonFluxIntegral,
};
use crate::platform::checksum_io::Checksum;

/// Stable identifier for GaiaXPy-reconstructed continuous XP integrated in 336–650 nm.
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1";

/// Pinned GaiaXPy version used only as a migration oracle (see issue #61).
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";

/// Integrated 336–650 nm reconstruction for one source.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReconstructedContribution {
    /// Gaia source identifier.
    pub source_id: String,
    /// Signed integrated photon flux.
    pub flux_336_650_ph_m2_s: f64,
    /// Propagated statistical uncertainty when available.
    pub statistical_uncertainty_336_650_ph_m2_s: Option<f64>,
    /// Additional systematic uncertainty assigned by the validated policy.
    pub systematic_uncertainty_336_650_ph_m2_s: f64,
    /// Positive signed-segment contribution.
    pub positive_integral_ph_m2_s: f64,
    /// Negative signed-segment contribution.
    pub negative_integral_ph_m2_s: f64,
    /// Number of negative samples.
    pub negative_sample_count: usize,
    /// Number of finite in-band samples.
    pub finite_sample_count: usize,
    /// Number of valid in-band wavelengths.
    pub valid_wavelength_count: usize,
    /// Pipe-separated quality flags.
    pub quality_flags: String,
    /// Whether reconstruction required extrapolation.
    pub extrapolated: bool,
    /// Typed workflow status identifier.
    pub reconstruction_status: String,
    /// Algorithm-qualified source checksum.
    pub input_checksum: Checksum,
    /// Algorithm-qualified calibration checksum.
    pub calibration_checksum: Checksum,
    /// Population contribution branch.
    pub branch: String,
}

/// Validate a raw Gaia DataLink `XP_CONTINUOUS` coefficient CSV for one source.
pub fn validate_continuous_coefficient_csv(bytes: &[u8], expected_source_id: &str) -> Result<()> {
    parse_continuous_coefficient_csv(bytes, expected_source_id).map(|_| ())
}

/// Parse directly into the one canonical XP continuous representation.
pub fn parse_continuous_coefficient_csv(
    bytes: &[u8],
    expected_source_id: &str,
) -> Result<CanonicalXpContinuousRecord> {
    parse_datalink_gaiaxpy_csv(bytes, expected_source_id)
}

/// Write one canonical coefficient record in GaiaXPy-compatible CSV form.
pub fn write_canonical_coefficient_csv(
    path: &Path,
    record: &CanonicalXpContinuousRecord,
) -> Result<()> {
    write_gaiaxpy_datalink_csv(path, record)
}

/// Read one canonical coefficient CSV without duplicate schema translation.
pub fn read_canonical_coefficient_csv(path: &Path) -> Result<CanonicalXpContinuousRecord> {
    let bytes = std::fs::read(path).with_context(|| path.display().to_string())?;
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes.as_slice());
    let headers = reader.headers()?.clone();
    let source_idx = headers
        .iter()
        .position(|header| header == "source_id")
        .context("source_id")?;
    let row = reader
        .records()
        .next()
        .transpose()
        .context("canonical coefficient row")?
        .ok_or_else(|| anyhow::anyhow!("empty canonical coefficient file"))?;
    let source_id = row.get(source_idx).context("source_id")?;
    parse_datalink_gaiaxpy_csv(&bytes, source_id)
}

/// Integrate a normalized reconstructed continuous spectrum CSV in Rust.
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

/// Convert a Rust integral into the canonical contribution schema.
pub fn integral_to_contribution(
    source_id: &str,
    integral: &PhotonFluxIntegral,
    input_checksum: Checksum,
    calibration_checksum: Checksum,
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
        input_checksum,
        calibration_checksum,
        branch: "xp_continuous_reconstructed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_html_coefficient_payload() {
        let error = validate_continuous_coefficient_csv(b"<html>error</html>", "1")
            .expect_err("html must fail");
        assert!(error.to_string().contains("HTML"));
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
        let error = parse_continuous_coefficient_csv(raw.as_bytes(), "42").expect_err("dup");
        assert!(error.to_string().contains("exactly one row"));
    }

    #[test]
    fn canonical_roundtrip_has_no_compatibility_clone() {
        let raw = minimal_datalink_csv("99", "(0.1,0.2)", "(0.3,0.4)");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("99.csv");
        let record = parse_datalink_gaiaxpy_csv(raw.as_bytes(), "99").unwrap();
        write_canonical_coefficient_csv(&path, &record).unwrap();
        let read = read_canonical_coefficient_csv(&path).unwrap();
        assert_eq!(read, record);
        assert_eq!(read.schema_version, CANONICAL_XP_CONTINUOUS_SCHEMA);
    }
}
