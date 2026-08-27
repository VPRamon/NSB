//! Monthly-averaged F10.7 evidence for the Noll/SkyCalc `msolflux` quantity.
//!
//! Partial-month averages must never masquerade as completed monthly means.

use super::record::{parse_date, parse_datetime, F107Kind, F107Record};
use super::store::F107Store;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use std::collections::BTreeMap;

/// How a monthly F10.7 value was obtained for Airglow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonthlyCompleteness {
    /// Finalized monthly observed index for a completed calendar month.
    CompleteObserved,
    /// Arithmetic mean of daily 45-day forecasts covering every day of the month.
    CompleteForecast,
    /// Observed dailies to date + forecast dailies for remaining days, full coverage.
    ProvisionalObservedPlusForecast,
    /// Official SWPC monthly solar-cycle prediction.
    OfficialMonthlyPrediction,
    /// Documented climatological fallback.
    Climatology,
}

impl MonthlyCompleteness {
    /// Stable identifier for metadata / CLI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompleteObserved => "complete-observed",
            Self::CompleteForecast => "complete-forecast",
            Self::ProvisionalObservedPlusForecast => "provisional-observed-plus-forecast",
            Self::OfficialMonthlyPrediction => "official-monthly-prediction",
            Self::Climatology => "climatology",
        }
    }
}

/// Provenance-carrying monthly F10.7 evidence selected for Airglow.
#[derive(Debug, Clone, PartialEq)]
pub struct MonthlyF107Evidence {
    /// Monthly-averaged F10.7 in sfu.
    pub value_sfu: f64,
    /// Completeness / method classification.
    pub method: MonthlyCompleteness,
    /// First calendar day of the month.
    pub month_start: NaiveDate,
    /// Last calendar day of the month.
    pub month_end: NaiveDate,
    /// Number of observed daily values contributing to a derived estimate.
    pub observed_days: u32,
    /// Number of forecast daily values contributing to a derived estimate.
    pub forecast_days: u32,
    /// Calendar days in the month.
    pub total_days: u32,
    /// Provider identity.
    pub provider: String,
    /// Forecast issuance timestamp when applicable.
    pub forecast_issued_at_utc: Option<String>,
    /// Retrieval timestamp when applicable.
    pub retrieved_at_utc: Option<String>,
    /// Record exposed in scientific metadata.
    pub record: F107Record,
    /// Resolver precedence step label.
    pub resolution_step: &'static str,
}

/// Inclusive calendar-month bounds for `date`.
pub fn month_bounds_for(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start");
    let end = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
    .expect("valid next month")
        - Duration::days(1);
    (start, end)
}

/// Number of calendar days in the month containing `date`.
pub fn days_in_month(date: NaiveDate) -> u32 {
    let (start, end) = month_bounds_for(date);
    (end - start).num_days() as u32 + 1
}

/// Whether a monthly observed product represents a **finalized** completed month
/// relative to evidence time `evidence_as_of` (typically store/record retrieval date).
///
/// A month is finalized only when:
/// - cadence is monthly observed;
/// - product is the finalized SWPC monthly index (not month-to-date / provisional);
/// - its last calendar day is strictly before `evidence_as_of`.
pub fn is_finalized_monthly_observation(record: &F107Record, evidence_as_of: NaiveDate) -> bool {
    if record.kind != F107Kind::Observed {
        return false;
    }
    if record.cadence.as_deref() != Some("monthly") {
        return false;
    }
    // Month-to-date / provisional products must never count as completed msolflux.
    if record.product.contains("month-to-date") || record.product.contains("provisional") {
        return false;
    }
    let Ok((_, month_end)) = record.validity_window() else {
        return false;
    };
    month_end < evidence_as_of
}

/// True when `forecast_issued_at_utc` is at or before `requested_at` (no future leakage).
///
/// Records without issuance cannot participate in forecast-derived Airglow inputs
/// that require issuance (45-day). Official monthly cycle predictions without
/// issuance are allowed only via [`select_official_monthly_prediction`].
pub fn forecast_issued_not_after(
    issued_at_utc: &str,
    requested_at: NaiveDateTime,
) -> Result<bool, String> {
    let issued = parse_datetime(issued_at_utc, "forecast_issued_at_utc").map_err(|e| e.0)?;
    Ok(issued <= requested_at)
}

fn parse_requested_at(requested_date: NaiveDate, requested_at: NaiveDateTime) -> NaiveDateTime {
    // Caller always supplies the evaluation instant; keep date consistent.
    let _ = requested_date;
    requested_at
}

/// Resolve monthly-compatible Airglow F10.7 evidence from a store.
pub fn resolve_monthly_evidence(
    store: &F107Store,
    requested_date: NaiveDate,
    requested_at: NaiveDateTime,
) -> Result<MonthlyF107Evidence, String> {
    let _ = parse_requested_at(requested_date, requested_at);
    let (month_start, month_end) = month_bounds_for(requested_date);
    let total_days = days_in_month(requested_date);

    if let Some(evidence) =
        try_complete_observed(store, requested_date, month_start, month_end, total_days)?
    {
        return Ok(evidence);
    }

    if let Some(evidence) = try_complete_45_day_forecast_mean(
        store,
        requested_date,
        requested_at,
        month_start,
        month_end,
        total_days,
    )? {
        return Ok(evidence);
    }

    if let Some(evidence) = try_provisional_observed_plus_forecast(
        store,
        requested_date,
        requested_at,
        month_start,
        month_end,
        total_days,
    )? {
        return Ok(evidence);
    }

    if let Some(evidence) =
        try_official_monthly_prediction(store, requested_date, month_start, month_end, total_days)?
    {
        return Ok(evidence);
    }

    Ok(MonthlyF107Evidence {
        value_sfu: store.climatology_sfu,
        method: MonthlyCompleteness::Climatology,
        month_start,
        month_end,
        observed_days: 0,
        forecast_days: 0,
        total_days,
        provider: "nsb".into(),
        forecast_issued_at_utc: None,
        retrieved_at_utc: store.retrieved_at_utc.clone(),
        record: super::record::climatology_record(
            requested_date,
            store.climatology_sfu,
            "documented-climatology-fallback",
        ),
        resolution_step: "climatology-fallback",
    })
}

fn try_complete_observed(
    store: &F107Store,
    requested_date: NaiveDate,
    month_start: NaiveDate,
    month_end: NaiveDate,
    total_days: u32,
) -> Result<Option<MonthlyF107Evidence>, String> {
    let evidence_as_of = store
        .retrieved_at_utc
        .as_deref()
        .and_then(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|dt| dt.naive_utc().date())
        })
        .or_else(|| {
            store
                .records
                .iter()
                .filter_map(|r| r.retrieved_at_utc.as_deref())
                .filter_map(|value| {
                    chrono::DateTime::parse_from_rfc3339(value)
                        .ok()
                        .map(|dt| dt.naive_utc().date())
                })
                .max()
        })
        // Without retrieval provenance, require the month to have ended before
        // the requested date (strict historical completeness).
        .unwrap_or(requested_date);

    let mut matches = Vec::new();
    for record in &store.records {
        if !is_finalized_monthly_observation(record, evidence_as_of) {
            continue;
        }
        if !record.covers(requested_date).map_err(|e| e.0)? {
            continue;
        }
        matches.push(record);
    }
    if matches.is_empty() {
        return Ok(None);
    }
    matches.sort_by_key(|record| {
        (
            validity_span_days(record).unwrap_or(u32::MAX),
            std::cmp::Reverse(record.product.as_str()),
        )
    });
    let record = matches[0].clone();
    Ok(Some(MonthlyF107Evidence {
        value_sfu: record.value_sfu,
        method: MonthlyCompleteness::CompleteObserved,
        month_start,
        month_end,
        observed_days: total_days,
        forecast_days: 0,
        total_days,
        provider: record.provider.clone(),
        forecast_issued_at_utc: None,
        retrieved_at_utc: record.retrieved_at_utc.clone(),
        record,
        resolution_step: "monthly-observed-complete",
    }))
}

fn try_complete_45_day_forecast_mean(
    store: &F107Store,
    requested_date: NaiveDate,
    requested_at: NaiveDateTime,
    month_start: NaiveDate,
    month_end: NaiveDate,
    total_days: u32,
) -> Result<Option<MonthlyF107Evidence>, String> {
    let by_day = collect_time_valid_45_day_by_day(store, requested_date, requested_at)?;
    if by_day.len() as u32 != total_days {
        return Ok(None);
    }
    // Every calendar day must be present.
    let mut day = month_start;
    while day <= month_end {
        if !by_day.contains_key(&day) {
            return Ok(None);
        }
        day += Duration::days(1);
    }
    Ok(Some(build_forecast_month_mean(
        &by_day,
        month_start,
        month_end,
        total_days,
        MonthlyCompleteness::CompleteForecast,
        "45-day-forecast-monthly-mean",
        "short-range-forecast-monthly-mean-complete",
        0,
        total_days,
    )?))
}

fn try_provisional_observed_plus_forecast(
    store: &F107Store,
    requested_date: NaiveDate,
    requested_at: NaiveDateTime,
    month_start: NaiveDate,
    month_end: NaiveDate,
    total_days: u32,
) -> Result<Option<MonthlyF107Evidence>, String> {
    // Only for the current (incomplete-at-evaluation) month.
    if month_end < requested_date {
        return Ok(None);
    }

    let mut by_day: BTreeMap<NaiveDate, F107Record> = BTreeMap::new();
    let mut observed_days = 0u32;
    let mut forecast_days = 0u32;

    // Observed dailies for days on/before the evaluation date.
    for record in &store.records {
        if record.kind != F107Kind::Observed || record.cadence.as_deref() != Some("daily") {
            continue;
        }
        let day = parse_date(&record.date, "date").map_err(|e| e.0)?;
        if day.year() != requested_date.year() || day.month() != requested_date.month() {
            continue;
        }
        if day > requested_date {
            continue;
        }
        record.validate().map_err(|e| e.0)?;
        if by_day.insert(day, record.clone()).is_none() {
            observed_days += 1;
        }
    }

    let forecast_by_day = collect_time_valid_45_day_by_day(store, requested_date, requested_at)?;
    // Forecast fills only remaining days after the evaluation date.
    for day_offset in 0..total_days {
        let day = month_start + Duration::days(i64::from(day_offset));
        if day <= requested_date {
            if !by_day.contains_key(&day) {
                return Ok(None); // gap in observed-to-date coverage
            }
            continue;
        }
        let Some(forecast) = forecast_by_day.get(&day) else {
            return Ok(None); // incomplete remaining-month forecast coverage
        };
        by_day.insert(day, forecast.clone());
        forecast_days += 1;
    }

    if by_day.len() as u32 != total_days {
        return Ok(None);
    }
    if observed_days == 0 {
        return Ok(None);
    }

    let mut evidence = build_forecast_month_mean(
        &by_day,
        month_start,
        month_end,
        total_days,
        MonthlyCompleteness::ProvisionalObservedPlusForecast,
        "current-month-observed-plus-forecast-mean",
        "provisional-current-month-observed-plus-forecast",
        observed_days,
        forecast_days,
    )?;
    evidence.record.kind = F107Kind::Forecast;
    evidence.observed_days = observed_days;
    evidence.forecast_days = forecast_days;
    Ok(Some(evidence))
}

fn try_official_monthly_prediction(
    store: &F107Store,
    requested_date: NaiveDate,
    month_start: NaiveDate,
    month_end: NaiveDate,
    total_days: u32,
) -> Result<Option<MonthlyF107Evidence>, String> {
    let mut matches = Vec::new();
    for record in &store.records {
        if record.kind != F107Kind::Forecast {
            continue;
        }
        if record.cadence.as_deref() != Some("monthly") {
            continue;
        }
        if record.product.contains("45-day") {
            continue;
        }
        if !record.covers(requested_date).map_err(|e| e.0)? {
            continue;
        }
        matches.push(record);
    }
    if matches.is_empty() {
        return Ok(None);
    }
    matches.sort_by(|a, b| {
        b.forecast_issued_at_utc
            .cmp(&a.forecast_issued_at_utc)
            .then_with(|| a.product.cmp(&b.product))
            .then_with(|| a.value_sfu.to_bits().cmp(&b.value_sfu.to_bits()))
    });
    let record = matches[0].clone();
    Ok(Some(MonthlyF107Evidence {
        value_sfu: record.value_sfu,
        method: MonthlyCompleteness::OfficialMonthlyPrediction,
        month_start,
        month_end,
        observed_days: 0,
        forecast_days: 0,
        total_days,
        provider: record.provider.clone(),
        forecast_issued_at_utc: record.forecast_issued_at_utc.clone(),
        retrieved_at_utc: record.retrieved_at_utc.clone(),
        record,
        resolution_step: "monthly-solar-cycle-forecast",
    }))
}

fn collect_time_valid_45_day_by_day(
    store: &F107Store,
    requested_date: NaiveDate,
    requested_at: NaiveDateTime,
) -> Result<BTreeMap<NaiveDate, F107Record>, String> {
    let year = requested_date.year();
    let month = requested_date.month();
    let mut by_day: BTreeMap<NaiveDate, F107Record> = BTreeMap::new();

    for record in &store.records {
        if record.kind != F107Kind::Forecast {
            continue;
        }
        if record.product != "45-day-forecast" {
            continue;
        }
        if record.cadence.as_deref() != Some("daily") {
            continue;
        }
        let Some(issued) = record.forecast_issued_at_utc.as_deref() else {
            continue;
        };
        if !forecast_issued_not_after(issued, requested_at)? {
            continue;
        }
        let day = parse_date(&record.date, "date").map_err(|e| e.0)?;
        if day.year() != year || day.month() != month {
            continue;
        }
        record.validate().map_err(|e| e.0)?;
        match by_day.get(&day) {
            None => {
                by_day.insert(day, record.clone());
            }
            Some(prior) => {
                if record.forecast_issued_at_utc > prior.forecast_issued_at_utc {
                    by_day.insert(day, record.clone());
                }
            }
        }
    }
    Ok(by_day)
}

#[allow(clippy::too_many_arguments)]
fn build_forecast_month_mean(
    by_day: &BTreeMap<NaiveDate, F107Record>,
    month_start: NaiveDate,
    month_end: NaiveDate,
    total_days: u32,
    method: MonthlyCompleteness,
    product: &str,
    resolution_step: &'static str,
    observed_days: u32,
    forecast_days: u32,
) -> Result<MonthlyF107Evidence, String> {
    let days: Vec<_> = by_day.values().collect();
    let mean = days.iter().map(|r| r.value_sfu).sum::<f64>() / days.len() as f64;
    if !mean.is_finite() || mean <= 0.0 {
        return Err("monthly aggregation produced a non-positive mean".into());
    }
    let issued = days
        .iter()
        .filter_map(|r| r.forecast_issued_at_utc.as_ref())
        .max()
        .cloned();
    let provider = days[0].provider.clone();
    let retrieved = days
        .iter()
        .filter_map(|r| r.retrieved_at_utc.as_ref())
        .max()
        .cloned();
    let source_locator = days[0].source_locator.clone();
    let provenance = format!(
        "derived aggregation={} method={} observed_days={observed_days} forecast_days={forecast_days} total_days={total_days} complete={} source={}",
        "arithmetic-calendar-month-mean",
        method.as_str(),
        by_day.len() as u32 == total_days,
        source_locator.unwrap_or_else(|| "45-day-forecast".into()),
    );

    let record = F107Record {
        date: month_start.format("%Y-%m-%d").to_string(),
        value_sfu: mean,
        kind: F107Kind::Forecast,
        provider: provider.clone(),
        product: product.into(),
        observation_date: None,
        forecast_issued_at_utc: issued.clone(),
        retrieved_at_utc: retrieved.clone(),
        valid_from: Some(month_start.format("%Y-%m-%d").to_string()),
        valid_through: Some(month_end.format("%Y-%m-%d").to_string()),
        cadence: Some("monthly".into()),
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: Some(provenance),
    };

    Ok(MonthlyF107Evidence {
        value_sfu: mean,
        method,
        month_start,
        month_end,
        observed_days,
        forecast_days,
        total_days,
        provider,
        forecast_issued_at_utc: issued,
        retrieved_at_utc: retrieved,
        record,
        resolution_step,
    })
}

fn validity_span_days(record: &F107Record) -> Option<u32> {
    let (from, through) = record.validity_window().ok()?;
    Some((through - from).num_days().max(0) as u32)
}
