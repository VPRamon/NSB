//! Versioned evidence contract for calibrated CTAO site profiles.
//!
//! This module defines the machine-readable inputs required before a named CTAO
//! profile can be promoted from a planning preset to a calibrated profile. It
//! deliberately does not perform that promotion: a valid asset is evidence for
//! later scientific review, not proof that review has happened.

use crate::site::SiteProfileId;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};
use thiserror::Error;

/// Supported schema version for [`SiteCalibrationAsset`].
pub(crate) const SITE_CALIBRATION_ASSET_SCHEMA_VERSION: u32 = 1;

/// CTAO site that may receive a dedicated calibrated profile.
///
/// Additional calibrated sites may be added; match with a wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CalibratedSiteId {
    /// CTAO-North at the Roque de los Muchachos Observatory.
    #[serde(rename = "ctao-north")]
    CtaNorth,
    /// CTAO-South in the Paranal/Atacama region.
    #[serde(rename = "ctao-south")]
    CtaSouth,
}

impl CalibratedSiteId {
    /// Stable identifier used in calibration metadata.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CtaNorth => "ctao-north",
            Self::CtaSouth => "ctao-south",
        }
    }

    /// Built-in planning profile associated with this calibration target.
    pub const fn planning_profile(self) -> SiteProfileId {
        match self {
            Self::CtaNorth => SiteProfileId::CtaNorth,
            Self::CtaSouth => SiteProfileId::CtaSouth,
        }
    }
}

/// Time and wavelength domain for which a calibration asset is valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCalibrationValidity {
    /// First inclusive UTC date covered by the calibration, formatted `YYYY-MM-DD`.
    pub valid_from: String,
    /// Last inclusive UTC date covered by the calibration, formatted `YYYY-MM-DD`.
    pub valid_through: String,
    /// Inclusive wavelength interval represented by the calibration, in nanometres.
    pub wavelength_nm: [u16; 2],
}

/// Atmospheric parameters and one-sigma uncertainties for a calibrated site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphericSiteCalibration {
    /// Representative site altitude in metres above mean sea level.
    pub representative_altitude_m: f64,
    /// One-sigma uncertainty of the representative altitude in metres.
    pub representative_altitude_uncertainty_m: f64,
    /// Surface pressure in hectopascals.
    pub surface_pressure_hpa: f64,
    /// One-sigma uncertainty of the surface pressure in hectopascals.
    pub surface_pressure_uncertainty_hpa: f64,
    /// Rayleigh scale height in kilometres.
    pub rayleigh_scale_height_km: f64,
    /// One-sigma uncertainty of the Rayleigh scale height in kilometres.
    pub rayleigh_scale_height_uncertainty_km: f64,
    /// Aerosol optical depth referenced at 550 nm.
    pub aerosol_optical_depth_550_nm: f64,
    /// One-sigma uncertainty of the aerosol optical depth at 550 nm.
    pub aerosol_optical_depth_uncertainty_550_nm: f64,
    /// Angstrom exponent used for aerosol wavelength scaling.
    pub angstrom_exponent: f64,
    /// One-sigma uncertainty of the Angstrom exponent.
    pub angstrom_exponent_uncertainty: f64,
}

/// Airglow continuum calibration and its declared temporal treatment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirglowCalibrationEvidence {
    /// Multiplicative continuum scale relative to the referenced template.
    pub continuum_scale: f64,
    /// One-sigma uncertainty of the continuum scale.
    pub continuum_scale_uncertainty: f64,
    /// Whether a temporal or seasonal correction is applied.
    pub temporal_correction_applied: bool,
    /// Stable identifier of the temporal correction model when one is applied.
    pub correction_model: Option<String>,
}

/// Immutable source used to derive or validate a site calibration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCalibrationReference {
    /// Stable lowercase identifier for the reference.
    pub id: String,
    /// Repository-relative path to the pinned reference bytes.
    pub path: String,
    /// Lowercase SHA-256 of the referenced bytes.
    pub sha256: String,
    /// Human-readable source and release identification.
    pub source: String,
    /// License or redistribution terms governing the reference.
    pub license: String,
}

/// Versioned, fail-closed evidence package for a CTAO site calibration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteCalibrationAsset {
    /// Schema version, currently `SITE_CALIBRATION_ASSET_SCHEMA_VERSION`.
    pub schema_version: u32,
    /// Stable lowercase identifier for this calibration release.
    pub calibration_id: String,
    /// CTAO site described by the calibration.
    pub site: CalibratedSiteId,
    /// Time and wavelength domain in which the calibration may be used.
    pub validity: SiteCalibrationValidity,
    /// Atmospheric parameters and uncertainties.
    pub atmosphere: AtmosphericSiteCalibration,
    /// Airglow continuum calibration and temporal treatment.
    pub airglow: AirglowCalibrationEvidence,
    /// Immutable references supporting the calibration.
    pub references: Vec<SiteCalibrationReference>,
    /// Explicit scientific or operational limitations.
    #[serde(default)]
    pub limitations: Vec<String>,
}

/// Error returned when a site-calibration asset cannot be parsed or validated.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct SiteCalibrationAssetError {
    message: String,
}

impl SiteCalibrationAssetError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl SiteCalibrationAsset {
    /// Parse and validate one strict TOML calibration asset.
    pub fn from_toml_str(input: &str) -> Result<Self, SiteCalibrationAssetError> {
        let asset: Self = toml::from_str(input).map_err(|error| {
            SiteCalibrationAssetError::new(format!("invalid site-calibration TOML: {error}"))
        })?;
        asset.validate()?;
        Ok(asset)
    }

    /// Validate schema, provenance, physical ranges and internal consistency.
    pub fn validate(&self) -> Result<(), SiteCalibrationAssetError> {
        if self.schema_version != SITE_CALIBRATION_ASSET_SCHEMA_VERSION {
            return Err(SiteCalibrationAssetError::new(format!(
                "unsupported site-calibration schema {}",
                self.schema_version
            )));
        }
        validate_identifier(&self.calibration_id, "calibration_id")?;
        self.validate_validity()?;
        self.validate_atmosphere()?;
        self.validate_airglow()?;
        self.validate_references()?;
        validate_nonempty_items(&self.limitations, "limitations")?;
        Ok(())
    }

    fn validate_validity(&self) -> Result<(), SiteCalibrationAssetError> {
        let valid_from = parse_date(&self.validity.valid_from, "validity.valid_from")?;
        let valid_through = parse_date(&self.validity.valid_through, "validity.valid_through")?;
        if valid_from > valid_through {
            return Err(SiteCalibrationAssetError::new(
                "validity.valid_from must not follow validity.valid_through",
            ));
        }
        let [minimum, maximum] = self.validity.wavelength_nm;
        if minimum < 100 || maximum > 3_000 || minimum >= maximum {
            return Err(SiteCalibrationAssetError::new(
                "validity.wavelength_nm must be an increasing interval within 100..=3000 nm",
            ));
        }
        Ok(())
    }

    fn validate_atmosphere(&self) -> Result<(), SiteCalibrationAssetError> {
        let atmosphere = &self.atmosphere;
        validate_range(
            atmosphere.representative_altitude_m,
            0.0,
            10_000.0,
            "atmosphere.representative_altitude_m",
        )?;
        validate_uncertainty(
            atmosphere.representative_altitude_uncertainty_m,
            10_000.0,
            "atmosphere.representative_altitude_uncertainty_m",
        )?;
        validate_range(
            atmosphere.surface_pressure_hpa,
            f64::MIN_POSITIVE,
            1_200.0,
            "atmosphere.surface_pressure_hpa",
        )?;
        validate_uncertainty(
            atmosphere.surface_pressure_uncertainty_hpa,
            atmosphere.surface_pressure_hpa,
            "atmosphere.surface_pressure_uncertainty_hpa",
        )?;
        validate_range(
            atmosphere.rayleigh_scale_height_km,
            f64::MIN_POSITIVE,
            100.0,
            "atmosphere.rayleigh_scale_height_km",
        )?;
        validate_uncertainty(
            atmosphere.rayleigh_scale_height_uncertainty_km,
            atmosphere.rayleigh_scale_height_km,
            "atmosphere.rayleigh_scale_height_uncertainty_km",
        )?;
        validate_range(
            atmosphere.aerosol_optical_depth_550_nm,
            0.0,
            10.0,
            "atmosphere.aerosol_optical_depth_550_nm",
        )?;
        validate_uncertainty(
            atmosphere.aerosol_optical_depth_uncertainty_550_nm,
            10.0,
            "atmosphere.aerosol_optical_depth_uncertainty_550_nm",
        )?;
        validate_range(
            atmosphere.angstrom_exponent,
            0.0,
            10.0,
            "atmosphere.angstrom_exponent",
        )?;
        validate_uncertainty(
            atmosphere.angstrom_exponent_uncertainty,
            10.0,
            "atmosphere.angstrom_exponent_uncertainty",
        )
    }

    fn validate_airglow(&self) -> Result<(), SiteCalibrationAssetError> {
        validate_range(
            self.airglow.continuum_scale,
            f64::MIN_POSITIVE,
            100.0,
            "airglow.continuum_scale",
        )?;
        validate_uncertainty(
            self.airglow.continuum_scale_uncertainty,
            100.0,
            "airglow.continuum_scale_uncertainty",
        )?;
        match (
            self.airglow.temporal_correction_applied,
            self.airglow.correction_model.as_deref(),
        ) {
            (true, Some(model)) if is_stable_identifier(model) => Ok(()),
            (true, _) => Err(SiteCalibrationAssetError::new(
                "airglow.correction_model must be a stable identifier when a correction is applied",
            )),
            (false, None) => Ok(()),
            (false, Some(_)) => Err(SiteCalibrationAssetError::new(
                "airglow.correction_model must be absent when no correction is applied",
            )),
        }
    }

    fn validate_references(&self) -> Result<(), SiteCalibrationAssetError> {
        if self.references.is_empty() {
            return Err(SiteCalibrationAssetError::new(
                "site calibration requires at least one immutable reference",
            ));
        }

        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for reference in &self.references {
            validate_identifier(&reference.id, "references[].id")?;
            validate_reference_path(&reference.path)?;
            validate_sha256(&reference.sha256)?;
            validate_nonempty(&reference.source, "references[].source")?;
            validate_nonempty(&reference.license, "references[].license")?;
            if !ids.insert(reference.id.as_str()) {
                return Err(SiteCalibrationAssetError::new(format!(
                    "duplicate site-calibration reference id {:?}",
                    reference.id
                )));
            }
            if !paths.insert(reference.path.as_str()) {
                return Err(SiteCalibrationAssetError::new(format!(
                    "duplicate site-calibration reference path {:?}",
                    reference.path
                )));
            }
        }
        Ok(())
    }
}

fn parse_date(value: &str, field: &str) -> Result<NaiveDate, SiteCalibrationAssetError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        SiteCalibrationAssetError::new(format!("{field} must use a valid YYYY-MM-DD date"))
    })
}

fn validate_range(
    value: f64,
    minimum: f64,
    maximum: f64,
    field: &str,
) -> Result<(), SiteCalibrationAssetError> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(SiteCalibrationAssetError::new(format!(
            "{field} must be finite and within {minimum}..={maximum}"
        )));
    }
    Ok(())
}

fn validate_uncertainty(
    value: f64,
    maximum: f64,
    field: &str,
) -> Result<(), SiteCalibrationAssetError> {
    validate_range(value, 0.0, maximum, field)
}

fn validate_identifier(value: &str, field: &str) -> Result<(), SiteCalibrationAssetError> {
    if !is_stable_identifier(value) {
        return Err(SiteCalibrationAssetError::new(format!(
            "{field} must contain lowercase ASCII letters, digits and single '-' separators"
        )));
    }
    Ok(())
}

fn is_stable_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn validate_reference_path(value: &str) -> Result<(), SiteCalibrationAssetError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.contains('\\')
        || value.split('/').any(|component| component.is_empty())
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::CurDir
                    | Component::ParentDir
            )
        })
    {
        return Err(SiteCalibrationAssetError::new(
            "references[].path must be a normalized repository-relative path",
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), SiteCalibrationAssetError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SiteCalibrationAssetError::new(
            "references[].sha256 must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn validate_nonempty(value: &str, field: &str) -> Result<(), SiteCalibrationAssetError> {
    if value.trim().is_empty() {
        return Err(SiteCalibrationAssetError::new(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

fn validate_nonempty_items(
    values: &[String],
    field: &str,
) -> Result<(), SiteCalibrationAssetError> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(SiteCalibrationAssetError::new(format!(
            "{field} must not contain empty entries"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_ASSET: &str = r#"
schema_version = 1
calibration_id = "ctao-south-reference-v1"
site = "ctao-south"
limitations = ["Valid only for the documented clear, moonless reference sample."]

[validity]
valid_from = "2025-01-01"
valid_through = "2025-12-31"
wavelength_nm = [300, 650]

[atmosphere]
representative_altitude_m = 2150.0
representative_altitude_uncertainty_m = 10.0
surface_pressure_hpa = 743.0
surface_pressure_uncertainty_hpa = 5.0
rayleigh_scale_height_km = 8.0
rayleigh_scale_height_uncertainty_km = 0.2
aerosol_optical_depth_550_nm = 0.03
aerosol_optical_depth_uncertainty_550_nm = 0.01
angstrom_exponent = 1.0
angstrom_exponent_uncertainty = 0.2

[airglow]
continuum_scale = 1.05
continuum_scale_uncertainty = 0.10
temporal_correction_applied = false

[[references]]
id = "ctao-south-atmosphere"
path = "site-calibration/ctao-south/atmosphere-v1.csv"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
source = "Documented CTAO-South atmospheric reference release"
license = "Redistribution terms recorded with the reference asset"
"#;

    fn valid_ctao_north_asset() -> String {
        VALID_ASSET
            .replace("ctao-south", "ctao-north")
            .replace("CTAO-South", "CTAO-North")
    }

    #[test]
    fn parses_a_valid_ctao_south_calibration_asset_deterministically() {
        let first = SiteCalibrationAsset::from_toml_str(VALID_ASSET).unwrap();
        let second = SiteCalibrationAsset::from_toml_str(VALID_ASSET).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.schema_version, SITE_CALIBRATION_ASSET_SCHEMA_VERSION);
        assert_eq!(first.site, CalibratedSiteId::CtaSouth);
        assert_eq!(first.site.planning_profile(), SiteProfileId::CtaSouth);
        assert_eq!(first.references.len(), 1);
    }

    #[test]
    fn parses_a_valid_ctao_north_calibration_asset_deterministically() {
        let input = valid_ctao_north_asset();
        let first = SiteCalibrationAsset::from_toml_str(&input).unwrap();
        let second = SiteCalibrationAsset::from_toml_str(&input).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.site, CalibratedSiteId::CtaNorth);
        assert_eq!(first.site.as_str(), "ctao-north");
        assert_eq!(first.site.planning_profile(), SiteProfileId::CtaNorth);
    }

    #[test]
    fn rejects_unknown_fields_and_generic_fallback_sites() {
        let unknown = VALID_ASSET.replacen(
            "schema_version = 1",
            "schema_version = 1\nunexpected = true",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&unknown).is_err());

        let generic =
            VALID_ASSET.replacen("site = \"ctao-south\"", "site = \"generic-clear-sky\"", 1);
        assert!(SiteCalibrationAsset::from_toml_str(&generic).is_err());
    }

    #[test]
    fn rejects_unsupported_schema_invalid_dates_and_negative_physical_values() {
        let unsupported = VALID_ASSET.replacen("schema_version = 1", "schema_version = 2", 1);
        assert!(SiteCalibrationAsset::from_toml_str(&unsupported).is_err());

        let invalid_date = VALID_ASSET.replacen("2025-12-31", "2025-02-30", 1);
        assert!(SiteCalibrationAsset::from_toml_str(&invalid_date).is_err());

        let negative_altitude = VALID_ASSET.replacen(
            "representative_altitude_m = 2150.0",
            "representative_altitude_m = -1.0",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&negative_altitude).is_err());

        let negative_pressure = VALID_ASSET.replacen(
            "surface_pressure_hpa = 743.0",
            "surface_pressure_hpa = -1.0",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&negative_pressure).is_err());

        let negative_angstrom =
            VALID_ASSET.replacen("angstrom_exponent = 1.0", "angstrom_exponent = -1.0", 1);
        assert!(SiteCalibrationAsset::from_toml_str(&negative_angstrom).is_err());
    }

    #[test]
    fn rejects_missing_provenance_bad_checksums_and_unsafe_paths() {
        let missing_source = VALID_ASSET.replacen(
            "source = \"Documented CTAO-South atmospheric reference release\"",
            "source = \"\"",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&missing_source).is_err());

        let uppercase_checksum = VALID_ASSET.replacen(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&uppercase_checksum).is_err());

        let unsafe_path = VALID_ASSET.replacen(
            "site-calibration/ctao-south/atmosphere-v1.csv",
            "site-calibration/../atmosphere-v1.csv",
            1,
        );
        assert!(SiteCalibrationAsset::from_toml_str(&unsafe_path).is_err());
    }

    #[test]
    fn rejects_duplicate_references_and_inconsistent_temporal_metadata() {
        let mut duplicate = SiteCalibrationAsset::from_toml_str(VALID_ASSET).unwrap();
        duplicate.references.push(duplicate.references[0].clone());
        assert!(duplicate.validate().is_err());

        let mut correction = SiteCalibrationAsset::from_toml_str(VALID_ASSET).unwrap();
        correction.airglow.temporal_correction_applied = true;
        assert!(correction.validate().is_err());
        correction.airglow.correction_model = Some("seasonal-v1".to_string());
        assert!(correction.validate().is_ok());
    }
}
