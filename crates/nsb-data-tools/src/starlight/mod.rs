//! Production Starlight dataset pipeline.

pub mod config;
pub mod map;
mod pipeline;
pub mod sources;

pub(crate) use pipeline::PIPELINE;
