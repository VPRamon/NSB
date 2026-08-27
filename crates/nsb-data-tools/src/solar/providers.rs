//! Parsers for NOAA/NWS SWPC F10.7 products (fixture-friendly).

use anyhow::{bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use nsb::{F107Kind, F107Record};
use serde::Deserialize;
use serde_json::Value;

const PROVIDER: &str = "noaa-swpc";

/// Parse SWPC `daily-solar-indices.txt` (DSD) into daily observed records.
pub fn parse_daily_solar_indices(text: &str, retrieved_at_utc: &str) -> Result<Vec<F107Record>> {
    let mut records = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(':') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            bail!(
                "daily-solar-indices line {}: expected >=4 columns",
                line_no + 1
            );
        }
        let year: i32 = parts[0]
            .parse()
            .with_context(|| format!("daily-solar-indices line {}: year", line_no + 1))?;
        let month: u32 = parts[1]
            .parse()
            .with_context(|| format!("daily-solar-indices line {}: month", line_no + 1))?;
        let day: u32 = parts[2]
            .parse()
            .with_context(|| format!("daily-solar-indices line {}: day", line_no + 1))?;
        let flux: f64 = parts[3]
            .parse()
            .with_context(|| format!("daily-solar-indices line {}: flux", line_no + 1))?;
        if !flux.is_finite() || flux <= 0.0 {
            // SWPC uses sentinels / missing markers; skip rather than poison the store.
            continue;
        }
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .with_context(|| format!("daily-solar-indices line {}: invalid date", line_no + 1))?
            .format("%Y-%m-%d")
            .to_string();
        records.push(F107Record {
            date: date.clone(),
            value_sfu: flux,
            kind: F107Kind::Observed,
            provider: PROVIDER.into(),
            product: "daily-solar-indices".into(),
            observation_date: Some(date.clone()),
            forecast_issued_at_utc: None,
            retrieved_at_utc: Some(retrieved_at_utc.into()),
            valid_from: Some(date.clone()),
            valid_through: Some(date),
            cadence: Some("daily".into()),
            uncertainty_sfu: None,
            range_low_sfu: None,
            range_high_sfu: None,
            source_locator: Some(
                "https://services.swpc.noaa.gov/text/daily-solar-indices.txt".into(),
            ),
        });
    }
    Ok(records)
}

#[derive(Debug, Deserialize)]
struct Forecast45Envelope {
    issued: String,
    data: Vec<Forecast45Datum>,
}

#[derive(Debug, Deserialize)]
struct Forecast45Datum {
    time: String,
    metric: String,
    value: f64,
}

/// Parse SWPC `45-day-forecast.json`.
pub fn parse_45_day_forecast_json(bytes: &[u8], retrieved_at_utc: &str) -> Result<Vec<F107Record>> {
    let envelope: Forecast45Envelope =
        serde_json::from_slice(bytes).context("45-day-forecast.json schema mismatch")?;
    if envelope.issued.trim().is_empty() {
        bail!("45-day-forecast.json missing issued timestamp");
    }
    let mut records = Vec::new();
    for item in envelope.data {
        if item.metric != "f107" {
            continue;
        }
        if !item.value.is_finite() || item.value <= 0.0 {
            bail!("45-day-forecast.json contains non-positive f107 value");
        }
        let date = item
            .time
            .get(..10)
            .context("45-day-forecast.json time missing YYYY-MM-DD prefix")?
            .to_string();
        NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .with_context(|| format!("45-day-forecast.json invalid date {date}"))?;
        records.push(F107Record {
            date: date.clone(),
            value_sfu: item.value,
            kind: F107Kind::Forecast,
            provider: PROVIDER.into(),
            product: "45-day-forecast".into(),
            observation_date: None,
            forecast_issued_at_utc: Some(envelope.issued.clone()),
            retrieved_at_utc: Some(retrieved_at_utc.into()),
            valid_from: Some(date.clone()),
            valid_through: Some(date),
            cadence: Some("daily".into()),
            uncertainty_sfu: None,
            range_low_sfu: None,
            range_high_sfu: None,
            source_locator: Some("https://services.swpc.noaa.gov/json/45-day-forecast.json".into()),
        });
    }
    if records.is_empty() {
        bail!("45-day-forecast.json contained no f107 metrics");
    }
    Ok(records)
}

/// Parse SWPC `predicted-solar-cycle.json` monthly predictions.
pub fn parse_predicted_solar_cycle(
    bytes: &[u8],
    retrieved_at_utc: &str,
) -> Result<Vec<F107Record>> {
    let value: Value =
        serde_json::from_slice(bytes).context("predicted-solar-cycle.json is not JSON")?;
    let rows = value
        .as_array()
        .context("predicted-solar-cycle.json must be a JSON array")?;
    let mut records = Vec::new();
    for row in rows {
        let time_tag = row
            .get("time-tag")
            .and_then(Value::as_str)
            .context("predicted-solar-cycle.json row missing time-tag")?;
        let predicted = row
            .get("predicted_f10.7")
            .and_then(Value::as_f64)
            .context("predicted-solar-cycle.json row missing predicted_f10.7")?;
        if !predicted.is_finite() || predicted <= 0.0 {
            bail!("predicted-solar-cycle.json non-positive predicted_f10.7");
        }
        let (valid_from, valid_through) = month_bounds(time_tag)?;
        let low = row.get("low_f10.7").and_then(Value::as_f64);
        let high = row.get("high_f10.7").and_then(Value::as_f64);
        records.push(F107Record {
            date: valid_from.clone(),
            value_sfu: predicted,
            kind: F107Kind::Forecast,
            provider: PROVIDER.into(),
            product: "predicted-solar-cycle".into(),
            observation_date: None,
            // Product has no issuance timestamp; pin retrieval time as issuance proxy
            // and keep product identity explicit in metadata.
            forecast_issued_at_utc: Some(retrieved_at_utc.into()),
            retrieved_at_utc: Some(retrieved_at_utc.into()),
            valid_from: Some(valid_from),
            valid_through: Some(valid_through),
            cadence: Some("monthly".into()),
            uncertainty_sfu: None,
            range_low_sfu: low,
            range_high_sfu: high,
            source_locator: Some(
                "https://services.swpc.noaa.gov/json/solar-cycle/predicted-solar-cycle.json".into(),
            ),
        });
    }
    if records.is_empty() {
        bail!("predicted-solar-cycle.json contained no rows");
    }
    Ok(records)
}

/// Parse a truncated/sample `observed-solar-cycle-indices.json` array.
pub fn parse_observed_solar_cycle_indices(
    bytes: &[u8],
    retrieved_at_utc: &str,
) -> Result<Vec<F107Record>> {
    let value: Value =
        serde_json::from_slice(bytes).context("observed-solar-cycle-indices is not JSON")?;
    let rows = value
        .as_array()
        .context("observed-solar-cycle-indices must be a JSON array")?;
    let mut records = Vec::new();
    for row in rows {
        let time_tag = row
            .get("time-tag")
            .and_then(Value::as_str)
            .context("observed indices row missing time-tag")?;
        let flux = row
            .get("f10.7")
            .and_then(Value::as_f64)
            .context("observed indices row missing f10.7")?;
        if !flux.is_finite() || flux <= 0.0 {
            continue;
        }
        let (valid_from, valid_through) = month_bounds(time_tag)?;
        records.push(F107Record {
            date: valid_from.clone(),
            value_sfu: flux,
            kind: F107Kind::Observed,
            provider: PROVIDER.into(),
            product: "observed-solar-cycle-indices".into(),
            observation_date: Some(valid_from.clone()),
            forecast_issued_at_utc: None,
            retrieved_at_utc: Some(retrieved_at_utc.into()),
            valid_from: Some(valid_from),
            valid_through: Some(valid_through),
            cadence: Some("monthly".into()),
            uncertainty_sfu: None,
            range_low_sfu: None,
            range_high_sfu: None,
            source_locator: Some(
                "https://services.swpc.noaa.gov/json/solar-cycle/observed-solar-cycle-indices.json"
                    .into(),
            ),
        });
    }
    Ok(records)
}

fn month_bounds(time_tag: &str) -> Result<(String, String)> {
    let date = NaiveDate::parse_from_str(&format!("{time_tag}-01"), "%Y-%m-%d")
        .with_context(|| format!("invalid month time-tag {time_tag}"))?;
    let (year, month) = (date.year(), date.month());
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .context("invalid next month")?;
    let last = next_month - chrono::Duration::days(1);
    Ok((
        date.format("%Y-%m-%d").to_string(),
        last.format("%Y-%m-%d").to_string(),
    ))
}
