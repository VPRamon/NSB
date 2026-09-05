//! Structural and integrity validation for the scientific asset manifest.

use super::types::{
    is_safe_data_relative_path, Asset, Manifest, EXPECTED_MANIFEST_SCHEMA_VERSION,
    REQUIRED_RUNTIME_ASSETS, STARLIGHT_MANIFEST_SCHEMA, STARLIGHT_MAP_SCHEMA,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Errors produced while validating a scientific asset manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestValidationError {
    /// Unsupported or missing `schema_version`.
    SchemaVersion {
        /// Observed version.
        found: u32,
        /// Expected version.
        expected: u32,
    },
    /// Duplicate `assets.path` entries.
    DuplicatePath(String),
    /// Manifest path escapes `crates/nsb/data` or is otherwise unsafe.
    UnsafePath(String),
    /// A required component asset is missing, not embedded, or has the wrong schema.
    RequiredAsset {
        /// Relative path that must be registered.
        path: String,
        /// Expected schema identifier.
        expected_schema: String,
        /// Human-readable reason.
        reason: String,
    },
    /// A `runtime_embedded` asset file is missing on disk.
    MissingEmbeddedFile(String),
    /// SHA-256 of an embedded file does not match the manifest.
    ChecksumMismatch {
        /// Relative asset path.
        path: String,
        /// Digest recorded in the manifest.
        expected: String,
        /// Digest of the file bytes.
        actual: String,
    },
    /// A release-shaped Starlight registration is incomplete or inconsistent.
    StarlightPolicy(String),
    /// Generic structural problem.
    Message(String),
}

impl std::fmt::Display for ManifestValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaVersion { found, expected } => write!(
                f,
                "unsupported asset manifest schema_version {found}; expected {expected}"
            ),
            Self::DuplicatePath(path) => write!(f, "duplicate asset path {path:?}"),
            Self::UnsafePath(path) => write!(
                f,
                "asset path {path:?} must be a relative path confined under crates/nsb/data"
            ),
            Self::RequiredAsset {
                path,
                expected_schema,
                reason,
            } => write!(
                f,
                "required runtime asset {path:?} (schema {expected_schema}): {reason}"
            ),
            Self::MissingEmbeddedFile(path) => {
                write!(f, "runtime_embedded asset {path:?} is missing from data/")
            }
            Self::ChecksumMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "checksum mismatch for runtime_embedded asset {path:?}: manifest {expected}, actual {actual}"
            ),
            Self::StarlightPolicy(message) => {
                write!(f, "starlight production asset policy: {message}")
            }
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ManifestValidationError {}

/// Parse `manifest.toml` text into a typed registry document.
pub fn parse_manifest(raw: &str) -> Result<Manifest, String> {
    toml::from_str(raw).map_err(|err| format!("failed to parse scientific asset manifest: {err}"))
}

/// Validate manifest structure, path confinement, and required component constraints.
///
/// This performs no filesystem I/O.
pub fn validate_manifest_structure(manifest: &Manifest) -> Result<(), ManifestValidationError> {
    if manifest.schema_version != EXPECTED_MANIFEST_SCHEMA_VERSION {
        return Err(ManifestValidationError::SchemaVersion {
            found: manifest.schema_version,
            expected: EXPECTED_MANIFEST_SCHEMA_VERSION,
        });
    }
    if manifest.assets.is_empty() {
        return Err(ManifestValidationError::Message(
            "asset manifest contains no assets".into(),
        ));
    }

    let mut seen = BTreeSet::new();
    for asset in &manifest.assets {
        validate_required_fields(asset)?;
        validate_path_confinement(&asset.path)?;
        if !seen.insert(asset.path.as_str()) {
            return Err(ManifestValidationError::DuplicatePath(asset.path.clone()));
        }
    }

    for &(path, schema) in REQUIRED_RUNTIME_ASSETS {
        match manifest.assets.iter().find(|asset| asset.path == path) {
            None => {
                return Err(ManifestValidationError::RequiredAsset {
                    path: path.into(),
                    expected_schema: schema.into(),
                    reason: "missing from manifest".into(),
                });
            }
            Some(asset) if !asset.runtime_embedded => {
                return Err(ManifestValidationError::RequiredAsset {
                    path: path.into(),
                    expected_schema: schema.into(),
                    reason: "must be marked runtime_embedded = true".into(),
                });
            }
            Some(asset) if asset.schema != schema => {
                return Err(ManifestValidationError::RequiredAsset {
                    path: path.into(),
                    expected_schema: schema.into(),
                    reason: format!("found schema {:?}", asset.schema),
                });
            }
            Some(_) => {}
        }
    }

    validate_starlight_release_policy(manifest)?;
    Ok(())
}

fn validate_required_fields(asset: &Asset) -> Result<(), ManifestValidationError> {
    for (label, value) in [
        ("path", asset.path.as_str()),
        ("schema", asset.schema.as_str()),
        ("sha256", asset.sha256.as_str()),
        ("source", asset.source.as_str()),
        ("license", asset.license.as_str()),
        ("generator", asset.generator.as_str()),
        ("generation_command", asset.generation_command.as_str()),
        ("validation_report", asset.validation_report.as_str()),
        ("calibration_status", asset.calibration_status.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ManifestValidationError::Message(format!(
                "asset {:?} has empty {label}",
                asset.path
            )));
        }
    }
    if asset.sha256.len() != 64 || !asset.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ManifestValidationError::Message(format!(
            "asset {:?} has invalid sha256 {:?}",
            asset.path, asset.sha256
        )));
    }
    Ok(())
}

/// Reject absolute paths, `..`, and other escapes from `crates/nsb/data`.
pub fn validate_path_confinement(path: &str) -> Result<(), ManifestValidationError> {
    if is_safe_data_relative_path(path) {
        Ok(())
    } else {
        Err(ManifestValidationError::UnsafePath(path.to_string()))
    }
}

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Verify existence and checksum for every `runtime_embedded = true` asset.
///
/// Assets with `runtime_embedded = false` are ignored: candidates and external
/// artefacts may appear in the manifest without becoming compile requirements.
pub fn validate_runtime_embedded_files(
    data_dir: &Path,
    manifest: &Manifest,
) -> Result<(), ManifestValidationError> {
    for asset in &manifest.assets {
        if !asset.runtime_embedded {
            continue;
        }
        validate_path_confinement(&asset.path)?;
        let path = data_dir.join(&asset.path);
        if !path.is_file() {
            return Err(ManifestValidationError::MissingEmbeddedFile(
                asset.path.clone(),
            ));
        }
        let bytes = fs::read(&path).map_err(|err| {
            ManifestValidationError::Message(format!(
                "failed to read {} for checksum: {err}",
                path.display()
            ))
        })?;
        let actual = hex_sha256(&bytes);
        if actual != asset.sha256 {
            return Err(ManifestValidationError::ChecksumMismatch {
                path: asset.path.clone(),
                expected: asset.sha256.clone(),
                actual,
            });
        }
    }
    Ok(())
}

/// Assets whose bundled runtime identity was verified (existence + SHA-256).
pub fn verified_runtime_embedded_assets(manifest: &Manifest) -> Vec<&Asset> {
    manifest
        .assets
        .iter()
        .filter(|asset| asset.runtime_embedded)
        .collect()
}

/// Select the optional production Starlight CSV/sidecar pair.
///
/// Distinguishes intentional absence of a production release from a malformed
/// release-shaped registration, which must fail the build.
pub fn select_production_starlight(
    manifest: &Manifest,
) -> Result<Option<(&Asset, &Asset)>, ManifestValidationError> {
    validate_starlight_release_policy(manifest)?;
    let maps: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_valid_production_starlight_map())
        .collect();
    let manifests: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_valid_production_starlight_manifest())
        .collect();
    match (maps.as_slice(), manifests.as_slice()) {
        ([], []) => Ok(None),
        ([map], [sidecar]) => Ok(Some((*map, *sidecar))),
        _ => unreachable!("validate_starlight_release_policy guarantees 0 or 1 valid pairs"),
    }
}

fn validate_starlight_release_policy(manifest: &Manifest) -> Result<(), ManifestValidationError> {
    let map_claims: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_starlight_release_map_claim())
        .collect();
    let sidecar_claims: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_starlight_release_manifest_claim())
        .collect();

    if map_claims.is_empty() && sidecar_claims.is_empty() {
        return Ok(());
    }

    if map_claims.len() != 1 || sidecar_claims.len() != 1 {
        return Err(ManifestValidationError::StarlightPolicy(format!(
            "expected exactly one release map claim and one release sidecar claim; found {} map claim(s) and {} sidecar claim(s)",
            map_claims.len(),
            sidecar_claims.len()
        )));
    }

    let map = map_claims[0];
    let sidecar = sidecar_claims[0];

    let map_stem = map.starlight_release_stem().ok_or_else(|| {
        ManifestValidationError::StarlightPolicy(format!(
            "release map claim {:?} must use a *.release.csv path",
            map.path
        ))
    })?;
    let sidecar_stem = sidecar.starlight_release_stem().ok_or_else(|| {
        ManifestValidationError::StarlightPolicy(format!(
            "release sidecar claim {:?} must use a *.manifest.toml path",
            sidecar.path
        ))
    })?;
    if map_stem != sidecar_stem {
        return Err(ManifestValidationError::StarlightPolicy(format!(
            "release map {:?} and sidecar {:?} do not share a release stem",
            map.path, sidecar.path
        )));
    }

    require_valid_production_map(map)?;
    require_valid_production_sidecar(sidecar)?;
    Ok(())
}

fn require_valid_production_map(asset: &Asset) -> Result<(), ManifestValidationError> {
    if asset.is_valid_production_starlight_map() {
        return Ok(());
    }
    let mut reasons = Vec::new();
    if !asset.path.ends_with(".release.csv") {
        reasons.push(format!(
            "path must end with .release.csv (found {:?})",
            asset.path
        ));
    }
    if asset.schema != STARLIGHT_MAP_SCHEMA {
        reasons.push(format!(
            "schema must be {STARLIGHT_MAP_SCHEMA} (found {:?})",
            asset.schema
        ));
    }
    if !asset.calibration_status.eq_ignore_ascii_case("production") {
        reasons.push(format!(
            "calibration_status must be production (found {:?})",
            asset.calibration_status
        ));
    }
    if !asset.runtime_embedded {
        reasons.push("runtime_embedded must be true".into());
    }
    Err(ManifestValidationError::StarlightPolicy(format!(
        "release map claim {:?} is not a valid production registration: {}",
        asset.path,
        reasons.join("; ")
    )))
}

fn require_valid_production_sidecar(asset: &Asset) -> Result<(), ManifestValidationError> {
    if asset.is_valid_production_starlight_manifest() {
        return Ok(());
    }
    let mut reasons = Vec::new();
    if !asset.path.ends_with(".manifest.toml") {
        reasons.push(format!(
            "path must end with .manifest.toml (found {:?})",
            asset.path
        ));
    }
    if asset.schema != STARLIGHT_MANIFEST_SCHEMA {
        reasons.push(format!(
            "schema must be {STARLIGHT_MANIFEST_SCHEMA} (found {:?})",
            asset.schema
        ));
    }
    if !asset.calibration_status.eq_ignore_ascii_case("production") {
        reasons.push(format!(
            "calibration_status must be production (found {:?})",
            asset.calibration_status
        ));
    }
    if !asset.runtime_embedded {
        reasons.push("runtime_embedded must be true".into());
    }
    Err(ManifestValidationError::StarlightPolicy(format!(
        "release sidecar claim {:?} is not a valid production registration: {}",
        asset.path,
        reasons.join("; ")
    )))
}
