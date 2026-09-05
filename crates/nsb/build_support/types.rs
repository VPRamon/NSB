//! Manifest types shared by the NSB build script.

use serde::Deserialize;
use std::collections::BTreeMap;

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
    /// Return whether this entry is the production Starlight release CSV.
    pub fn is_production_starlight_map(&self) -> bool {
        self.schema == STARLIGHT_MAP_SCHEMA
            && self.calibration_status.eq_ignore_ascii_case("production")
            && self.runtime_embedded
            && self.path.ends_with(".release.csv")
    }

    /// Return whether this entry is the production Starlight runtime sidecar.
    pub fn is_production_starlight_manifest(&self) -> bool {
        self.schema == STARLIGHT_MANIFEST_SCHEMA
            && self.calibration_status.eq_ignore_ascii_case("production")
            && self.runtime_embedded
            && self.path.ends_with(".manifest.toml")
    }
}
