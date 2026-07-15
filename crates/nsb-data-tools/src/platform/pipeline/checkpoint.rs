//! Compact, partition-oriented checkpoints with bounded diagnostics.

use super::contracts::{
    PartitionCompletion, ProcessingMode, RowSelection, PIPELINE_SCHEMA_VERSION,
};
use crate::platform::artifact_io;
use crate::platform::checksum_io::Checksum;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Maximum diagnostic samples retained in a production checkpoint.
pub const MAX_DIAGNOSTIC_SAMPLES: usize = 32;
const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 1024;

/// Bounded representative failure or exclusion detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSample {
    /// Absolute row offset associated with the sample.
    pub row_offset: u64,
    /// Stable diagnostic category.
    pub category: String,
    /// Human-readable detail bounded for checkpoint scalability.
    pub message: String,
}

impl DiagnosticSample {
    /// Construct and validate a diagnostic sample.
    pub fn new(
        row_offset: u64,
        category: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<Self> {
        let sample = Self {
            row_offset,
            category: category.into(),
            message: message.into(),
        };
        sample.validate()?;
        Ok(sample)
    }

    /// Validate category and message bounds.
    pub fn validate(&self) -> Result<()> {
        if self.category.trim().is_empty() || self.message.trim().is_empty() {
            bail!("diagnostic category and message must be non-empty");
        }
        if self.category.len() > MAX_DIAGNOSTIC_TEXT_BYTES
            || self.message.len() > MAX_DIAGNOSTIC_TEXT_BYTES
        {
            bail!("diagnostic category and message must not exceed 1024 bytes");
        }
        Ok(())
    }
}

/// Versioned production checkpoint whose size is bounded by partition metadata,
/// HEALPix state references, aggregate counters, and representative diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionCheckpoint {
    /// Contract schema version.
    pub schema_version: u32,
    /// Immutable official partition identifier.
    pub partition_id: String,
    /// Processing intent.
    pub mode: ProcessingMode,
    /// Explicit full or bounded row selection.
    pub row_selection: RowSelection,
    /// Absolute row offset at which resume continues.
    pub next_row_offset: u64,
    /// Aggregate number of rows scanned.
    pub rows_scanned: u64,
    /// Aggregate accepted rows.
    pub rows_valid: u64,
    /// Aggregate scientifically excluded rows.
    pub rows_excluded: u64,
    /// Aggregate rows that failed unexpectedly.
    pub rows_failed: u64,
    /// Rolling checksum for the input prefix represented by this checkpoint.
    pub rolling_input_checksum: Option<Checksum>,
    /// Checksum of the durably persisted HEALPix accumulator, when available.
    pub healpix_checksum: Option<Checksum>,
    /// Bounded representative diagnostic samples.
    pub diagnostics: Vec<DiagnosticSample>,
}

impl PartitionCheckpoint {
    /// Create an empty checkpoint for one immutable partition.
    pub fn new(
        partition_id: impl Into<String>,
        mode: ProcessingMode,
        row_selection: RowSelection,
    ) -> Result<Self> {
        let checkpoint = Self {
            schema_version: PIPELINE_SCHEMA_VERSION,
            partition_id: partition_id.into(),
            mode,
            row_selection,
            next_row_offset: 0,
            rows_scanned: 0,
            rows_valid: 0,
            rows_excluded: 0,
            rows_failed: 0,
            rolling_input_checksum: None,
            healpix_checksum: None,
            diagnostics: Vec::new(),
        };
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    /// Atomically accumulate one processed batch while retaining only a bounded
    /// set of representative diagnostics.
    pub fn record_batch<I>(
        &mut self,
        rows_scanned: u64,
        rows_valid: u64,
        rows_excluded: u64,
        rows_failed: u64,
        next_row_offset: u64,
        samples: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = DiagnosticSample>,
    {
        let classified = rows_valid
            .checked_add(rows_excluded)
            .and_then(|value| value.checked_add(rows_failed))
            .ok_or_else(|| anyhow::anyhow!("batch row counters overflowed"))?;
        if classified != rows_scanned {
            bail!("batch row accounting mismatch: scanned={rows_scanned}, classified={classified}");
        }
        if next_row_offset < self.next_row_offset || next_row_offset < rows_scanned {
            bail!("checkpoint row offset must advance monotonically");
        }

        let mut updated = self.clone();
        updated.rows_scanned = updated
            .rows_scanned
            .checked_add(rows_scanned)
            .ok_or_else(|| anyhow::anyhow!("checkpoint rows_scanned overflowed"))?;
        updated.rows_valid = updated
            .rows_valid
            .checked_add(rows_valid)
            .ok_or_else(|| anyhow::anyhow!("checkpoint rows_valid overflowed"))?;
        updated.rows_excluded = updated
            .rows_excluded
            .checked_add(rows_excluded)
            .ok_or_else(|| anyhow::anyhow!("checkpoint rows_excluded overflowed"))?;
        updated.rows_failed = updated
            .rows_failed
            .checked_add(rows_failed)
            .ok_or_else(|| anyhow::anyhow!("checkpoint rows_failed overflowed"))?;
        updated.next_row_offset = next_row_offset;
        for sample in samples {
            sample.validate()?;
            if updated.diagnostics.len() < MAX_DIAGNOSTIC_SAMPLES {
                updated.diagnostics.push(sample);
            }
        }
        updated.validate()?;
        *self = updated;
        Ok(())
    }

    /// Set the rolling input checksum after the represented prefix is flushed.
    pub fn set_rolling_input_checksum(&mut self, checksum: Checksum) {
        self.rolling_input_checksum = Some(checksum);
    }

    /// Set the checksum of a durably persisted HEALPix accumulator.
    pub fn set_healpix_checksum(&mut self, checksum: Checksum) {
        self.healpix_checksum = Some(checksum);
    }

    /// Produce completion evidence from an explicit end-of-partition signal.
    pub fn completion(&self, reached_end_of_partition: bool) -> Result<PartitionCompletion> {
        self.validate()?;
        let completion = if reached_end_of_partition && self.row_selection.is_full_partition() {
            PartitionCompletion::Complete {
                rows_processed: self.rows_scanned,
            }
        } else {
            PartitionCompletion::Partial {
                rows_processed: self.rows_scanned,
            }
        };
        completion.validate_for(self.row_selection)?;
        Ok(completion)
    }

    /// Strictly validate a checkpoint loaded from disk.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PIPELINE_SCHEMA_VERSION {
            bail!(
                "unsupported partition-checkpoint schema version {}; expected {}",
                self.schema_version,
                PIPELINE_SCHEMA_VERSION
            );
        }
        if self.partition_id.trim().is_empty() {
            bail!("partition checkpoint requires a non-empty partition_id");
        }
        self.row_selection.validate()?;
        let classified = self
            .rows_valid
            .checked_add(self.rows_excluded)
            .and_then(|value| value.checked_add(self.rows_failed))
            .ok_or_else(|| anyhow::anyhow!("checkpoint row counters overflowed"))?;
        if classified != self.rows_scanned {
            bail!(
                "checkpoint row accounting mismatch: scanned={}, classified={classified}",
                self.rows_scanned
            );
        }
        if self.next_row_offset < self.rows_scanned {
            bail!("checkpoint next_row_offset cannot precede rows_scanned");
        }
        if self.diagnostics.len() > MAX_DIAGNOSTIC_SAMPLES {
            bail!("checkpoint contains too many diagnostic samples");
        }
        for sample in &self.diagnostics {
            sample.validate()?;
        }
        Ok(())
    }

    /// Persist this checkpoint transactionally after validation.
    pub fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        artifact_io::write_json_atomic(path, self)
    }

    /// Load and strictly validate a checkpoint.
    pub fn read(path: &Path) -> Result<Self> {
        let checkpoint: Self = artifact_io::read_json(path)?;
        checkpoint.validate()?;
        Ok(checkpoint)
    }
}
