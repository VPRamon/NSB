//! Machine-verifiable review conditions for `approved_with_conditions`.
//!
//! Free-form strings are accepted by the schema so existing templates parse,
//! but they never satisfy automatic promotion.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
                let file = evidence.repository_root.join(path);
                let actual = checksum_io::sha256_file(&file)
                    .with_context(|| format!("condition {} read {}", self.id, file.display()))?;
                if actual != *sha256 {
                    bail!(
                        "condition {} file {} checksum mismatch: expected {sha256}, actual {actual}",
                        self.id,
                        path
                    );
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
        fs::write(&path, b"tampered").unwrap();
        assert!(ok
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
