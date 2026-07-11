//! Fail-closed approval artefacts for production starlight releases.
//!
//! Approval JSON never grants production access by its mere presence.  Every
//! supporting file is checksum-verified below an explicit artefact root, and
//! callers must provide the release/map/manifest bindings that apply at the
//! point where an approval is consumed.

use crate::checksum_io::sha256_file;
use anyhow::{bail, Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub const APPROVAL_SCHEMA_VERSION: u32 = 1;
pub const STARLIGHT_PRODUCTION_BAND_NM: [f64; 2] = [300.0, 650.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalArtifactType {
    MissingFlux,
    IndependentValidation,
    Redistribution,
    NsideReview,
}

impl ApprovalArtifactType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingFlux => "missing_flux",
            Self::IndependentValidation => "independent_validation",
            Self::Redistribution => "redistribution",
            Self::NsideReview => "nside_review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerKind {
    Human,
    Automated,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalFileDigest {
    /// UTF-8 path relative to the approval artefact root.
    pub path: String,
    /// SHA-256, with the mandatory `sha256:` algorithm prefix.
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StarlightApproval {
    pub schema_version: u32,
    pub artifact_type: ApprovalArtifactType,
    pub decision: ApprovalDecision,
    pub production_use: bool,
    pub reviewer_kind: ReviewerKind,
    pub reviewer_name: String,
    /// RFC3339 timestamp carrying an explicit UTC offset.
    pub date: String,
    pub release_id: String,
    pub band_nm: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nside: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "manifest_sha")]
    pub manifest_sha256: Option<String>,
    #[serde(alias = "input_file_digests")]
    pub input_files: Vec<ApprovalFileDigest>,
    #[serde(alias = "output_file_digests")]
    pub output_files: Vec<ApprovalFileDigest>,
    pub rationale: String,
    pub references: Vec<String>,
}

impl StarlightApproval {
    pub fn is_positive(&self) -> bool {
        self.decision == ApprovalDecision::Approved && self.production_use
    }
}

/// Compatibility requirements supplied by the consumer of an approval.
#[derive(Debug, Clone, Copy)]
pub struct ApprovalRequirements<'a> {
    pub artifact_type: ApprovalArtifactType,
    pub release_id: &'a str,
    pub nside: Option<u32>,
    pub map_sha256: Option<&'a str>,
    pub manifest_sha256: Option<&'a str>,
    pub require_positive: bool,
}

impl<'a> ApprovalRequirements<'a> {
    pub const fn production(artifact_type: ApprovalArtifactType, release_id: &'a str) -> Self {
        Self {
            artifact_type,
            release_id,
            nside: None,
            map_sha256: None,
            manifest_sha256: None,
            require_positive: true,
        }
    }
}

#[derive(Debug)]
pub struct VerifiedApproval {
    pub path: PathBuf,
    pub sha256: String,
    pub approval: StarlightApproval,
}

/// Load and validate an approval path contained by `artifact_root`.
pub fn load_and_validate_approval(
    artifact_root: &Path,
    approval_path: &Path,
    requirements: ApprovalRequirements<'_>,
) -> Result<VerifiedApproval> {
    let root = canonical_artifact_root(artifact_root)?;
    let path = resolve_approval_path(&root, approval_path)?;
    let raw = std::fs::read(&path)
        .with_context(|| format!("failed to read approval artefact {}", path.display()))?;
    let approval: StarlightApproval = serde_json::from_slice(&raw)
        .with_context(|| format!("failed to parse approval artefact {}", path.display()))?;
    validate_approval(&root, &approval, requirements)
        .with_context(|| format!("invalid approval artefact {}", path.display()))?;
    Ok(VerifiedApproval {
        sha256: format!("sha256:{}", sha256_file(&path)?),
        path,
        approval,
    })
}

pub fn validate_approval(
    artifact_root: &Path,
    approval: &StarlightApproval,
    requirements: ApprovalRequirements<'_>,
) -> Result<()> {
    let root = canonical_artifact_root(artifact_root)?;
    if approval.schema_version != APPROVAL_SCHEMA_VERSION {
        bail!(
            "unsupported approval schema_version {}; expected {}",
            approval.schema_version,
            APPROVAL_SCHEMA_VERSION
        );
    }
    if approval.artifact_type != requirements.artifact_type {
        bail!(
            "approval artifact_type={} is incompatible with required {}",
            approval.artifact_type.as_str(),
            requirements.artifact_type.as_str()
        );
    }
    if approval.band_nm != STARLIGHT_PRODUCTION_BAND_NM {
        bail!(
            "approval band_nm must be exactly [{}, {}]",
            STARLIGHT_PRODUCTION_BAND_NM[0],
            STARLIGHT_PRODUCTION_BAND_NM[1]
        );
    }
    validate_substantive_text("release_id", &approval.release_id, 1)?;
    validate_substantive_text("reviewer_name", &approval.reviewer_name, 3)?;
    validate_substantive_text("rationale", &approval.rationale, 20)?;
    if approval.release_id != requirements.release_id {
        bail!(
            "approval release_id {:?} does not match required release {:?}",
            approval.release_id,
            requirements.release_id
        );
    }
    DateTime::parse_from_rfc3339(approval.date.trim())
        .with_context(|| "approval date must be a valid RFC3339 timestamp")?;
    if approval.references.is_empty() {
        bail!("approval references must not be empty");
    }
    for reference in &approval.references {
        validate_substantive_text("reference", reference, 3)?;
    }

    if approval.production_use && approval.decision != ApprovalDecision::Approved {
        bail!("production_use=true requires decision=approved");
    }
    if requirements.require_positive && !approval.is_positive() {
        bail!("production approval requires decision=approved and production_use=true");
    }
    if approval.is_positive() && approval.reviewer_kind != ReviewerKind::Human {
        bail!("positive production approval requires reviewer_kind=human");
    }

    if approval.artifact_type == ApprovalArtifactType::NsideReview && approval.nside.is_none() {
        bail!("nside_review approval requires nside");
    }
    if let Some(required) = requirements.nside {
        if approval.nside != Some(required) {
            bail!(
                "approval nside {:?} does not match required nside {required}",
                approval.nside
            );
        }
    }

    validate_optional_binding("map_sha256", approval.map_sha256.as_deref())?;
    validate_optional_binding("manifest_sha256", approval.manifest_sha256.as_deref())?;
    if let Some(required) = requirements.map_sha256 {
        require_matching_binding("map_sha256", approval.map_sha256.as_deref(), required)?;
    }
    if let Some(required) = requirements.manifest_sha256 {
        require_matching_binding(
            "manifest_sha256",
            approval.manifest_sha256.as_deref(),
            required,
        )?;
    }

    if approval.input_files.is_empty() || approval.output_files.is_empty() {
        bail!("approval must checksum at least one input file and one output file");
    }
    let mut seen = BTreeSet::new();
    for (kind, files) in [
        ("input", approval.input_files.as_slice()),
        ("output", approval.output_files.as_slice()),
    ] {
        for digest in files {
            validate_digest_entry(&root, kind, digest, &mut seen)?;
        }
    }
    Ok(())
}

fn validate_digest_entry(
    root: &Path,
    kind: &str,
    digest: &ApprovalFileDigest,
    seen: &mut BTreeSet<String>,
) -> Result<()> {
    validate_substantive_text(&format!("{kind} file path"), &digest.path, 1)?;
    if !seen.insert(digest.path.clone()) {
        bail!("duplicate approval file path {:?}", digest.path);
    }
    let expected = normalize_sha256(&digest.sha256)
        .with_context(|| format!("invalid {kind} digest for {:?}", digest.path))?;
    let path = resolve_contained_path(root, Path::new(&digest.path), &format!("{kind} file"))?;
    if !path.is_file() {
        bail!(
            "approval {kind} path is not a regular file: {}",
            path.display()
        );
    }
    let actual = sha256_file(&path)?;
    if actual != expected {
        bail!(
            "approval {kind} checksum mismatch for {:?}: expected sha256:{expected}, actual sha256:{actual}",
            digest.path
        );
    }
    Ok(())
}

fn canonical_artifact_root(root: &Path) -> Result<PathBuf> {
    let canonical = root.canonicalize().with_context(|| {
        format!(
            "failed to resolve approval artifact root {}",
            root.display()
        )
    })?;
    if !canonical.is_dir() {
        bail!(
            "approval artifact root is not a directory: {}",
            root.display()
        );
    }
    Ok(canonical)
}

fn resolve_contained_path(root: &Path, path: &Path, label: &str) -> Result<PathBuf> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("{label} path must be relative and contained by the artifact root");
    }
    let candidate = root.join(path);
    let canonical = candidate
        .canonicalize()
        .with_context(|| format!("failed to resolve {label} {}", candidate.display()))?;
    if !canonical.starts_with(root) {
        bail!(
            "{label} escapes the approval artifact root: {}",
            path.display()
        );
    }
    Ok(canonical)
}

fn resolve_approval_path(root: &Path, path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return resolve_contained_path(root, path, "approval artefact");
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve approval artefact {}", path.display()))?;
    if !canonical.starts_with(root) {
        bail!(
            "approval artefact escapes the approval artifact root: {}",
            path.display()
        );
    }
    Ok(canonical)
}

fn validate_optional_binding(name: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        normalize_sha256(value).with_context(|| format!("invalid approval {name}"))?;
    }
    Ok(())
}

fn require_matching_binding(name: &str, found: Option<&str>, required: &str) -> Result<()> {
    let required =
        normalize_sha256(required).with_context(|| format!("invalid required {name}"))?;
    let found = found.ok_or_else(|| anyhow::anyhow!("approval requires {name}"))?;
    let found = normalize_sha256(found).with_context(|| format!("invalid approval {name}"))?;
    if found != required {
        bail!("approval {name} does not match the required release binding");
    }
    Ok(())
}

/// Validate and normalize a SHA-256 value to lowercase hex without its prefix.
pub fn normalize_sha256(value: &str) -> Result<String> {
    let value = value.trim();
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| anyhow::anyhow!("SHA-256 must use the sha256: prefix"))?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("SHA-256 must contain exactly 64 hexadecimal digits");
    }
    Ok(hex.to_ascii_lowercase())
}

fn validate_substantive_text(name: &str, value: &str, minimum_len: usize) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.len() < minimum_len || trimmed.chars().any(char::is_control) {
        bail!("approval {name} is missing or invalid");
    }
    let lower = trimmed.to_ascii_lowercase();
    for blocked in [
        "todo",
        "tbd",
        "placeholder",
        "pending review",
        "unreviewed",
        "unknown reviewer",
        "changeme",
        "replace me",
        "required human reviewer",
        "required substantive",
        "required release id",
        "required path within",
        "example.com",
        "${",
    ] {
        if lower.contains(blocked) {
            bail!("approval {name} contains blocked placeholder marker {blocked:?}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use siderust::checksum::to_hex;

    #[test]
    fn validates_human_positive_approval_and_real_file_digests() -> Result<()> {
        let fixture = Fixture::new()?;
        validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )?;
        Ok(())
    }

    #[test]
    fn positive_approval_requires_human_reviewer() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.approval.reviewer_kind = ReviewerKind::Automated;
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("automated approval must fail closed");
        assert!(error.to_string().contains("reviewer_kind=human"));
        Ok(())
    }

    #[test]
    fn rejects_checksum_mismatch_and_missing_file() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.approval.output_files[0].sha256 = format!("sha256:{}", "0".repeat(64));
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("checksum mismatch must fail closed");
        assert!(error.to_string().contains("checksum mismatch"));

        fixture.approval.output_files[0].path = "missing.json".to_string();
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("missing file must fail closed");
        assert!(error.to_string().contains("failed to resolve output file"));
        Ok(())
    }

    #[test]
    fn rejects_traversal_placeholder_and_wrong_release() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.approval.input_files[0].path = "../outside".to_string();
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("traversal must fail closed");
        assert!(error.to_string().contains("must be relative"));

        let mut fixture = Fixture::new()?;
        fixture.approval.rationale = "TODO replace me with a real rationale".to_string();
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("placeholder must fail closed");
        assert!(error.to_string().contains("placeholder marker"));

        let mut fixture = Fixture::new()?;
        fixture.approval.release_id = "different-release".to_string();
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("release mismatch must fail closed");
        assert!(error
            .to_string()
            .contains("does not match required release"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_timestamp_band_and_sha_shape() -> Result<()> {
        let mut fixture = Fixture::new()?;
        fixture.approval.date = "2026-02-31T12:00:00Z".to_string();
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("invalid calendar date must fail closed");
        assert!(error.to_string().contains("RFC3339"));

        let mut fixture = Fixture::new()?;
        fixture.approval.band_nm = [336.0, 650.0];
        let error = validate_approval(
            fixture.dir.path(),
            &fixture.approval,
            fixture.requirements(),
        )
        .expect_err("wrong band must fail closed");
        assert!(error.to_string().contains("exactly [300, 650]"));

        assert!(normalize_sha256(&"a".repeat(64)).is_err());
        assert!(normalize_sha256("sha256:not-hex").is_err());
        Ok(())
    }

    struct Fixture {
        dir: tempfile::TempDir,
        approval: StarlightApproval,
        map_sha256: String,
    }

    impl Fixture {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            std::fs::write(dir.path().join("input.json"), b"input evidence\n")?;
            std::fs::write(dir.path().join("output.json"), b"output evidence\n")?;
            let input_sha = digest(b"input evidence\n");
            let output_sha = digest(b"output evidence\n");
            let map_sha256 = format!("sha256:{}", "a".repeat(64));
            let approval = StarlightApproval {
                schema_version: APPROVAL_SCHEMA_VERSION,
                artifact_type: ApprovalArtifactType::MissingFlux,
                decision: ApprovalDecision::Approved,
                production_use: true,
                reviewer_kind: ReviewerKind::Human,
                reviewer_name: "Synthetic fixture maintainer".to_string(),
                date: "2026-07-11T12:00:00+02:00".to_string(),
                release_id: "synthetic-release-v1".to_string(),
                band_nm: STARLIGHT_PRODUCTION_BAND_NM,
                nside: None,
                map_sha256: Some(map_sha256.clone()),
                manifest_sha256: None,
                input_files: vec![ApprovalFileDigest {
                    path: "input.json".to_string(),
                    sha256: input_sha,
                }],
                output_files: vec![ApprovalFileDigest {
                    path: "output.json".to_string(),
                    sha256: output_sha,
                }],
                rationale: "Synthetic fixture verifies the complete approval contract.".to_string(),
                references: vec!["fixture-reference-v1".to_string()],
            };
            Ok(Self {
                dir,
                approval,
                map_sha256,
            })
        }

        fn requirements(&self) -> ApprovalRequirements<'_> {
            ApprovalRequirements {
                artifact_type: ApprovalArtifactType::MissingFlux,
                release_id: "synthetic-release-v1",
                nside: None,
                map_sha256: Some(&self.map_sha256),
                manifest_sha256: None,
                require_positive: true,
            }
        }
    }

    fn digest(raw: &[u8]) -> String {
        let digest: [u8; 32] = Sha256::digest(raw).into();
        format!("sha256:{}", to_hex(&digest))
    }
}
