//! Runtime-readable registry for scientific assets shipped with NSB.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const MANIFEST: &str = include_str!("../data/manifest.toml");

/// Versioned registry of all scientific files under `crates/nsb/data`.
#[derive(Debug, Deserialize)]
pub struct AssetRegistry {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Registered scientific assets.
    pub assets: Vec<ScientificAsset>,
}

/// Provenance and integrity metadata for one scientific asset.
#[derive(Debug, Deserialize)]
pub struct ScientificAsset {
    /// Path relative to `crates/nsb/data`.
    pub path: String,
    /// Versioned file-format identifier.
    pub schema: String,
    /// Lowercase hexadecimal SHA-256 digest.
    pub sha256: String,
    /// Scientific source and release information.
    pub source: String,
    /// Dataset redistribution terms or an explicit unresolved limitation.
    pub license: String,
    /// Program or workflow that generated the file.
    pub generator: String,
    /// Reproduction command or an explicit non-reproducibility statement.
    pub generation_command: String,
    /// Repository path to validation evidence.
    pub validation_report: String,
    /// Scientific maturity of the asset.
    pub calibration_status: String,
    /// Whether runtime code embeds the asset.
    pub runtime_embedded: bool,
    /// Header key/value pairs that must agree with the manifest.
    #[serde(default)]
    pub header: BTreeMap<String, String>,
}

impl AssetRegistry {
    /// Look up a registered asset by path relative to `crates/nsb/data`.
    pub fn asset(&self, path: &str) -> Option<&ScientificAsset> {
        self.assets.iter().find(|asset| asset.path == path)
    }
}

/// Return the parsed, immutable scientific-asset registry.
pub fn asset_registry() -> &'static AssetRegistry {
    static REGISTRY: OnceLock<AssetRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        toml::from_str(MANIFEST).expect("bundled scientific asset manifest must parse")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_versioned_and_paths_are_unique() {
        let registry = asset_registry();
        assert_eq!(registry.schema_version, 1);
        let mut paths: Vec<&str> = registry
            .assets
            .iter()
            .map(|asset| asset.path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), registry.assets.len());
    }
}
