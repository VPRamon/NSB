//! Temporary Python `datetime` compatibility for tempoch UTC values.
//!
//! `tempoch` remains the canonical time implementation. This module only owns
//! conversion at the Python boundary until a reusable tempoch Python package is
//! available. Keep all Python datetime/period adaptation here so migration is a
//! deletion plus facade import change rather than an NSB scientific refactor.

use chrono::{DateTime, FixedOffset, Utc};
use pyo3::prelude::*;
use tempoch::{Period, Time, UTC};

use super::super::api::invalid_input;

fn aware_datetime(value: &Bound<'_, PyAny>, name: &str) -> PyResult<DateTime<FixedOffset>> {
    let offset = value
        .call_method0("utcoffset")
        .map_err(|_| invalid_input(format!("{name} must be a timezone-aware datetime.datetime")))?;
    if offset.is_none() {
        return Err(invalid_input(format!(
            "{name} must be timezone-aware; naive datetimes are not accepted"
        )));
    }

    value
        .extract::<DateTime<FixedOffset>>()
        .map_err(|_| invalid_input(format!("{name} must be a timezone-aware datetime.datetime")))
}

pub(super) fn datetime_to_time(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Time<UTC>> {
    let value = aware_datetime(value, name)?;
    Time::<UTC>::try_from_chrono(value.with_timezone(&Utc))
        .map_err(|error| invalid_input(format!("{name} is outside tempoch UTC range: {error}")))
}

pub(super) fn time_to_datetime(value: Time<UTC>) -> PyResult<DateTime<Utc>> {
    value
        .try_to_chrono()
        .map_err(|error| invalid_input(format!("UTC instant is outside chrono range: {error}")))
}

pub(super) fn period_to_datetimes(period: Period<UTC>) -> PyResult<(DateTime<Utc>, DateTime<Utc>)> {
    Ok((
        time_to_datetime(period.start)?,
        time_to_datetime(period.end)?,
    ))
}
