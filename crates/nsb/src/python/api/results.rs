// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use pyo3::prelude::*;
use tempoch::{Period, TimeContext, UTC};
use tempoch_py::interop::period_to_datetimes_with;

use crate::{NsbComponent, NsbComponentMetadata, NsbResult, ThresholdQueryResult};

#[pyclass(
    name = "NsbComponentMetadata",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
#[derive(Clone)]
pub(in crate::python) struct PyNsbComponentMetadata {
    status: String,
    provenance: String,
    validated_domain: String,
    airglow_selection_kind: Option<String>,
    airglow_requested_model: Option<String>,
    airglow_resolved_model: Option<String>,
    airglow_used_automatic_fallback: Option<bool>,
    airglow_fallback_reason: Option<String>,
    airglow_physical_outcome: Option<String>,
    airglow_physical_zero_reason: Option<String>,
    solar_activity_provenance: Option<String>,
}

impl From<NsbComponentMetadata> for PyNsbComponentMetadata {
    fn from(value: NsbComponentMetadata) -> Self {
        let selection = value.airglow_selection.as_ref();
        let evaluation = value.airglow_evaluation.as_ref();
        Self {
            status: value.status.as_str().to_owned(),
            provenance: value.provenance.into_owned(),
            validated_domain: value.validated_domain.into_owned(),
            airglow_selection_kind: selection
                .map(|selection| selection.selection_kind.as_str().to_owned()),
            airglow_requested_model: selection
                .and_then(|selection| selection.requested_model)
                .map(|model| model.as_str().to_owned()),
            airglow_resolved_model: selection
                .and_then(|selection| selection.resolved_model)
                .map(|model| model.as_str().to_owned()),
            airglow_used_automatic_fallback: selection
                .map(|selection| selection.used_automatic_fallback),
            airglow_fallback_reason: selection
                .and_then(|selection| selection.fallback_reason)
                .map(|reason| reason.as_str().to_owned()),
            airglow_physical_outcome: evaluation
                .map(|evaluation| evaluation.physical_outcome.as_str().to_owned()),
            airglow_physical_zero_reason: evaluation
                .and_then(|evaluation| evaluation.physical_zero_reason)
                .map(|reason| reason.as_str().to_owned()),
            solar_activity_provenance: value
                .solar_activity
                .as_ref()
                .map(|solar| solar.provenance_fragment()),
        }
    }
}

#[pyclass(
    name = "NsbComponent",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
#[derive(Clone)]
pub(in crate::python) struct PyNsbComponent {
    name: String,
    integrated_photons_cm2_ns_sr: f64,
    b_flux_s10: f64,
    v_flux_s10: f64,
    relative_uncertainty: Option<f64>,
    statistical_uncertainty_photons_cm2_ns_sr: Option<f64>,
    systematic_uncertainty_photons_cm2_ns_sr: Option<f64>,
    total_uncertainty_photons_cm2_ns_sr: Option<f64>,
    metadata: PyNsbComponentMetadata,
}

impl From<NsbComponent> for PyNsbComponent {
    fn from(value: NsbComponent) -> Self {
        Self {
            name: value.name.to_owned(),
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_flux_s10: value.b_flux_s10.value(),
            v_flux_s10: value.v_flux_s10.value(),
            relative_uncertainty: value.relative_uncertainty,
            statistical_uncertainty_photons_cm2_ns_sr: value
                .statistical_uncertainty
                .map(|q| q.value()),
            systematic_uncertainty_photons_cm2_ns_sr: value
                .systematic_uncertainty
                .map(|q| q.value()),
            total_uncertainty_photons_cm2_ns_sr: value.total_uncertainty.map(|q| q.value()),
            metadata: value.metadata.into(),
        }
    }
}

#[pyclass(
    name = "NsbResult",
    frozen,
    module = "nsb",
    get_all,
    skip_from_py_object
)]
pub(in crate::python) struct PyNsbResult {
    integrated_photons_cm2_ns_sr: f64,
    b_mag_per_arcsec2: f64,
    v_mag_per_arcsec2: f64,
    components: Vec<PyNsbComponent>,
}

impl From<NsbResult> for PyNsbResult {
    fn from(value: NsbResult) -> Self {
        Self {
            integrated_photons_cm2_ns_sr: value.integrated.value(),
            b_mag_per_arcsec2: value.b_mag.value(),
            v_mag_per_arcsec2: value.v_mag.value(),
            components: value.components.into_iter().map(Into::into).collect(),
        }
    }
}

#[pyclass(
    name = "ThresholdQueryResult",
    frozen,
    module = "nsb",
    skip_from_py_object
)]
pub(in crate::python) struct PyThresholdQueryResult {
    threshold_photons_cm2_ns_sr: f64,
    periods: Vec<Period<UTC>>,
    // Validation and the later getter must use the same upstream civil-time data.
    time_context: TimeContext,
}

impl PyThresholdQueryResult {
    pub(in crate::python) fn try_from_inner(value: ThresholdQueryResult) -> crate::Result<Self> {
        let time_context = TimeContext::new();
        let value = python_periods(value, &time_context)?;
        Ok(Self {
            threshold_photons_cm2_ns_sr: value.threshold.value(),
            periods: value.periods,
            time_context,
        })
    }
}

#[pymethods]
impl PyThresholdQueryResult {
    #[getter]
    fn threshold_photons_cm2_ns_sr(&self) -> f64 {
        self.threshold_photons_cm2_ns_sr
    }

    #[getter]
    fn periods(&self, py: Python<'_>) -> PyResult<Vec<(Py<PyAny>, Py<PyAny>)>> {
        self.periods
            .iter()
            .map(|period| {
                let (start, end) = period_to_datetimes_with(py, *period, &self.time_context)?;
                Ok((start.unbind(), end.unbind()))
            })
            .collect()
    }
}

// Rust instants retain their native precision and range. Only Python results
// project to civil microseconds, using ceil(start) and floor(end).
fn python_periods(
    mut result: ThresholdQueryResult,
    context: &TimeContext,
) -> crate::Result<ThresholdQueryResult> {
    use chrono::{Datelike, Timelike};
    use tempoch::{Time, TT};
    let error = || {
        crate::NsbError::Unsupported(
            "schedule boundary cannot be represented inward by Python datetime".into(),
        )
    };
    for period in &mut result.periods {
        let original = *period;
        let endpoint = |value: Time<UTC>, start: bool| -> crate::Result<Time<UTC>> {
            let civil = value.try_to_chrono_with(context).map_err(|_| error())?;
            if !(1..=9999).contains(&civil.year()) || civil.nanosecond() >= 1_000_000_000 {
                return Err(error());
            }
            let mut civil = civil
                .with_nanosecond(civil.nanosecond() / 1000 * 1000)
                .ok_or_else(error)?;
            // Chrono rounds native instants to nanoseconds. The truncated
            // microsecond can lie on either side of a sub-nanosecond endpoint;
            // at most one inward microsecond step is needed to resolve that.
            for _ in 0..2 {
                if !(1..=9999).contains(&civil.year()) || civil.nanosecond() >= 1_000_000_000 {
                    return Err(error());
                }
                let projected =
                    Time::<UTC>::try_from_chrono_with(civil, context).map_err(|_| error())?;
                if projected >= original.start
                    && projected <= original.end
                    && projected.to::<TT>() >= original.start.to::<TT>()
                    && projected.to::<TT>() <= original.end.to::<TT>()
                    && (projected - value).abs() <= crate::units::Second::new(0.1)
                    && projected.try_to_chrono_with(context).map_err(|_| error())? == civil
                {
                    return Ok(projected);
                }
                civil = civil
                    .checked_add_signed(chrono::Duration::microseconds(if start { 1 } else { -1 }))
                    .ok_or_else(error)?;
            }
            Err(error())
        };
        let start = endpoint(original.start, true)?;
        let end = endpoint(original.end, false)?;
        // Check both native ordering and the exact civil labels the upstream
        // period_to_datetimes_with getter will export; no collapsed tuple.
        if start >= end
            || start.try_to_chrono_with(context).map_err(|_| error())?
                >= end.try_to_chrono_with(context).map_err(|_| error())?
        {
            return Err(error());
        }
        *period = Period::new(start, end);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{
        radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance, Second,
    };
    use chrono::{DateTime, Utc};
    use tempoch::{Time, TT};

    fn result(period: Period<UTC>) -> ThresholdQueryResult {
        ThresholdQueryResult {
            threshold: Radiance::new(0.0),
            periods: vec![period],
        }
    }

    fn offset(base: Time<UTC>, nanoseconds: f64) -> Time<UTC> {
        if nanoseconds.fract() == 0.0 {
            Time::<UTC>::from_chrono(
                base.to_chrono().unwrap() + chrono::Duration::nanoseconds(nanoseconds as i64),
            )
        } else {
            base + Second::new(nanoseconds * 1e-9)
        }
    }

    #[test]
    fn python_projection_rejects_interval_collapsing_at_datetime_resolution() {
        let base = Time::<UTC>::from_chrono("2026-01-01T00:00:00Z".parse().unwrap());
        for (a, b) in [(0.0, 800.0), (100.0, 900.0), (100.0, 1800.0), (0.1, 1999.9)] {
            let period = Period::new(offset(base, a), offset(base, b));
            assert!(
                matches!(
                    PyThresholdQueryResult::try_from_inner(result(period)),
                    Err(crate::NsbError::Unsupported(_))
                ),
                "{a}..{b} ns"
            );
        }
    }

    #[test]
    fn python_projection_survives_the_actual_result_getter_inside_native_bounds() {
        Python::initialize();
        Python::attach(|py| {
            for label in [
                "2026-01-01T00:00:00Z",
                "2026-01-01T23:59:59.999999Z",
                "2016-12-31T23:59:59.999990Z",
                "2017-01-01T00:00:00Z",
            ] {
                let base = Time::<UTC>::from_chrono(label.parse().unwrap());
                for (a, b, expected_a, expected_b) in [
                    (0.0, 1000.0, 0, 1000),
                    (100.0, 2800.0, 1000, 2000),
                    (999.0, 3001.0, 1000, 3000),
                    (0.1, 2999.9, 1000, 2000),
                ] {
                    let original = Period::new(offset(base, a), offset(base, b));
                    let value = Py::new(
                        py,
                        PyThresholdQueryResult::try_from_inner(result(original))
                            .unwrap_or_else(|error| panic!("{label}: {a}..{b}: {error}")),
                    )
                    .unwrap();
                    let periods: Vec<(DateTime<Utc>, DateTime<Utc>)> = value
                        .bind(py)
                        .getattr("periods")
                        .unwrap()
                        .extract()
                        .unwrap();
                    let (start, end) = periods[0];
                    assert!(start < end, "{label}: {a}..{b}");
                    let base_civil = base.to_chrono().unwrap();
                    assert_eq!(
                        start,
                        base_civil + chrono::Duration::nanoseconds(expected_a)
                    );
                    assert_eq!(end, base_civil + chrono::Duration::nanoseconds(expected_b));
                    for (exported, native, is_start) in
                        [(start, original.start, true), (end, original.end, false)]
                    {
                        let back = Time::<UTC>::from_chrono(exported);
                        assert!(if is_start {
                            back >= native && back.to::<TT>() >= native.to::<TT>()
                        } else {
                            back <= native && back.to::<TT>() <= native.to::<TT>()
                        });
                    }
                }
            }
        });
    }

    #[test]
    fn python_projection_rejects_leap_labels_and_python_year_overflow() {
        for label in ["2016-12-31T23:59:60Z", "+10000-01-01T00:00:00Z"] {
            let start = Time::<UTC>::from_chrono(label.parse().unwrap());
            assert!(matches!(
                PyThresholdQueryResult::try_from_inner(result(Period::new(
                    start,
                    start + Second::new(1.0)
                ))),
                Err(crate::NsbError::Unsupported(_))
            ));
        }
        // Native UTC can opt into pre-definition civil labels; Python still
        // has its independent lower year limit even with that explicit policy.
        let context = TimeContext::new().allow_pre_definition_utc();
        let start =
            Time::<UTC>::try_from_chrono_with("0000-01-01T00:00:00Z".parse().unwrap(), &context)
                .unwrap();
        assert!(matches!(
            python_periods(
                result(Period::new(start, start + Second::new(1.0))),
                &context
            ),
            Err(crate::NsbError::Unsupported(_))
        ));

        // Rounding across a leap label must not skip a physical second when
        // the adapter's maximum inward displacement is only 0.1 seconds.
        let start = Time::<UTC>::from_chrono("2016-12-31T23:59:59.999999900Z".parse().unwrap());
        let end = Time::<UTC>::from_chrono("2017-01-01T00:00:00.000002Z".parse().unwrap());
        assert!(PyThresholdQueryResult::try_from_inner(result(Period::new(start, end))).is_err());
    }
}
