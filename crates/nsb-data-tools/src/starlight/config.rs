//! Versioned production Starlight configuration.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Starlight-specific inputs and scientific policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarlightConfig {
    /// Reproducible snapshot or full Gaia production pipeline.
    #[serde(default)]
    pub mode: StarlightMode,
    /// Official bulk-product inventories required in production mode.
    #[serde(default)]
    pub gaia_products: Vec<GaiaProductConfig>,
    /// Download retry, timeout, and cache policy.
    #[serde(default)]
    pub acquisition: AcquisitionConfig,
    /// Canonical HEALPix map policy.
    #[serde(default)]
    pub map: StarlightMapConfig,
    /// Spectral product requested from each source.
    #[serde(default)]
    pub product_band: StarlightProductBand,
    /// Optional checksum-pinned 300–336 nm correction.
    #[serde(default)]
    pub ultraviolet_correction: Option<UvCorrectionConfig>,
    /// Optional checksum-pinned non-XP photometric inference artifact.
    #[serde(default)]
    pub photometric_inference: Option<ArtifactPinConfig>,
    /// Optional checksum-pinned Gaia selection-function artifact.
    #[serde(default)]
    pub selection_function: Option<ArtifactPinConfig>,
}

/// Network policy for resumable official-source acquisition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionConfig {
    #[serde(default = "default_connect_timeout_seconds")]
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_request_timeout_seconds")]
    pub request_timeout_seconds: u64,
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

impl Default for AcquisitionConfig {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: default_connect_timeout_seconds(),
            request_timeout_seconds: default_request_timeout_seconds(),
            max_attempts: default_max_attempts(),
        }
    }
}

/// Starlight pipeline intent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StarlightMode {
    /// Reproduce an already materialized map.
    #[default]
    Snapshot,
    /// Construct the full Gaia-derived production candidate.
    Production,
}

/// Spectral band emitted by the Starlight product.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StarlightProductBand {
    /// Direct Gaia XP continuous integral only.
    #[default]
    #[serde(rename = "measured-336-650")]
    Measured336To650,
    /// Independently corrected UV plus the unchanged Gaia integral.
    #[serde(rename = "combined-300-650")]
    Combined300To650,
}

/// Location and immutable identity of a checksum-pinned calibration artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPinConfig {
    pub artifact_path: PathBuf,
    pub sha256: String,
}

/// Location and immutable identity of a UV correction artifact.
pub type UvCorrectionConfig = ArtifactPinConfig;

/// One official bulk distribution and its checksum inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaiaProductConfig {
    /// Stable product identifier used in paths and manifests.
    pub id: String,
    /// Base HTTPS directory containing the partition files.
    pub base_url: String,
    /// HTTPS checksum-manifest URL.
    pub checksum_manifest_url: String,
    /// Pinned SHA-256 of the complete upstream checksum manifest.
    pub checksum_manifest_sha256: String,
    /// Checksum algorithm declared by the upstream manifest.
    pub checksum_algorithm: OfficialChecksumAlgorithm,
    /// Optional exact upstream partition count.
    pub expected_partitions: Option<usize>,
    /// Required filename prefix.
    pub filename_prefix: String,
    /// Required filename suffix.
    pub filename_suffix: String,
}

/// Upstream checksum algorithms accepted for source verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OfficialChecksumAlgorithm {
    Md5,
    Sha256,
}

impl OfficialChecksumAlgorithm {
    pub(crate) fn digest_len(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha256 => 64,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
        }
    }
}

/// HEALPix canonical-map policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarlightMapConfig {
    /// Resolution used directly for source-level accumulation and publication.
    #[serde(default = "default_canonical_nside")]
    pub canonical_nside: u32,
}

impl Default for StarlightMapConfig {
    fn default() -> Self {
        Self {
            canonical_nside: default_canonical_nside(),
        }
    }
}

fn default_canonical_nside() -> u32 {
    128
}

pub(crate) fn validate_canonical_nside(nside: u32) -> Result<()> {
    if nside == 0 || !nside.is_power_of_two() || nside > 4096 {
        bail!("Starlight canonical_nside must be a power of two between 1 and 4096");
    }
    Ok(())
}

fn default_connect_timeout_seconds() -> u64 {
    30
}

fn default_request_timeout_seconds() -> u64 {
    30 * 60
}

fn default_max_attempts() -> u32 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;

    #[test]
    fn canonical_nside_accepts_multiple_source_level_resolutions() {
        validate_canonical_nside(128).unwrap();
        validate_canonical_nside(256).unwrap();
    }

    #[test]
    fn canonical_nside_fails_closed_for_unsupported_values() {
        for nside in [0, 3, 8192] {
            assert!(validate_canonical_nside(nside).is_err());
        }
    }

    #[test]
    fn changing_canonical_nside_changes_serialized_configuration_identity() {
        let mut map = StarlightMapConfig::default();
        let first = serde_json::to_vec(&map).unwrap();
        map.canonical_nside = 256;
        let second = serde_json::to_vec(&map).unwrap();
        assert_ne!(
            checksum_io::sha256_bytes(&first),
            checksum_io::sha256_bytes(&second)
        );
    }
}
