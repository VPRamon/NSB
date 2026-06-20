//! Directional unresolved-starlight component.
//!
//! The model is intentionally map-backed: callers either provide a
//! [`StarlightMap`] or load a documented standard map once such a dataset is
//! generated and bundled. The previous direction-independent spectrum is not
//! used here.

mod map;
mod model;
mod output;
mod photometry;
mod provenance;

pub use map::{StarlightMap, StarlightPixel};
pub use model::Starlight;
pub use output::StarlightOutputs;
pub use provenance::StarlightProvenance;

#[cfg(test)]
mod tests;
