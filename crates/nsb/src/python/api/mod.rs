//! NSB-owned Python API.
//!
//! Keep this module limited to concepts whose scientific and compatibility
//! contract belongs to NSB. Generic astronomy and time interop lives in
//! `python::compat` and is deliberately replaceable.

mod config;
mod errors;
mod evaluator;
mod queries;
mod results;
mod selectors;

pub(super) use config::PyNsbModelConfig;
pub(super) use errors::{add_exceptions, invalid_input};
pub(super) use evaluator::{PyNsbEvaluator, PySiteWindowContext};
pub(super) use queries::{PyPointQuery, PyThresholdQuery};
pub(super) use results::{
    PyNsbComponent, PyNsbComponentMetadata, PyNsbResult, PyThresholdQueryResult,
};
pub(super) use selectors::{
    install_component_mask_constants, PyAirglowModel, PyComponentMask, PyMoonlightModel,
    PySiteProfile, PyZodiacalExtinction,
};
