use std::sync::Arc;

use pyo3::prelude::*;

use crate::{NsbEvaluator, NsbModelConfig, SiteWindowContext};

use super::config::PyNsbModelConfig;
use super::errors::to_py_err;
use super::queries::{PyPointQuery, PyThresholdQuery};
use super::results::{PyNsbResult, PyThresholdQueryResult};

#[pyclass(
    name = "SiteWindowContext",
    frozen,
    module = "nsb",
    skip_from_py_object
)]
pub(super) struct PySiteWindowContext {
    // The context is reused across Python calls. Arc lets detached Rust work own
    // a stable handle without cloning the prepared scientific state.
    inner: Arc<SiteWindowContext>,
}

#[pyclass(name = "NsbEvaluator", frozen, module = "nsb", skip_from_py_object)]
pub(super) struct PyNsbEvaluator {
    inner: NsbEvaluator,
}

#[pymethods]
impl PyNsbEvaluator {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(py: Python<'_>, config: Option<PyRef<'_, PyNsbModelConfig>>) -> PyResult<Self> {
        let config = config.map_or_else(NsbModelConfig::generic_clear_sky, |value| value.inner());
        let inner = py
            .detach(move || NsbEvaluator::with_config(config))
            .map_err(to_py_err)?;
        Ok(Self { inner })
    }

    #[getter]
    fn config(&self) -> PyNsbModelConfig {
        PyNsbModelConfig::from_inner(self.inner.config().clone())
    }

    fn evaluate(&self, py: Python<'_>, query: PyRef<'_, PyPointQuery>) -> PyResult<PyNsbResult> {
        let query = query.inner();
        py.detach(|| self.inner.evaluate(&query))
            .map(PyNsbResult::from)
            .map_err(to_py_err)
    }

    fn prepare_site_window_context(
        &self,
        py: Python<'_>,
        query: PyRef<'_, PyThresholdQuery>,
    ) -> PyResult<PySiteWindowContext> {
        let query = query.inner();
        let inner = py
            .detach(|| self.inner.prepare_site_window_context(&query))
            .map_err(to_py_err)?;
        Ok(PySiteWindowContext {
            inner: Arc::new(inner),
        })
    }

    fn periods_below_threshold(
        &self,
        py: Python<'_>,
        query: PyRef<'_, PyThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let query = query.inner();
        let result = py
            .detach(|| self.inner.periods_below_threshold(&query))
            .map_err(to_py_err)?;
        PyThresholdQueryResult::try_from_inner(result)
    }

    fn periods_below_threshold_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, PyThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.inner();
        let result = py
            .detach(|| {
                self.inner
                    .periods_below_threshold_with_context(&context, &query)
            })
            .map_err(to_py_err)?;
        PyThresholdQueryResult::try_from_inner(result)
    }
}
