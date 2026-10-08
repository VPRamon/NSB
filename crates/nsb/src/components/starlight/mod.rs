// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Directional unresolved-starlight component.
//!
//! The runtime component is intentionally map-backed. Public callers select a
//! [`StarlightProduct`] through [`crate::NsbModelConfig`] and evaluate through
//! [`crate::NsbEvaluator`]. [`StarlightMap`] and validation/provenance records
//! remain the advanced API for constructing, admitting, and inspecting products.
//! The concrete directional evaluator and component-only output are internal.

mod map;
mod model;
mod output;
mod photometry;
mod product;
mod provenance;
mod validated;

pub use map::{StarlightMap, StarlightPixel};
pub(crate) use model::Starlight;
pub(crate) use output::StarlightOutputs;
pub use product::StarlightProduct;
pub use provenance::StarlightProvenance;
pub use provenance::{StarlightBrightStarInputProvenance, StarlightBrightStarSupplementProvenance};
pub use validated::{StarlightValidationDiagnostics, ValidatedStarlightMap};

#[cfg(test)]
mod tests;
