// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use std::sync::Arc;

use pyo3::prelude::*;

use crate::{NsbEvaluator, NsbModelConfig, PointQuery, SiteWindowContext, ThresholdQuery};

use super::errors::to_py_err;
use super::results::{PyNsbResult, PyThresholdQueryResult};

#[pyclass(name = "SiteWindowContext", frozen, module = "nsb")]
pub(in crate::python) struct PySiteWindowContext {
    // Detached Rust work needs an owned handle to reusable prepared state.
    inner: Arc<SiteWindowContext>,
}

#[pymethods]
impl NsbEvaluator {
    /// Component-separated restricted band; same physical gates as integrate_band.
    #[pyo3(name = "integrate_band_components")]
    fn py_integrate_band_components(
        &self,
        py: Python<'_>,
        query: PyRef<'_, PointQuery>,
        lower_nm: f64,
        upper_nm: f64,
    ) -> PyResult<super::results::PyRestrictedBandResult> {
        let query = query.clone();
        py.detach(|| {
            self.integrate_band_components(
                &query,
                crate::units::length::Nanometers::new(lower_nm),
                crate::units::length::Nanometers::new(upper_nm),
            )
        })
        .map(Into::into)
        .map_err(to_py_err)
    }

    /// Existing component integral in photons/(cm2 ns sr), restricted to 300–650 nm.
    /// Raises UnsupportedError for starlight or nonzero moonlight; never fills gaps.
    #[pyo3(name = "integrate_band")]
    fn py_integrate_band(
        &self,
        py: Python<'_>,
        query: PyRef<'_, PointQuery>,
        lower_nm: f64,
        upper_nm: f64,
    ) -> PyResult<f64> {
        let query = query.clone();
        py.detach(|| {
            self.integrate_band(
                &query,
                crate::units::length::Nanometers::new(lower_nm),
                crate::units::length::Nanometers::new(upper_nm),
            )
        })
        .map(|value| value.value())
        .map_err(to_py_err)
    }

    /// Complete interval-certified search; unsupported or incomplete coverage
    /// raises UnsupportedError. No uniform-scan fallback is used.
    #[pyo3(name = "periods_below_threshold_certified")]
    fn py_periods_below_threshold_certified(
        &self,
        py: Python<'_>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let query = query.clone();
        py.detach(|| self.periods_below_threshold_certified(&query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
    }

    /// Complete certified search reusing a compatible site context.
    #[pyo3(name = "periods_below_threshold_certified_with_context")]
    fn py_periods_below_threshold_certified_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.clone();
        py.detach(|| self.periods_below_threshold_certified_with_context(&context, &query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
    }

    /// Step-free adaptive discovery; empirical model-resolution assumptions,
    /// not a continuous completeness certificate. Errors return no schedule.
    #[pyo3(name = "periods_below_threshold_adaptive")]
    fn py_periods_below_threshold_adaptive(
        &self,
        py: Python<'_>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let query = query.clone();
        py.detach(|| self.periods_below_threshold_adaptive(&query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
    }

    #[pyo3(name = "periods_below_threshold_adaptive_with_context")]
    fn py_periods_below_threshold_adaptive_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, ThresholdQuery>,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.clone();
        py.detach(|| self.periods_below_threshold_adaptive_with_context(&context, &query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
    }

    /// Inclusive min/max search with shared adaptive discovery and inward
    /// conversion to Python datetime precision.
    #[pyo3(name = "periods_in_band_adaptive")]
    fn py_periods_in_band_adaptive(
        &self,
        py: Python<'_>,
        query: PyRef<'_, ThresholdQuery>,
        minimum: f64,
    ) -> PyResult<PyThresholdQueryResult> {
        let query = query.clone();
        py.detach(|| {
            self.periods_in_band_adaptive(
                &query,
                crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(
                    minimum,
                ),
            )
        })
        .and_then(PyThresholdQueryResult::try_from_inner)
        .map_err(to_py_err)
    }

    #[pyo3(name = "periods_in_band_adaptive_with_context")]
    fn py_periods_in_band_adaptive_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, ThresholdQuery>,
        minimum: f64,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.clone();
        py.detach(|| {
            self.periods_in_band_adaptive_with_context(
                &context,
                &query,
                crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(
                    minimum,
                ),
            )
        })
        .and_then(PyThresholdQueryResult::try_from_inner)
        .map_err(to_py_err)
    }

    #[pyo3(name = "periods_in_band_certified_with_context")]
    fn py_periods_in_band_certified_with_context(
        &self,
        py: Python<'_>,
        context: PyRef<'_, PySiteWindowContext>,
        query: PyRef<'_, ThresholdQuery>,
        minimum: f64,
    ) -> PyResult<PyThresholdQueryResult> {
        let context = Arc::clone(&context.inner);
        let query = query.clone();
        py.detach(|| {
            self.periods_in_band_certified_with_context(
                &context,
                &query,
                crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(
                    minimum,
                ),
            )
        })
        .and_then(PyThresholdQueryResult::try_from_inner)
        .map_err(to_py_err)
    }

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
        py.detach(|| self.periods_below_threshold(&query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
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
        py.detach(|| self.periods_below_threshold_with_context(&context, &query))
            .and_then(PyThresholdQueryResult::try_from_inner)
            .map_err(to_py_err)
    }
}
