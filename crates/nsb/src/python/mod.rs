//! Optional Python adapter for NSB.
//!
//! NSB-owned types are exposed directly where Python preserves their Rust
//! identity and semantics. Boundary adapters live in `api` and `compat`.

mod api;
mod compat;

use pyo3::prelude::*;

use crate::{
    AirglowModel, ComponentMask, MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery,
    SiteProfileId, ThresholdQuery, ZodiacalExtinction,
};
use api::{
    add_exceptions, install_component_mask_constants, PyNsbComponent, PyNsbComponentMetadata,
    PyNsbResult, PySiteWindowContext, PyThresholdQueryResult,
};
use compat::{PyDirection, PyObserver};

#[pymodule]
#[pyo3(name = "nsb")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    add_exceptions(module)?;

    module.add_class::<PyObserver>()?;
    module.add_class::<PyDirection>()?;
    module.add_class::<SiteProfileId>()?;
    module.add_class::<MoonlightModel>()?;
    module.add_class::<AirglowModel>()?;
    module.add_class::<ZodiacalExtinction>()?;
    module.add_class::<ComponentMask>()?;
    install_component_mask_constants(module.py(), module)?;

    module.add_class::<NsbModelConfig>()?;
    module.add_class::<PointQuery>()?;
    module.add_class::<ThresholdQuery>()?;
    module.add_class::<PyNsbComponentMetadata>()?;
    module.add_class::<PyNsbComponent>()?;
    module.add_class::<PyNsbResult>()?;
    module.add_class::<PyThresholdQueryResult>()?;
    module.add_class::<PySiteWindowContext>()?;
    module.add_class::<NsbEvaluator>()?;

    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    module.add("MODEL_VERSION", crate::MODEL_VERSION)?;
    module.add("SIDERUST_VERSION", crate::SIDERUST_VERSION)?;
    module.add("SIDERUST_SOURCE", crate::SIDERUST_SOURCE)?;
    Ok(())
}
