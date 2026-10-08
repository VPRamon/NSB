// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Manifest types shared by the NSB build script.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// Supported `schema_version` for `crates/nsb/data/manifest.toml`.
pub const EXPECTED_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Production Starlight map schema selected by the build script.
pub const STARLIGHT_MAP_SCHEMA: &str = "nsb-healpix-starlight-v1";
/// Production Starlight runtime-sidecar schema selected by the build script.
pub const STARLIGHT_MANIFEST_SCHEMA: &str = "nsb-starlight-runtime-manifest-v1";

/// Component-owned assets that must be present as `runtime_embedded` with a fixed schema.
pub const REQUIRED_RUNTIME_ASSETS: &[(&str, &str)] = &[
    ("airglow_palace_v1.dat", "nsb-airglow-palace-continuum-v1"),
    ("f107_store.json", "nsb-f107-store-v1"),
    ("moonlight_mie_nsb_v1.dat", "nsb-moonlight-mie-phase-v1"),
    (
        "moonlight_multiscatter_nsb_v1.dat",
        "nsb-moonlight-multiscatter-v1",
    ),
    ("solar_spectrum.dat", "wavelength-nm_irradiance-w-m2-nm-v1"),
];

/// Top-level scientific asset registry document.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Registered scientific assets in declaration order.
    pub assets: Vec<Asset>,
}

/// One registry entry from `manifest.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Asset {
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
    /// Optional header key/value pairs (candidate maps, etc.).
    #[serde(default)]
    pub header: BTreeMap<String, String>,
}

impl Asset {
    /// Return whether this entry is a fully valid production Starlight release CSV.
    pub fn is_valid_production_starlight_map(&self) -> bool {
        self.schema == STARLIGHT_MAP_SCHEMA
            && self.calibration_status.eq_ignore_ascii_case("production")
            && self.runtime_embedded
            && self.path.ends_with(".release.csv")
    }

    /// Return whether this entry is a fully valid production Starlight sidecar.
    pub fn is_valid_production_starlight_manifest(&self) -> bool {
        self.schema == STARLIGHT_MANIFEST_SCHEMA
            && self.calibration_status.eq_ignore_ascii_case("production")
            && self.runtime_embedded
            && self.path.ends_with(".manifest.toml")
    }

    /// Return whether this entry makes an active Starlight production-map claim.
    ///
    /// Release-shaped bytes may be checksum-registered as staged candidates with
    /// `runtime_embedded = false`; those do not activate bundled production.
    /// Any entry that claims production or runtime embedding still fails closed
    /// unless it forms a valid production pair.
    pub fn is_starlight_release_map_claim(&self) -> bool {
        (self.path.ends_with(".release.csv") || self.schema == STARLIGHT_MAP_SCHEMA)
            && (self.runtime_embedded || self.calibration_status.eq_ignore_ascii_case("production"))
    }

    /// Return whether this entry makes an active Starlight production-sidecar claim.
    pub fn is_starlight_release_manifest_claim(&self) -> bool {
        (self.schema == STARLIGHT_MANIFEST_SCHEMA
            || (self.path.ends_with(".manifest.toml")
                && starlight_release_stem(&self.path).is_some()))
            && (self.runtime_embedded || self.calibration_status.eq_ignore_ascii_case("production"))
    }

    /// Stem shared by `*.release.csv` / `*.manifest.toml` release pair paths.
    pub fn starlight_release_stem(&self) -> Option<&str> {
        starlight_release_stem(&self.path)
    }
}

/// Extract the release stem from a Starlight release map or sidecar path.
pub fn starlight_release_stem(path: &str) -> Option<&str> {
    path.strip_suffix(".release.csv")
        .or_else(|| path.strip_suffix(".manifest.toml"))
}

/// Return whether `path` is a safe relative path confined under `data/`.
pub fn is_safe_data_relative_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\0') {
        return false;
    }
    // Reject absolute Unix/Windows spellings before Path parsing differences.
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return false;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return false;
    }
    let mut has_normal = false;
    for component in p.components() {
        match component {
            Component::Normal(part) => {
                if part.is_empty() {
                    return false;
                }
                has_normal = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    has_normal
}
