//! Typed contracts for durable NSB data-product pipelines.
//!
//! The module separates persisted contracts, production admission, compact
//! checkpoints, and cache-state transitions from executable argument parsing.
//! This keeps orchestration decisions testable without spawning sibling tools.

pub mod admission;
pub mod checkpoint;
pub mod contracts;
pub mod state;

pub use admission::{AdmissionDecision, ProductionAdmission};
pub use checkpoint::{DiagnosticSample, PartitionCheckpoint, MAX_DIAGNOSTIC_SAMPLES};
pub use contracts::{
    Gate, GateStatus, PartitionCompletion, ProcessingMode, RowSelection,
    PIPELINE_SCHEMA_VERSION,
};
pub use state::{
    CacheInputState, PartitionState, ResumeAction, TransitionEvidence,
};
