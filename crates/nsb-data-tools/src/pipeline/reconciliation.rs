//! Deterministic, strict reconciliation of completed production partitions.

use super::contracts::{
    PartitionCompletion, ProcessingMode, RowSelection, PIPELINE_SCHEMA_VERSION,
};
use crate::artifact_io;
use crate::checksum_io::{Checksum, ChecksumAlgorithm};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Durable evidence for one fully processed production partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionManifest {
    /// Contract schema version.
    pub schema_version: u32,
    /// Immutable official partition identifier.
    pub partition_id: String,
    /// Verified checksum of the official input.
    pub input_checksum: Checksum,
    /// Verified checksum of the durable partition output.
    pub output_checksum: Checksum,
    /// Verified checksum of the durable HEALPix contribution.
    pub healpix_checksum: Checksum,
    /// Processing intent that produced this partition.
    pub processing_mode: ProcessingMode,
    /// Row coverage requested by the processing run.
    pub row_selection: RowSelection,
    /// Durable end-of-partition evidence.
    pub completion: PartitionCompletion,
    /// Rows scanned from the official partition.
    pub rows_scanned: u64,
    /// Scientifically accepted rows.
    pub rows_valid: u64,
    /// Scientifically excluded rows.
    pub rows_excluded: u64,
    /// Unexpectedly failed rows.
    pub rows_failed: u64,
}

impl PartitionManifest {
    /// Strictly validate production completeness and row accounting.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PIPELINE_SCHEMA_VERSION {
            bail!(
                "unsupported partition-manifest schema version {}; expected {}",
                self.schema_version,
                PIPELINE_SCHEMA_VERSION
            );
        }
        if self.partition_id.trim().is_empty() {
            bail!("partition manifest requires a non-empty partition_id");
        }
        if self.processing_mode != ProcessingMode::Production {
            bail!("partition reconciliation accepts production manifests only");
        }
        if self.row_selection != RowSelection::FullPartition {
            bail!("partition reconciliation requires full-partition row selection");
        }
        self.completion.validate_for(self.row_selection)?;
        if !self.completion.is_complete() {
            bail!("partition reconciliation requires complete processing evidence");
        }
        if self.completion.rows_processed() != self.rows_scanned {
            bail!("completion row count does not match rows_scanned");
        }
        let classified = self
            .rows_valid
            .checked_add(self.rows_excluded)
            .and_then(|value| value.checked_add(self.rows_failed))
            .ok_or_else(|| anyhow::anyhow!("partition row counters overflowed"))?;
        if classified != self.rows_scanned {
            bail!(
                "partition row accounting mismatch: scanned={}, classified={classified}",
                self.rows_scanned
            );
        }
        if self.output_checksum.algorithm() != ChecksumAlgorithm::Sha256
            || self.healpix_checksum.algorithm() != ChecksumAlgorithm::Sha256
        {
            bail!("generated partition outputs require SHA-256 checksums");
        }
        Ok(())
    }
}

/// Canonically ordered reconciliation of complete production partitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationManifest {
    /// Contract schema version.
    pub schema_version: u32,
    /// Partition manifests sorted by immutable partition identifier.
    pub partitions: Vec<PartitionManifest>,
    /// Aggregate scanned rows.
    pub rows_scanned: u64,
    /// Aggregate valid rows.
    pub rows_valid: u64,
    /// Aggregate scientifically excluded rows.
    pub rows_excluded: u64,
    /// Aggregate unexpectedly failed rows.
    pub rows_failed: u64,
}

impl ReconciliationManifest {
    /// Build a canonical reconciliation independent of processing order.
    pub fn from_partitions(mut partitions: Vec<PartitionManifest>) -> Result<Self> {
        for partition in &partitions {
            partition.validate()?;
        }
        partitions.sort_by(|left, right| left.partition_id.cmp(&right.partition_id));
        if partitions
            .windows(2)
            .any(|pair| pair[0].partition_id == pair[1].partition_id)
        {
            bail!("reconciliation contains a duplicate partition_id");
        }

        let mut manifest = Self {
            schema_version: PIPELINE_SCHEMA_VERSION,
            partitions,
            rows_scanned: 0,
            rows_valid: 0,
            rows_excluded: 0,
            rows_failed: 0,
        };
        for partition in &manifest.partitions {
            manifest.rows_scanned = checked_sum(
                manifest.rows_scanned,
                partition.rows_scanned,
                "rows_scanned",
            )?;
            manifest.rows_valid =
                checked_sum(manifest.rows_valid, partition.rows_valid, "rows_valid")?;
            manifest.rows_excluded = checked_sum(
                manifest.rows_excluded,
                partition.rows_excluded,
                "rows_excluded",
            )?;
            manifest.rows_failed =
                checked_sum(manifest.rows_failed, partition.rows_failed, "rows_failed")?;
        }
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate canonical ordering, uniqueness, and aggregate arithmetic.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PIPELINE_SCHEMA_VERSION {
            bail!(
                "unsupported reconciliation schema version {}; expected {}",
                self.schema_version,
                PIPELINE_SCHEMA_VERSION
            );
        }
        for partition in &self.partitions {
            partition.validate()?;
        }
        if self
            .partitions
            .windows(2)
            .any(|pair| pair[0].partition_id >= pair[1].partition_id)
        {
            bail!("reconciliation partitions must be strictly ordered and unique");
        }
        let rebuilt = Self::from_validated_partitions(self.partitions.clone())?;
        if self.rows_scanned != rebuilt.rows_scanned
            || self.rows_valid != rebuilt.rows_valid
            || self.rows_excluded != rebuilt.rows_excluded
            || self.rows_failed != rebuilt.rows_failed
        {
            bail!("reconciliation aggregate counters do not match partition manifests");
        }
        Ok(())
    }

    /// Canonical JSON bytes suitable for deterministic hashing or comparison.
    pub fn canonical_json(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    /// Atomically persist the validated reconciliation manifest.
    pub fn write(&self, path: &Path) -> Result<()> {
        self.validate()?;
        artifact_io::write_json_atomic(path, self)
    }

    /// Read and strictly validate a persisted reconciliation manifest.
    pub fn read(path: &Path) -> Result<Self> {
        let manifest: Self = artifact_io::read_json(path)?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn from_validated_partitions(partitions: Vec<PartitionManifest>) -> Result<Self> {
        let mut rows_scanned = 0;
        let mut rows_valid = 0;
        let mut rows_excluded = 0;
        let mut rows_failed = 0;
        for partition in &partitions {
            rows_scanned = checked_sum(rows_scanned, partition.rows_scanned, "rows_scanned")?;
            rows_valid = checked_sum(rows_valid, partition.rows_valid, "rows_valid")?;
            rows_excluded =
                checked_sum(rows_excluded, partition.rows_excluded, "rows_excluded")?;
            rows_failed = checked_sum(rows_failed, partition.rows_failed, "rows_failed")?;
        }
        Ok(Self {
            schema_version: PIPELINE_SCHEMA_VERSION,
            partitions,
            rows_scanned,
            rows_valid,
            rows_excluded,
            rows_failed,
        })
    }
}

fn checked_sum(left: u64, right: u64, label: &str) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| anyhow::anyhow!("reconciliation {label} overflowed"))
}
