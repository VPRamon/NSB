//! NSB evaluator: point evaluation and threshold-period search.
//!
//! This module is the library-facing orchestration layer. It accepts typed
//! observing inputs, invokes the physical component models, sums their
//! radiances, and provides an event-driven planning search. CLI concerns such
//! as named-site parsing and timestamp parsing intentionally live outside this
//! crate.

mod core;
#[cfg(feature = "window-search-diagnostics")]
mod diagnostics;
mod metadata;
pub(crate) mod search;
mod types;

pub use core::NsbEvaluator;
#[cfg(feature = "window-search-diagnostics")]
pub use diagnostics::WindowSearchDiagnostics;
pub use metadata::{
    BandDiagnostic, ComponentCalibrationStatus as CalibrationStatus, NsbComponentMetadata,
};
pub use types::{
    ComponentMask, NsbComponent, NsbComponentDescriptor, NsbModelConfig, NsbResult, Observer,
    PointQuery, SiteWindowContext, Target, ThresholdQuery, ThresholdQueryResult,
};
