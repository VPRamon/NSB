use pyo3::prelude::*;
use std::sync::Arc;

use crate::{NsbEvaluator, SiteWindowContext};

use super::config::PyNsbModelConfig;
use super::coordinates::PyObserver;
use super::errors::to_py_err;
use super::queries::{PyPointQuery, PyThresholdQuery};
use super::results::{PyNsbComponentDescriptor, PyNsbResult, PyThresholdQueryResult};
use super::selectors::PyComponentMask;

#[pyclass(name = "SiteWindowContext", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PySiteWindowContext {
    inner: SiteWindowContext,
}

impl PySiteWindowContext {
    fn inner(&self) -> SiteWindowContext {
        self.inner.clone()
    }
}

#[pyclass(name = "NsbEvaluator", frozen, module = "nsb")]
pub(super) struct PyNsbEvaluator {
    inner: Arc<NsbEvaluator>,
}

#[pymethods]
impl PyNsbEvaluator {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(py: Python<'_>, config: Option<&PyNsbModelConfig>) -> PyResult<Self> {
        let config = config
            .map(PyNsbModelConfig::inner)
            .unwrap_or_else(crate::NsbModelConfig::generic_clear_sky);
        let evaluator = py
            .detach(move || NsbEvaluator::with_config(config))
            .map_err(to_py_err)?;
        Ok(Self {
            inner: Arc::new(evaluator),
        })
    }

    #[getter]
    fn config(&self, py: Python<'_>) -> PyResult<Py<PyNsbModelConfig>> {
        Py::new(
            py,
            PyNsbModelConfig::from_inner(self.inner.config().clone()),
        )
    }

    fn describe_components(
        &self,
        py: Python<'_>,
        observer: &PyObserver,
        components: &PyComponentMask,
    ) -> PyResult<Vec<Py<PyNsbComponentDescriptor>>> {
        let evaluator = Arc::clone(&self.inner);
        let observer = observer.inner();
        let components = components.inner();
        let descriptions = py
            .detach(move || evaluator.describe_components(observer, components))
            .map_err(to_py_err)?;
        descriptions
            .into_iter()
            .map(|description| Py::new(py, description.into()))
            .collect()
    }

    fn evaluate(&self, py: Python<'_>, query: &PyPointQuery) -> PyResult<Py<PyNsbResult>> {
        let evaluator = Arc::clone(&self.inner);
        let query = query.inner();
        let result = py
            .detach(move || evaluator.evaluate(&query))
            .map_err(to_py_err)?;
        Py::new(py, result.into())
    }

    fn prepare_site_window_context(
        &self,
        py: Python<'_>,
        query: &PyThresholdQuery,
    ) -> PyResult<Py<PySiteWindowContext>> {
        let evaluator = Arc::clone(&self.inner);
        let query = query.inner();
        let context = py
            .detach(move || evaluator.prepare_site_window_context(&query))
            .map_err(to_py_err)?;
        Py::new(py, PySiteWindowContext { inner: context })
    }

    fn periods_below_threshold(
        &self,
        py: Python<'_>,
        query: &PyThresholdQuery,
    ) -> PyResult<Py<PyThresholdQueryResult>> {
        let evaluator = Arc::clone(&self.inner);
        let query = query.inner();
        let result = py
            .detach(move || evaluator.periods_below_threshold(&query))
            .map_err(to_py_err)?;
        Py::new(py, PyThresholdQueryResult::try_from_inner(result)?)
    }

    fn periods_below_threshold_with_context(
        &self,
        py: Python<'_>,
        context: &PySiteWindowContext,
        query: &PyThresholdQuery,
    ) -> PyResult<Py<PyThresholdQueryResult>> {
        let evaluator = Arc::clone(&self.inner);
        let context = context.inner();
        let query = query.inner();
        let result = py
            .detach(move || evaluator.periods_below_threshold_with_context(&context, &query))
            .map_err(to_py_err)?;
        Py::new(py, PyThresholdQueryResult::try_from_inner(result)?)
    }
}
