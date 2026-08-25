//! Machine-verifiable review conditions for `approved_with_conditions`.
//!
//! Free-form strings are accepted by the schema so existing templates parse,
//! but they never satisfy automatic promotion.

use crate::platform::checksum_io;
use crate::starlight::validation::ArtifactManifest;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Canonical immutable evidence bundle pinned by the human decisions in #103.
pub const REVIEW_BUNDLE_PATH: &str =
    "docs/nsb_components/starlight/release-candidate/review-bundle-v1.toml";
const REVIEW_BUNDLE_SCHEMA_VERSION: u32 = 1;
const REVIEW_BUNDLE_SCHEMA: &str = "nsb-starlight-review-bundle-v1";
const VALIDATION_ARTIFACT_MANIFEST_ID: &str = "validation_artifact_manifest";
const VALIDATION_ARTIFACT_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewBundle {
    schema_version: u32,
    schema: String,
    artifacts: Vec<ReviewBundleArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewBundleArtifact {
    id: String,
    path: String,
    sha256: String,
}

/// Verify every file pinned by the Starlight human-review bundle and, for the
/// validation artifact manifest, every file that manifest pins transitively.
///
/// This prevents a reviewer-approved top-level manifest from remaining valid
/// after a preregistration, region definition, validation result, rendered
/// report, candidate map, or other nested validation artifact is changed or
/// removed. Paths are required to stay repository-relative and byte counts are
/// checked in addition to SHA-256 digests.
pub fn verify_review_bundle_evidence(repository_root: &Path, bundle_path: &Path) -> Result<()> {
    let resolved_bundle = repository_path(repository_root, bundle_path, "review bundle")?;
    let raw = fs::read_to_string(&resolved_bundle)
        .with_context(|| format!("read review bundle {}", resolved_bundle.display()))?;
    let bundle: ReviewBundle = toml::from_str(&raw)
        .with_context(|| format!("parse review bundle {}", resolved_bundle.display()))?;

    if bundle.schema_version != REVIEW_BUNDLE_SCHEMA_VERSION {
        bail!(
            "unsupported review bundle schema_version {}; expected {REVIEW_BUNDLE_SCHEMA_VERSION}",
            bundle.schema_version
        );
    }
    if bundle.schema != REVIEW_BUNDLE_SCHEMA {
        bail!(
            "unsupported review bundle schema {:?}; expected {REVIEW_BUNDLE_SCHEMA:?}",
            bundle.schema
        );
    }
    if bundle.artifacts.is_empty() {
        bail!("review bundle must pin at least one artifact");
    }

    let mut ids = BTreeSet::new();
    let mut saw_validation_manifest = false;
    for artifact in &bundle.artifacts {
        if artifact.id.trim().is_empty() {
            bail!("review bundle artifact id must not be empty");
        }
        if !ids.insert(artifact.id.as_str()) {
            bail!("review bundle contains duplicate artifact id {:?}", artifact.id);
        }
        require_digest("review bundle artifact sha256", &artifact.sha256)?;
        let artifact_path = repository_path(
            repository_root,
            Path::new(&artifact.path),
            &format!("review bundle artifact {}", artifact.id),
        )?;
        let actual = checksum_io::sha256_file(&artifact_path).with_context(|| {
            format!(
                "checksum review bundle artifact {} at {}",
                artifact.id,
                artifact_path.display()
            )
        })?;
        if actual != artifact.sha256 {
            bail!(
                "review bundle artifact {} checksum mismatch: expected {}, actual {}",
                artifact.id,
                artifact.sha256,
                actual
            );
        }

        if artifact.id == VALIDATION_ARTIFACT_MANIFEST_ID {
            saw_validation_manifest = true;
            verify_validation_artifact_manifest(repository_root, &artifact_path)?;
        }
    }

    if !saw_validation_manifest {
        bail!(
            "review bundle is missing required artifact {VALIDATION_ARTIFACT_MANIFEST_ID:?}"
        );
    }
    Ok(())
}

fn verify_validation_artifact_manifest(repository_root: &Path, manifest_path: &Path) -> Result<()> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read validation artifact manifest {}", manifest_path.display()))?;
    let manifest: ArtifactManifest = toml::from_str(&raw)
        .with_context(|| format!("parse validation artifact manifest {}", manifest_path.display()))?;
    if manifest.schema_version != VALIDATION_ARTIFACT_MANIFEST_SCHEMA_VERSION {
        bail!(
            "unsupported validation artifact manifest schema_version {}; expected {VALIDATION_ARTIFACT_MANIFEST_SCHEMA_VERSION}",
            manifest.schema_version
        );
    }
    if manifest.artifacts.is_empty() {
        bail!("validation artifact manifest must pin at least one artifact");
    }

    let mut paths = BTreeSet::<PathBuf>::new();
    for artifact in &manifest.artifacts {
        if artifact.name.trim().is_empty() {
            bail!("validation artifact name must not be empty");
        }
        require_digest("validation artifact sha256", &artifact.sha256)?;
        if !paths.insert(artifact.path.clone()) {
            bail!(
                "validation artifact manifest contains duplicate path {}",
                artifact.path.display()
            );
        }
        let path = repository_path(
            repository_root,
            &artifact.path,
            &format!("validation artifact {}", artifact.name),
        )?;
        let bytes = fs::read(&path).with_context(|| {
            format!(
                "read validation artifact {} at {}",
                artifact.name,
                path.display()
            )
        })?;
        let actual_bytes = u64::try_from(bytes.len()).context("validation artifact length fits u64")?;
        if actual_bytes != artifact.bytes {
            bail!(
                "validation artifact {} byte-count mismatch: expected {}, actual {}",
                artifact.name,
                artifact.bytes,
                actual_bytes
            );
        }
        let actual_sha256 = checksum_io::sha256_bytes(&bytes);
        if actual_sha256 != artifact.sha256 {
            bail!(
                "validation artifact {} checksum mismatch: expected {}, actual {}",
                artifact.name,
                artifact.sha256,
                actual_sha256
            );
        }
    }
    Ok(())
}

fn repository_path(repository_root: &Path, relative: &Path, label: &str) -> Result<PathBuf> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        bail!("{label} path must be a non-empty repository-relative path");
    }
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!(
                "{label} path {} must not contain traversal or absolute components",
                relative.display()
            );
        }
    }
    Ok(repository_root.join(relative))
}

/// One recorded condition on an `approved_with_conditions` decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReviewCondition {
    /// Unstructured human prose. Always blocks automatic promotion.
    FreeText(String),
    /// Structured, machine-verifiable condition.
    Structured(StructuredCondition),
}

/// Stable, checksum-linked condition object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredCondition {
    pub id: String,
    pub description: String,
    pub verifier: ConditionVerifier,
}

/// Supported fail-closed verifiers. Unknown `type` values fail to deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionVerifier {
    /// Recompute SHA-256 of a repository-relative file.
    RepositoryFileSha256 { path: String, sha256: String },
    /// Require the frozen candidate SHA-256.
    CandidateSha256 { sha256: String },
    /// Require the packed runtime map SHA-256 produced by this promotion.
    RuntimeMapSha256 { sha256: String },
    /// Require the redistribution inventory SHA-256.
    InventorySha256 { sha256: String },
}

/// Evidence available while verifying conditions.
pub struct ConditionEvidence<'a> {
    pub repository_root: &'a Path,
    pub candidate_sha256: &'a str,
    pub runtime_map_sha256: Option<&'a str>,
    pub inventory_sha256: Option<&'a str>,
}

impl ReviewCondition {
    pub fn require_text_non_empty(&self) -> Result<()> {
        match self {
            Self::FreeText(text) => {
                if text.trim().is_empty() {
                    bail!("decision condition must not be empty");
                }
                Ok(())
            }
            Self::Structured(condition) => {
                if condition.id.trim().is_empty() {
                    bail!("structured condition id must not be empty");
                }
                if condition.description.trim().is_empty() {
                    bail!(
                        "structured condition {} description must not be empty",
                        condition.id
                    );
                }
                Ok(())
            }
        }
    }

    pub fn verify(&self, evidence: &ConditionEvidence<'_>) -> Result<()> {
        match self {
            Self::FreeText(text) => bail!(
                "approved_with_conditions free-text condition {text:?} is not machine-verifiable and blocks automatic promotion"
            ),
            Self::Structured(condition) => condition.verify(evidence),
        }
    }
}

impl StructuredCondition {
    fn verify(&self, evidence: &ConditionEvidence<'_>) -> Result<()> {
        match &self.verifier {
            ConditionVerifier::RepositoryFileSha256 { path, sha256 } => {
                require_digest("repository_file_sha256", sha256)?;
                let file = repository_path(
                    evidence.repository_root,
                    Path::new(path),
                    &format!("condition {} repository file", self.id),
                )?;
                let actual = checksum_io::sha256_file(&file)
                    .with_context(|| format!("condition {} read {}", self.id, file.display()))?;
                if actual != *sha256 {
                    bail!(
                        "condition {} file {} checksum mismatch: expected {sha256}, actual {actual}",
                        self.id,
                        path
                    );
                }
                if path == REVIEW_BUNDLE_PATH {
                    verify_review_bundle_evidence(evidence.repository_root, Path::new(path))
                        .with_context(|| {
                            format!(
                                "condition {} transitive Starlight review evidence failed verification",
                                self.id
                            )
                        })?;
                }
                Ok(())
            }
            ConditionVerifier::CandidateSha256 { sha256 } => {
                require_digest("candidate_sha256", sha256)?;
                if sha256 != evidence.candidate_sha256 {
                    bail!(
                        "condition {} candidate checksum mismatch: expected {}, condition pins {sha256}",
                        self.id,
                        evidence.candidate_sha256
                    );
                }
                Ok(())
            }
            ConditionVerifier::RuntimeMapSha256 { sha256 } => {
                require_digest("runtime_map_sha256", sha256)?;
                let actual = evidence.runtime_map_sha256.with_context(|| {
                    format!("condition {} requires packed runtime map evidence", self.id)
                })?;
                if sha256 != actual {
                    bail!(
                        "condition {} runtime map checksum mismatch: expected {actual}, condition pins {sha256}",
                        self.id
                    );
                }
                Ok(())
            }
            ConditionVerifier::InventorySha256 { sha256 } => {
                require_digest("inventory_sha256", sha256)?;
                let actual = evidence.inventory_sha256.with_context(|| {
                    format!("condition {} requires inventory checksum evidence", self.id)
                })?;
                if sha256 != actual {
                    bail!(
                        "condition {} inventory checksum mismatch: expected {actual}, condition pins {sha256}",
                        self.id
                    );
                }
                Ok(())
            }
        }
    }
}

fn require_digest(name: &str, value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{name} must be a 64-digit SHA-256");
    }
    Ok(())
}

pub fn verify_approved_with_conditions(
    conditions: &[ReviewCondition],
    evidence: &ConditionEvidence<'_>,
) -> Result<()> {
    if conditions.is_empty() {
        bail!("approved_with_conditions requires at least one recorded condition");
    }
    for condition in conditions {
        condition.require_text_non_empty()?;
        condition.verify(evidence)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn evidence<'a>(
        root: &'a Path,
        candidate: &'a str,
        runtime: Option<&'a str>,
        inventory: Option<&'a str>,
    ) -> ConditionEvidence<'a> {
        ConditionEvidence {
            repository_root: root,
            candidate_sha256: candidate,
            runtime_map_sha256: runtime,
            inventory_sha256: inventory,
        }
    }

    #[test]
    fn free_text_blocks_promotion() {
        let dir = TempDir::new().unwrap();
        let err = ReviewCondition::FreeText("Fix X before production".into())
            .verify(&evidence(dir.path(), &"a".repeat(64), None, None))
            .unwrap_err();
        assert!(err.to_string().contains("not machine-verifiable"));
    }

    #[test]
    fn repository_file_sha256_accepts_matching_bytes_and_rejects_tamper() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pin.txt");
        fs::write(&path, b"exact-bytes").unwrap();
        let sha = checksum_io::sha256_bytes(b"exact-bytes");
        let ok = ReviewCondition::Structured(StructuredCondition {
            id: "pin-file".into(),
            description: "pin.txt".into(),
            verifier: ConditionVerifier::RepositoryFileSha256 {
                path: "pin.txt".into(),
                sha256: sha.clone(),
            },
        });
        ok.verify(&evidence(dir.path(), &"a".repeat(64), None, None))
            .unwrap();
        fs::write(&path, b"tampered" ).unwrap();
        assert!(ok
            .verify(&evidence(dir.path(), &"a".repeat(64), None, None))
            .is_err());
    }

    fn write_transitive_fixture(
        root: &Path,
        nested_sha_override: Option<&str>,
        nested_bytes_override: Option<u64>,
    ) -> String {
        let nested_relative = Path::new("docs/nsb_components/starlight/validation/results/nested.txt");
        let nested_path = root.join(nested_relative);
        fs::create_dir_all(nested_path.parent().unwrap()).unwrap();
        fs::write(&nested_path, b"alpha").unwrap();
        let nested_sha = nested_sha_override
            .map(str::to_string)
            .unwrap_or_else(|| checksum_io::sha256_file(&nested_path).unwrap());
        let nested_bytes = nested_bytes_override.unwrap_or(5);

        let manifest_relative = Path::new(
            "docs/nsb_components/starlight/validation/results/validation-artifact-manifest-v1.toml",
        );
        let manifest_path = root.join(manifest_relative);
        fs::create_dir_all(manifest_path.parent().unwrap()).unwrap();
        fs::write(
            &manifest_path,
            format!(
                "schema_version = 1\ngenerated_at_unix_seconds = 1\n\n[[artifacts]]\nname = \"nested.txt\"\npath = \"{}\"\nsha256 = \"{nested_sha}\"\nbytes = {nested_bytes}\n",
                nested_relative.display()
            ),
        )
        .unwrap();
        let manifest_sha = checksum_io::sha256_file(&manifest_path).unwrap();

        let bundle_path = root.join(REVIEW_BUNDLE_PATH);
        fs::create_dir_all(bundle_path.parent().unwrap()).unwrap();
        fs::write(
            &bundle_path,
            format!(
                "schema_version = 1\nschema = \"nsb-starlight-review-bundle-v1\"\n\n[[artifacts]]\nid = \"validation_artifact_manifest\"\npath = \"{}\"\nsha256 = \"{manifest_sha}\"\n",
                manifest_relative.display()
            ),
        )
        .unwrap();
        checksum_io::sha256_file(&bundle_path).unwrap()
    }

    #[test]
    fn transitive_review_bundle_accepts_complete_evidence() {
        let dir = TempDir::new().unwrap();
        write_transitive_fixture(dir.path(), None, None);
        verify_review_bundle_evidence(dir.path(), Path::new(REVIEW_BUNDLE_PATH)).unwrap();
    }

    #[test]
    fn transitive_review_bundle_rejects_nested_tamper() {
        let dir = TempDir::new().unwrap();
        write_transitive_fixture(dir.path(), None, None);
        fs::write(
            dir.path()
                .join("docs/nsb_components/starlight/validation/results/nested.txt"),
            b"bravo",
        )
        .unwrap();
        let error = verify_review_bundle_evidence(dir.path(), Path::new(REVIEW_BUNDLE_PATH))
            .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"), "{error}");
    }

    #[test]
    fn transitive_review_bundle_rejects_missing_nested_artifact() {
        let dir = TempDir::new().unwrap();
        write_transitive_fixture(dir.path(), None, None);
        fs::remove_file(
            dir.path()
                .join("docs/nsb_components/starlight/validation/results/nested.txt"),
        )
        .unwrap();
        assert!(verify_review_bundle_evidence(dir.path(), Path::new(REVIEW_BUNDLE_PATH)).is_err());
    }

    #[test]
    fn transitive_review_bundle_rejects_wrong_nested_checksum() {
        let dir = TempDir::new().unwrap();
        write_transitive_fixture(dir.path(), Some(&"b".repeat(64)), None);
        let error = verify_review_bundle_evidence(dir.path(), Path::new(REVIEW_BUNDLE_PATH))
            .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"), "{error}");
    }

    #[test]
    fn transitive_review_bundle_rejects_wrong_nested_byte_count() {
        let dir = TempDir::new().unwrap();
        write_transitive_fixture(dir.path(), None, Some(6));
        let error = verify_review_bundle_evidence(dir.path(), Path::new(REVIEW_BUNDLE_PATH))
            .unwrap_err();
        assert!(error.to_string().contains("byte-count mismatch"), "{error}");
    }

    #[test]
    fn canonical_review_bundle_condition_checks_nested_evidence() {
        let dir = TempDir::new().unwrap();
        let bundle_sha = write_transitive_fixture(dir.path(), None, None);
        let condition = ReviewCondition::Structured(StructuredCondition {
            id: "review-bundle-v1".into(),
            description: "exact human review evidence".into(),
            verifier: ConditionVerifier::RepositoryFileSha256 {
                path: REVIEW_BUNDLE_PATH.into(),
                sha256: bundle_sha,
            },
        });
        condition
            .verify(&evidence(dir.path(), &"a".repeat(64), None, None))
            .unwrap();
        fs::write(
            dir.path()
                .join("docs/nsb_components/starlight/validation/results/nested.txt"),
            b"bravo",
        )
        .unwrap();
        assert!(condition
            .verify(&evidence(dir.path(), &"a".repeat(64), None, None))
            .is_err());
    }

    #[test]
    fn missing_runtime_evidence_fails_closed() {
        let dir = TempDir::new().unwrap();
        let err = ReviewCondition::Structured(StructuredCondition {
            id: "runtime".into(),
            description: "packed map".into(),
            verifier: ConditionVerifier::RuntimeMapSha256 {
                sha256: "a".repeat(64),
            },
        })
        .verify(&evidence(dir.path(), &"b".repeat(64), None, None))
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("requires packed runtime map evidence"));
    }

    #[test]
    fn unsupported_verifier_type_fails_to_parse() {
        let raw = r#"{"id":"x","description":"y","verifier":{"type":"please_trust_us"}}"#;
        assert!(serde_json::from_str::<StructuredCondition>(raw).is_err());
    }
}
