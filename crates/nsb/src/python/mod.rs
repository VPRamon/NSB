//! Optional Python adapter for NSB.
//!
//! The public `nsb` Python package is a thin facade over the private `_nsb`
//! extension. NSB-owned bindings live in `api`; temporary Siderust/tempoch
//! interoperability lives in `compat` so it can be removed independently.

mod api;
mod compat;

use pyo3::prelude::*;

use api::{
    add_exceptions, install_component_mask_constants, PyAirglowModel,
    PyComponentMask, PyMoonlightModel, PyNsbComponent, PyNsbComponentMetadata, PyNsbEvaluator,
    PyNsbModelConfig, PyNsbResult, PyPointQuery, PySiteProfile, PySiteWindowContext,
    PyThresholdQuery, PyThresholdQueryResult, PyZodiacalExtinction,
};
use compat::{PyDirection, PyObserver};

#[pymodule]
#[pyo3(name = "_nsb")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    add_exceptions(module)?;

    module.add_class::<PyObserver>()?;
    module.add_class::<PyDirection>()?;
    module.add_class::<PySiteProfile>()?;
    module.add_class::<PyMoonlightModel>()?;
    module.add_class::<PyAirglowModel>()?;
    module.add_class::<PyZodiacalExtinction>()?;
    module.add_class::<PyComponentMask>()?;
    install_component_mask_constants(module.py(), module)?;

    module.add_class::<PyNsbModelConfig>()?;
    module.add_class::<PyPointQuery>()?;
    module.add_class::<PyThresholdQuery>()?;
    module.add_class::<PyNsbComponentMetadata>()?;
    module.add_class::<PyNsbComponent>()?;
    module.add_class::<PyNsbResult>()?;
    module.add_class::<PyThresholdQueryResult>()?;
    module.add_class::<PySiteWindowContext>()?;
    module.add_class::<PyNsbEvaluator>()?;

    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("MODEL_VERSION", crate::MODEL_VERSION)?;
    module.add("SIDERUST_VERSION", crate::SIDERUST_VERSION)?;
    module.add("SIDERUST_SOURCE", crate::SIDERUST_SOURCE)?;
    Ok(())
}
