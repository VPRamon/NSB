//! Structural and integrity validation for the scientific asset manifest.

use super::types::{Asset, Manifest, EXPECTED_MANIFEST_SCHEMA_VERSION, REQUIRED_RUNTIME_ASSETS};
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
    /// Production Starlight CSV/manifest pair count is invalid.
    StarlightPair {
        /// Number of production map assets.
        maps: usize,
        /// Number of production sidecar assets.
        manifests: usize,
    },
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
            Self::StarlightPair { maps, manifests } => write!(
                f,
                "expected either zero or one production starlight CSV/TOML pair; found {maps} maps and {manifests} manifests"
            ),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ManifestValidationError {}

/// Parse `manifest.toml` text into a typed registry document.
pub fn parse_manifest(raw: &str) -> Result<Manifest, String> {
    toml::from_str(raw).map_err(|err| format!("failed to parse scientific asset manifest: {err}"))
}

/// Validate manifest structure and required component constraints (no filesystem I/O).
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

    let maps = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_production_starlight_map())
        .count();
    let manifests = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_production_starlight_manifest())
        .count();
    if !matches!((maps, manifests), (0, 0) | (1, 1)) {
        return Err(ManifestValidationError::StarlightPair { maps, manifests });
    }

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

/// Select the optional production Starlight CSV/sidecar pair.
pub fn select_production_starlight(
    manifest: &Manifest,
) -> Result<Option<(&Asset, &Asset)>, ManifestValidationError> {
    let maps: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_production_starlight_map())
        .collect();
    let manifests: Vec<&Asset> = manifest
        .assets
        .iter()
        .filter(|asset| asset.is_production_starlight_manifest())
        .collect();
    match (maps.as_slice(), manifests.as_slice()) {
        ([], []) => Ok(None),
        ([map], [sidecar]) => Ok(Some((*map, *sidecar))),
        _ => Err(ManifestValidationError::StarlightPair {
            maps: maps.len(),
            manifests: manifests.len(),
        }),
    }
}
