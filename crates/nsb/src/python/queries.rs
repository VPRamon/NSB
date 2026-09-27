use chrono::{DateTime, FixedOffset, Utc};
use pyo3::prelude::*;
use qtty::angular::Degrees;
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::{Period, Time, UTC};

use crate::{PointQuery, ThresholdQuery};

use super::coordinates::{PyObserver, PyTarget};
use super::errors::invalid_input;
use super::selectors::PyComponentMask;

pub(super) fn py_datetime_to_time(value: DateTime<FixedOffset>) -> PyResult<Time<UTC>> {
    Time::<UTC>::try_from_chrono(value.with_timezone(&Utc))
        .map_err(|error| invalid_input(format!("datetime is outside tempoch UTC range: {error}")))
}

pub(super) fn time_to_py_datetime(value: Time<UTC>) -> PyResult<DateTime<Utc>> {
    value
        .try_to_chrono()
        .map_err(|error| invalid_input(format!("UTC instant is outside chrono range: {error}")))
}

fn validate_finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be finite")))
    }
}

fn validate_altitude(name: &str, value: f64) -> PyResult<()> {
    validate_finite(name, value)?;
    if (-90.0..=90.0).contains(&value) {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be in [-90, 90]")))
    }
}

#[pyclass(name = "PointQuery", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyPointQuery {
    inner: PointQuery,
}

impl PyPointQuery {
    pub(super) fn inner(&self) -> PointQuery {
        self.inner.clone()
    }
}

#[pymethods]
impl PyPointQuery {
    #[new]
    #[pyo3(signature = (observer, time, target, *, components=None))]
    fn new(
        observer: &PyObserver,
        time: DateTime<FixedOffset>,
        target: &PyTarget,
        components: Option<&PyComponentMask>,
    ) -> PyResult<Self> {
        let mut query = PointQuery::new(
            observer.inner(),
            py_datetime_to_time(time)?,
            target.inner(),
        );
        if let Some(components) = components {
            query = query.with_components(components.inner());
        }
        Ok(Self { inner: query })
    }

    fn with_components(&self, py: Python<'_>, components: &PyComponentMask) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self {
                inner: self.inner.clone().with_components(components.inner()),
            },
        )
    }

    #[getter]
    fn observer(&self, py: Python<'_>) -> PyResult<Py<PyObserver>> {
        Py::new(py, PyObserver::from_inner(self.inner.observer))
    }

    #[getter]
    fn time(&self) -> PyResult<DateTime<Utc>> {
        time_to_py_datetime(self.inner.time)
    }

    #[getter]
    fn target(&self, py: Python<'_>) -> PyResult<Py<PyTarget>> {
        Py::new(py, PyTarget::from_inner(self.inner.target))
    }

    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Py<PyComponentMask>> {
        Py::new(py, PyComponentMask::from_inner(self.inner.components))
    }
}

#[pyclass(name = "ThresholdQuery", frozen, module = "nsb")]
#[derive(Clone)]
pub(super) struct PyThresholdQuery {
    inner: ThresholdQuery,
}

impl PyThresholdQuery {
    pub(super) fn inner(&self) -> ThresholdQuery {
        self.inner.clone()
    }
}

#[pymethods]
impl PyThresholdQuery {
    #[new]
    #[pyo3(signature = (
        observer,
        target,
        start,
        end,
        threshold_photons_cm2_ns_sr,
        *,
        components=None,
        sample_step_s=600.0
    ))]
    fn new(
        observer: &PyObserver,
        target: &PyTarget,
        start: DateTime<FixedOffset>,
        end: DateTime<FixedOffset>,
        threshold_photons_cm2_ns_sr: f64,
        components: Option<&PyComponentMask>,
        sample_step_s: f64,
    ) -> PyResult<Self> {
        validate_finite(
            "threshold_photons_cm2_ns_sr",
            threshold_photons_cm2_ns_sr,
        )?;
        if threshold_photons_cm2_ns_sr < 0.0 {
            return Err(invalid_input(
                "threshold_photons_cm2_ns_sr must be non-negative",
            ));
        }
        validate_finite("sample_step_s", sample_step_s)?;
        if sample_step_s <= 0.0 {
            return Err(invalid_input("sample_step_s must be positive"));
        }

        let start = py_datetime_to_time(start)?;
        let end = py_datetime_to_time(end)?;
        let window = Period::try_new(start, end)
            .map_err(|error| invalid_input(format!("invalid UTC search window: {error}")))?;

        let mut query = ThresholdQuery::new(
            observer.inner(),
            target.inner(),
            window,
            BandPhotonRadiance::new(threshold_photons_cm2_ns_sr),
        )
        .with_sample_step(Second::new(sample_step_s));
        if let Some(components) = components {
            query = query.with_components(components.inner());
        }
        Ok(Self { inner: query })
    }

    fn with_components(&self, py: Python<'_>, components: &PyComponentMask) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self {
                inner: self.inner.clone().with_components(components.inner()),
            },
        )
    }

    fn with_sample_step_s(&self, py: Python<'_>, sample_step_s: f64) -> PyResult<Py<Self>> {
        validate_finite("sample_step_s", sample_step_s)?;
        if sample_step_s <= 0.0 {
            return Err(invalid_input("sample_step_s must be positive"));
        }
        Py::new(
            py,
            Self {
                inner: self
                    .inner
                    .clone()
                    .with_sample_step(Second::new(sample_step_s)),
            },
        )
    }

    fn with_sun_altitude_ceiling_deg(
        &self,
        py: Python<'_>,
        value: Option<f64>,
    ) -> PyResult<Py<Self>> {
        if let Some(value) = value {
            validate_altitude("sun_altitude_ceiling_deg", value)?;
        }
        Py::new(
            py,
            Self {
                inner: self
                    .inner
                    .clone()
                    .with_sun_altitude_ceiling(value.map(Degrees::new)),
            },
        )
    }

    fn with_target_altitude_floor_deg(
        &self,
        py: Python<'_>,
        value: Option<f64>,
    ) -> PyResult<Py<Self>> {
        if let Some(value) = value {
            validate_altitude("target_altitude_floor_deg", value)?;
        }
        Py::new(
            py,
            Self {
                inner: self
                    .inner
                    .clone()
                    .with_target_altitude_floor(value.map(Degrees::new)),
            },
        )
    }

    #[getter]
    fn observer(&self, py: Python<'_>) -> PyResult<Py<PyObserver>> {
        Py::new(py, PyObserver::from_inner(self.inner.observer))
    }

    #[getter]
    fn target(&self, py: Python<'_>) -> PyResult<Py<PyTarget>> {
        Py::new(py, PyTarget::from_inner(self.inner.target))
    }

    #[getter]
    fn start(&self) -> PyResult<DateTime<Utc>> {
        time_to_py_datetime(self.inner.window.start)
    }

    #[getter]
    fn end(&self) -> PyResult<DateTime<Utc>> {
        time_to_py_datetime(self.inner.window.end)
    }

    #[getter]
    fn threshold_photons_cm2_ns_sr(&self) -> f64 {
        self.inner.threshold.value()
    }

    #[getter]
    fn sample_step_s(&self) -> f64 {
        self.inner.sample_step.value()
    }

    #[getter]
    fn sun_altitude_ceiling_deg(&self) -> Option<f64> {
        self.inner.sun_altitude_ceiling.map(|value| value.value())
    }

    #[getter]
    fn target_altitude_floor_deg(&self) -> Option<f64> {
        self.inner.target_altitude_floor.map(|value| value.value())
    }

    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Py<PyComponentMask>> {
        Py::new(py, PyComponentMask::from_inner(self.inner.components))
    }
}
