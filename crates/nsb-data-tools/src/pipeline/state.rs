//! Evidence-driven cache and partition state transitions.

use super::contracts::{
    PartitionCompletion, ProcessingMode, RowSelection, PIPELINE_SCHEMA_VERSION,
};
use crate::checksum_io::Checksum;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Persisted lifecycle state for one immutable input partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheInputState {
    /// Partition exists only in the official plan.
    Planned,
    /// Download is in progress or has been interrupted.
    Downloading,
    /// Final download file exists but its official checksum is not yet proven.
    Downloaded,
    /// Official input checksum has been recomputed and matched.
    ChecksumVerified,
    /// Scientific processing is in progress or resumable.
    Processing,
    /// Processing emitted a durable candidate output.
    Processed,
    /// Candidate output checksum and structure have been verified.
    OutputVerified,
    /// Partition counts and accumulator have reconciled into a durable manifest.
    Reconciled,
    /// Source input may be deleted according to cleanup policy.
    Releasable,
    /// Source input has been deleted after durable promotion.
    Deleted,
    /// An operation failed and requires explicit recovery.
    Failed,
}

/// Deterministic action selected when resuming from a persisted state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    /// Start or resume acquisition.
    Acquire,
    /// Recompute and compare the official checksum.
    VerifyInputChecksum,
    /// Start or resume scientific processing.
    Process,
    /// Verify the durably written output.
    VerifyOutput,
    /// Reconcile the partition manifest and aggregate state.
    Reconcile,
    /// Check production evidence and authorize release.
    AuthorizeRelease,
    /// Cleanup is optional and may be performed safely.
    CleanupOptional,
    /// Partition lifecycle is complete.
    Complete,
    /// Inspect failure evidence before choosing a retry point.
    InspectFailure,
}

/// Evidence required to perform a state transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionEvidence {
    /// Acquisition was intentionally started.
    DownloadStarted,
    /// Download completed with the given non-zero durable size.
    DownloadCompleted { bytes: u64 },
    /// Official checksum and recomputed checksum matched.
    ChecksumMatched {
        /// Checksum from the authoritative inventory.
        expected: Checksum,
        /// Checksum recomputed from the local file.
        actual: Checksum,
    },
    /// Processing started with explicit intent and row coverage.
    ProcessingStarted {
        /// Pilot, candidate, or production intent.
        mode: ProcessingMode,
        /// Full-partition or bounded row selection.
        row_selection: RowSelection,
    },
    /// Processing completed with explicit completeness evidence.
    ProcessingCompleted {
        /// Partial or complete coverage evidence.
        completion: PartitionCompletion,
    },
    /// Output verification produced a stable checksum.
    OutputVerified {
        /// Checksum of the verified durable output.
        checksum: Checksum,
    },
    /// Reconciliation committed a stable manifest.
    Reconciled {
        /// Checksum of the reconciliation manifest.
        manifest_checksum: Checksum,
    },
    /// All production release preconditions were independently checked.
    ReleaseAuthorized,
    /// Cleanup deleted the source input.
    Deleted,
    /// Operation failed with a non-empty diagnostic.
    Failed { reason: String },
    /// Retry acquisition after a recorded failure.
    RetryDownload,
    /// Retry processing after a recorded failure with verified input evidence.
    RetryProcessing,
}

/// Strict, versioned state record for one partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionState {
    /// Contract schema version.
    pub schema_version: u32,
    /// Immutable official partition identifier.
    pub partition_id: String,
    /// Current lifecycle state.
    pub state: CacheInputState,
    /// Durable source file size after download.
    pub input_bytes: Option<u64>,
    /// Verified official input checksum.
    pub input_checksum: Option<Checksum>,
    /// Processing intent selected for this attempt.
    pub processing_mode: Option<ProcessingMode>,
    /// Explicit row selection selected for this attempt.
    pub row_selection: Option<RowSelection>,
    /// Partial or complete processing evidence.
    pub completion: Option<PartitionCompletion>,
    /// Verified output checksum.
    pub output_checksum: Option<Checksum>,
    /// Reconciliation manifest checksum.
    pub reconciliation_checksum: Option<Checksum>,
    /// Most recent failure diagnostic.
    pub last_error: Option<String>,
}

impl PartitionState {
    /// Create the planned state for one official partition.
    pub fn planned(partition_id: impl Into<String>) -> Result<Self> {
        let state = Self {
            schema_version: PIPELINE_SCHEMA_VERSION,
            partition_id: partition_id.into(),
            state: CacheInputState::Planned,
            input_bytes: None,
            input_checksum: None,
            processing_mode: None,
            row_selection: None,
            completion: None,
            output_checksum: None,
            reconciliation_checksum: None,
            last_error: None,
        };
        state.validate()?;
        Ok(state)
    }

    /// Apply one legal transition. Validation happens on a clone so rejected
    /// transitions cannot partially mutate persisted state.
    pub fn transition(
        &mut self,
        next: CacheInputState,
        evidence: TransitionEvidence,
    ) -> Result<()> {
        let mut updated = self.clone();
        match (self.state, next, evidence) {
            (
                CacheInputState::Planned,
                CacheInputState::Downloading,
                TransitionEvidence::DownloadStarted,
            ) => {
                updated.last_error = None;
            }
            (
                CacheInputState::Downloading,
                CacheInputState::Downloaded,
                TransitionEvidence::DownloadCompleted { bytes },
            ) => {
                if bytes == 0 {
                    bail!("downloaded partition must have non-zero size");
                }
                updated.input_bytes = Some(bytes);
                updated.last_error = None;
            }
            (
                CacheInputState::Downloaded,
                CacheInputState::ChecksumVerified,
                TransitionEvidence::ChecksumMatched { expected, actual },
            ) => {
                if expected != actual {
                    bail!("official input checksum did not match recomputed checksum");
                }
                updated.input_checksum = Some(actual);
                updated.last_error = None;
            }
            (
                CacheInputState::ChecksumVerified,
                CacheInputState::Processing,
                TransitionEvidence::ProcessingStarted {
                    mode,
                    row_selection,
                },
            ) => {
                row_selection.validate()?;
                updated.processing_mode = Some(mode);
                updated.row_selection = Some(row_selection);
                updated.completion = None;
                updated.output_checksum = None;
                updated.reconciliation_checksum = None;
                updated.last_error = None;
            }
            (
                CacheInputState::Processing,
                CacheInputState::Processed,
                TransitionEvidence::ProcessingCompleted { completion },
            ) => {
                let selection = updated
                    .row_selection
                    .ok_or_else(|| anyhow::anyhow!("processing state is missing row selection"))?;
                completion.validate_for(selection)?;
                updated.completion = Some(completion);
                updated.last_error = None;
            }
            (
                CacheInputState::Processed,
                CacheInputState::OutputVerified,
                TransitionEvidence::OutputVerified { checksum },
            ) => {
                updated.output_checksum = Some(checksum);
                updated.last_error = None;
            }
            (
                CacheInputState::OutputVerified,
                CacheInputState::Reconciled,
                TransitionEvidence::Reconciled { manifest_checksum },
            ) => {
                updated.reconciliation_checksum = Some(manifest_checksum);
                updated.last_error = None;
            }
            (
                CacheInputState::Reconciled,
                CacheInputState::Releasable,
                TransitionEvidence::ReleaseAuthorized,
            ) => {
                updated.ensure_release_evidence()?;
                updated.last_error = None;
            }
            (
                CacheInputState::Releasable,
                CacheInputState::Deleted,
                TransitionEvidence::Deleted,
            ) => {
                updated.last_error = None;
            }
            (
                current,
                CacheInputState::Failed,
                TransitionEvidence::Failed { reason },
            ) if current != CacheInputState::Deleted => {
                if reason.trim().is_empty() {
                    bail!("failed transition requires a non-empty reason");
                }
                updated.last_error = Some(reason);
            }
            (
                CacheInputState::Failed,
                CacheInputState::Downloading,
                TransitionEvidence::RetryDownload,
            ) => {
                updated.input_bytes = None;
                updated.input_checksum = None;
                updated.processing_mode = None;
                updated.row_selection = None;
                updated.completion = None;
                updated.output_checksum = None;
                updated.reconciliation_checksum = None;
                updated.last_error = None;
            }
            (
                CacheInputState::Failed,
                CacheInputState::Processing,
                TransitionEvidence::RetryProcessing,
            ) => {
                if updated.input_checksum.is_none()
                    || updated.processing_mode.is_none()
                    || updated.row_selection.is_none()
                {
                    bail!("processing retry requires verified input and prior processing configuration");
                }
                updated.last_error = None;
            }
            (current, requested, _) => bail!(
                "illegal partition-state transition from {current:?} to {requested:?}"
            ),
        }
        updated.state = next;
        updated.validate()?;
        *self = updated;
        Ok(())
    }

    /// Recovery action for every persisted state.
    pub const fn resume_action(&self) -> ResumeAction {
        match self.state {
            CacheInputState::Planned | CacheInputState::Downloading => ResumeAction::Acquire,
            CacheInputState::Downloaded => ResumeAction::VerifyInputChecksum,
            CacheInputState::ChecksumVerified | CacheInputState::Processing => {
                ResumeAction::Process
            }
            CacheInputState::Processed => ResumeAction::VerifyOutput,
            CacheInputState::OutputVerified => ResumeAction::Reconcile,
            CacheInputState::Reconciled => ResumeAction::AuthorizeRelease,
            CacheInputState::Releasable => ResumeAction::CleanupOptional,
            CacheInputState::Deleted => ResumeAction::Complete,
            CacheInputState::Failed => ResumeAction::InspectFailure,
        }
    }

    /// Validate persisted invariants for the current state.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PIPELINE_SCHEMA_VERSION {
            bail!(
                "unsupported partition-state schema version {}; expected {}",
                self.schema_version,
                PIPELINE_SCHEMA_VERSION
            );
        }
        if self.partition_id.trim().is_empty() {
            bail!("partition state requires a non-empty partition_id");
        }
        if self.input_bytes == Some(0) {
            bail!("persisted input size must be non-zero");
        }
        if self.last_error.as_ref().is_some_and(|value| value.trim().is_empty()) {
            bail!("persisted last_error must be non-empty");
        }
        if self.state == CacheInputState::Failed && self.last_error.is_none() {
            bail!("failed state requires error evidence");
        }
        if let Some(selection) = self.row_selection {
            selection.validate()?;
        }
        if let Some(completion) = self.completion {
            let selection = self
                .row_selection
                .ok_or_else(|| anyhow::anyhow!("completion evidence requires row selection"))?;
            completion.validate_for(selection)?;
        }
        if self.requires_input_checksum() && self.input_checksum.is_none() {
            bail!("state {:?} requires verified input checksum evidence", self.state);
        }
        if self.requires_processing_configuration()
            && (self.processing_mode.is_none() || self.row_selection.is_none())
        {
            bail!("state {:?} requires processing mode and row selection", self.state);
        }
        if self.requires_completion() && self.completion.is_none() {
            bail!("state {:?} requires completion evidence", self.state);
        }
        if self.requires_output_checksum() && self.output_checksum.is_none() {
            bail!("state {:?} requires output checksum evidence", self.state);
        }
        if self.requires_reconciliation() && self.reconciliation_checksum.is_none() {
            bail!("state {:?} requires reconciliation evidence", self.state);
        }
        if matches!(self.state, CacheInputState::Releasable | CacheInputState::Deleted) {
            self.ensure_release_evidence()?;
        }
        Ok(())
    }

    fn ensure_release_evidence(&self) -> Result<()> {
        if self.processing_mode != Some(ProcessingMode::Production) {
            bail!("only production processing can authorize source release");
        }
        if self.row_selection != Some(RowSelection::FullPartition) {
            bail!("bounded or sampled processing cannot authorize source release");
        }
        if !self.completion.is_some_and(PartitionCompletion::is_complete) {
            bail!("source release requires complete partition processing evidence");
        }
        if self.input_checksum.is_none()
            || self.output_checksum.is_none()
            || self.reconciliation_checksum.is_none()
        {
            bail!("source release requires input, output, and reconciliation checksums");
        }
        Ok(())
    }

    const fn requires_input_checksum(&self) -> bool {
        matches!(
            self.state,
            CacheInputState::ChecksumVerified
                | CacheInputState::Processing
                | CacheInputState::Processed
                | CacheInputState::OutputVerified
                | CacheInputState::Reconciled
                | CacheInputState::Releasable
                | CacheInputState::Deleted
        )
    }

    const fn requires_processing_configuration(&self) -> bool {
        matches!(
            self.state,
            CacheInputState::Processing
                | CacheInputState::Processed
                | CacheInputState::OutputVerified
                | CacheInputState::Reconciled
                | CacheInputState::Releasable
                | CacheInputState::Deleted
        )
    }

    const fn requires_completion(&self) -> bool {
        matches!(
            self.state,
            CacheInputState::Processed
                | CacheInputState::OutputVerified
                | CacheInputState::Reconciled
                | CacheInputState::Releasable
                | CacheInputState::Deleted
        )
    }

    const fn requires_output_checksum(&self) -> bool {
        matches!(
            self.state,
            CacheInputState::OutputVerified
                | CacheInputState::Reconciled
                | CacheInputState::Releasable
                | CacheInputState::Deleted
        )
    }

    const fn requires_reconciliation(&self) -> bool {
        matches!(
            self.state,
            CacheInputState::Reconciled
                | CacheInputState::Releasable
                | CacheInputState::Deleted
        )
    }
}
