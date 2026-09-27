mod config;
mod coordinates;
mod errors;
mod evaluator;
mod queries;
mod results;
mod selectors;

use pyo3::prelude::*;

use config::PyNsbModelConfig;
use coordinates::{PyObserver, PyTarget};
use evaluator::{PyNsbEvaluator, PySiteWindowContext};
use queries::{PyPointQuery, PyThresholdQuery};
use results::{
    PyBandDiagnostic, PyNsbComponent, PyNsbComponentDescriptor, PyNsbComponentMetadata,
    PyNsbResult, PyThresholdQueryResult, PyUtcPeriod,
};
use selectors::{
    install_component_mask_constants, PyAirglowModel, PyComponentMask, PyMoonlightModel,
    PySiteProfile, PyStarlightProduct, PyZodiacalExtinction, PyZodiacalModel,
};

#[pymodule]
fn nsb(module: &Bound<'_, PyModule>) -> PyResult<()> {
    errors::add_exceptions(module)?;

    module.add_class::<PyObserver>()?;
    module.add_class::<PyTarget>()?;
    module.add_class::<PySiteProfile>()?;
    module.add_class::<PyMoonlightModel>()?;
    module.add_class::<PyAirglowModel>()?;
    module.add_class::<PyZodiacalModel>()?;
    module.add_class::<PyZodiacalExtinction>()?;
    module.add_class::<PyStarlightProduct>()?;
    module.add_class::<PyComponentMask>()?;
    install_component_mask_constants(module)?;

    module.add_class::<PyNsbModelConfig>()?;
    module.add_class::<PyPointQuery>()?;
    module.add_class::<PyThresholdQuery>()?;
    module.add_class::<PyBandDiagnostic>()?;
    module.add_class::<PyNsbComponentMetadata>()?;
    module.add_class::<PyNsbComponent>()?;
    module.add_class::<PyNsbComponentDescriptor>()?;
    module.add_class::<PyNsbResult>()?;
    module.add_class::<PyUtcPeriod>()?;
    module.add_class::<PyThresholdQueryResult>()?;
    module.add_class::<PySiteWindowContext>()?;
    module.add_class::<PyNsbEvaluator>()?;

    module.add("__version__", crate::NSB_VERSION)?;
    module.add("MODEL_VERSION", crate::MODEL_VERSION)?;
    module.add("SIDERUST_VERSION", crate::SIDERUST_VERSION)?;
    module.add("SIDERUST_SOURCE", crate::SIDERUST_SOURCE)?;

    Ok(())
}
