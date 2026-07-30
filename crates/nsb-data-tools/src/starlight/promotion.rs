//! Fail-closed release-candidate promotion mechanism (#89).
//!
//! This module parses the immutable release-candidate manifest
//! (`nsb-starlight-release-candidate-v1`, see
//! `docs/nsb_components/starlight/release-candidate/`) and the paired human
//! scientific and redistribution decision records, verifies the exact
//! candidate map bytes against every pinned checksum, and — only if every
//! check passes — renders a *draft* production `manifest.toml` fragment. It
//! never mutates the candidate map bytes or the repository's
//! `crates/nsb/data/manifest.toml`, and it never grants approval itself:
//! only recorded `approved` / `approved_with_conditions` decisions with a
//! named reviewer, an RFC 3339 review timestamp, and a checksum pin matching
//! the exact candidate can satisfy [`run_promotion`]. The human decisions
//! are owned exclusively by issue #47; this module cannot manufacture
//! consent, only validate the shape and internal consistency of decisions a
//! human already recorded.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Schema identifier for the release-candidate manifest.
pub const RELEASE_CANDIDATE_SCHEMA: &str = "nsb-starlight-release-candidate-v1";
const RELEASE_CANDIDATE_SCHEMA_VERSION: u32 = 1;
const DECISION_SCHEMA_VERSION: u32 = 1;

/// Schema identifier a promoted map must use (matches `crates/nsb/build.rs`).
pub const PRODUCTION_MAP_SCHEMA: &str = "nsb-healpix-starlight-v2";
/// Schema identifier a promoted runtime sidecar manifest must use.
pub const PRODUCTION_MANIFEST_SCHEMA: &str = "nsb-starlight-runtime-manifest-v1";

/// Fail-closed lock on whether the pinned candidate checksum is admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    /// The checksum below names an admissible, checksum-locked release.
    Pinned,
    /// The checksum below is retained for provenance only; a regenerated
    /// candidate is expected (see #94/#95) before promotion can proceed.
    AwaitingRegeneration,
}

impl CandidateStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::AwaitingRegeneration => "awaiting_regeneration",
        }
    }
}

/// Technical (non-human) validation state of the candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    TechnicalPass,
    PendingRegeneration,
}

impl ValidationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::TechnicalPass => "technical_pass",
            Self::PendingRegeneration => "pending_regeneration",
        }
    }
}

/// Shared human-decision vocabulary for both the scientific and
/// redistribution review tracks, matching issue #47.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Approved,
    ApprovedWithConditions,
    Rejected,
}

impl ReviewStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::ApprovedWithConditions => "approved_with_conditions",
            Self::Rejected => "rejected",
        }
    }
}

/// `[candidate]` table of the release-candidate manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSection {
    pub status: CandidateStatus,
    pub candidate_sha256: String,
    pub map_path: String,
    pub map_schema: String,
    pub band: String,
    pub units: String,
    pub nside: u32,
    pub ordering: String,
    pub gaia_release: String,
    #[serde(default)]
    pub model_versions: BTreeMap<String, String>,
}

/// `[gates]` table of the release-candidate manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatesSection {
    pub validation_status: ValidationStatus,
    pub scientific_review_status: ReviewStatus,
    pub redistribution_review_status: ReviewStatus,
    /// Authoritative kill switch. Never set to `true` by this module.
    pub promotion_eligible: bool,
}

/// Complete release-candidate manifest (`nsb-starlight-release-candidate-v1`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidateManifest {
    pub schema_version: u32,
    pub schema: String,
    pub candidate: CandidateSection,
    pub gates: GatesSection,
    pub notes: String,
}

impl ReleaseCandidateManifest {
    /// Load, parse, and structurally validate a release-candidate manifest.
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("read release-candidate manifest {}", path.display()))?;
        let manifest: Self = toml::from_str(&raw)
            .with_context(|| format!("parse release-candidate manifest {}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != RELEASE_CANDIDATE_SCHEMA_VERSION {
            bail!(
                "unsupported release-candidate schema_version {}; expected {RELEASE_CANDIDATE_SCHEMA_VERSION}",
                self.schema_version
            );
        }
        if self.schema != RELEASE_CANDIDATE_SCHEMA {
            bail!(
                "unsupported release-candidate schema {:?}; expected {RELEASE_CANDIDATE_SCHEMA:?}",
                self.schema
            );
        }
        require_text("candidate.map_path", &self.candidate.map_path)?;
        require_text("candidate.map_schema", &self.candidate.map_schema)?;
        require_text("candidate.band", &self.candidate.band)?;
        require_text("candidate.units", &self.candidate.units)?;
        require_text("candidate.ordering", &self.candidate.ordering)?;
        require_text("candidate.gaia_release", &self.candidate.gaia_release)?;
        require_sha256(
            "candidate.candidate_sha256",
            &self.candidate.candidate_sha256,
        )?;
        if self.candidate.nside == 0 || !self.candidate.nside.is_power_of_two() {
            bail!(
                "candidate.nside must be a positive power of two, got {}",
                self.candidate.nside
            );
        }
        require_text("notes", &self.notes)?;
        Ok(())
    }
}

/// One human decision record (`scientific-review-decision-v1` or
/// `redistribution-review-decision-v1`), keyed to a candidate checksum.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDecision {
    pub schema_version: u32,
    pub decision: ReviewStatus,
    #[serde(default)]
    pub reviewer_name: Option<String>,
    #[serde(default)]
    pub reviewer_role: Option<String>,
    #[serde(default)]
    pub reviewed_at_utc: Option<String>,
    pub candidate_sha256: String,
    #[serde(default)]
    pub conditions: Vec<String>,
    pub notes: String,
}

/// Which review track a decision belongs to, for error labelling only.
#[derive(Debug, Clone, Copy)]
enum DecisionKind {
    Scientific,
    Redistribution,
}

impl DecisionKind {
    fn label(self) -> &'static str {
        match self {
            Self::Scientific => "scientific",
            Self::Redistribution => "redistribution",
        }
    }
}

impl ReviewDecision {
    fn validate(&self, kind: DecisionKind) -> Result<()> {
        if self.schema_version != DECISION_SCHEMA_VERSION {
            bail!(
                "unsupported {} decision schema_version {}; expected {DECISION_SCHEMA_VERSION}",
                kind.label(),
                self.schema_version
            );
        }
        require_sha256(
            &format!("{} decision candidate_sha256", kind.label()),
            &self.candidate_sha256,
        )?;
        require_text(&format!("{} decision notes", kind.label()), &self.notes)?;
        for condition in &self.conditions {
            require_text(&format!("{} decision condition", kind.label()), condition)?;
        }
        Ok(())
    }

    /// Fail-closed promotion gate for one decision.
    ///
    /// Rejects a pending or rejected decision, a missing/placeholder
    /// reviewer, a malformed review timestamp, `approved_with_conditions`
    /// with no recorded condition, and a decision that pins a candidate
    /// checksum other than `expected_candidate_sha256`.
    fn require_approved(&self, kind: DecisionKind, expected_candidate_sha256: &str) -> Result<()> {
        match self.decision {
            ReviewStatus::Pending => bail!(
                "{} review decision is pending; the human decision in #47 has not been recorded",
                kind.label()
            ),
            ReviewStatus::Rejected => {
                bail!("{} review decision is rejected", kind.label())
            }
            ReviewStatus::Approved | ReviewStatus::ApprovedWithConditions => {}
        }
        if self.decision == ReviewStatus::ApprovedWithConditions && self.conditions.is_empty() {
            bail!(
                "{} decision approved_with_conditions requires at least one recorded condition",
                kind.label()
            );
        }
        let reviewer_name = self.reviewer_name.as_deref().unwrap_or_default();
        require_text(
            &format!("{} decision reviewer_name", kind.label()),
            reviewer_name,
        )
        .with_context(|| {
            format!(
                "{} decision is missing an authorized reviewer",
                kind.label()
            )
        })?;
        let reviewer_role = self.reviewer_role.as_deref().unwrap_or_default();
        require_text(
            &format!("{} decision reviewer_role", kind.label()),
            reviewer_role,
        )
        .with_context(|| {
            format!(
                "{} decision is missing an authorized reviewer",
                kind.label()
            )
        })?;
        let reviewed_at = self.reviewed_at_utc.as_deref().unwrap_or_default();
        require_rfc3339_utc(
            &format!("{} decision reviewed_at_utc", kind.label()),
            reviewed_at,
        )
        .with_context(|| format!("{} decision has no valid review timestamp", kind.label()))?;
        if self.candidate_sha256 != expected_candidate_sha256 {
            bail!(
                "{} decision pins candidate {}, but the release candidate is {}",
                kind.label(),
                self.candidate_sha256,
                expected_candidate_sha256
            );
        }
        Ok(())
    }
}

/// Inputs for [`run_promotion`].
#[derive(Debug, Clone)]
pub struct PromotionInputs {
    pub release_candidate: PathBuf,
    pub scientific_decision: PathBuf,
    pub redistribution_decision: PathBuf,
    pub repository_root: PathBuf,
    /// Optional path to write the draft production manifest fragment.
    pub output: Option<PathBuf>,
}

/// Result of a successful [`run_promotion`] call.
#[derive(Debug, Clone)]
pub struct PromotionOutcome {
    /// Draft production `manifest.toml` fragment; never applied automatically.
    pub draft_manifest_fragment: String,
    /// Where the draft was written, if `--output` was supplied.
    pub written_to: Option<PathBuf>,
}

/// Verify a release candidate and its human decisions, then draft (but never
/// apply) the production manifest change.
///
/// Fails closed on: an unpinned/awaiting-regeneration candidate, a map
/// checksum mismatch or byte-level tamper, a registry entry that diverges
/// from the release candidate (tamper), a non-`technical_pass` validation
/// status, a pending/rejected decision, a missing reviewer identity, an
/// invalid review timestamp, a decision that pins the wrong candidate
/// checksum, a gate/decision mismatch, or `gates.promotion_eligible == false`.
/// No map bytes or repository manifest are ever written; only the draft
/// output (if requested) is written, and only after every check passes.
pub fn run_promotion(inputs: &PromotionInputs) -> Result<PromotionOutcome> {
    let candidate = ReleaseCandidateManifest::load(&inputs.release_candidate)?;

    let map_path = inputs.repository_root.join(&candidate.candidate.map_path);
    if !map_path.is_file() {
        bail!(
            "candidate map {} does not exist under repository root {}",
            candidate.candidate.map_path,
            inputs.repository_root.display()
        );
    }
    let actual_map_sha256 = checksum_io::sha256_file(&map_path)
        .with_context(|| format!("checksum candidate map {}", map_path.display()))?;

    if candidate.candidate.status != CandidateStatus::Pinned {
        bail!(
            "release candidate status is {:?} ({}), not pinned; promotion is blocked pending regeneration (see #94/#95)",
            candidate.candidate.status,
            candidate.candidate.status.as_str()
        );
    }
    if actual_map_sha256 != candidate.candidate.candidate_sha256 {
        bail!(
            "candidate map checksum mismatch or tamper detected: release candidate pins {}, actual file is {}",
            candidate.candidate.candidate_sha256,
            actual_map_sha256
        );
    }

    verify_registry_matches(&inputs.repository_root, &candidate.candidate)?;

    if candidate.gates.validation_status != ValidationStatus::TechnicalPass {
        bail!(
            "release candidate validation_status is {} , not technical_pass",
            candidate.gates.validation_status.as_str()
        );
    }

    let scientific = load_decision(&inputs.scientific_decision, DecisionKind::Scientific)?;
    scientific.require_approved(
        DecisionKind::Scientific,
        &candidate.candidate.candidate_sha256,
    )?;
    let redistribution = load_decision(
        &inputs.redistribution_decision,
        DecisionKind::Redistribution,
    )?;
    redistribution.require_approved(
        DecisionKind::Redistribution,
        &candidate.candidate.candidate_sha256,
    )?;

    require_gate_matches_decision(
        DecisionKind::Scientific,
        candidate.gates.scientific_review_status,
        scientific.decision,
    )?;
    require_gate_matches_decision(
        DecisionKind::Redistribution,
        candidate.gates.redistribution_review_status,
        redistribution.decision,
    )?;

    if !candidate.gates.promotion_eligible {
        bail!(
            "release candidate gates.promotion_eligible is false; promotion is blocked until a maintainer records #47 approval"
        );
    }

    let draft = render_production_manifest_draft(&candidate.candidate, &actual_map_sha256);
    let written_to = match &inputs.output {
        Some(output) => {
            if let Some(parent) = output.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("create draft output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(output, &draft)
                .with_context(|| format!("write draft manifest {}", output.display()))?;
            Some(output.clone())
        }
        None => None,
    };

    Ok(PromotionOutcome {
        draft_manifest_fragment: draft,
        written_to,
    })
}

fn require_gate_matches_decision(
    kind: DecisionKind,
    gate: ReviewStatus,
    decision: ReviewStatus,
) -> Result<()> {
    if gate != decision {
        bail!(
            "{} gate status is {}, but the recorded decision is {}; update the release candidate before promoting",
            kind.label(),
            gate.as_str(),
            decision.as_str()
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct AssetRegistry {
    assets: Vec<RegistryAsset>,
}

#[derive(Debug, Deserialize)]
struct RegistryAsset {
    path: String,
    schema: String,
    sha256: String,
}

/// Cross-check the repository's asset registry against the release
/// candidate's pinned schema and checksum, to catch registry/candidate
/// drift or tamper that a checksum-only check would miss.
fn verify_registry_matches(repository_root: &Path, candidate: &CandidateSection) -> Result<()> {
    let map_path = repository_root.join(&candidate.map_path);
    let manifest_path = map_path
        .parent()
        .context("candidate map_path has no parent directory")?
        .join("manifest.toml");
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read asset registry {}", manifest_path.display()))?;
    let registry: AssetRegistry = toml::from_str(&raw)
        .with_context(|| format!("parse asset registry {}", manifest_path.display()))?;
    let file_name = Path::new(&candidate.map_path)
        .file_name()
        .and_then(|name| name.to_str())
        .context("candidate map_path has no file name")?;
    let asset = registry
        .assets
        .iter()
        .find(|asset| asset.path == file_name)
        .with_context(|| format!("asset registry has no entry for {file_name}"))?;
    if asset.schema != candidate.map_schema {
        bail!(
            "tamper detected: asset registry schema for {file_name} is {:?}, release candidate pins {:?}",
            asset.schema,
            candidate.map_schema
        );
    }
    if asset.sha256 != candidate.candidate_sha256 {
        bail!(
            "tamper detected: asset registry checksum for {file_name} is {}, release candidate pins {}",
            asset.sha256,
            candidate.candidate_sha256
        );
    }
    Ok(())
}

fn load_decision(path: &Path, kind: DecisionKind) -> Result<ReviewDecision> {
    let raw = fs::read(path)
        .with_context(|| format!("read {} review decision {}", kind.label(), path.display()))?;
    let decision: ReviewDecision = serde_json::from_slice(&raw)
        .with_context(|| format!("parse {} review decision {}", kind.label(), path.display()))?;
    decision.validate(kind)?;
    Ok(decision)
}

fn render_production_manifest_draft(candidate: &CandidateSection, map_sha256: &str) -> String {
    let stem = Path::new(&candidate.map_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("starlight");
    let release_path = format!("{stem}.release.csv");
    let sidecar_path = format!("{stem}.manifest.toml");

    let mut out = String::new();
    out.push_str("# DRAFT ONLY -- generated by `nsb-data dataset starlight promote`.\n");
    out.push_str("# This fragment previews the production crates/nsb/data/manifest.toml\n");
    out.push_str("# entries that promotion would add. It has NOT been applied to any\n");
    out.push_str("# repository file. A maintainer applies it by hand as part of the #47\n");
    out.push_str("# promotion pull request, after copying the byte-identical candidate map\n");
    out.push_str("# to its production filename; the map bytes themselves are never rewritten.\n\n");
    writeln!(out, "[[assets]]").unwrap();
    writeln!(out, "path = {release_path:?}").unwrap();
    writeln!(out, "schema = {PRODUCTION_MAP_SCHEMA:?}").unwrap();
    writeln!(out, "sha256 = {map_sha256:?}").unwrap();
    writeln!(out, "gaia_release = {:?}", candidate.gaia_release).unwrap();
    writeln!(out, "band = {:?}", candidate.band).unwrap();
    writeln!(out, "units = {:?}", candidate.units).unwrap();
    writeln!(out, "calibration_status = \"production\"").unwrap();
    writeln!(out, "runtime_embedded = true").unwrap();
    out.push('\n');
    writeln!(out, "[[assets]]").unwrap();
    writeln!(out, "path = {sidecar_path:?}").unwrap();
    writeln!(out, "schema = {PRODUCTION_MANIFEST_SCHEMA:?}").unwrap();
    writeln!(out, "calibration_status = \"production\"").unwrap();
    writeln!(out, "runtime_embedded = true").unwrap();
    out
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || [
            "placeholder",
            "todo",
            "tbd",
            "unknown",
            "unspecified",
            "none",
            "not recorded",
            "pending",
        ]
        .iter()
        .any(|marker| normalized == *marker || normalized.contains(&format!("<{marker}>")))
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

fn require_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be a 64-character lowercase hexadecimal SHA-256, got {value:?}");
    }
    Ok(())
}

fn require_rfc3339_utc(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .with_context(|| format!("{label} must be an RFC 3339 timestamp, got {value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_MAP_BYTES: &[u8] =
        b"# synthetic test-only starlight release candidate fixture, not real science data\npixel,flux\n0,1.0\n1,2.0\n";
    // SHA-256 of `SYNTHETIC_MAP_BYTES`, computed once and pinned here so the
    // fixtures below exercise the real checksum-verification code path.
    const SYNTHETIC_CANDIDATE_SHA256: &str =
        "5ac4d2e001d3c7c1c74285d635744a9a4fca1fc8575d44ccc228bef46eeea176";

    struct SyntheticRepo {
        _dir: tempfile::TempDir,
        root: PathBuf,
        release_candidate: PathBuf,
        scientific_decision: PathBuf,
        redistribution_decision: PathBuf,
    }

    fn release_candidate_toml(
        status: &str,
        validation_status: &str,
        promotion_eligible: bool,
    ) -> String {
        format!(
            r#"schema_version = 1
schema = "nsb-starlight-release-candidate-v1"
notes = "Synthetic test-only fixture for nsb-data-tools promotion unit tests; not real scientific evidence."

[candidate]
status = "{status}"
candidate_sha256 = "{SYNTHETIC_CANDIDATE_SHA256}"
map_path = "crates/nsb/data/starlight_nside128.csv"
map_schema = "nsb-healpix-starlight-candidate-v5"
band = "synthetic test-only combined band"
units = "ph_m-2_s-1"
nside = 128
ordering = "nested"
gaia_release = "Gaia DR3 (synthetic test fixture)"

[candidate.model_versions]
uv_correction_current = "synthetic-test-only-v1"

[gates]
validation_status = "{validation_status}"
scientific_review_status = "approved"
redistribution_review_status = "approved"
promotion_eligible = {promotion_eligible}
"#
        )
    }

    fn decision_json(decision: &str, extra_conditions: &str) -> String {
        format!(
            r#"{{
  "schema_version": 1,
  "decision": "{decision}",
  "reviewer_name": "Synthetic Test Reviewer",
  "reviewer_role": "synthetic-test-only-role",
  "reviewed_at_utc": "2026-07-30T00:00:00Z",
  "candidate_sha256": "{SYNTHETIC_CANDIDATE_SHA256}",
  "conditions": [{extra_conditions}],
  "notes": "Synthetic test-only decision fixture; not a real human review."
}}"#
        )
    }

    fn write_synthetic_repo(
        release_candidate: &str,
        scientific_decision: &str,
        redistribution_decision: &str,
    ) -> SyntheticRepo {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let data_dir = root.join("crates/nsb/data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("starlight_nside128.csv"), SYNTHETIC_MAP_BYTES).unwrap();
        fs::write(
            data_dir.join("manifest.toml"),
            format!(
                "schema_version = 1\n\n[[assets]]\npath = \"starlight_nside128.csv\"\nschema = \"nsb-healpix-starlight-candidate-v5\"\nsha256 = \"{SYNTHETIC_CANDIDATE_SHA256}\"\ncalibration_status = \"candidate\"\nruntime_embedded = false\n"
            ),
        )
        .unwrap();

        let release_candidate_path = root.join("release-candidate-v1.toml");
        fs::write(&release_candidate_path, release_candidate).unwrap();
        let scientific_decision_path = root.join("scientific-review-decision-v1.json");
        fs::write(&scientific_decision_path, scientific_decision).unwrap();
        let redistribution_decision_path = root.join("redistribution-review-decision-v1.json");
        fs::write(&redistribution_decision_path, redistribution_decision).unwrap();

        SyntheticRepo {
            _dir: dir,
            root,
            release_candidate: release_candidate_path,
            scientific_decision: scientific_decision_path,
            redistribution_decision: redistribution_decision_path,
        }
    }

    fn valid_synthetic_repo() -> SyntheticRepo {
        write_synthetic_repo(
            &release_candidate_toml("pinned", "technical_pass", true),
            &decision_json("approved", "\"synthetic-test-only-condition\""),
            &decision_json("approved", "\"synthetic-test-only-condition\""),
        )
    }

    fn inputs(repo: &SyntheticRepo, output: Option<PathBuf>) -> PromotionInputs {
        PromotionInputs {
            release_candidate: repo.release_candidate.clone(),
            scientific_decision: repo.scientific_decision.clone(),
            redistribution_decision: repo.redistribution_decision.clone(),
            repository_root: repo.root.clone(),
            output,
        }
    }

    #[test]
    fn awaiting_regeneration_status_fails_closed() {
        let repo = write_synthetic_repo(
            &release_candidate_toml("awaiting_regeneration", "pending_regeneration", false),
            &decision_json("approved", "\"c\""),
            &decision_json("approved", "\"c\""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("not pinned"));
    }

    #[test]
    fn wrong_checksum_or_tamper_on_map_bytes_fails_closed() {
        let repo = valid_synthetic_repo();
        fs::write(
            repo.root.join("crates/nsb/data/starlight_nside128.csv"),
            b"tampered synthetic bytes\n",
        )
        .unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn registry_drift_is_treated_as_tamper() {
        let repo = valid_synthetic_repo();
        fs::write(
            repo.root.join("crates/nsb/data/manifest.toml"),
            "schema_version = 1\n\n[[assets]]\npath = \"starlight_nside128.csv\"\nschema = \"nsb-healpix-starlight-candidate-v5\"\nsha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\ncalibration_status = \"candidate\"\nruntime_embedded = false\n",
        )
        .unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("tamper detected"));
    }

    #[test]
    fn pending_scientific_decision_fails_closed() {
        let repo = write_synthetic_repo(
            &release_candidate_toml("pinned", "technical_pass", true),
            &decision_json("pending", ""),
            &decision_json("approved", "\"c\""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("scientific review decision is pending"));
    }

    #[test]
    fn rejected_redistribution_decision_fails_closed() {
        let mut candidate = release_candidate_toml("pinned", "technical_pass", true);
        candidate = candidate.replace(
            "redistribution_review_status = \"approved\"",
            "redistribution_review_status = \"rejected\"",
        );
        let repo = write_synthetic_repo(
            &candidate,
            &decision_json("approved", "\"c\""),
            &decision_json("rejected", ""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("redistribution review decision is rejected"));
    }

    #[test]
    fn missing_reviewer_identity_fails_closed() {
        let repo = valid_synthetic_repo();
        let tampered = fs::read_to_string(&repo.scientific_decision)
            .unwrap()
            .replace(
                "\"reviewer_name\": \"Synthetic Test Reviewer\"",
                "\"reviewer_name\": null",
            );
        fs::write(&repo.scientific_decision, tampered).unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("missing an authorized reviewer"));
    }

    #[test]
    fn decision_pinning_wrong_candidate_checksum_fails_closed() {
        let repo = valid_synthetic_repo();
        let other_valid_sha256: String = "c".repeat(64);
        assert_ne!(other_valid_sha256, SYNTHETIC_CANDIDATE_SHA256);
        let tampered = fs::read_to_string(&repo.scientific_decision)
            .unwrap()
            .replace(SYNTHETIC_CANDIDATE_SHA256, &other_valid_sha256);
        fs::write(&repo.scientific_decision, tampered).unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("pins candidate"));
    }

    #[test]
    fn gate_decision_mismatch_fails_closed() {
        let mut candidate = release_candidate_toml("pinned", "technical_pass", true);
        candidate = candidate.replace(
            "scientific_review_status = \"approved\"",
            "scientific_review_status = \"pending\"",
        );
        let repo = write_synthetic_repo(
            &candidate,
            &decision_json("approved", "\"c\""),
            &decision_json("approved", "\"c\""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("but the recorded decision is"));
    }

    #[test]
    fn promotion_eligible_false_fails_closed_even_when_everything_else_passes() {
        let repo = write_synthetic_repo(
            &release_candidate_toml("pinned", "technical_pass", false),
            &decision_json("approved", "\"c\""),
            &decision_json("approved", "\"c\""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("promotion_eligible is false"));
    }

    #[test]
    fn approved_with_conditions_requires_at_least_one_condition() {
        let repo = write_synthetic_repo(
            &release_candidate_toml("pinned", "technical_pass", true),
            &decision_json("approved_with_conditions", ""),
            &decision_json("approved", "\"c\""),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("requires at least one recorded condition"));
    }

    #[test]
    fn fully_approved_synthetic_candidate_drafts_without_mutating_sources() {
        let repo = valid_synthetic_repo();
        let map_path = repo.root.join("crates/nsb/data/starlight_nside128.csv");
        let manifest_path = repo.root.join("crates/nsb/data/manifest.toml");
        let map_before = fs::read(&map_path).unwrap();
        let manifest_before = fs::read(&manifest_path).unwrap();

        let output = repo.root.join("draft/production-manifest-draft.toml");
        let outcome = run_promotion(&inputs(&repo, Some(output.clone()))).unwrap();

        assert!(outcome
            .draft_manifest_fragment
            .contains(PRODUCTION_MAP_SCHEMA));
        assert!(outcome
            .draft_manifest_fragment
            .contains(PRODUCTION_MANIFEST_SCHEMA));
        assert!(outcome
            .draft_manifest_fragment
            .contains("starlight_nside128.release.csv"));
        assert_eq!(outcome.written_to, Some(output.clone()));
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            outcome.draft_manifest_fragment
        );

        assert_eq!(
            fs::read(&map_path).unwrap(),
            map_before,
            "map bytes must never be mutated"
        );
        assert_eq!(
            fs::read(&manifest_path).unwrap(),
            manifest_before,
            "repository manifest.toml must never be mutated"
        );
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let repo = valid_synthetic_repo();
        let tampered = fs::read_to_string(&repo.release_candidate)
            .unwrap()
            .replace(
                "schema = \"nsb-starlight-release-candidate-v1\"",
                "schema = \"nsb-starlight-release-candidate-v2\"",
            );
        fs::write(&repo.release_candidate, tampered).unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported release-candidate schema"));
    }

    #[test]
    fn documented_pending_release_candidate_never_promotes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let release_candidate =
            root.join("docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml");
        let scientific_decision = root.join(
            "docs/nsb_components/starlight/release-candidate/scientific-review-decision-v1.json",
        );
        let redistribution_decision = root.join(
            "docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json",
        );
        let manifest = ReleaseCandidateManifest::load(&release_candidate).unwrap();
        assert!(!manifest.gates.promotion_eligible);
        assert_eq!(
            manifest.candidate.status,
            CandidateStatus::AwaitingRegeneration
        );

        let error = run_promotion(&PromotionInputs {
            release_candidate,
            scientific_decision,
            redistribution_decision,
            repository_root: root,
            output: None,
        })
        .unwrap_err();
        assert!(error.to_string().contains("not pinned"));
    }
}
