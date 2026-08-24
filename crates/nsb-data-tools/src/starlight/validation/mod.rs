//! Independent Starlight validation pipeline (GitHub issue #87).
//!
//! This module is deliberately independent of `crate::starlight::map`: it
//! re-implements its own minimal candidate-map reader and its own HEALPix
//! pixel-center geometry so that a bug shared between the production writer
//! and its validator cannot hide from either.
//!
//! Nothing in this module may ever set `scientifically_validated = true`.
//! Human scientific approval of a specific checksum is recorded only in
//! issue #47; this pipeline only produces technical evidence and a pending
//! review template for that decision.

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
