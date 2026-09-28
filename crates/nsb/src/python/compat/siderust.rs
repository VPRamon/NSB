//! Temporary Python compatibility wrappers for Siderust astronomy types.
//!
//! These classes exist only because the currently reusable `siderust-py`
//! package is pinned to an older qtty/tempoch stack than NSB. Keep this module
//! free of NSB-specific policy so it can be deleted once upstream bindings are
//! version-compatible.

use pyo3::prelude::*;
use qtty::angular::Degrees;
use qtty::length::Meters;

use crate::{Observer, Target};

use super::super::api::invalid_input;

fn finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be finite")))
    }
}

#[pyclass(name = "Observer", frozen, module = "nsb", from_py_object)]
#[derive(Clone, Copy)]
pub(in crate::python) struct PyObserver {
    inner: Observer,
}

impl PyObserver {
    pub(in crate::python) const fn inner(&self) -> Observer {
        self.inner
    }

    pub(in crate::python) const fn from_inner(inner: Observer) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyObserver {
    #[new]
    #[pyo3(signature = (lon_deg, lat_deg, height_m=0.0))]
    fn new(lon_deg: f64, lat_deg: f64, height_m: f64) -> PyResult<Self> {
        finite("lon_deg", lon_deg)?;
        finite("lat_deg", lat_deg)?;
        finite("height_m", height_m)?;

        Ok(Self {
            inner: Observer::new(
                Degrees::new(lon_deg),
                Degrees::new(lat_deg),
                Meters::new(height_m),
            ),
        })
    }

    #[getter]
    fn lon_deg(&self) -> f64 {
        self.inner.lon.value()
    }

    #[getter]
    fn lat_deg(&self) -> f64 {
        self.inner.lat.value()
    }

    #[getter]
    fn height_m(&self) -> f64 {
        self.inner.height.value()
    }

    fn __repr__(&self) -> String {
        format!(
            "Observer(lon_deg={}, lat_deg={}, height_m={})",
            self.inner.lon.value(),
            self.inner.lat.value(),
            self.inner.height.value()
        )
    }
}

#[pyclass(name = "Direction", frozen, module = "nsb", from_py_object)]
#[derive(Clone, Copy)]
pub(in crate::python) struct PyDirection {
    inner: Target,
}

impl PyDirection {
    pub(in crate::python) const fn inner(&self) -> Target {
        self.inner
    }

    pub(in crate::python) const fn from_inner(inner: Target) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyDirection {
    #[new]
    fn new(ra_deg: f64, dec_deg: f64) -> PyResult<Self> {
        finite("ra_deg", ra_deg)?;
        finite("dec_deg", dec_deg)?;

        Ok(Self {
            inner: Target::new(Degrees::new(ra_deg), Degrees::new(dec_deg)),
        })
    }

    #[getter]
    fn ra_deg(&self) -> f64 {
        self.inner.azimuth.value()
    }

    #[getter]
    fn dec_deg(&self) -> f64 {
        self.inner.polar.value()
    }

    fn __repr__(&self) -> String {
        format!(
            "Direction(ra_deg={}, dec_deg={})",
            self.inner.azimuth.value(),
            self.inner.polar.value()
        )
    }
}
