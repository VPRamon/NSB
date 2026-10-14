// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Versioned, fail-closed redistribution/licensing review contract (#88).
//!
//! The promotion workflow consumes an artifact inventory and an authorized
//! human decision record supplied with the candidate evidence. Production
//! registration and publication are derived from the immutable approval snapshot.
//! This module validates the shape, checksums, channels, and conditions of any
//! future promotion evidence; it never grants approval itself.

use crate::platform::checksum_io;
use crate::starlight::conditions::{
    verify_approved_with_conditions, ConditionEvidence, ReviewCondition,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const INVENTORY_SCHEMA_VERSION: u32 = 1;
pub const DECISION_SCHEMA_VERSION: u32 = 1;

/// How an artifact's bytes currently reach (or do not reach) a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionClass {
    /// Acquired from an upstream source but never redistributed by NSB.
    DownloadOnly,
    /// Bytes are committed to this repository and ship with every checkout.
    RepositoryEmbedded,
    /// Produced by an NSB pipeline from other inputs; may or may not be
    /// committed to the repository (see `distributed`).
    GeneratedDerivedOutput,
    /// Referenced only in documentation/attribution text, not shipped as
    /// standalone bytes with independent redistribution semantics.
    DocumentationReference,
}

/// Lifecycle of a derived runtime product. Approval itself lives in the decision,
/// not in mutable publication metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactLifecycle {
    Candidate,
    Staged,
    ProductionRegistered,
    Published,
}

pub(crate) const COMPACT_IDS: [&str; 2] = ["compact-runtime-map-v1", "compact-runtime-sidecar-v1"];
pub(crate) const REQUIRED_RUNTIME_CHANNELS: [&str; 4] =
    ["git_repository", "crates_io", "pypi_wheel", "pypi_sdist"];
pub(crate) const CURRENT_INVENTORY: &str =
    "docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml";
pub(crate) const APPROVAL_INVENTORY: &str =
    "docs/nsb_components/starlight/licensing/approval-inventory-v1.toml";

/// One immutable inventory entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryArtifact {
    pub id: String,
    pub category: String,
    pub source: String,
    pub release: String,
    pub license: String,
    #[serde(default)]
    pub license_url: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub superseded_by_sha256: Option<String>,
    pub distribution_class: DistributionClass,
    pub distributed: bool,
    #[serde(default)]
    pub channels: Vec<String>,
    /// Future publication scope, frozen before review. Not evidence of upload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planned_channels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<ArtifactLifecycle>,
    #[serde(default)]
    pub tracking_issue: Option<String>,
    pub notes: String,
}

/// Complete artifact inventory (schema v1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactInventory {
    pub schema_version: u32,
    #[serde(default)]
    pub artifacts: Vec<InventoryArtifact>,
}

/// Redistribution decision recorded by an authorized human reviewer.
///
/// Allowed values mirror the scientific-review vocabulary documented in
/// issue #103: production admits only `approved`, or `approved_with_conditions`
/// when every condition is machine-verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedistributionDecision {
    Pending,
    Approved,
    ApprovedWithConditions,
    Rejected,
}

/// One artifact's checksum and authorized channels as pinned by the reviewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedArtifactApproval {
    pub id: String,
    pub sha256: String,
    #[serde(default)]
    pub approved_channels: Vec<String>,
}

/// Complete redistribution review decision record (schema v1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedistributionReviewDecision {
    pub schema_version: u32,
    pub decision: RedistributionDecision,
    #[serde(default)]
    pub reviewer_name: Option<String>,
    #[serde(default)]
    pub reviewer_role: Option<String>,
    #[serde(default)]
    pub reviewed_at_utc: Option<String>,
    pub candidate_sha256: String,
    pub inventory_sha256: String,
    pub review_bundle_sha256: String,
    #[serde(default)]
    pub pinned_artifacts: Vec<PinnedArtifactApproval>,
    #[serde(default)]
    pub conditions: Vec<ReviewCondition>,
    #[serde(default)]
    pub restrictions: Vec<String>,
    pub notes: String,
}

/// Loaded, checksum-linked inventory and decision pair.
#[derive(Debug, Clone)]
pub struct RedistributionReview {
    inventory: ArtifactInventory,
    inventory_sha256: String,
    decision: RedistributionReviewDecision,
}

impl ArtifactInventory {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != INVENTORY_SCHEMA_VERSION {
            bail!(
                "unsupported artifact inventory schema_version {}",
                self.schema_version
            );
        }
        if self.artifacts.is_empty() {
            bail!("artifact inventory must list at least one artifact");
        }
        let mut ids = BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            if !ids.insert(artifact.id.as_str()) {
                bail!("duplicate artifact id {}", artifact.id);
            }
        }
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&InventoryArtifact> {
        self.artifacts.iter().find(|artifact| artifact.id == id)
    }

    /// Artifacts currently reaching at least one consumer channel.
    pub fn distributed(&self) -> impl Iterator<Item = &InventoryArtifact> {
        self.artifacts
            .iter()
            .filter(|artifact| artifact.distributed)
    }
}

impl InventoryArtifact {
    fn validate(&self) -> Result<()> {
        require_identifier("artifact id", &self.id)?;
        require_text("artifact category", &self.category)?;
        require_text("artifact source", &self.source)?;
        require_text("artifact release", &self.release)?;
        require_text("artifact license", &self.license)?;
        require_text("artifact notes", &self.notes)?;
        if let Some(sha256) = &self.sha256 {
            require_sha256("artifact sha256", sha256)?;
        }
        if let Some(sha256) = &self.superseded_by_sha256 {
            require_sha256("artifact superseded_by_sha256", sha256)?;
        }
        if self.distributed {
            if self.channels.is_empty() {
                bail!(
                    "distributed artifact {} declares no distribution channel",
                    self.id
                );
            }
            if self.sha256.is_none() {
                bail!(
                    "distributed artifact {} requires a pinned sha256 for its class {:?}",
                    self.id,
                    self.distribution_class
                );
            }
        } else if !self.channels.is_empty() {
            bail!(
                "artifact {} is not marked distributed but declares channels {:?}",
                self.id,
                self.channels
            );
        }
        if !self.planned_channels.is_empty() && (self.sha256.is_none() || self.lifecycle.is_none())
        {
            bail!(
                "planned artifact {} requires checksum and lifecycle",
                self.id
            );
        }
        let mut planned = BTreeSet::new();
        for channel in &self.planned_channels {
            require_text("planned artifact channel", channel)?;
            if !planned.insert(channel.as_str()) {
                bail!("artifact {} repeats planned channel {}", self.id, channel);
            }
        }
        if let Some(lifecycle) = self.lifecycle {
            if self.planned_channels.is_empty() {
                bail!("lifecycle artifact {} requires planned channels", self.id);
            }
            if self.distributed != (lifecycle == ArtifactLifecycle::Published) {
                bail!(
                    "artifact {} lifecycle disagrees with actual distribution",
                    self.id
                );
            }
            if self.channels.iter().any(|c| !planned.contains(c.as_str())) {
                bail!("artifact {} uses an unplanned publication channel", self.id);
            }
        }
        let mut channels = BTreeSet::new();
        for channel in &self.channels {
            require_text("artifact channel", channel)?;
            if !channels.insert(channel.as_str()) {
                bail!("artifact {} repeats channel {}", self.id, channel);
            }
        }
        Ok(())
    }
}

impl RedistributionReviewDecision {
    fn validate(&self) -> Result<()> {
        if self.schema_version != DECISION_SCHEMA_VERSION {
            bail!(
                "unsupported redistribution decision schema_version {}",
                self.schema_version
            );
        }
        require_sha256("decision candidate_sha256", &self.candidate_sha256)?;
        require_sha256("decision inventory_sha256", &self.inventory_sha256)?;
        require_sha256("decision review_bundle_sha256", &self.review_bundle_sha256)?;
        require_text("decision notes", &self.notes)?;
        let mut ids = BTreeSet::new();
        for pinned in &self.pinned_artifacts {
            require_identifier("pinned artifact id", &pinned.id)?;
            require_sha256("pinned artifact sha256", &pinned.sha256)?;
            for channel in &pinned.approved_channels {
                require_text("pinned artifact approved_channel", channel)?;
            }
            if !ids.insert(pinned.id.as_str()) {
                bail!("decision repeats pinned artifact id {}", pinned.id);
            }
        }
        for condition in &self.conditions {
            condition
                .require_text_non_empty()
                .context("redistribution decision has an empty or invalid condition")?;
        }
        for restriction in &self.restrictions {
            require_text("decision restriction", restriction)?;
        }
        Ok(())
    }
}

impl RedistributionReview {
    /// Load exact inventory and decision bytes, parse, and structurally
    /// validate both, then verify the decision pins the actual inventory
    /// checksum. This does not yet decide whether promotion is authorized;
    /// call [`Self::require_approved`] for that fail-closed gate.
    pub fn load(inventory_path: &Path, decision_path: &Path) -> Result<Self> {
        let inventory_bytes = fs::read(inventory_path).with_context(|| {
            format!(
                "read redistribution artifact inventory {}",
                inventory_path.display()
            )
        })?;
        let inventory_text = std::str::from_utf8(&inventory_bytes).with_context(|| {
            format!(
                "redistribution artifact inventory {} is not valid UTF-8",
                inventory_path.display()
            )
        })?;
        let inventory: ArtifactInventory = toml::from_str(inventory_text).with_context(|| {
            format!(
                "parse redistribution artifact inventory {}",
                inventory_path.display()
            )
        })?;
        inventory.validate()?;
        let inventory_sha256 = checksum_io::sha256_bytes(&inventory_bytes);

        let decision_bytes = fs::read(decision_path).with_context(|| {
            format!(
                "read redistribution review decision {}",
                decision_path.display()
            )
        })?;
        let decision: RedistributionReviewDecision = serde_json::from_slice(&decision_bytes)
            .with_context(|| {
                format!(
                    "parse redistribution review decision {}",
                    decision_path.display()
                )
            })?;
        decision.validate()?;

        if decision.inventory_sha256 != inventory_sha256 {
            bail!(
                "redistribution decision checksum mismatch: pins inventory {}, actual inventory is {}",
                decision.inventory_sha256,
                inventory_sha256
            );
        }

        Ok(Self {
            inventory,
            inventory_sha256,
            decision,
        })
    }

    pub fn inventory(&self) -> &ArtifactInventory {
        &self.inventory
    }

    pub fn inventory_sha256(&self) -> &str {
        &self.inventory_sha256
    }

    pub fn decision(&self) -> &RedistributionReviewDecision {
        &self.decision
    }

    /// Fail-closed promotion gate.
    ///
    /// Rejects a pending or rejected decision, a missing/placeholder
    /// reviewer, a malformed review timestamp, any distributed artifact the
    /// decision does not pin with a matching checksum, and any channel a
    /// distributed artifact uses that the decision does not authorize.
    pub fn require_approved(&self, expected_candidate_sha256: &str) -> Result<()> {
        match self.decision.decision {
            RedistributionDecision::Pending => {
                bail!(
                    "redistribution review decision is pending; the human decision in #103 has not been recorded"
                )
            }
            RedistributionDecision::Rejected => {
                bail!("redistribution review decision is rejected")
            }
            RedistributionDecision::Approved | RedistributionDecision::ApprovedWithConditions => {}
        }

        let reviewer_name = self.decision.reviewer_name.as_deref().unwrap_or_default();
        require_text("reviewer_name", reviewer_name)
            .context("redistribution decision is missing an authorized reviewer")?;
        let reviewer_role = self.decision.reviewer_role.as_deref().unwrap_or_default();
        require_text("reviewer_role", reviewer_role)
            .context("redistribution decision is missing an authorized reviewer")?;
        let reviewed_at = self.decision.reviewed_at_utc.as_deref().unwrap_or_default();
        require_rfc3339_utc("reviewed_at_utc", reviewed_at)
            .context("redistribution decision has no valid review timestamp")?;

        if self.decision.decision == RedistributionDecision::ApprovedWithConditions
            && self.decision.conditions.is_empty()
        {
            bail!("approved_with_conditions requires at least one recorded condition");
        }
        if self.decision.candidate_sha256 != expected_candidate_sha256 {
            bail!(
                "redistribution decision pins candidate {}, but the release candidate is {}",
                self.decision.candidate_sha256,
                expected_candidate_sha256
            );
        }

        for pinned in &self.decision.pinned_artifacts {
            let artifact = self
                .inventory
                .find(&pinned.id)
                .with_context(|| format!("decision pins unknown artifact {}", pinned.id))?;
            if artifact.sha256.as_deref() != Some(pinned.sha256.as_str()) {
                bail!(
                    "redistribution decision checksum mismatch for {}",
                    pinned.id
                );
            }
        }
        for artifact in self.inventory.distributed() {
            let pinned = self
                .decision
                .pinned_artifacts
                .iter()
                .find(|pinned| pinned.id == artifact.id)
                .with_context(|| {
                    format!(
                        "redistribution decision does not pin distributed artifact {}",
                        artifact.id
                    )
                })?;
            let expected_sha256 = artifact.sha256.as_deref().with_context(|| {
                format!(
                    "distributed artifact {} has no checksum in the inventory to verify against",
                    artifact.id
                )
            })?;
            if pinned.sha256 != expected_sha256 {
                bail!(
                    "redistribution decision checksum mismatch for {}: inventory has {}, decision pins {}",
                    artifact.id,
                    expected_sha256,
                    pinned.sha256
                );
            }
            let authorized: BTreeSet<&str> = pinned
                .approved_channels
                .iter()
                .map(String::as_str)
                .collect();
            for channel in &artifact.channels {
                if !authorized.contains(channel.as_str()) {
                    bail!(
                        "redistribution decision does not authorize channel {} for artifact {}",
                        channel,
                        artifact.id
                    );
                }
            }
        }
        Ok(())
    }

    /// Check authorization of a staged product on its intended future channels,
    /// even while its inventory entry correctly says `distributed = false`.
    pub(crate) fn require_staged_channels(
        &self,
        id: &str,
        sha256: &str,
        channels: &[&str],
    ) -> Result<()> {
        let artifact = self
            .inventory
            .find(id)
            .with_context(|| format!("staged runtime artifact {id} is missing from inventory"))?;
        if artifact.sha256.as_deref() != Some(sha256) {
            bail!("staged runtime inventory checksum mismatch for {id}");
        }
        let pinned = self
            .decision
            .pinned_artifacts
            .iter()
            .find(|p| p.id == id)
            .with_context(|| {
                format!("redistribution decision does not pin staged runtime artifact {id}")
            })?;
        if pinned.sha256 != sha256 {
            bail!("staged runtime decision checksum mismatch for {id}");
        }
        for channel in &artifact.planned_channels {
            if !pinned.approved_channels.contains(channel) {
                bail!("redistribution decision does not authorize channel {channel} for staged runtime artifact {id}");
            }
        }
        for channel in channels {
            if !pinned.approved_channels.iter().any(|c| c == channel) {
                bail!("redistribution decision does not authorize channel {channel} for staged runtime artifact {id}");
            }
        }
        Ok(())
    }

    /// Derive the only allowed mutable inventory from the frozen approval.
    /// Registration is not publication: actual channels are recorded separately,
    /// after uploads, and never broaden the reviewer's intended scope.
    pub(crate) fn production_inventory(
        &self,
        registry_sha256: &str,
        published_channels: &[String],
    ) -> Result<ArtifactInventory> {
        let mut inventory = self.inventory.clone();
        for id in COMPACT_IDS {
            let artifact = inventory
                .artifacts
                .iter_mut()
                .find(|a| a.id == id)
                .with_context(|| format!("compact approval inventory is missing {id}"))?;
            if artifact.lifecycle != Some(ArtifactLifecycle::Staged) || artifact.distributed {
                bail!("compact approval snapshot must describe staged, undistributed artifacts");
            }
            let sha = artifact
                .sha256
                .as_deref()
                .context("compact approval needs checksum")?;
            self.require_staged_channels(id, sha, &REQUIRED_RUNTIME_CHANNELS)?;
            if REQUIRED_RUNTIME_CHANNELS
                .iter()
                .any(|c| !artifact.planned_channels.iter().any(|p| p == c))
            {
                bail!("compact approval inventory does not plan all required runtime channels");
            }
            if published_channels
                .iter()
                .any(|c| !artifact.planned_channels.contains(c))
            {
                bail!("publication channel is outside the immutable approved plan for {id}");
            }
            artifact.channels = published_channels.to_vec();
            artifact.channels.sort();
            artifact.channels.dedup();
            artifact.distributed = !artifact.channels.is_empty();
            artifact.lifecycle = Some(if artifact.distributed {
                ArtifactLifecycle::Published
            } else {
                ArtifactLifecycle::ProductionRegistered
            });
        }
        if let Some(registry) = inventory
            .artifacts
            .iter_mut()
            .find(|a| a.id == "starlight-manifest-toml")
        {
            registry.sha256 = Some(registry_sha256.into());
        }
        inventory.validate()?;
        Ok(inventory)
    }

    pub(crate) fn verify_current_inventory(
        &self,
        path: &Path,
        registered: bool,
        registry_sha256: &str,
    ) -> Result<Vec<String>> {
        let current: ArtifactInventory = toml::from_str(&fs::read_to_string(path)?)?;
        current.validate()?;
        let channels = current
            .find(COMPACT_IDS[0])
            .context("current inventory lacks compact map")?
            .channels
            .clone();
        let expected = if registered {
            self.production_inventory(registry_sha256, &channels)?
        } else {
            self.inventory.clone()
        };
        if current != expected {
            bail!("current publication inventory disagrees with immutable approval or production registry");
        }
        Ok(channels)
    }

    pub fn verify_conditions(&self, evidence: &ConditionEvidence<'_>) -> Result<()> {
        if self.decision.conditions.is_empty() {
            return Ok(());
        }
        verify_approved_with_conditions(&self.decision.conditions, evidence)
            .context("redistribution approved_with_conditions is not machine-satisfied")
    }
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["placeholder", "todo", "tbd", "unknown", "unspecified"]
            .iter()
            .any(|marker| normalized == *marker || normalized.contains(&format!("<{marker}>")))
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

fn require_identifier(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("{label} must be an ASCII identifier");
    }
    Ok(())
}

fn require_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} SHA-256 must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}

/// Minimal structural check for an RFC3339 UTC timestamp such as
/// `2026-07-30T21:00:00Z`. This intentionally does not validate calendar
/// correctness (days-in-month, leap seconds); it only fails closed on
/// missing, non-ASCII, or obviously malformed values.
fn require_rfc3339_utc(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    let shape_is_valid = || -> bool {
        if !value.is_ascii() || value.len() < 20 {
            return false;
        }
        let bytes = value.as_bytes();
        let separators_ok = bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes[10] == b'T'
            && bytes[13] == b':'
            && bytes[16] == b':';
        if !separators_ok {
            return false;
        }
        let numeric_ranges = [0..4usize, 5..7, 8..10, 11..13, 14..16, 17..19];
        if !numeric_ranges.iter().all(|range| {
            value[range.clone()]
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        }) {
            return false;
        }
        value.ends_with('Z') || value[19..].contains('+') || value[19..].contains('-')
    };
    if !shape_is_valid() {
        bail!("{label} must be an RFC3339 UTC timestamp");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn synthetic_artifact(id: &str, distributed: bool, sha256: Option<&str>) -> InventoryArtifact {
        InventoryArtifact {
            id: id.to_string(),
            category: "synthetic-test-category".to_string(),
            source: "SYNTHETIC-NON-PRODUCTION test fixture source".to_string(),
            release: "synthetic-test-release".to_string(),
            license: "synthetic-test-license".to_string(),
            license_url: None,
            sha256: sha256.map(str::to_string),
            superseded_by_sha256: None,
            distribution_class: if distributed {
                DistributionClass::RepositoryEmbedded
            } else {
                DistributionClass::DownloadOnly
            },
            distributed,
            channels: if distributed {
                vec!["git_repository".to_string()]
            } else {
                Vec::new()
            },
            planned_channels: Vec::new(),
            lifecycle: None,
            tracking_issue: None,
            notes: "synthetic-test-notes: fixture data only, not a real artifact".to_string(),
        }
    }

    fn synthetic_inventory() -> ArtifactInventory {
        ArtifactInventory {
            schema_version: INVENTORY_SCHEMA_VERSION,
            artifacts: vec![
                synthetic_artifact("synthetic-upstream-input", false, None),
                synthetic_artifact("synthetic-distributed-output", true, Some(&"a".repeat(64))),
            ],
        }
    }

    fn write_inventory(
        dir: &TempDir,
        inventory: &ArtifactInventory,
    ) -> (std::path::PathBuf, String) {
        let bytes = toml::to_string_pretty(inventory).unwrap().into_bytes();
        let path = dir.path().join("artifact-inventory-v1.toml");
        fs::write(&path, &bytes).unwrap();
        (path, checksum_io::sha256_bytes(&bytes))
    }

    fn approved_decision(inventory_sha256: &str) -> RedistributionReviewDecision {
        RedistributionReviewDecision {
            schema_version: DECISION_SCHEMA_VERSION,
            decision: RedistributionDecision::Approved,
            reviewer_name: Some("Synthetic Test Reviewer".to_string()),
            reviewer_role: Some("synthetic-test-role".to_string()),
            reviewed_at_utc: Some("2026-01-01T00:00:00Z".to_string()),
            candidate_sha256: "c".repeat(64),
            inventory_sha256: inventory_sha256.to_string(),
            review_bundle_sha256: "d".repeat(64),
            pinned_artifacts: vec![PinnedArtifactApproval {
                id: "synthetic-distributed-output".to_string(),
                sha256: "a".repeat(64),
                approved_channels: vec!["git_repository".to_string()],
            }],
            conditions: Vec::new(),
            restrictions: Vec::new(),
            notes: "synthetic-test-notes: fixture decision, not a real approval".to_string(),
        }
    }

    fn write_decision(
        dir: &TempDir,
        decision: &RedistributionReviewDecision,
    ) -> std::path::PathBuf {
        let path = dir.path().join("redistribution-review-decision-v1.json");
        fs::write(&path, serde_json::to_vec_pretty(decision).unwrap()).unwrap();
        path
    }

    #[test]
    fn inventory_rejects_unknown_schema_version_and_duplicate_ids() {
        let mut inventory = synthetic_inventory();
        inventory.schema_version = 999;
        assert!(inventory.validate().is_err());

        let mut inventory = synthetic_inventory();
        inventory
            .artifacts
            .push(synthetic_artifact("synthetic-upstream-input", false, None));
        assert!(inventory.validate().is_err());
    }

    #[test]
    fn inventory_rejects_distributed_without_channel_or_checksum() {
        let mut inventory = synthetic_inventory();
        inventory.artifacts[1].channels.clear();
        assert!(inventory.validate().is_err());

        let mut inventory = synthetic_inventory();
        inventory.artifacts[1].sha256 = None;
        assert!(inventory.validate().is_err());

        let mut inventory = synthetic_inventory();
        inventory.artifacts[0]
            .channels
            .push("git_repository".to_string());
        assert!(inventory.validate().is_err());
    }

    #[test]
    fn approved_synthetic_decision_passes_the_fail_closed_gate() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let decision_path = write_decision(&dir, &approved_decision(&inventory_sha256));

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        review.require_approved(&"c".repeat(64)).unwrap();
    }

    #[test]
    fn staged_channels_require_inventory_identity_and_explicit_future_approval() {
        let dir = TempDir::new().unwrap();
        let mut inventory = synthetic_inventory();
        inventory.artifacts[0].sha256 = Some("b".repeat(64));
        let (path, sha) = write_inventory(&dir, &inventory);
        let decision = write_decision(&dir, &approved_decision(&sha));
        let mut review = RedistributionReview::load(&path, &decision).unwrap();
        assert!(review
            .require_staged_channels("absent", &"b".repeat(64), &[])
            .is_err());
        let id = "synthetic-upstream-input";
        assert!(review
            .require_staged_channels(id, &"a".repeat(64), &[])
            .is_err());
        assert!(review
            .require_staged_channels(id, &"b".repeat(64), &[])
            .is_err());
        review
            .decision
            .pinned_artifacts
            .push(PinnedArtifactApproval {
                id: id.into(),
                sha256: "a".repeat(64),
                approved_channels: vec!["git_repository".into()],
            });
        assert!(review
            .require_staged_channels(id, &"b".repeat(64), &[])
            .is_err());
        review.decision.pinned_artifacts.last_mut().unwrap().sha256 = "b".repeat(64);
        assert!(review
            .require_staged_channels(id, &"b".repeat(64), &["crates_io"])
            .is_err());
        review
            .decision
            .pinned_artifacts
            .last_mut()
            .unwrap()
            .approved_channels
            .push("crates_io".into());
        review
            .require_staged_channels(id, &"b".repeat(64), &["git_repository", "crates_io"])
            .unwrap();
        assert!(!review.inventory.artifacts[0].distributed);
    }

    #[test]
    fn pending_decision_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.decision = RedistributionDecision::Pending;
        decision.reviewer_name = None;
        decision.reviewer_role = None;
        decision.reviewed_at_utc = None;
        decision.pinned_artifacts.clear();
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(error.contains("pending"), "unexpected error: {error}");
    }

    #[test]
    fn missing_reviewer_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.reviewer_name = None;
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("authorized reviewer"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn inventory_checksum_mismatch_is_rejected_at_load() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, _real_sha256) = write_inventory(&dir, &inventory);
        let decision_path = write_decision(&dir, &approved_decision(&"0".repeat(64)));

        let error = RedistributionReview::load(&inventory_path, &decision_path)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("checksum mismatch"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn pinned_artifact_checksum_mismatch_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.pinned_artifacts[0].sha256 = "b".repeat(64);
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("checksum mismatch"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn unauthorized_channel_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.pinned_artifacts[0].approved_channels = vec!["some_other_channel".to_string()];
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not authorize channel"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn missing_pinned_artifact_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.pinned_artifacts.clear();
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("does not pin distributed artifact"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejected_decision_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.decision = RedistributionDecision::Rejected;
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(error.contains("rejected"), "unexpected error: {error}");
    }

    #[test]
    fn approved_with_conditions_requires_at_least_one_condition() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.decision = RedistributionDecision::ApprovedWithConditions;
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(error.contains("condition"), "unexpected error: {error}");

        let mut decision = approved_decision(&inventory_sha256);
        decision.decision = RedistributionDecision::ApprovedWithConditions;
        decision.conditions = vec![crate::starlight::conditions::ReviewCondition::FreeText(
            "synthetic-test-condition".to_string(),
        )];
        let decision_path = write_decision(&dir, &decision);
        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        review.require_approved(&"c".repeat(64)).unwrap();
    }

    #[test]
    fn malformed_review_timestamp_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&inventory_sha256);
        decision.reviewed_at_utc = Some("not-a-timestamp".to_string());
        let decision_path = write_decision(&dir, &decision);

        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"c".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(error.contains("timestamp"), "unexpected error: {error}");
    }

    #[test]
    fn candidate_mismatch_is_rejected() {
        let dir = TempDir::new().unwrap();
        let inventory = synthetic_inventory();
        let (inventory_path, inventory_sha256) = write_inventory(&dir, &inventory);
        let decision_path = write_decision(&dir, &approved_decision(&inventory_sha256));
        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        let error = review
            .require_approved(&"b".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("pins candidate"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn placeholder_text_fails_closed() {
        let mut artifact = synthetic_artifact("synthetic-placeholder-check", false, None);
        artifact.license = "TODO".to_string();
        assert!(artifact.validate().is_err());

        artifact.license = "synthetic-test-license".to_string();
        artifact.notes = "unspecified".to_string();
        assert!(artifact.validate().is_err());
    }

    #[test]
    fn documentation_reference_uses_the_same_distributed_checksum_rule() {
        let mut artifact = synthetic_artifact("documentation-contract", true, None);
        artifact.distribution_class = DistributionClass::DocumentationReference;
        assert!(artifact.validate().is_err());
        artifact.sha256 = Some("a".repeat(64));
        artifact.validate().unwrap();
    }

    #[test]
    fn actual_inventory_accepts_complete_fixture_approvals_and_rejects_bad_pins() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = TempDir::new().unwrap();
        let inventory: ArtifactInventory =
            toml::from_str(&fs::read_to_string(root.join(APPROVAL_INVENTORY)).unwrap()).unwrap();
        let (path, sha) = write_inventory(&dir, &inventory);
        let mut decision = approved_decision(&sha);
        decision.pinned_artifacts = inventory
            .artifacts
            .iter()
            .filter(|a| a.distributed || !a.planned_channels.is_empty())
            .map(|a| PinnedArtifactApproval {
                id: a.id.clone(),
                sha256: a.sha256.clone().unwrap(),
                approved_channels: a
                    .channels
                    .iter()
                    .chain(&a.planned_channels)
                    .cloned()
                    .collect(),
            })
            .collect();
        for disposition in [
            RedistributionDecision::Approved,
            RedistributionDecision::ApprovedWithConditions,
        ] {
            let mut valid = decision.clone();
            valid.decision = disposition;
            if disposition == RedistributionDecision::ApprovedWithConditions {
                valid.conditions = vec![crate::starlight::conditions::ReviewCondition::FreeText(
                    "fixture shape only; promotion separately verifies conditions".into(),
                )];
            }
            let decision_path = write_decision(&dir, &valid);
            let review = RedistributionReview::load(&path, &decision_path).unwrap();
            review.require_approved(&"c".repeat(64)).unwrap();
        }
        let doc = "starlight-external-manifest-sidecar-contract";
        let doc_index = decision
            .pinned_artifacts
            .iter()
            .position(|a| a.id == doc)
            .unwrap();
        for case in 0..4 {
            let mut invalid = decision.clone();
            match case {
                0 => {
                    invalid.pinned_artifacts.remove(doc_index);
                }
                1 => invalid.pinned_artifacts[doc_index].sha256 = "0".repeat(64),
                2 => {
                    invalid.pinned_artifacts[doc_index].id =
                        "different-documentation-identity".into()
                }
                _ => {
                    invalid.pinned_artifacts[doc_index].approved_channels = vec!["crates_io".into()]
                }
            }
            let decision_path = write_decision(&dir, &invalid);
            assert!(RedistributionReview::load(&path, &decision_path)
                .unwrap()
                .require_approved(&"c".repeat(64))
                .is_err());
        }
        let mut missing_digest = inventory.clone();
        missing_digest
            .artifacts
            .iter_mut()
            .find(|a| a.id == doc)
            .unwrap()
            .sha256 = None;
        assert!(missing_digest.validate().is_err());
        assert_eq!(
            inventory.find(doc).unwrap().sha256.as_deref(),
            Some(
                checksum_io::sha256_file(
                    &root.join("docs/nsb_components/starlight/external-manifest.md")
                )
                .unwrap()
                .as_str()
            )
        );
    }

    #[test]
    fn real_repository_inventory_and_pending_template_load_and_stay_pending() {
        // Exercises the actual checked-in files as an integration smoke test:
        // the pending template must load cleanly but must never satisfy the
        // fail-closed promotion gate.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repository_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .expect("crate lives two levels below the repository root");
        let inventory_path = repository_root.join(APPROVAL_INVENTORY);
        let decision_path = repository_root
            .join("docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json");
        if !inventory_path.exists() || !decision_path.exists() {
            // The Rust module can be exercised independently of the docs
            // tree (e.g. a sparse checkout); do not fail the unit test suite
            // for an unrelated checkout shape.
            return;
        }
        let review = RedistributionReview::load(&inventory_path, &decision_path).unwrap();
        assert_eq!(review.decision().decision, RedistributionDecision::Pending);
        assert!(review.require_approved(&"c".repeat(64)).is_err());
    }

    #[test]
    fn restricted_gaia_xp_and_calspec_inputs_must_not_be_distributed() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repository_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .expect("crate lives two levels below the repository root");
        let inventory_path = repository_root
            .join("docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml");
        if !inventory_path.exists() {
            return;
        }
        let bytes = fs::read(&inventory_path).unwrap();
        let inventory: ArtifactInventory =
            toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        inventory.validate().unwrap();
        for artifact in &inventory.artifacts {
            if matches!(
                artifact.id.as_str(),
                "gaia-source-bulk" | "gaia-xp-continuous-bulk" | "calspec-reference-spectra"
            ) {
                assert!(
                    !artifact.distributed,
                    "{} must never be distributed",
                    artifact.id
                );
                assert_ne!(
                    artifact.distribution_class,
                    DistributionClass::RepositoryEmbedded
                );
            }
        }
        let data = repository_root.join("crates/nsb/data");
        for entry in fs::read_dir(&data).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(
                !name.starts_with("GaiaSource_")
                    && !name.starts_with("XpContinuousMeanSpectrum_")
                    && !name.ends_with(".fits")
                    && !name.ends_with(".hdf5"),
                "restricted input {name} must not be in crates/nsb/data"
            );
        }
    }
}
