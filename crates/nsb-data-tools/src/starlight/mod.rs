//! Production Starlight dataset pipeline.

pub mod config;
pub mod map;
pub mod migration;
mod pipeline;
pub mod photometric;
pub mod selection;
pub mod sources;
pub mod uv;
mod worker;
pub mod xp;

pub(crate) use pipeline::PIPELINE;
