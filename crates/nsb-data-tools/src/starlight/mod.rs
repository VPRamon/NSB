//! Production Starlight dataset pipeline.

pub mod config;
pub mod map;
pub mod migration;
mod pipeline;
pub mod sources;
mod worker;
pub mod xp;

pub(crate) use pipeline::PIPELINE;
