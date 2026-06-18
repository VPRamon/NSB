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
