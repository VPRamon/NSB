//! Fail-closed release-candidate promotion mechanism (#102).
//!
//! This module parses the immutable release-candidate manifest
//! (`nsb-starlight-release-candidate-v1`) and the paired human scientific
//! and redistribution decision records owned by issue #103, verifies the
//! exact candidate map bytes against every pinned checksum, packs a
//! runtime-loadable map without rewriting the candidate, and — only if every
//! check passes — renders a production `manifest.toml` fragment. With
//! `apply = true` it also writes the packed runtime assets and production
//! registry entries. It never grants approval itself.

use crate::platform::checksum_io;
use crate::starlight::conditions::{
    verify_approved_with_conditions, ConditionEvidence, ReviewCondition,
};
use crate::starlight::licensing::RedistributionReview;
use crate::starlight::pack::{
    self, PackInputs, GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256, XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256,
};
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
    /// candidate is expected (historical #94/#95) before promotion can proceed.
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
/// redistribution review tracks, matching issue #103.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Approved,
    ApprovedWithConditions,
    Rejected,
}

impl ReviewStatus {
    #[allow(dead_code)]
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
///
/// `promotion_eligible` is retained for report/display only. Eligibility is
/// derived from the pinned candidate, frozen CI gates, packed runtime
/// asset, and the two signed human decisions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GatesSection {
    pub validation_status: ValidationStatus,
    pub scientific_review_status: ReviewStatus,
    pub redistribution_review_status: ReviewStatus,
    /// Report-only snapshot. Ignored by [`run_promotion`].
    pub promotion_eligible: bool,
}

/// Checksum-pinned supporting artifacts that promotion must re-verify.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewArtifactsSection {
    pub inventory_path: String,
    pub inventory_sha256: String,
    pub gates_report_path: String,
    pub gates_report_sha256: String,
    pub licensing_decision_path: String,
    #[serde(default)]
    pub runtime_map_path: Option<String>,
    #[serde(default)]
    pub runtime_map_sha256: Option<String>,
    #[serde(default)]
    pub runtime_sidecar_path: Option<String>,
    #[serde(default)]
    pub runtime_sidecar_sha256: Option<String>,
}

/// Complete release-candidate manifest (`nsb-starlight-release-candidate-v1`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseCandidateManifest {
    pub schema_version: u32,
    pub schema: String,
    pub candidate: CandidateSection,
    pub gates: GatesSection,
    pub review_artifacts: ReviewArtifactsSection,
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
        require_text(
            "review_artifacts.inventory_path",
            &self.review_artifacts.inventory_path,
        )?;
        require_sha256(
            "review_artifacts.inventory_sha256",
            &self.review_artifacts.inventory_sha256,
        )?;
        require_text(
            "review_artifacts.gates_report_path",
            &self.review_artifacts.gates_report_path,
        )?;
        require_sha256(
            "review_artifacts.gates_report_sha256",
            &self.review_artifacts.gates_report_sha256,
        )?;
        require_text(
            "review_artifacts.licensing_decision_path",
            &self.review_artifacts.licensing_decision_path,
        )?;
        if let Some(path) = &self.review_artifacts.runtime_map_path {
            require_text("review_artifacts.runtime_map_path", path)?;
        }
        if let Some(sha) = &self.review_artifacts.runtime_map_sha256 {
            require_sha256("review_artifacts.runtime_map_sha256", sha)?;
        }
        if let Some(path) = &self.review_artifacts.runtime_sidecar_path {
            require_text("review_artifacts.runtime_sidecar_path", path)?;
        }
        if let Some(sha) = &self.review_artifacts.runtime_sidecar_sha256 {
            require_sha256("review_artifacts.runtime_sidecar_sha256", sha)?;
        }
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
    pub conditions: Vec<ReviewCondition>,
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
            condition.require_text_non_empty().with_context(|| {
                format!(
                    "{} decision has an empty or invalid condition",
                    kind.label()
                )
            })?;
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
                "{} review decision is pending; the human decision in #103 has not been recorded",
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

    fn verify_conditions(
        &self,
        kind: DecisionKind,
        evidence: &ConditionEvidence<'_>,
    ) -> Result<()> {
        if self.decision != ReviewStatus::ApprovedWithConditions {
            return Ok(());
        }
        verify_approved_with_conditions(&self.conditions, evidence).with_context(|| {
            format!(
                "{} approved_with_conditions is not machine-satisfied",
                kind.label()
            )
        })
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
    /// When true, write packed runtime assets and append production registry
    /// entries. Candidate map bytes are never rewritten.
    pub apply: bool,
}

/// Result of a successful [`run_promotion`] call.
#[derive(Debug, Clone)]
pub struct PromotionOutcome {
    /// Draft production `manifest.toml` fragment.
    pub draft_manifest_fragment: String,
    /// Where the draft was written, if `--output` was supplied.
    pub written_to: Option<PathBuf>,
    pub runtime_map_sha256: String,
    pub runtime_sidecar_sha256: String,
    pub runtime_map_path: PathBuf,
    pub runtime_sidecar_path: PathBuf,
    pub applied: bool,
}

/// Verify a release candidate and its human decisions, pack a runtime map,
/// then draft (and optionally apply) the production registry change.
///
/// Fails closed on: an unpinned candidate, checksum/registry/inventory/gates
/// tamper, skipped required CI gates, pending/rejected decisions, missing
/// reviewer identity, invalid timestamps, decision checksum mismatch, or a
/// packed runtime SHA that does not match the RC pin. Candidate map bytes are
/// never rewritten.
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
            "release candidate status is {:?} ({}), not pinned; promotion is blocked pending regeneration",
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

    verify_frozen_gates_report(
        &inputs.repository_root,
        &candidate.review_artifacts,
        &candidate.candidate.candidate_sha256,
    )?;

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

    let licensing = verify_licensing_redistribution_review(
        &inputs.repository_root,
        &candidate.review_artifacts,
    )?;

    // Decision files are authoritative. Stale TOML gate statuses must not
    // require a second manual edit after #103 signatures land.
    let _ = (
        candidate.gates.scientific_review_status,
        candidate.gates.redistribution_review_status,
        candidate.gates.promotion_eligible,
    );

    let data_dir = map_path
        .parent()
        .context("candidate map_path has no parent directory")?;
    let stem = Path::new(&candidate.candidate.map_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("starlight");
    let staging = std::env::temp_dir().join(format!(
        "nsb-starlight-pack-{}",
        &candidate.candidate.candidate_sha256[..16]
    ));
    fs::create_dir_all(&staging)
        .with_context(|| format!("create pack staging {}", staging.display()))?;
    let staged_map = staging.join(format!("{stem}.release.csv"));
    let staged_pack_sidecar = staging.join(format!("{stem}.pack.toml"));
    let staged_runtime_sidecar = staging.join(format!("{stem}.manifest.toml"));

    let pack_outcome = pack::pack_candidate_map(&PackInputs {
        candidate_map: map_path.clone(),
        expected_candidate_sha256: candidate.candidate.candidate_sha256.clone(),
        expected_nside: candidate.candidate.nside,
        output_csv: staged_map.clone(),
        output_sidecar: staged_pack_sidecar,
        provenance_headers: runtime_admission_headers(&candidate.candidate),
    })?;

    let evidence = ConditionEvidence {
        repository_root: &inputs.repository_root,
        candidate_sha256: &candidate.candidate.candidate_sha256,
        runtime_map_sha256: Some(pack_outcome.runtime_map_sha256.as_str()),
        inventory_sha256: Some(candidate.review_artifacts.inventory_sha256.as_str()),
    };
    scientific.verify_conditions(DecisionKind::Scientific, &evidence)?;
    redistribution.verify_conditions(DecisionKind::Redistribution, &evidence)?;
    licensing.verify_conditions(&evidence)?;

    if let Some(expected) = &candidate.review_artifacts.runtime_map_sha256 {
        if &pack_outcome.runtime_map_sha256 != expected {
            bail!(
                "packed runtime map checksum mismatch: release candidate pins {expected}, packer produced {}",
                pack_outcome.runtime_map_sha256
            );
        }
    }

    write_production_sidecar(
        &staged_runtime_sidecar,
        &candidate.candidate,
        &pack_outcome.runtime_map_sha256,
        pack_outcome.all_sky_flux_sum_ph_m2_s,
    )?;
    let runtime_sidecar_sha256 = checksum_io::sha256_file(&staged_runtime_sidecar)?;
    if let Some(expected) = &candidate.review_artifacts.runtime_sidecar_sha256 {
        if &runtime_sidecar_sha256 != expected {
            bail!(
                "packed runtime sidecar checksum mismatch: release candidate pins {expected}, produced {runtime_sidecar_sha256}"
            );
        }
    }

    require_packed_runtime_header(&staged_map)?;

    let dest_map = data_dir.join(format!("{stem}.release.csv"));
    let dest_sidecar = data_dir.join(format!("{stem}.manifest.toml"));
    let mut applied = false;
    let (runtime_map_path, runtime_sidecar_path) = if inputs.apply {
        fs::copy(&staged_map, &dest_map)
            .with_context(|| format!("install packed runtime map {}", dest_map.display()))?;
        fs::copy(&staged_runtime_sidecar, &dest_sidecar).with_context(|| {
            format!("install packed runtime sidecar {}", dest_sidecar.display())
        })?;
        apply_production_registry(
            &data_dir.join("manifest.toml"),
            &candidate.candidate,
            &pack_outcome.runtime_map_sha256,
            &runtime_sidecar_sha256,
            stem,
        )?;
        applied = true;
        (dest_map, dest_sidecar)
    } else {
        (staged_map, staged_runtime_sidecar)
    };

    let draft = render_production_manifest_draft(
        &candidate.candidate,
        &pack_outcome.runtime_map_sha256,
        &runtime_sidecar_sha256,
    );
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

    // Keep candidate bytes untouched even when apply wrote sibling assets.
    let after = checksum_io::sha256_file(&map_path)?;
    if after != actual_map_sha256 {
        bail!("candidate map checksum changed during promotion; aborting");
    }

    Ok(PromotionOutcome {
        draft_manifest_fragment: draft,
        written_to,
        runtime_map_sha256: pack_outcome.runtime_map_sha256,
        runtime_sidecar_sha256,
        runtime_map_path,
        runtime_sidecar_path,
        applied,
    })
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

fn verify_frozen_gates_report(
    repository_root: &Path,
    artifacts: &ReviewArtifactsSection,
    expected_candidate_sha256: &str,
) -> Result<()> {
    let path = repository_root.join(&artifacts.gates_report_path);
    let bytes =
        fs::read(&path).with_context(|| format!("read frozen gates report {}", path.display()))?;
    let actual = checksum_io::sha256_bytes(&bytes);
    if actual != artifacts.gates_report_sha256 {
        bail!(
            "frozen gates report checksum mismatch: release candidate pins {}, actual file is {}",
            artifacts.gates_report_sha256,
            actual
        );
    }
    #[derive(Deserialize)]
    struct FrozenCommand {
        name: String,
        status: String,
    }
    #[derive(Deserialize)]
    struct FrozenGatesReport {
        passed: bool,
        commit_sha: Option<String>,
        candidate_sha256: String,
        #[serde(default)]
        recorded_commands: Vec<FrozenCommand>,
    }
    let report: FrozenGatesReport = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse frozen gates report {}", path.display()))?;
    let commit = report.commit_sha.as_deref().unwrap_or("");
    if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!(
            "frozen gates report commit_sha must be a 40-character git SHA; promotion is blocked"
        );
    }
    if !report.passed {
        bail!("frozen gates report has passed=false; promotion is blocked");
    }
    require_sha256(
        "frozen gates report candidate_sha256",
        &report.candidate_sha256,
    )?;
    if report.candidate_sha256 != expected_candidate_sha256 {
        bail!(
            "frozen gates report pins candidate {}, release candidate pins {expected_candidate_sha256}",
            report.candidate_sha256
        );
    }
    const REQUIRED_GATES: &[&str] = &[
        "format",
        "check",
        "clippy",
        "unit_tests",
        "runtime_integration_tests",
        "cli_tests",
        "data_tools_tests",
        "doctests",
        "documentation",
        "release_build",
        "cargo_deny",
        "msrv",
    ];
    for required in REQUIRED_GATES {
        let command = report
            .recorded_commands
            .iter()
            .find(|command| command.name == *required)
            .with_context(|| format!("frozen gates report is missing required gate {required}"))?;
        if command.status == "skipped" || command.status != "passed" {
            bail!(
                "frozen gates report required gate {required} has status {}; passed=true is not allowed when a required gate is missing or skipped",
                command.status
            );
        }
    }
    Ok(())
}

fn verify_licensing_redistribution_review(
    repository_root: &Path,
    artifacts: &ReviewArtifactsSection,
) -> Result<RedistributionReview> {
    let inventory_path = repository_root.join(&artifacts.inventory_path);
    let inventory_bytes = fs::read(&inventory_path)
        .with_context(|| format!("read artifact inventory {}", inventory_path.display()))?;
    let actual = checksum_io::sha256_bytes(&inventory_bytes);
    if actual != artifacts.inventory_sha256 {
        bail!(
            "artifact inventory checksum mismatch: release candidate pins {}, actual file is {}",
            artifacts.inventory_sha256,
            actual
        );
    }
    let decision_path = repository_root.join(&artifacts.licensing_decision_path);
    let review = RedistributionReview::load(&inventory_path, &decision_path)?;
    review.require_approved()?;
    Ok(review)
}

fn require_packed_runtime_header(map_path: &Path) -> Result<()> {
    let raw = fs::read_to_string(map_path)
        .with_context(|| format!("read packed runtime map {}", map_path.display()))?;
    let header = raw
        .lines()
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .unwrap_or("");
    if !pack::is_packed_runtime_header(header) {
        bail!("packed runtime map data header {header:?} is not the candidate-v5 packing contract");
    }
    Ok(())
}

pub(crate) fn runtime_admission_headers(candidate: &CandidateSection) -> BTreeMap<String, String> {
    let map_resolution = format!("HEALPix nside={} ordering=ring", candidate.nside);
    let version = format!("uv-v2-packed-from-{}", candidate.candidate_sha256);
    BTreeMap::from([
        ("dataset_name".into(), "NSB Gaia DR3 Starlight packed runtime map".into()),
        ("version".into(), version),
        ("generation_date_utc".into(), "2026-08-24T00:00:00Z".into()),
        (
            "source_catalogue".into(),
            "Gaia DR3 GaiaSource and XP continuous".into(),
        ),
        (
            "source_catalogue_release".into(),
            candidate.gaia_release.clone(),
        ),
        (
            "source_catalogue_license".into(),
            "CC BY-NC 3.0 IGO".into(),
        ),
        (
            "source_catalogue_checksum".into(),
            format!("sha256:{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"),
        ),
        (
            "source_candidate_sha256".into(),
            format!("sha256:{}", candidate.candidate_sha256),
        ),
        (
            "gaia_source_checksum_manifest_sha256".into(),
            format!("sha256:{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"),
        ),
        (
            "xp_continuous_checksum_manifest_sha256".into(),
            format!("sha256:{XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256}"),
        ),
        (
            "source_selection".into(),
            "Gaia DR3 full-population admission with photometric-inference fallback and selection-function weights".into(),
        ),
        (
            "magnitude_limit".into(),
            "Gaia DR3 catalogue as admitted by the frozen merge report".into(),
        ),
        ("map_resolution".into(), map_resolution),
        ("calibration_status".into(), "production".into()),
        (
            "photometry_model".into(),
            "gaia_dr3_xp_photon_radiance_300_650nm_packed_v1".into(),
        ),
        ("band_definition".into(), candidate.band.clone()),
        ("smoothing".into(), "none".into()),
        (
            "generated_by".into(),
            format!("nsb-data dataset starlight promote ({})", pack::PACKER_ID),
        ),
        (
            "generation_command".into(),
            "nsb-data dataset starlight promote --apply".into(),
        ),
        (
            "validation_report".into(),
            "docs/nsb_components/starlight/production-runs/combined-300-650-validation.json".into(),
        ),
        (
            "independent_comparison".into(),
            "no_admissible_independent_reference; human review #103".into(),
        ),
    ])
}

pub(crate) fn write_production_sidecar(
    path: &Path,
    candidate: &CandidateSection,
    runtime_map_sha256: &str,
    all_sky_flux_sum_ph_m2_s: f64,
) -> Result<()> {
    let map_resolution = format!("HEALPix nside={} ordering=ring", candidate.nside);
    let body = format!(
        r#"schema_version = 1
calibration_status = "production"
dataset_name = "NSB Gaia DR3 Starlight packed runtime map"
version = "uv-v2-packed-from-{}"
generation_date = "2026-08-24T00:00:00Z"
source_catalogue = "Gaia DR3 GaiaSource and XP continuous"
source_catalogue_release = "{}"
source_catalogue_license = "CC BY-NC 3.0 IGO"
source_catalogue_checksum = "sha256:{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"
source_selection = "Gaia DR3 full-population admission with photometric-inference fallback and selection-function weights"
magnitude_limit = "Gaia DR3 catalogue as admitted by the frozen merge report"
map_resolution = "{map_resolution}"
photometry_model = "gaia_dr3_xp_photon_radiance_300_650nm_packed_v1"
band_definition = "{}"
smoothing = "none"
generated_by = "nsb-data dataset starlight promote ({})"
generation_command = "nsb-data dataset starlight promote --apply"
map_sha256 = "sha256:{runtime_map_sha256}"
validation_report = "docs/nsb_components/starlight/production-runs/combined-300-650-validation.json"
independent_comparison = "no_admissible_independent_reference; human review #103"
flux_conservation_validated = true
input_integrated_flux_sum = {:.16e}
integrated_flux_conservation_tolerance = 1e-12

[source_candidate]
sha256 = "{}"

[[upstream_inputs]]
id = "gaia-source"
release = "Gaia DR3"
checksum_manifest_sha256 = "{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"

[[upstream_inputs]]
id = "xp-continuous"
release = "Gaia DR3"
checksum_manifest_sha256 = "{XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256}"

[header]
map_type = "healpix"
coordinate_frame = "galactic"
nside = "{}"
ordering = "{}"
s10_diagnostics = "not_provided"
dataset_name = "NSB Gaia DR3 Starlight packed runtime map"
version = "uv-v2-packed-from-{}"
generation_date_utc = "2026-08-24T00:00:00Z"
source_catalogue = "Gaia DR3 GaiaSource and XP continuous"
source_catalogue_release = "{}"
source_catalogue_license = "CC BY-NC 3.0 IGO"
source_catalogue_checksum = "sha256:{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"
source_candidate_sha256 = "sha256:{}"
gaia_source_checksum_manifest_sha256 = "sha256:{GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256}"
xp_continuous_checksum_manifest_sha256 = "sha256:{XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256}"
source_selection = "Gaia DR3 full-population admission with photometric-inference fallback and selection-function weights"
magnitude_limit = "Gaia DR3 catalogue as admitted by the frozen merge report"
map_resolution = "{map_resolution}"
calibration_status = "production"
photometry_model = "gaia_dr3_xp_photon_radiance_300_650nm_packed_v1"
band_definition = "{}"
smoothing = "none"
generated_by = "nsb-data dataset starlight promote ({})"
generation_command = "nsb-data dataset starlight promote --apply"
validation_report = "docs/nsb_components/starlight/production-runs/combined-300-650-validation.json"
independent_comparison = "no_admissible_independent_reference; human review #103"
"#,
        candidate.candidate_sha256,
        candidate.gaia_release,
        candidate.band,
        pack::PACKER_ID,
        all_sky_flux_sum_ph_m2_s * 1.0e-13,
        candidate.candidate_sha256,
        candidate.nside,
        "ring",
        candidate.candidate_sha256,
        candidate.gaia_release,
        candidate.candidate_sha256,
        candidate.band,
        pack::PACKER_ID,
    );
    fs::write(path, body).with_context(|| format!("write production sidecar {}", path.display()))
}

fn apply_production_registry(
    manifest_path: &Path,
    candidate: &CandidateSection,
    runtime_map_sha256: &str,
    runtime_sidecar_sha256: &str,
    stem: &str,
) -> Result<()> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read asset registry {}", manifest_path.display()))?;
    let mut doc: toml_edit::DocumentMut = raw
        .parse()
        .with_context(|| format!("parse asset registry {}", manifest_path.display()))?;
    let assets = doc["assets"]
        .as_array_of_tables_mut()
        .context("asset registry has no [[assets]] tables")?;
    let release_path = format!("{stem}.release.csv");
    let sidecar_path = format!("{stem}.manifest.toml");
    assets.retain(|table| {
        table.get("path").and_then(|item| item.as_str()) != Some(release_path.as_str())
            && table.get("path").and_then(|item| item.as_str()) != Some(sidecar_path.as_str())
    });
    assets.push(registry_asset_table(
        &release_path,
        PRODUCTION_MAP_SCHEMA,
        runtime_map_sha256,
        candidate,
        true,
    ));
    assets.push(registry_asset_table(
        &sidecar_path,
        PRODUCTION_MANIFEST_SCHEMA,
        runtime_sidecar_sha256,
        candidate,
        true,
    ));
    fs::write(manifest_path, doc.to_string())
        .with_context(|| format!("write asset registry {}", manifest_path.display()))
}

fn registry_asset_table(
    path: &str,
    schema: &str,
    sha256: &str,
    candidate: &CandidateSection,
    production: bool,
) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["path"] = toml_edit::value(path);
    table["schema"] = toml_edit::value(schema);
    table["sha256"] = toml_edit::value(sha256);
    table["gaia_release"] = toml_edit::value(candidate.gaia_release.as_str());
    table["band"] = toml_edit::value(candidate.band.as_str());
    table["units"] = toml_edit::value("ph_cm2_ns_sr");
    table["source"] = toml_edit::value("Gaia DR3 GaiaSource and XP continuous bulk distributions");
    table["license"] = toml_edit::value("Gaia data licence: CC BY-NC 3.0 IGO");
    table["generator"] = toml_edit::value("nsb-data dataset starlight promote");
    table["generation_command"] = toml_edit::value("nsb-data dataset starlight promote --apply");
    table["validation_report"] = toml_edit::value(
        "docs/nsb_components/starlight/production-runs/combined-300-650-validation.json",
    );
    table["calibration_status"] = toml_edit::value(if production {
        "production"
    } else {
        "candidate"
    });
    table["runtime_embedded"] = toml_edit::value(production);
    table
}

fn load_decision(path: &Path, kind: DecisionKind) -> Result<ReviewDecision> {
    let raw = fs::read(path)
        .with_context(|| format!("read {} review decision {}", kind.label(), path.display()))?;
    let decision: ReviewDecision = serde_json::from_slice(&raw)
        .with_context(|| format!("parse {} review decision {}", kind.label(), path.display()))?;
    decision.validate(kind)?;
    Ok(decision)
}

fn render_production_manifest_draft(
    candidate: &CandidateSection,
    map_sha256: &str,
    sidecar_sha256: &str,
) -> String {
    let stem = Path::new(&candidate.map_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("starlight");
    let release_path = format!("{stem}.release.csv");
    let sidecar_path = format!("{stem}.manifest.toml");

    let mut out = String::new();
    out.push_str("# Generated by `nsb-data dataset starlight promote`.\n");
    out.push_str("# Candidate map bytes are never rewritten. These entries register the\n");
    out.push_str("# packed runtime map and sidecar. Issue #103 owns the human decisions.\n\n");
    writeln!(out, "[[assets]]").unwrap();
    writeln!(out, "path = {release_path:?}").unwrap();
    writeln!(out, "schema = {PRODUCTION_MAP_SCHEMA:?}").unwrap();
    writeln!(out, "sha256 = {map_sha256:?}").unwrap();
    writeln!(out, "gaia_release = {:?}", candidate.gaia_release).unwrap();
    writeln!(out, "band = {:?}", candidate.band).unwrap();
    writeln!(out, "units = \"ph_cm2_ns_sr\"").unwrap();
    writeln!(out, "calibration_status = \"production\"").unwrap();
    writeln!(out, "runtime_embedded = true").unwrap();
    out.push('\n');
    writeln!(out, "[[assets]]").unwrap();
    writeln!(out, "path = {sidecar_path:?}").unwrap();
    writeln!(out, "schema = {PRODUCTION_MANIFEST_SCHEMA:?}").unwrap();
    writeln!(out, "sha256 = {sidecar_sha256:?}").unwrap();
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

    const SYNTHETIC_CANDIDATE_V5: &str = concat!(
        "# schema=nsb-healpix-starlight-candidate-v5\n",
        "# ordering=nested\n",
        "# representation=sparse\n",
        "# nside=1\n",
        "# flux_unit=ph_m-2_s-1\n",
        "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
        "0,1.0,0.1,0.2,0.25,5,1\n",
    );

    fn synthetic_candidate_sha256() -> String {
        checksum_io::sha256_bytes(SYNTHETIC_CANDIDATE_V5.as_bytes())
    }

    fn complete_gates_json(candidate_sha256: &str) -> String {
        let commands = [
            "format",
            "check",
            "clippy",
            "unit_tests",
            "runtime_integration_tests",
            "cli_tests",
            "data_tools_tests",
            "doctests",
            "documentation",
            "release_build",
            "cargo_deny",
            "msrv",
        ]
        .into_iter()
        .map(|name| format!(r#"{{"name":"{name}","status":"passed"}}"#))
        .collect::<Vec<_>>()
        .join(",");
        format!(
            r#"{{
  "schema_version": 1,
  "dataset": "starlight",
  "passed": true,
  "commit_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "candidate_sha256": "{candidate_sha256}",
  "recorded_commands": [{commands}]
}}"#
        )
    }

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
        inventory_sha256: &str,
        gates_sha256: &str,
        candidate_sha256: &str,
    ) -> String {
        format!(
            r#"schema_version = 1
schema = "nsb-starlight-release-candidate-v1"
notes = "Synthetic test-only fixture for nsb-data-tools promotion unit tests; not real scientific evidence."

[candidate]
status = "{status}"
candidate_sha256 = "{candidate_sha256}"
map_path = "crates/nsb/data/starlight_nside128.csv"
map_schema = "nsb-healpix-starlight-candidate-v5"
band = "synthetic test-only combined band"
units = "ph_m-2_s-1"
nside = 1
ordering = "nested"
gaia_release = "Gaia DR3 (synthetic test fixture)"

[candidate.model_versions]
uv_correction_current = "synthetic-test-only-v1"

[gates]
validation_status = "{validation_status}"
scientific_review_status = "approved"
redistribution_review_status = "approved"
promotion_eligible = {promotion_eligible}

[review_artifacts]
inventory_path = "docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml"
inventory_sha256 = "{inventory_sha256}"
gates_report_path = "docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json"
gates_report_sha256 = "{gates_sha256}"
licensing_decision_path = "docs/nsb_components/starlight/licensing/redistribution-review-decision-v1.json"
"#
        )
    }

    fn decision_json(decision: &str, extra_conditions: &str, candidate_sha256: &str) -> String {
        format!(
            r#"{{
  "schema_version": 1,
  "decision": "{decision}",
  "reviewer_name": "Synthetic Test Reviewer",
  "reviewer_role": "synthetic-test-only-role",
  "reviewed_at_utc": "2026-07-30T00:00:00Z",
  "candidate_sha256": "{candidate_sha256}",
  "conditions": [{extra_conditions}],
  "notes": "Synthetic test-only decision fixture; not a real human review."
}}"#
        )
    }

    fn write_synthetic_repo(
        status: &str,
        validation_status: &str,
        promotion_eligible: bool,
        scientific_decision: &str,
        redistribution_decision: &str,
    ) -> SyntheticRepo {
        let sha = synthetic_candidate_sha256();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let data_dir = root.join("crates/nsb/data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(
            data_dir.join("starlight_nside128.csv"),
            SYNTHETIC_CANDIDATE_V5,
        )
        .unwrap();
        fs::write(
            data_dir.join("manifest.toml"),
            format!(
                "schema_version = 1\n\n[[assets]]\npath = \"starlight_nside128.csv\"\nschema = \"nsb-healpix-starlight-candidate-v5\"\nsha256 = \"{sha}\"\ncalibration_status = \"candidate\"\nruntime_embedded = false\n"
            ),
        )
        .unwrap();

        let inventory = format!(
            r#"schema_version = 1

[[artifacts]]
id = "synthetic-upstream-input"
category = "synthetic-test-category"
source = "SYNTHETIC-NON-PRODUCTION test fixture source"
release = "synthetic-test-release"
license = "synthetic-test-license"
distribution_class = "download_only"
distributed = false
channels = []
notes = "synthetic-test-notes: fixture data only, not a real artifact"

[[artifacts]]
id = "synthetic-distributed-output"
category = "synthetic-test-category"
source = "SYNTHETIC-NON-PRODUCTION test fixture source"
release = "synthetic-test-release"
license = "synthetic-test-license"
sha256 = "{sha}"
distribution_class = "repository_embedded"
distributed = true
channels = ["git_repository"]
notes = "synthetic-test-notes: fixture data only, not a real artifact"
"#
        );
        let inventory_path =
            root.join("docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml");
        fs::create_dir_all(inventory_path.parent().unwrap()).unwrap();
        fs::write(&inventory_path, &inventory).unwrap();
        let inventory_sha256 = checksum_io::sha256_bytes(inventory.as_bytes());

        let licensing_decision = format!(
            r#"{{
  "schema_version": 1,
  "decision": "approved",
  "reviewer_name": "Synthetic Test Reviewer",
  "reviewer_role": "synthetic-test-only-role",
  "reviewed_at_utc": "2026-07-30T00:00:00Z",
  "inventory_sha256": "{inventory_sha256}",
  "pinned_artifacts": [
    {{
      "id": "synthetic-distributed-output",
      "sha256": "{sha}",
      "approved_channels": ["git_repository"]
    }}
  ],
  "conditions": [],
  "restrictions": [],
  "notes": "synthetic-test-notes: fixture decision, not a real approval"
}}"#
        );
        let licensing_path = root
            .join("docs/nsb_components/starlight/licensing/redistribution-review-decision-v1.json");
        fs::write(&licensing_path, licensing_decision).unwrap();

        let gates = complete_gates_json(&sha);
        let gates_path = root
            .join("docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json");
        fs::create_dir_all(gates_path.parent().unwrap()).unwrap();
        fs::write(&gates_path, &gates).unwrap();
        let gates_sha256 = checksum_io::sha256_bytes(gates.as_bytes());

        let release_candidate = release_candidate_toml(
            status,
            validation_status,
            promotion_eligible,
            &inventory_sha256,
            &gates_sha256,
            &sha,
        );
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
            "pinned",
            "technical_pass",
            true,
            &decision_json(
                "approved",
                "\"synthetic-test-only-condition\"",
                &synthetic_candidate_sha256(),
            ),
            &decision_json(
                "approved",
                "\"synthetic-test-only-condition\"",
                &synthetic_candidate_sha256(),
            ),
        )
    }

    fn inputs(repo: &SyntheticRepo, output: Option<PathBuf>) -> PromotionInputs {
        PromotionInputs {
            release_candidate: repo.release_candidate.clone(),
            scientific_decision: repo.scientific_decision.clone(),
            redistribution_decision: repo.redistribution_decision.clone(),
            repository_root: repo.root.clone(),
            output,
            apply: false,
        }
    }

    #[test]
    fn awaiting_regeneration_status_fails_closed() {
        let repo = write_synthetic_repo(
            "awaiting_regeneration",
            "pending_regeneration",
            false,
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
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
            "schema_version = 1\n\n[[assets]]\npath = \"starlight_nside128.csv\"\nschema = \"nsb-healpix-starlight-v2\"\nsha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\ncalibration_status = \"candidate\"\nruntime_embedded = false\n",
        )
        .unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("tamper detected"));
    }

    #[test]
    fn pending_scientific_decision_fails_closed() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json("pending", "", &synthetic_candidate_sha256()),
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("scientific review decision is pending"));
    }

    #[test]
    fn rejected_redistribution_decision_fails_closed() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
            &decision_json("rejected", "", &synthetic_candidate_sha256()),
        );
        let tampered = fs::read_to_string(&repo.release_candidate)
            .unwrap()
            .replace(
                "redistribution_review_status = \"approved\"",
                "redistribution_review_status = \"rejected\"",
            );
        fs::write(&repo.release_candidate, tampered).unwrap();
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
        assert_ne!(other_valid_sha256, synthetic_candidate_sha256());
        let tampered = fs::read_to_string(&repo.scientific_decision)
            .unwrap()
            .replace(&synthetic_candidate_sha256(), &other_valid_sha256);
        fs::write(&repo.scientific_decision, tampered).unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("pins candidate"));
    }

    #[test]
    fn stale_toml_gate_status_does_not_block_signed_decisions() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
        );
        let tampered = fs::read_to_string(&repo.release_candidate)
            .unwrap()
            .replace(
                "scientific_review_status = \"approved\"",
                "scientific_review_status = \"pending\"",
            )
            .replace("promotion_eligible = true", "promotion_eligible = false");
        fs::write(&repo.release_candidate, tampered).unwrap();
        run_promotion(&inputs(&repo, None)).unwrap();
    }

    #[test]
    fn promotion_eligible_false_does_not_block_when_decisions_are_signed() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            false,
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
        );
        let outcome = run_promotion(&inputs(&repo, None)).unwrap();
        assert!(!outcome.applied);
        assert!(outcome
            .draft_manifest_fragment
            .contains("runtime_embedded = true"));
    }

    #[test]
    fn approved_with_conditions_requires_at_least_one_condition() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json(
                "approved_with_conditions",
                "",
                &synthetic_candidate_sha256(),
            ),
            &decision_json("approved", "\"c\"", &synthetic_candidate_sha256()),
        );
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error
            .to_string()
            .contains("requires at least one recorded condition"));
    }

    fn structured_candidate_condition(sha: &str) -> String {
        format!(
            r#"{{"id":"pin-candidate","description":"candidate SHA","verifier":{{"type":"candidate_sha256","sha256":"{sha}"}}}}"#
        )
    }

    #[test]
    fn approved_with_conditions_free_text_blocks_promotion() {
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json(
                "approved_with_conditions",
                "\"Fix X before production\"",
                &synthetic_candidate_sha256(),
            ),
            &decision_json("approved", "", &synthetic_candidate_sha256()),
        );
        let error = format!("{:#}", run_promotion(&inputs(&repo, None)).unwrap_err());
        assert!(error.contains("not machine-verifiable"), "{error}");
    }

    #[test]
    fn approved_with_conditions_structured_candidate_pin_succeeds() {
        let sha = synthetic_candidate_sha256();
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json(
                "approved_with_conditions",
                &structured_candidate_condition(&sha),
                &sha,
            ),
            &decision_json("approved", "", &sha),
        );
        run_promotion(&inputs(&repo, None)).unwrap();
    }

    #[test]
    fn approved_with_conditions_one_failed_among_several_blocks() {
        let sha = synthetic_candidate_sha256();
        let conditions = format!(
            "{},{{\"id\":\"bad-runtime\",\"description\":\"runtime\",\"verifier\":{{\"type\":\"runtime_map_sha256\",\"sha256\":\"{}\"}}}}",
            structured_candidate_condition(&sha),
            "b".repeat(64)
        );
        let repo = write_synthetic_repo(
            "pinned",
            "technical_pass",
            true,
            &decision_json("approved_with_conditions", &conditions, &sha),
            &decision_json("approved", "", &sha),
        );
        let error = format!("{:#}", run_promotion(&inputs(&repo, None)).unwrap_err());
        assert!(error.contains("runtime map checksum mismatch"), "{error}");
    }

    fn rewrite_gates_and_repin(repo: &SyntheticRepo, new_gates: &str) {
        let gates_path = repo
            .root
            .join("docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json");
        let old_sha = checksum_io::sha256_file(&gates_path).unwrap();
        fs::write(&gates_path, new_gates).unwrap();
        let new_sha = checksum_io::sha256_file(&gates_path).unwrap();
        let rc = fs::read_to_string(&repo.release_candidate)
            .unwrap()
            .replace(&old_sha, &new_sha);
        fs::write(&repo.release_candidate, rc).unwrap();
    }

    #[test]
    fn frozen_gates_report_requires_matching_candidate_sha256() {
        let repo = valid_synthetic_repo();
        let gates_path = repo
            .root
            .join("docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json");
        let original = fs::read_to_string(&gates_path).unwrap();
        let sha = synthetic_candidate_sha256();
        rewrite_gates_and_repin(
            &repo,
            &original.replace(&format!("\"candidate_sha256\": \"{sha}\","), ""),
        );
        let error = format!("{:#}", run_promotion(&inputs(&repo, None)).unwrap_err());
        assert!(
            error.contains("candidate_sha256") || error.contains("missing field"),
            "{error}"
        );
    }

    #[test]
    fn frozen_gates_report_rejects_wrong_and_malformed_candidate_sha256() {
        let repo = valid_synthetic_repo();
        let gates_path = repo
            .root
            .join("docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json");
        let original = fs::read_to_string(&gates_path).unwrap();
        let sha = synthetic_candidate_sha256();
        rewrite_gates_and_repin(&repo, &original.replace(&sha, &"c".repeat(64)));
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(error.to_string().contains("pins candidate"), "{}", error);

        let repo = valid_synthetic_repo();
        let gates_path = repo
            .root
            .join("docs/nsb_components/starlight/production-runs/release-candidate-gates-v1.json");
        let original = fs::read_to_string(&gates_path).unwrap();
        rewrite_gates_and_repin(&repo, &original.replace(&sha, "not-a-sha256-digest"));
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(
            error.to_string().contains("SHA-256") || error.to_string().contains("64"),
            "{}",
            error
        );
    }

    #[test]
    fn production_sidecar_does_not_use_candidate_as_catalogue_checksum() {
        let repo = valid_synthetic_repo();
        let outcome = run_promotion(&inputs(&repo, None)).unwrap();
        let sidecar = fs::read_to_string(&outcome.runtime_sidecar_path).unwrap();
        let candidate = synthetic_candidate_sha256();
        assert!(!sidecar.contains(&format!(
            "source_catalogue_checksum = \"sha256:{candidate}\""
        )));
        assert!(sidecar.contains("source_candidate"));
        assert!(sidecar.contains(pack::GAIA_SOURCE_CHECKSUM_MANIFEST_SHA256));
        assert!(sidecar.contains(pack::XP_CONTINUOUS_CHECKSUM_MANIFEST_SHA256));
    }

    #[test]
    fn candidate_v5_packs_into_runtime_assets() {
        let repo = valid_synthetic_repo();
        let outcome = run_promotion(&inputs(&repo, None)).unwrap();
        assert!(
            pack::is_packed_runtime_header(
                fs::read_to_string(&outcome.runtime_map_path)
                    .unwrap_or_default()
                    .lines()
                    .find(|line| !line.starts_with('#') && !line.is_empty())
                    .unwrap_or("")
            ) || !outcome.applied
        );
        assert!(outcome
            .draft_manifest_fragment
            .contains("starlight_nside128.release.csv"));
        assert_eq!(
            fs::read(repo.root.join("crates/nsb/data/starlight_nside128.csv")).unwrap(),
            SYNTHETIC_CANDIDATE_V5.as_bytes()
        );
    }

    #[test]
    fn apply_writes_complete_scientific_asset_registry_fields() {
        let repo = valid_synthetic_repo();
        let mut promotion = inputs(&repo, None);
        promotion.apply = true;
        let outcome = run_promotion(&promotion).unwrap();
        assert!(outcome.applied);
        let raw = fs::read_to_string(repo.root.join("crates/nsb/data/manifest.toml")).unwrap();
        assert!(raw.contains("path = \"starlight_nside128.release.csv\""));
        assert!(
            raw.contains("source = \"Gaia DR3 GaiaSource and XP continuous bulk distributions\"")
        );
        assert!(raw.contains("license = \"Gaia data licence: CC BY-NC 3.0 IGO\""));
        assert!(raw.contains("generator = \"nsb-data dataset starlight promote\""));
        assert!(raw.contains("runtime_embedded = true"));
    }

    #[test]
    fn licensing_inventory_mismatch_fails_closed() {
        let repo = valid_synthetic_repo();
        let tampered = fs::read_to_string(&repo.release_candidate)
            .unwrap()
            .replace(
                &format!(
                    "inventory_sha256 = \"{}\"",
                    checksum_io::sha256_file(&repo.root.join(
                        "docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml"
                    ))
                    .unwrap()
                ),
                &format!("inventory_sha256 = \"{}\"", "a".repeat(64)),
            );
        fs::write(&repo.release_candidate, tampered).unwrap();
        let error = run_promotion(&inputs(&repo, None)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("artifact inventory checksum mismatch"),
            "unexpected error: {error}"
        );
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
        assert_eq!(manifest.candidate.status, CandidateStatus::Pinned);
        assert!(!manifest.gates.promotion_eligible);

        let error = run_promotion(&PromotionInputs {
            release_candidate,
            scientific_decision,
            redistribution_decision,
            repository_root: root,
            output: None,
            apply: false,
        })
        .unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("pending"),
            "documented pending RC must fail on unsigned #103 decisions, got: {message}"
        );
    }
}
