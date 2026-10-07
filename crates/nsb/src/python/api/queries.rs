// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::units::angular::Degrees;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use crate::units::Second;
use pyo3::prelude::*;
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::TransformFrame;
use siderust_py::interop::{
    direction_from_python, direction_to_python, observer_from_python, observer_to_python,
};
use tempoch::{Period, Time, UTC};
use tempoch_py::interop::{datetime_to_time as upstream_datetime_to_time, time_to_datetime};

use crate::{ComponentMask, PointQuery, ThresholdQuery};

use super::invalid_input;

fn datetime_to_time(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Time<UTC>> {
    upstream_datetime_to_time(value).map_err(|error| invalid_input(format!("{name}: {error}")))
}

fn target_from_icrs(value: direction::ICRS) -> crate::Target {
    value.to_frame()
}

fn target_to_icrs(value: &crate::Target) -> direction::ICRS {
    value.to_frame()
}

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
        observer: &Bound<'_, PyAny>,
        time: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
        components: Option<ComponentMask>,
    ) -> PyResult<Self> {
        let target = target_from_icrs(direction_from_python(target)?);
        let mut query = Self::new(
            observer_from_python(observer)?,
            datetime_to_time(time, "time")?,
            target,
        );
        if let Some(components) = components {
            query = query.with_components(components);
        }
        Ok(query)
    }

    #[getter(observer)]
    fn py_observer(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        observer_to_python(py, &self.observer)
    }

    #[getter(time)]
    fn py_time<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        time_to_datetime(py, self.time)
    }

    #[getter(target)]
    fn py_target(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let target = target_to_icrs(&self.target);
        direction_to_python(py, &target)
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
        observer: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
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

        let target = target_from_icrs(direction_from_python(target)?);
        let mut query = Self::new(
            observer_from_python(observer)?,
            target,
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
    fn py_observer(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        observer_to_python(py, &self.observer)
    }

    #[getter(target)]
    fn py_target(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let target = target_to_icrs(&self.target);
        direction_to_python(py, &target)
    }

    #[getter(start)]
    fn py_start<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        time_to_datetime(py, self.window.start)
    }

    #[getter(end)]
    fn py_end<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        time_to_datetime(py, self.window.end)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wrapped_difference_degrees(left: f64, right: f64) -> f64 {
        ((left - right + 180.0).rem_euclid(360.0) - 180.0).abs()
    }

    #[test]
    fn icrs_target_conversion_applies_frame_bias_and_round_trips() {
        let icrs = direction::ICRS::new(Degrees::new(266.41683), Degrees::new(-29.00781));
        let target = target_from_icrs(icrs);

        let ra_shift = wrapped_difference_degrees(target.azimuth.value(), icrs.azimuth.value());
        let dec_shift = (target.polar.value() - icrs.polar.value()).abs();
        assert!(
            ra_shift > 1.0e-8 || dec_shift > 1.0e-8,
            "ICRS coordinates must not be numerically reinterpreted as EquatorialMeanJ2000"
        );
        assert!((target.azimuth.value() - 266.416_831_566_809).abs() < 1.0e-12);
        assert!((target.polar.value() + 29.007_807_821_057_128).abs() < 1.0e-12);

        let recovered = target_to_icrs(&target);
        assert!(
            wrapped_difference_degrees(recovered.azimuth.value(), icrs.azimuth.value()) < 1.0e-12
        );
        assert!((recovered.polar.value() - icrs.polar.value()).abs() < 1.0e-12);
    }
}
