// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tempoch::{Time, UTC};

pub fn parse_utc(input: &str) -> Result<Time<UTC>> {
    let dt = DateTime::parse_from_rfc3339(input)
        .with_context(|| format!("invalid UTC RFC3339 timestamp: {input:?}"))?;
    Ok(Time::<UTC>::from_chrono(dt.with_timezone(&Utc)))
}

pub fn format_utc(time: Time<UTC>) -> String {
    match time.to_chrono() {
        Some(dt) => dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        None => "<unrepresentable UTC instant>".to_string(),
    }
}

/// Window output uses milliseconds. Contract inward before formatting so the
/// displayed interval never expands a native adaptive/certified interval.
pub fn inward_periods(periods: Vec<tempoch::Period<UTC>>) -> Result<Vec<tempoch::Period<UTC>>> {
    use chrono::Timelike;
    periods
        .into_iter()
        .map(|period| {
            let endpoint = |value: Time<UTC>, start: bool| -> Result<Time<UTC>> {
                let civil = value
                    .to_chrono()
                    .context("window UTC cannot be formatted")?;
                let mut civil = civil
                    .with_nanosecond(civil.nanosecond() / 1_000_000 * 1_000_000)
                    .context("window UTC millisecond precision")?;
                for _ in 0..=100 {
                    let projected = Time::<UTC>::try_from_chrono(civil)?;
                    if projected >= period.start
                        && projected <= period.end
                        && projected.to::<tempoch::TT>() >= period.start.to::<tempoch::TT>()
                        && projected.to::<tempoch::TT>() <= period.end.to::<tempoch::TT>()
                    {
                        return Ok(projected);
                    }
                    civil = civil
                        .checked_add_signed(chrono::Duration::milliseconds(if start {
                            1
                        } else {
                            -1
                        }))
                        .context("window UTC precision overflow")?;
                }
                anyhow::bail!("window cannot be represented inward at CLI millisecond precision")
            };
            let result =
                tempoch::Period::new(endpoint(period.start, true)?, endpoint(period.end, false)?);
            anyhow::ensure!(
                result.start < result.end,
                "window collapses at CLI millisecond precision"
            );
            Ok(result)
        })
        .collect()
}
