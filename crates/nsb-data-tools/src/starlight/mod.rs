//! Production Starlight dataset pipeline.

pub mod config;
pub mod map;
pub mod migration;
pub mod photometric;
mod pipeline;
pub mod selection;
pub mod sources;
pub mod uv;
pub mod validation;
mod worker;
pub mod xp;

pub(crate) use pipeline::PIPELINE;
