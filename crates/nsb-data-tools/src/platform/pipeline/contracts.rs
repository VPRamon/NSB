//! Versioned, strictly typed contracts shared by data-product stages.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Current schema version for persisted pipeline contracts.
pub const PIPELINE_SCHEMA_VERSION: u32 = 1;

/// Scientific and operational intent of a processing run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
    /// Small representative run that can never authorize production promotion.
    Pilot,
    /// Reproducible candidate generation without production cleanup authority.
    Candidate,
    /// Complete partition processing eligible for production admission.
    Production,
}

/// Explicit input-row selection without sentinel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "rows", rename_all = "snake_case")]
pub enum RowSelection {
    /// Process the complete partition.
    FullPartition,
    /// Process exactly the first non-zero number of rows.
    FirstRows(u64),
}

impl RowSelection {
    /// Construct a bounded row selection, rejecting zero rather than treating it
    /// as an ambiguous full-file sentinel.
    pub fn first_rows(rows: u64) -> Result<Self> {
        if rows == 0 {
            bail!("bounded row selection must contain at least one row");
        }
        Ok(Self::FirstRows(rows))
    }

    /// Optional numeric limit suitable for adapters that accept an absent limit
    /// for full-partition processing.
    pub const fn limit(self) -> Option<u64> {
        match self {
            Self::FullPartition => None,
            Self::FirstRows(rows) => Some(rows),
        }
    }

    /// Whether the selection represents the complete input partition.
    pub const fn is_full_partition(self) -> bool {
        matches!(self, Self::FullPartition)
    }

    /// Validate a value loaded from a persisted contract.
    pub fn validate(self) -> Result<()> {
        if matches!(self, Self::FirstRows(0)) {
            bail!("persisted bounded row selection must be non-zero");
        }
        Ok(())
    }
}

/// Evidence describing whether a partition was processed completely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartitionCompletion {
    /// The run processed only a strict prefix or sample of the partition.
    Partial {
        /// Number of rows observed by the run.
        rows_processed: u64,
    },
    /// The run reached the durable end of the partition.
    Complete {
        /// Number of rows observed by the run.
        rows_processed: u64,
    },
}

impl PartitionCompletion {
    /// Number of processed rows represented by this evidence.
    pub const fn rows_processed(self) -> u64 {
        match self {
            Self::Partial { rows_processed } | Self::Complete { rows_processed } => rows_processed,
        }
    }

    /// Whether processing covered the complete partition.
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    /// Validate completion evidence against the requested row selection.
    pub fn validate_for(self, selection: RowSelection) -> Result<()> {
        selection.validate()?;
        if self.rows_processed() == 0 {
            bail!("partition completion evidence must contain at least one processed row");
        }
        match (selection, self) {
            (RowSelection::FullPartition, Self::Complete { .. }) => Ok(()),
            (RowSelection::FullPartition, Self::Partial { .. }) => Ok(()),
            (RowSelection::FirstRows(limit), Self::Partial { rows_processed })
                if rows_processed <= limit =>
            {
                Ok(())
            }
            (RowSelection::FirstRows(_), Self::Complete { .. }) => {
                bail!("a bounded row selection cannot prove complete partition processing")
            }
            (RowSelection::FirstRows(limit), Self::Partial { rows_processed }) => bail!(
                "partial completion processed {rows_processed} rows beyond configured limit {limit}"
            ),
        }
    }
}

/// Outcome of one validation or readiness gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "detail", rename_all = "snake_case")]
pub enum GateStatus {
    /// The gate executed and passed.
    Passed,
    /// The gate executed and failed with a non-empty reason.
    Failed(String),
    /// The gate did not execute and therefore cannot count as passed.
    NotRun(String),
    /// The gate is explicitly outside the current operation.
    NotApplicable(String),
}

impl GateStatus {
    /// Whether this status is an executed pass.
    pub const fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Human-readable blocker reason when the gate is not an executed pass.
    pub fn blocker_reason(&self) -> Option<&str> {
        match self {
            Self::Passed => None,
            Self::Failed(reason) | Self::NotRun(reason) | Self::NotApplicable(reason) => {
                Some(reason)
            }
        }
    }

    /// Validate persisted status details.
    pub fn validate(&self) -> Result<()> {
        if self
            .blocker_reason()
            .is_some_and(|reason| reason.trim().is_empty())
        {
            bail!("non-passing gate status requires a non-empty reason");
        }
        Ok(())
    }
}

/// Named production-admission gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    /// Stable gate identifier.
    pub name: String,
    /// Whether production admission requires an executed pass.
    pub required_for_production: bool,
    /// Current gate outcome.
    pub status: GateStatus,
}

impl Gate {
    /// Construct and validate a gate.
    pub fn new(
        name: impl Into<String>,
        required_for_production: bool,
        status: GateStatus,
    ) -> Result<Self> {
        let gate = Self {
            name: name.into(),
            required_for_production,
            status,
        };
        gate.validate()?;
        Ok(gate)
    }

    /// Validate a persisted gate contract.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("pipeline gate name must not be empty");
        }
        self.status.validate()
    }
}
