//! Manifest types shared by the NSB build script.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// Supported `schema_version` for `crates/nsb/data/manifest.toml`.
pub const EXPECTED_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Production Starlight map schema selected by the build script.
pub const STARLIGHT_MAP_SCHEMA: &str = "nsb-healpix-starlight-v2";
/// Production Starlight runtime-sidecar schema selected by the build script.
pub const STARLIGHT_MANIFEST_SCHEMA: &str = "nsb-starlight-runtime-manifest-v1";

/// Component-owned assets that must be present as `runtime_embedded` with a fixed schema.
pub const REQUIRED_RUNTIME_ASSETS: &[(&str, &str)] = &[
    ("airglow_cont.dat", "skycalc-airglow-continuum-v1"),
    ("f107_store.json", "nsb-f107-store-v1"),
    ("mie_m15s1.dat", "moonlight-mie-angle-wavelength-grid-v1"),
    (
        "sscatcor_m15s1.dat",
        "moonlight-multiple-scattering-grid-v1",
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

    /// Return whether this entry looks like a Starlight release map registration.
    ///
    /// Release-shaped entries must either form a valid production pair or fail
    /// the build; they must not silently become “Starlight unavailable”.
    pub fn is_starlight_release_map_claim(&self) -> bool {
        self.path.ends_with(".release.csv") || self.schema == STARLIGHT_MAP_SCHEMA
    }

    /// Return whether this entry looks like a Starlight release sidecar registration.
    pub fn is_starlight_release_manifest_claim(&self) -> bool {
        self.schema == STARLIGHT_MANIFEST_SCHEMA
            || (self.path.ends_with(".manifest.toml")
                && starlight_release_stem(&self.path).is_some())
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
