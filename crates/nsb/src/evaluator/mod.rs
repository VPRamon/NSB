//! NSB evaluator: point evaluation and threshold-period search.
//!
//! This module is the library-facing orchestration layer. It accepts typed
//! observing inputs, invokes the physical component models, sums their
//! radiances, and provides an event-driven planning search. CLI concerns such
//! as named-site parsing and timestamp parsing intentionally live outside this
//! crate.

mod airglow_maturity;
mod core;
mod metadata;
mod search;
mod types;

pub use core::NsbEvaluator;
pub use metadata::{
    BandDiagnostic, ComponentCalibrationStatus as CalibrationStatus, NsbComponentMetadata,
};
pub use types::{
    ComponentMask, MoonlightModel, NsbComponent, NsbComponentDescriptor, NsbModelConfig, NsbResult,
    Observer, PointQuery, StarlightModel, Target, ThresholdQuery, ThresholdQueryResult,
};
