// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Observing-window planning and threshold search.
//!
//! This module answers when observing constraints and an NSB threshold hold. It
//! consumes [`crate::NsbEvaluator`] for authoritative radiance evaluation and
//! owns reusable site/window preparation, astronomical filters, and threshold
//! crossing search.
//!
//! Convenience methods such as [`crate::NsbEvaluator::periods_below_threshold`]
//! remain available as thin wrappers defined in this module so the evaluator
//! does not depend on planning.

mod filters;
mod prepare;
mod scan;
mod threshold;
mod types;

// Feasibility spike for #234. Keep numerical controls and incomplete-result
// semantics out of the supported API until component envelopes are available.
#[cfg(test)]
mod adaptive;
#[cfg(test)]
mod adaptive_bench;
#[cfg(test)]
mod adaptive_validation;
#[cfg(test)]
mod bounds;

pub use types::{SiteWindowContext, ThresholdQuery, ThresholdQueryResult};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
