//! Bundled offline F10.7 store (runtime-embedded, build-time checksum-verified).

use super::store::F107Store;
use crate::assets::{bundled_asset, BundledAssetMetadata};
use crate::error::{NsbError, Result};
use std::sync::OnceLock;

const RAW: &str = include_str!("../../data/f107_store.json");

/// Path relative to `crates/nsb/data` as recorded in the scientific asset registry.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const BUNDLED_F107_RELATIVE_PATH: &str = "f107_store.json";

/// Canonical scientific provenance for the bundled F10.7 store.
///
/// Integrity and schema constraints are enforced by `build.rs`; this returns the
/// generated static metadata without re-checking the registry at runtime.
#[cfg_attr(not(test), allow(dead_code))]
pub fn bundled_f107_asset() -> &'static BundledAssetMetadata {
    bundled_asset(BUNDLED_F107_RELATIVE_PATH)
        .expect("f107_store.json must be registered by the build script")
}

/// Load the bundled offline F10.7 store (parsed once).
///
/// Provenance/integrity were validated at build time. The scientific JSON store
/// is still deserialized once at runtime because converting it to Rust literals
/// would bloat compile time without changing scientific behaviour.
pub fn bundled_f107_store() -> Result<&'static F107Store> {
    static STORE: OnceLock<std::result::Result<F107Store, String>> = OnceLock::new();
    let loaded = STORE.get_or_init(|| F107Store::from_json_str(RAW).map_err(|error| error.0));
    match loaded {
        Ok(store) => Ok(store),
        Err(message) => Err(NsbError::DataParse {
            file: "data/f107_store.json",
            message: message.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solar_activity::store::hex_sha256;

    #[test]
    fn bundled_f107_metadata_matches_embedded_bytes() {
        let asset = bundled_f107_asset();
        assert_eq!(asset.path, BUNDLED_F107_RELATIVE_PATH);
        assert_eq!(asset.schema, "nsb-f107-store-v1");
        assert_eq!(hex_sha256(RAW.as_bytes()), asset.sha256);
        assert!(!asset.source.is_empty());
        assert!(!asset.validation_report.is_empty());
        assert_eq!(asset.calibration_status, "planning-proxy");
    }
}
