//! Versioned production Starlight configuration.

use serde::{Deserialize, Serialize};

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
    /// Target and validation HEALPix resolutions.
    #[serde(default)]
    pub map: StarlightMapConfig,
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

/// HEALPix production and sweep policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarlightMapConfig {
    #[serde(default = "default_target_nside")]
    pub target_nside: u32,
    #[serde(default = "default_sweep_nsides")]
    pub sweep_nsides: Vec<u32>,
}

impl Default for StarlightMapConfig {
    fn default() -> Self {
        Self {
            target_nside: default_target_nside(),
            sweep_nsides: default_sweep_nsides(),
        }
    }
}

fn default_target_nside() -> u32 {
    128
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

fn default_sweep_nsides() -> Vec<u32> {
    vec![64, 128, 256, 512]
}
