use std::sync::Arc;

use pyo3::prelude::*;

use crate::{NsbEvaluator, NsbModelConfig, PointQuery, SiteWindowContext, ThresholdQuery};

use super::errors::to_py_err;
use super::results::{PyNsbResult, PyThresholdQueryResult};

#[pyclass(
    name = "SiteWindowContext",
    frozen,
    module = "nsb",
    skip_from_py_object
)]
pub(in crate::python) struct PySiteWindowContext {
    // Detached Rust work needs an owned handle to reusable prepared state.
    inner: Arc<SiteWindowContext>,
}

#[pymethods]
impl NsbEvaluator {
    #[new]
    #[pyo3(signature = (config=None))]
    fn py_new(py: Python<'_>, config: Option<PyRef<'_, NsbModelConfig>>) -> PyResult<Self> {
        let config = config.map_or_else(NsbModelConfig::generic_clear_sky, |value| value.clone());
        py.detach(move || Self::with_config(config))
            .map_err(to_py_err)
    }

    #[getter(config)]
    fn py_config(&self) -> NsbModelConfig {
        self.config().clone()
    }

    #[pyo3(name = "evaluate")]
    fn py_evaluate(&self, py: Python<'_>, query: PyRef<'_, PointQuery>) -> PyResult<PyNsbResult> {
        let query = query.clone();
        py.detach(|| self.evaluate(&query))
            .map(PyNsbResult::from)
            .map_err(to_py_err)
    }

    #[pyo3(name = "prepare_site_window_context")]
    fn py_prepare_site_window_context(
        &self,
        py: Python<'_>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PySiteWindowContext> {
        let query = query.clone();
        let inner = py
            .detach(|| self.prepare_site_window_context(&query))
            .map_err(to_py_err)?;
        Ok(PySiteWindowContext {
            inner: Arc::new(inner),
        })
    }

    #[pyo3(name = "periods_below_threshold")]
    fn py_periods_below_threshold(
        &self,
        py: Python<'_>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let query = query.clone();
        let result = py
            .detach(|| self.periods_below_threshold(&query))
            .map_err(to_py_err)?;
        PyThresholdQueryResult::try_from_inner(result)
    }

    #[pyo3(name = "periods_below_threshold_with_context")]
    fn py_periods_below_threshold_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.clone();
        let result = py
            .detach(|| self.periods_below_threshold_with_context(&context, &query))
            .map_err(to_py_err)?;
        PyThresholdQueryResult::try_from_inner(result)
    }
}
