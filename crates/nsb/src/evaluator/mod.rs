//! NSB evaluator: point evaluation and component composition.
//!
//! This module constructs immutable component models and evaluates night-sky
//! background at a single observing instant. Observing-window planning and
//! threshold search live in [`crate::planning`].

mod core;
mod metadata;
mod point;
mod types;

pub use core::NsbEvaluator;
pub use metadata::{
    BandDiagnostic, ComponentCalibrationStatus as CalibrationStatus, NsbComponentMetadata,
};
pub use types::{
    ComponentMask, NsbComponent, NsbComponentDescriptor, NsbModelConfig, NsbResult, Observer,
    PointQuery, Target,
};
