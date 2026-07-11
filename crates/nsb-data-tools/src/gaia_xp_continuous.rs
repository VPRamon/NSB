//! Gaia DR3 XP continuous coefficient products and reconstructed-spectrum metadata.
//!
//! Coefficient CSV files are retrieved via Gaia DataLink (`RETRIEVAL_TYPE=XP_CONTINUOUS`).
//! Spectrum calibration uses pinned GaiaXPy offline; NSB integrates the resulting
//! normalized grids with the same 336–650 nm photon-flux contract as sampled XP.

use anyhow::{bail, Context, Result};
use csv::ReaderBuilder;
use std::path::Path;

pub use crate::gaia_xp::{
    integrate_photon_flux, parse_normalized_record, PhotonFluxIntegral, XpProduct, BAND_MAX_NM,
    BAND_MIN_NM, NORMALIZED_FLUX_COLUMN, NORMALIZED_FLUX_ERROR_COLUMN, NORMALIZED_WAVELENGTH_COLUMN,
};

/// Stable identifier for GaiaXPy-reconstructed continuous XP integrated in 336–650 nm.
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_continuous_reconstructed_336_650nm_v1";

/// Pinned GaiaXPy version used for offline reconstruction (see `tools/starlight-xp-continuous`).
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";

const REQUIRED_COEFFICIENT_COLUMNS: [&str; 5] = [
    "source_id",
    "bp_coefficients",
    "rp_coefficients",
    "bp_coefficient_errors",
    "rp_coefficient_errors",
];

/// Validate a raw Gaia DataLink `XP_CONTINUOUS` coefficient CSV for one source.
pub fn validate_continuous_coefficient_csv(bytes: &[u8], expected_source_id: &str) -> Result<()> {
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
    let idx = headers
        .iter()
        .position(|entry| entry == "source_id")
        .expect("source_id checked above");
    let source_id = record.get(idx).unwrap_or_default();
    if source_id != expected_source_id {
        bail!(
            "XP continuous coefficient source_id mismatch: expected {expected_source_id}, found {source_id}"
        );
    }
    Ok(())
}

/// Integrate a normalized reconstructed continuous spectrum CSV (GaiaXPy output).
pub fn integrate_reconstructed_csv(path: &Path) -> Result<(String, PhotonFluxIntegral)> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(
            std::fs::File::open(path)
                .with_context(|| format!("failed to open reconstructed spectrum {}", path.display()))?,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_html_coefficient_payload() {
        let err = validate_continuous_coefficient_csv(b"<html>error</html>", "1")
            .expect_err("html must fail");
        assert!(err.to_string().contains("HTML"));
    }
}
