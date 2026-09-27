use pyo3::prelude::*;
use qtty::angular::Degrees;
use qtty::length::Meters;

use crate::{Observer, Target};

use super::errors::invalid_input;

fn finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be finite")))
    }
}

#[pyclass(name = "Observer", frozen, module = "nsb")]
#[derive(Clone, Copy)]
pub(super) struct PyObserver {
    inner: Observer,
}

impl PyObserver {
    pub(super) fn inner(&self) -> Observer {
        self.inner
    }

    pub(super) fn from_inner(inner: Observer) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyObserver {
    #[new]
    #[pyo3(signature = (*, latitude_deg, longitude_deg, elevation_m))]
    fn new(latitude_deg: f64, longitude_deg: f64, elevation_m: f64) -> PyResult<Self> {
        finite("latitude_deg", latitude_deg)?;
        finite("longitude_deg", longitude_deg)?;
        finite("elevation_m", elevation_m)?;

        if !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(invalid_input("latitude_deg must be in [-90, 90]"));
        }
        if !(-180.0..180.0).contains(&longitude_deg) {
            return Err(invalid_input(
                "longitude_deg must be in [-180, 180) and positive eastward",
            ));
        }

        Ok(Self {
            inner: Observer::new_raw(
                Degrees::new(longitude_deg),
                Degrees::new(latitude_deg),
                Meters::new(elevation_m),
            ),
        })
    }

    #[getter]
    fn latitude_deg(&self) -> f64 {
        self.inner.lat.value()
    }

    #[getter]
    fn longitude_deg(&self) -> f64 {
        self.inner.lon.value()
    }

    #[getter]
    fn elevation_m(&self) -> f64 {
        self.inner.height.value()
    }

    fn __repr__(&self) -> String {
        format!(
            "Observer(latitude_deg={}, longitude_deg={}, elevation_m={})",
            self.inner.lat.value(),
            self.inner.lon.value(),
            self.inner.height.value()
        )
    }
}

#[pyclass(name = "Target", frozen, module = "nsb")]
#[derive(Clone, Copy)]
pub(super) struct PyTarget {
    inner: Target,
}

impl PyTarget {
    pub(super) fn inner(&self) -> Target {
        self.inner
    }

    pub(super) fn from_inner(inner: Target) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyTarget {
    #[new]
    #[pyo3(signature = (*, ra_deg, dec_deg))]
    fn new(ra_deg: f64, dec_deg: f64) -> PyResult<Self> {
        finite("ra_deg", ra_deg)?;
        finite("dec_deg", dec_deg)?;

        if !(0.0..360.0).contains(&ra_deg) {
            return Err(invalid_input("ra_deg must be in [0, 360)"));
        }
        if !(-90.0..=90.0).contains(&dec_deg) {
            return Err(invalid_input("dec_deg must be in [-90, 90]"));
        }

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

    #[getter]
    fn reference_frame(&self) -> &'static str {
        "equatorial-mean-j2000"
    }

    fn __repr__(&self) -> String {
        format!(
            "Target(ra_deg={}, dec_deg={}, reference_frame='equatorial-mean-j2000')",
            self.inner.azimuth.value(),
            self.inner.polar.value()
        )
    }
}
