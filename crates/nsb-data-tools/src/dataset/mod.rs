//! Typed, portable dataset pipeline engine.

mod airglow_palace;
mod config;
mod engine;
mod execution;
mod model;
mod moonlight_mie;
mod pipeline;
mod slurm;
mod solar_spectrum;

pub use config::{RunConfig, SourceConfig};
pub use engine::{execute, resume, run_worker, status};
pub use model::{
    Artifact, BuildPlan, DatasetName, Executor, Operation, RunManifest, RunStatus, ValidationGate,
    ValidationReport,
};
pub use pipeline::DatasetPipeline;
