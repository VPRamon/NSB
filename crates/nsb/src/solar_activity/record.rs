//! Typed F10.7 records and validation.

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Classification of an F10.7 value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum F107Kind {
    /// Measured historical value.
    Observed,
    /// Official forecast covering the requested date.
    Forecast,
    /// Documented climatological planning fallback.
    Climatology,
    /// Caller-owned explicit override.
    Explicit,
}

impl F107Kind {
    /// Stable lowercase identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Forecast => "forecast",
            Self::Climatology => "climatology",
            Self::Explicit => "explicit",
        }
    }
}

/// One provenance-carrying F10.7 datum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct F107Record {
    /// UTC calendar date the value applies to (or month start for monthly cadence).
    pub date: String,
    /// F10.7 value in solar flux units.
    pub value_sfu: f64,
    /// Observation / forecast / climatology / explicit.
    pub kind: F107Kind,
    /// Data provider identity (for example `noaa-swpc` or `caller`).
    pub provider: String,
    /// Provider product identity.
    pub product: String,
    /// Observation date when `kind == observed`.
    #[serde(default)]
    pub observation_date: Option<String>,
    /// Forecast issuance timestamp (RFC3339 UTC) when applicable.
    #[serde(default)]
    pub forecast_issued_at_utc: Option<String>,
    /// When this record was retrieved into the local store.
    #[serde(default)]
    pub retrieved_at_utc: Option<String>,
    /// Inclusive validity start (`YYYY-MM-DD`) when different from `date`.
    #[serde(default)]
    pub valid_from: Option<String>,
    /// Inclusive validity end (`YYYY-MM-DD`).
    #[serde(default)]
    pub valid_through: Option<String>,
    /// Cadence label such as `daily` or `monthly`.
    #[serde(default)]
    pub cadence: Option<String>,
    /// Optional one-sigma uncertainty in sfu.
    #[serde(default)]
    pub uncertainty_sfu: Option<f64>,
    /// Optional forecast range low in sfu.
    #[serde(default)]
    pub range_low_sfu: Option<f64>,
    /// Optional forecast range high in sfu.
    #[serde(default)]
    pub range_high_sfu: Option<f64>,
    /// Source locator / URL or local path reference.
    #[serde(default)]
    pub source_locator: Option<String>,
}

/// Validation failure for an F10.7 record or store.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct F107ValidationError(pub String);

impl F107Record {
    /// Validate physical and temporal consistency of this record.
    pub fn validate(&self) -> Result<(), F107ValidationError> {
        if !self.value_sfu.is_finite() || self.value_sfu <= 0.0 {
            return Err(F107ValidationError(format!(
                "value_sfu must be finite and positive, got {}",
                self.value_sfu
            )));
        }
        let date = parse_date(&self.date, "date")?;
        if self.provider.trim().is_empty() {
            return Err(F107ValidationError("provider must be non-empty".into()));
        }
        if self.product.trim().is_empty() {
            return Err(F107ValidationError("product must be non-empty".into()));
        }
        for (label, optional) in [
            ("observation_date", &self.observation_date),
            ("valid_from", &self.valid_from),
            ("valid_through", &self.valid_through),
        ] {
            if let Some(value) = optional {
                parse_date(value, label)?;
            }
        }
        if let Some(issued) = &self.forecast_issued_at_utc {
            parse_datetime(issued, "forecast_issued_at_utc")?;
        }
        if let Some(retrieved) = &self.retrieved_at_utc {
            parse_datetime(retrieved, "retrieved_at_utc")?;
        }
        for (label, optional) in [
            ("uncertainty_sfu", self.uncertainty_sfu),
            ("range_low_sfu", self.range_low_sfu),
            ("range_high_sfu", self.range_high_sfu),
        ] {
            if let Some(value) = optional {
                if !value.is_finite() || value < 0.0 {
                    return Err(F107ValidationError(format!(
                        "{label} must be finite and non-negative"
                    )));
                }
            }
        }
        if let (Some(low), Some(high)) = (self.range_low_sfu, self.range_high_sfu) {
            if low > high {
                return Err(F107ValidationError(
                    "range_low_sfu must not exceed range_high_sfu".into(),
                ));
            }
        }
        let valid_from = self
            .valid_from
            .as_deref()
            .map(|value| parse_date(value, "valid_from"))
            .transpose()?
            .unwrap_or(date);
        let valid_through = self
            .valid_through
            .as_deref()
            .map(|value| parse_date(value, "valid_through"))
            .transpose()?
            .unwrap_or(date);
        if valid_from > valid_through {
            return Err(F107ValidationError(
                "valid_from must not follow valid_through".into(),
            ));
        }
        match self.kind {
            F107Kind::Observed => {
                if self.observation_date.is_none() {
                    return Err(F107ValidationError(
                        "observed records require observation_date".into(),
                    ));
                }
                if self.forecast_issued_at_utc.is_some() {
                    return Err(F107ValidationError(
                        "observed records must not set forecast_issued_at_utc".into(),
                    ));
                }
            }
            F107Kind::Forecast => {
                if self.forecast_issued_at_utc.is_none() {
                    return Err(F107ValidationError(
                        "forecast records require forecast_issued_at_utc".into(),
                    ));
                }
                if self.observation_date.is_some() {
                    return Err(F107ValidationError(
                        "forecast records must not set observation_date".into(),
                    ));
                }
            }
            F107Kind::Climatology | F107Kind::Explicit => {}
        }
        Ok(())
    }

    /// Inclusive validity window as UTC dates.
    pub fn validity_window(&self) -> Result<(NaiveDate, NaiveDate), F107ValidationError> {
        let date = parse_date(&self.date, "date")?;
        let valid_from = self
            .valid_from
            .as_deref()
            .map(|value| parse_date(value, "valid_from"))
            .transpose()?
            .unwrap_or(date);
        let valid_through = self
            .valid_through
            .as_deref()
            .map(|value| parse_date(value, "valid_through"))
            .transpose()?
            .unwrap_or(date);
        Ok((valid_from, valid_through))
    }

    /// Whether `requested` falls inside this record's validity window.
    pub fn covers(&self, requested: NaiveDate) -> Result<bool, F107ValidationError> {
        let (from, through) = self.validity_window()?;
        Ok(requested >= from && requested <= through)
    }
}

pub(crate) fn parse_date(value: &str, label: &str) -> Result<NaiveDate, F107ValidationError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| F107ValidationError(format!("{label} must be YYYY-MM-DD, got {value:?}")))
}

pub(crate) fn parse_datetime(
    value: &str,
    label: &str,
) -> Result<NaiveDateTime, F107ValidationError> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(dt.naive_utc());
    }
    // Accept trailing Z already handled by RFC3339; also allow space-separated forms.
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.fZ"))
        .map_err(|_| F107ValidationError(format!("{label} must be RFC3339 UTC, got {value:?}")))
}

/// Build a caller-owned explicit record for a requested date.
pub fn explicit_record(date: NaiveDate, value_sfu: f64) -> F107Record {
    let date = date.format("%Y-%m-%d").to_string();
    F107Record {
        date: date.clone(),
        value_sfu,
        kind: F107Kind::Explicit,
        provider: "caller".into(),
        product: "explicit-override".into(),
        observation_date: None,
        forecast_issued_at_utc: None,
        retrieved_at_utc: None,
        valid_from: Some(date.clone()),
        valid_through: Some(date),
        cadence: None,
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: None,
    }
}

/// Build a climatology record for a requested date.
pub fn climatology_record(date: NaiveDate, value_sfu: f64, notes_product: &str) -> F107Record {
    let date = date.format("%Y-%m-%d").to_string();
    F107Record {
        date: date.clone(),
        value_sfu,
        kind: F107Kind::Climatology,
        provider: "nsb".into(),
        product: notes_product.into(),
        observation_date: None,
        forecast_issued_at_utc: None,
        retrieved_at_utc: None,
        valid_from: Some(date.clone()),
        valid_through: Some(date),
        cadence: Some("climatology".into()),
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: Some("bundled F10.7 store climatology_sfu".into()),
    }
}
