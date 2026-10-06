//! Independent Starlight validation pipeline (GitHub issue #87).
//!
//! This module is deliberately independent of `crate::starlight::map`: it
//! re-implements its own minimal candidate-map reader and its own HEALPix
//! pixel-center geometry so that a bug shared between the production writer
//! and its validator cannot hide from either.
//!
//! This literature-audit route produces technical evidence. Scientific
//! production readiness is established by the separately checksum-pinned
//! external cross-validation artifact.

pub mod acquire;
pub mod candidate_map;
pub mod metrics;
pub mod preregistration;
pub mod references;
pub mod regions;
pub mod report;
pub mod run;
pub mod transformed_grid;
pub mod transforms;

use serde::{Deserialize, Serialize};

/// Manifest of every input and output artifact produced by one `run`
/// invocation, each with its own independently recomputed SHA-256.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub generated_at_unix_seconds: u64,
    pub artifacts: Vec<crate::dataset::Artifact>,
}
