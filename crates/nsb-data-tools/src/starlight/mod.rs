//! Production Starlight dataset pipeline.

pub mod conditions;
pub mod config;
pub mod licensing;
pub mod map;
pub mod migration;
pub mod pack;
pub mod photometric;
mod pipeline;
pub mod promotion;
pub mod selection;
pub mod sources;
pub mod uncertainty;
pub mod uv;
pub mod validation;
mod worker;
pub mod xp;

pub(crate) use pipeline::PIPELINE;
