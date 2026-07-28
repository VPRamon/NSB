//! Typed, portable dataset pipeline engine.

mod config;
mod engine;
mod execution;
mod model;
mod pipeline;
mod slurm;

pub use config::{RunConfig, SourceConfig};
pub use engine::{execute, resume, run_worker, status};
pub use model::{
    Artifact, BuildPlan, DatasetName, Executor, Operation, RunManifest, RunStatus, ValidationGate,
    ValidationReport,
};
pub use pipeline::DatasetPipeline;
