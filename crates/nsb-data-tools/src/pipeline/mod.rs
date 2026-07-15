//! Typed contracts for durable NSB data-product pipelines.
//!
//! The module separates persisted contracts, production admission, compact
//! checkpoints, reconciliation, and cache-state transitions from executable
//! argument parsing. This keeps orchestration decisions testable without
//! spawning sibling tools.

pub mod admission;
pub mod checkpoint;
pub mod contracts;
pub mod reconciliation;
pub mod state;
pub mod store;

pub use admission::{AdmissionDecision, ProductionAdmission};
pub use checkpoint::{DiagnosticSample, PartitionCheckpoint, MAX_DIAGNOSTIC_SAMPLES};
pub use contracts::{
    Gate, GateStatus, PartitionCompletion, ProcessingMode, RowSelection, PIPELINE_SCHEMA_VERSION,
};
pub use reconciliation::{PartitionManifest, ReconciliationManifest};
pub use state::{CacheInputState, PartitionState, ResumeAction, TransitionEvidence};
pub use store::{read_partition_state, write_partition_state};
