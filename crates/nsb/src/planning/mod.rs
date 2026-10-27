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

mod astronomy;
mod filters;
mod healpix_partition;
mod intervals;
mod prepare;
#[cfg(test)]
#[path = "tests/reference_scan.rs"]
mod reference_scan;
mod threshold;
mod transitions;
mod types;

// Internal certificates and resource controls are not a supported public API.
mod bounds;
mod discovery;
#[cfg(test)]
#[path = "tests/discovery_bench.rs"]
mod discovery_bench;
#[cfg(test)]
#[path = "tests/discovery_tests.rs"]
mod discovery_tests;
#[cfg(test)]
#[path = "tests/strict_tests.rs"]
mod strict_tests;
#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod test_fixtures;

pub use types::{SiteWindowContext, ThresholdQuery, ThresholdQueryResult};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
