use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use qtty::angular::Degrees;
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::Period;

use crate::{PointQuery, ThresholdQuery};

use super::invalid_input;
use super::selectors::PyComponentMask;
use super::super::compat::{datetime_to_time, time_to_datetime, PyDirection, PyObserver};

fn finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be finite")))
    }
}

#[pyclass(name = "PointQuery", frozen, module = "nsb", skip_from_py_object)]
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
        observer: PyRef<'_, PyObserver>,
        time: &Bound<'_, PyAny>,
        target: PyRef<'_, PyDirection>,
        components: Option<PyRef<'_, PyComponentMask>>,
    ) -> PyResult<Self> {
        let mut query = PointQuery::new(
            observer.inner(),
            datetime_to_time(time, "time")?,
            target.inner(),
        );
        if let Some(components) = components {
            query = query.with_components(components.inner());
        }
        Ok(Self { inner: query })
    }

    #[getter]
    fn observer(&self) -> PyObserver {
        PyObserver::from_inner(self.inner.observer)
    }

    #[getter]
    fn time(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.inner.time)
    }

    #[getter]
    fn target(&self) -> PyDirection {
        PyDirection::from_inner(self.inner.target)
    }

    #[getter]
    fn components(&self) -> PyComponentMask {
        PyComponentMask::from_inner(self.inner.components)
    }
}

#[pyclass(name = "ThresholdQuery", frozen, module = "nsb", skip_from_py_object)]
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
        sample_step_s=600.0,
        sun_altitude_ceiling_deg=Some(-18.0),
        target_altitude_floor_deg=Some(0.0)
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        observer: PyRef<'_, PyObserver>,
        target: PyRef<'_, PyDirection>,
        start: &Bound<'_, PyAny>,
        end: &Bound<'_, PyAny>,
        threshold_photons_cm2_ns_sr: f64,
        components: Option<PyRef<'_, PyComponentMask>>,
        sample_step_s: f64,
        sun_altitude_ceiling_deg: Option<f64>,
        target_altitude_floor_deg: Option<f64>,
    ) -> PyResult<Self> {
        finite("threshold_photons_cm2_ns_sr", threshold_photons_cm2_ns_sr)?;
        if threshold_photons_cm2_ns_sr < 0.0 {
            return Err(invalid_input("threshold_photons_cm2_ns_sr must be non-negative"));
        }
        finite("sample_step_s", sample_step_s)?;
        if sample_step_s <= 0.0 {
            return Err(invalid_input("sample_step_s must be positive"));
        }
        if let Some(value) = sun_altitude_ceiling_deg {
            finite("sun_altitude_ceiling_deg", value)?;
        }
        if let Some(value) = target_altitude_floor_deg {
            finite("target_altitude_floor_deg", value)?;
        }

        let start = datetime_to_time(start, "start")?;
        let end = datetime_to_time(end, "end")?;
        let window = Period::try_new(start, end)
            .map_err(|error| invalid_input(format!("invalid UTC search window: {error}")))?;

        let mut query = ThresholdQuery::new(
            observer.inner(),
            target.inner(),
            window,
            BandPhotonRadiance::new(threshold_photons_cm2_ns_sr),
        )
        .with_sample_step(Second::new(sample_step_s))
        .with_sun_altitude_ceiling(sun_altitude_ceiling_deg.map(Degrees::new))
        .with_target_altitude_floor(target_altitude_flooor_deg.map(Degrees::new));
        if let Some(components) = components {
            query = query.with_components(components.inner());
        }

        Ok(Self { inner: query })
    }

    #[getter]
    fn observer(&self) -> PyObserver {
        PyObserver::from_inner(self.inner.observer)
    }

    #[getter]
    fn target(&self) -> PyDirection {
        PyDirection::from_inner(self.inner.target)
    }

    #[getter]
    fn start(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.inner.window.start)
    }

    #[getter]
    fn end(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.inner.window.end)
    }

    #[getter]
    fn threshold_photons_cm2_ns_sr(&self) -> f64 {
        self.inner.threshold.value()
    }

    #[getter]
    fn components(&self) -> PyComponentMask {
        PyComponentMask::from_inner(self.inner.components)
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
}
