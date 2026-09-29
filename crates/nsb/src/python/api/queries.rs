use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use crate::units::angular::Degrees;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use crate::units::Second;
use tempoch::Period;

use crate::{ComponentMask, PointQuery, ThresholdQuery};

use super::super::compat::{datetime_to_time, time_to_datetime, PyDirection, PyObserver};
use super::invalid_input;

fn finite(name: &str, value: f64) -> PyResult<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(invalid_input(format!("{name} must be finite")))
    }
}

#[pymethods]
impl PointQuery {
    #[new]
    #[pyo3(signature = (observer, time, target, *, components=None))]
    fn py_new(
        observer: PyObserver,
        time: &Bound<'_, PyAny>,
        target: PyDirection,
        components: Option<ComponentMask>,
    ) -> PyResult<Self> {
        let mut query = Self::new(
            observer.inner(),
            datetime_to_time(time, "time")?,
            target.inner(),
        );
        if let Some(components) = components {
            query = query.with_components(components);
        }
        Ok(query)
    }

    #[getter(observer)]
    fn py_observer(&self) -> PyObserver {
        PyObserver::from_inner(self.observer)
    }

    #[getter(time)]
    fn py_time(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.time)
    }

    #[getter(target)]
    fn py_target(&self) -> PyDirection {
        PyDirection::from_inner(self.target)
    }

    #[getter(components)]
    fn py_components(&self) -> ComponentMask {
        self.components
    }
}

#[pymethods]
impl ThresholdQuery {
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
    fn py_new(
        observer: PyObserver,
        target: PyDirection,
        start: &Bound<'_, PyAny>,
        end: &Bound<'_, PyAny>,
        threshold_photons_cm2_ns_sr: f64,
        components: Option<ComponentMask>,
        sample_step_s: f64,
        sun_altitude_ceiling_deg: Option<f64>,
        target_altitude_floor_deg: Option<f64>,
    ) -> PyResult<Self> {
        finite("threshold_photons_cm2_ns_sr", threshold_photons_cm2_ns_sr)?;
        if threshold_photons_cm2_ns_sr < 0.0 {
            return Err(invalid_input(
                "threshold_photons_cm2_ns_sr must be non-negative",
            ));
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

        let mut query = Self::new(
            observer.inner(),
            target.inner(),
            window,
            BandPhotonRadiance::new(threshold_photons_cm2_ns_sr),
        )
        .with_sample_step(Second::new(sample_step_s))
        .with_sun_altitude_ceiling(sun_altitude_ceiling_deg.map(Degrees::new))
        .with_target_altitude_floor(target_altitude_floor_deg.map(Degrees::new));
        if let Some(components) = components {
            query = query.with_components(components);
        }

        Ok(query)
    }

    #[getter(observer)]
    fn py_observer(&self) -> PyObserver {
        PyObserver::from_inner(self.observer)
    }

    #[getter(target)]
    fn py_target(&self) -> PyDirection {
        PyDirection::from_inner(self.target)
    }

    #[getter(start)]
    fn py_start(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.window.start)
    }

    #[getter(end)]
    fn py_end(&self) -> PyResult<DateTime<Utc>> {
        time_to_datetime(self.window.end)
    }

    #[getter(threshold_photons_cm2_ns_sr)]
    fn py_threshold_photons_cm2_ns_sr(&self) -> f64 {
        self.threshold.value()
    }

    #[getter(components)]
    fn py_components(&self) -> ComponentMask {
        self.components
    }

    #[getter(sample_step_s)]
    fn py_sample_step_s(&self) -> f64 {
        self.sample_step.value()
    }

    #[getter(sun_altitude_ceiling_deg)]
    fn py_sun_altitude_ceiling_deg(&self) -> Option<f64> {
        self.sun_altitude_ceiling.map(|value| value.value())
    }

    #[getter(target_altitude_floor_deg)]
    fn py_target_altitude_floor_deg(&self) -> Option<f64> {
        self.target_altitude_floor.map(|value| value.value())
    }
}
