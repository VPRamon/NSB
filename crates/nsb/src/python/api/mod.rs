// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! NSB-owned Python API.
//!
//! Keep this module limited to concepts whose scientific and compatibility
//! contract belongs to NSB. Generic astronomy and time interoperability is
//! delegated to `siderust-py` and `tempoch-py`.

mod config;
mod errors;
mod evaluator;
mod queries;
mod results;
mod selectors;

pub(in crate::python) use errors::{add_exceptions, invalid_input};
pub(in crate::python) use evaluator::PySiteWindowContext;
pub(in crate::python) use results::{
    PyNsbComponent, PyNsbComponentMetadata, PyNsbResult, PyThresholdQueryResult,
};
pub(in crate::python) use selectors::{install_component_mask_constants, PySiteProfile};
