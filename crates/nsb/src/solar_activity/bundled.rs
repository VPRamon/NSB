//! Bundled offline F10.7 store (runtime-embedded, checksum-pinned).

use super::store::{hex_sha256, F107Store};
use crate::assets::{asset_registry, ScientificAsset};
use crate::error::{NsbError, Result};
use std::sync::OnceLock;

const RAW: &str = include_str!("../../data/f107_store.json");

/// Path relative to `crates/nsb/data` as recorded in the scientific asset registry.
pub const BUNDLED_F107_RELATIVE_PATH: &str = "f107_store.json";
/// Runtime/API asset path label for the bundled F10.7 store.
pub const BUNDLED_F107_ASSET_PATH: &str = "NSB/data/f107_store.json";
/// Pin of embedded F10.7 store bytes (integrity only; not provenance).
///
/// Verified at load time rather than via a compile-time `assert_data_checksum!`
/// because the JSON snapshot is large enough that const SHA-256 evaluation is
/// impractically slow. Manifest + this constant remain the dual pin.
pub const BUNDLED_F107_EMBEDDED_SHA256: &str =
    "47bc6923069739223d4244f8a9ad7821149ce905aaf3cfd89721ff4c9bde9a17";

/// Canonical scientific provenance for the bundled F10.7 store.
pub fn bundled_f107_asset() -> &'static ScientificAsset {
    asset_registry()
        .asset(BUNDLED_F107_RELATIVE_PATH)
        .expect("f107_store.json must be registered in the scientific asset manifest")
}

fn ensure_registry_matches_embedded_bytes() -> Result<()> {
    let digest = hex_sha256(RAW.as_bytes());
    if digest != BUNDLED_F107_EMBEDDED_SHA256 {
        return Err(NsbError::DataParse {
            file: "data/f107_store.json",
            message: format!(
                "embedded F10.7 store sha256 {digest} does not match pin {BUNDLED_F107_EMBEDDED_SHA256}"
            ),
        });
    }
    let asset = bundled_f107_asset();
    if asset.sha256 != BUNDLED_F107_EMBEDDED_SHA256 {
        return Err(NsbError::DataParse {
            file: "data/manifest.toml",
            message: format!(
                "F10.7 store registry sha256 {} does not match embedded pin {}",
                asset.sha256, BUNDLED_F107_EMBEDDED_SHA256
            ),
        });
    }
    if !asset.runtime_embedded {
        return Err(NsbError::DataParse {
            file: "data/manifest.toml",
            message: "f107_store.json must be marked runtime_embedded".into(),
        });
    }
    if asset.schema != "nsb-f107-store-v1" {
        return Err(NsbError::DataParse {
            file: "data/manifest.toml",
            message: format!("unexpected F10.7 store schema {}", asset.schema),
        });
    }
    Ok(())
}

/// Load the bundled offline F10.7 store (parsed once).
pub fn bundled_f107_store() -> Result<&'static F107Store> {
    static STORE: OnceLock<std::result::Result<F107Store, String>> = OnceLock::new();
    let loaded = STORE.get_or_init(|| {
        ensure_registry_matches_embedded_bytes().map_err(|error| error.to_string())?;
        F107Store::from_json_str(RAW).map_err(|error| error.0)
    });
    match loaded {
        Ok(store) => Ok(store),
        Err(message) => Err(NsbError::DataParse {
            file: "data/f107_store.json",
            message: message.clone(),
        }),
    }
}
