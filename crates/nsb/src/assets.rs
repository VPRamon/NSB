//! Build-time verified scientific asset metadata.
//!
//! `crates/nsb/data/manifest.toml` remains the canonical declarative registry.
//! The build script parses and validates it, then embeds the resulting metadata
//! as static Rust data. Runtime code must not parse the TOML manifest.

/// Provenance and integrity metadata for one verified bundled scientific asset.
///
/// Values are generated from `crates/nsb/data/manifest.toml` during compilation
/// for assets whose existence and SHA-256 digest were verified by the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct BundledAssetMetadata {
    /// Path relative to `crates/nsb/data`.
    pub path: &'static str,
    /// Versioned file-format identifier.
    pub schema: &'static str,
    /// Lowercase hexadecimal SHA-256 digest.
    pub sha256: &'static str,
    /// Scientific source and release information.
    pub source: &'static str,
    /// Dataset redistribution terms or an explicit unresolved limitation.
    pub license: &'static str,
    /// Program or workflow that generated the file.
    pub generator: &'static str,
    /// Reproduction command or an explicit non-reproducibility statement.
    pub generation_command: &'static str,
    /// Repository path to validation evidence.
    pub validation_report: &'static str,
    /// Scientific maturity of the asset.
    pub calibration_status: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/nsb_bundled_assets.rs"));

/// Look up a verified bundled asset by path relative to `crates/nsb/data`.
///
/// Only `runtime_embedded` assets that passed build-time existence and SHA-256
/// checks are present. Candidate/external registry entries are not exposed.
pub fn bundled_asset(path: &str) -> Option<&'static BundledAssetMetadata> {
    BUNDLED_ASSETS.iter().find(|asset| asset.path == path)
}

/// Iterate every verified bundled (`runtime_embedded`) asset.
pub fn bundled_assets() -> impl Iterator<Item = &'static BundledAssetMetadata> {
    BUNDLED_ASSETS.iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_bundled_assets_are_versioned_and_unique() {
        assert_eq!(ASSET_MANIFEST_SCHEMA_VERSION, 1);
        let mut paths: Vec<&str> = BUNDLED_ASSETS.iter().map(|asset| asset.path).collect();
        let before = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), before);
        assert!(bundled_assets().count() >= 5);
        assert!(bundled_asset("airglow_cont.dat").is_some());
        assert!(bundled_asset("f107_store.json").is_some());
        // Candidates are registered in manifest.toml but not build-verified here.
        assert!(bundled_asset("starlight_nside128.csv").is_none());
        assert!(bundled_asset("merge_report.json").is_none());
    }
}
