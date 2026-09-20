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

#[cfg(feature = "window-search-diagnostics")]
mod diagnostics;

#[cfg(feature = "window-search-diagnostics")]
pub use diagnostics::WindowSearchDiagnostics;
pub use types::{SiteWindowContext, ThresholdQuery, ThresholdQueryResult};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
