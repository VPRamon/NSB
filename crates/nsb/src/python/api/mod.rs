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

pub(in crate::python) use config::PyNsbModelConfig;
pub(in crate::python) use errors::{add_exceptions, invalid_input};
pub(in crate::python) use evaluator::{PyNsbEvaluator, PySiteWindowContext};
pub(in crate::python) use queries::{PyPointQuery, PyThresholdQuery};
pub(in crate::python) use results::{
    PyNsbComponent, PyNsbComponentMetadata, PyNsbResult, PyThresholdQueryResult,
};
pub(in crate::python) use selectors::{
    install_component_mask_constants, PyAirglowModel, PyComponentMask, PyMoonlightModel,
    PySiteProfile, PyZodiacalExtinction,
};
