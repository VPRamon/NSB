//! Production Starlight dataset pipeline.

pub mod conditions;
pub mod config;
pub(crate) mod healpix;
pub mod licensing;
pub mod map;
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
