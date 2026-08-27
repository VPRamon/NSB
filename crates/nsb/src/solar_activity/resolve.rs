//! Offline F10.7 resolver (no network I/O).
//!
//! Airglow's Noll/SkyCalc solar correction expects a **monthly-averaged** F10.7
//! quantity (`msolflux`). This resolver therefore never feeds a raw daily
//! observation or daily forecast value into Airglow.

use super::record::{climatology_record, explicit_record, F107Kind, F107Record};
use super::store::F107Store;
use crate::components::airglow::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;
use std::sync::Arc;
use tempoch::{Time, UTC};

/// How Airglow / the evaluator obtains F10.7.
#[derive(Debug, Clone, Default)]
pub enum SolarActivitySource {
    /// Caller-owned scalar override (highest precedence).
    Explicit(SolarFluxUnits),
    /// Resolve against a caller-selected pinned local dataset.
    Dataset(Arc<F107Store>),
    /// Resolve against the bundled offline store (observations, forecasts,
    /// climatology). Never performs network I/O.
    #[default]
    Automatic,
    /// Compatibility path that always returns [`DEFAULT_SOLAR_RADIO_FLUX`]
    /// labelled as an explicit legacy constant (not automatic resolution).
    LegacyDefault,
}

impl PartialEq for SolarActivitySource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Explicit(a), Self::Explicit(b)) => a == b,
            (Self::Automatic, Self::Automatic) | (Self::LegacyDefault, Self::LegacyDefault) => true,
            (Self::Dataset(a), Self::Dataset(b)) => {
                a.dataset_id == b.dataset_id
                    && a.snapshot_id == b.snapshot_id
                    && a.checksum_sha256 == b.checksum_sha256
            }
            _ => false,
        }
    }
}

/// Resolved F10.7 value plus full provenance for scientific metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSolarActivity {
    /// Flux applied to the airglow continuum correction.
    pub value: SolarFluxUnits,
    /// Selected record provenance.
    pub record: F107Record,
    /// Dataset identity when a store was used.
    pub dataset_id: Option<String>,
    /// Snapshot identity when a store was used.
    pub snapshot_id: Option<String>,
    /// Content checksum of the selected store when available.
    pub checksum_sha256: Option<String>,
    /// Requested UTC calendar date.
    pub requested_date: NaiveDate,
    /// Which precedence step produced this value.
    pub resolution_step: &'static str,
}

impl ResolvedSolarActivity {
    /// Compact provenance fragment for component metadata strings.
    pub fn provenance_fragment(&self) -> String {
        format!(
            "F10.7 {} sfu kind={} provider={} product={} requested_date={} observation_date={} forecast_issued_at={} dataset={} snapshot={} checksum={} resolution={} uncertainty_sfu={:?} range=[{:?}, {:?}]; measured F10.7 does not make Airglow site-calibrated",
            self.value.value(),
            self.record.kind.as_str(),
            self.record.provider,
            self.record.product,
            self.requested_date,
            self.record.observation_date.as_deref().unwrap_or("n/a"),
            self.record
                .forecast_issued_at_utc
                .as_deref()
                .unwrap_or("n/a"),
            self.dataset_id.as_deref().unwrap_or("n/a"),
            self.snapshot_id.as_deref().unwrap_or("n/a"),
            self.checksum_sha256.as_deref().unwrap_or("n/a"),
            self.resolution_step,
            self.record.uncertainty_sfu,
            self.record.range_low_sfu,
            self.record.range_high_sfu,
        )
    }

    /// Whether this resolution should be treated as degraded planning maturity
    /// relative to a measured historical observation.
    pub fn is_degraded_planning_input(&self) -> bool {
        matches!(self.record.kind, F107Kind::Forecast | F107Kind::Climatology)
    }
}

/// Resolve F10.7 for `requested_time` against `source`.
///
/// Precedence (Noll/SkyCalc monthly-averaged quantity):
/// 1. explicit caller override (validated finite and positive)
/// 2. monthly measured observation covering the requested UTC date
/// 3. monthly-compatible forecast:
///    - calendar-month mean of covering 45-day daily forecasts when available
///    - else monthly `predicted-solar-cycle` covering that month
/// 4. documented climatological fallback
/// 5. legacy neutralizing constant only via [`SolarActivitySource::LegacyDefault`]
///
/// Raw daily observations/forecasts are never selected as the Airglow input.
/// This function performs no network I/O.
pub fn resolve_f107(
    requested_time: Time<UTC>,
    source: &SolarActivitySource,
) -> crate::Result<ResolvedSolarActivity> {
    let requested_date = utc_calendar_date(requested_time);
    match source {
        SolarActivitySource::Explicit(flux) => {
            validate_explicit_flux(*flux)?;
            Ok(ResolvedSolarActivity {
                value: *flux,
                record: explicit_record(requested_date, flux.value()),
                dataset_id: None,
                snapshot_id: None,
                checksum_sha256: None,
                requested_date,
                resolution_step: "explicit-override",
            })
        }
        SolarActivitySource::LegacyDefault => Ok(ResolvedSolarActivity {
            value: DEFAULT_SOLAR_RADIO_FLUX,
            record: explicit_record(requested_date, DEFAULT_SOLAR_RADIO_FLUX.value()),
            dataset_id: None,
            snapshot_id: None,
            checksum_sha256: None,
            requested_date,
            resolution_step: "legacy-default-constant",
        }),
        SolarActivitySource::Dataset(store) => resolve_from_store(requested_date, store),
        SolarActivitySource::Automatic => {
            let store = super::bundled::bundled_f107_store()?;
            resolve_from_store(requested_date, store)
        }
    }
}

fn validate_explicit_flux(flux: SolarFluxUnits) -> crate::Result<()> {
    let value = flux.value();
    if !value.is_finite() || value <= 0.0 {
        return Err(crate::error::NsbError::OutOfRange(format!(
            "explicit F10.7 must be finite and positive, got {value}"
        )));
    }
    Ok(())
}

fn resolve_from_store(
    requested_date: NaiveDate,
    store: &F107Store,
) -> crate::Result<ResolvedSolarActivity> {
    if let Some(record) = select_monthly_observed(store, requested_date)? {
        return Ok(resolved(
            store,
            requested_date,
            record,
            "monthly-observed-local-store",
        ));
    }
    if let Some(record) = select_monthly_compatible_forecast(store, requested_date)? {
        let step = if record.product.contains("45-day") {
            "short-range-forecast-monthly-mean"
        } else if record.product.contains("predicted-solar-cycle")
            || record.cadence.as_deref() == Some("monthly")
        {
            "monthly-solar-cycle-forecast"
        } else {
            "official-forecast"
        };
        return Ok(resolved(store, requested_date, record, step));
    }
    Ok(ResolvedSolarActivity {
        value: SolarFluxUnits::new(store.climatology_sfu),
        record: climatology_record(
            requested_date,
            store.climatology_sfu,
            "documented-climatology-fallback",
        ),
        dataset_id: Some(store.dataset_id.clone()),
        snapshot_id: Some(store.snapshot_id.clone()),
        checksum_sha256: store.checksum_sha256.clone(),
        requested_date,
        resolution_step: "climatology-fallback",
    })
}

fn resolved(
    store: &F107Store,
    requested_date: NaiveDate,
    record: F107Record,
    step: &'static str,
) -> ResolvedSolarActivity {
    ResolvedSolarActivity {
        value: SolarFluxUnits::new(record.value_sfu),
        record,
        dataset_id: Some(store.dataset_id.clone()),
        snapshot_id: Some(store.snapshot_id.clone()),
        checksum_sha256: store.checksum_sha256.clone(),
        requested_date,
        resolution_step: step,
    }
}

/// Select a **monthly** observation covering `requested`.
///
/// Daily observations are retained in the store for diagnostics but are never
/// chosen as the Noll/SkyCalc Airglow input.
fn select_monthly_observed(
    store: &F107Store,
    requested: NaiveDate,
) -> crate::Result<Option<F107Record>> {
    let mut matches =
        store
            .observed_covering(requested)
            .map_err(|error| crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            })?;
    matches.retain(|record| record.cadence.as_deref() == Some("monthly"));
    if matches.is_empty() {
        return Ok(None);
    }
    // Prefer narrower validity windows, then deterministic product name.
    matches.sort_by_key(|record| {
        (
            validity_span_days(record).unwrap_or(u32::MAX),
            std::cmp::Reverse(record.product.as_str()),
        )
    });
    Ok(Some(matches[0].clone()))
}

/// Select a monthly-compatible forecast for Airglow.
///
/// Prefer a calendar-month mean of 45-day daily forecasts when any day in the
/// requested month is covered; otherwise use monthly solar-cycle predictions.
fn select_monthly_compatible_forecast(
    store: &F107Store,
    requested: NaiveDate,
) -> crate::Result<Option<F107Record>> {
    if let Some(aggregated) = aggregate_45_day_month_mean(store, requested)? {
        return Ok(Some(aggregated));
    }
    select_monthly_cycle_forecast(store, requested)
}

fn aggregate_45_day_month_mean(
    store: &F107Store,
    requested: NaiveDate,
) -> crate::Result<Option<F107Record>> {
    let year = requested.year();
    let month = requested.month();
    let mut by_day: BTreeMap<NaiveDate, F107Record> = BTreeMap::new();

    for record in &store.records {
        if record.kind != F107Kind::Forecast {
            continue;
        }
        if !record.product.contains("45-day") {
            continue;
        }
        if record.cadence.as_deref() != Some("daily") {
            continue;
        }
        let day = super::record::parse_date(&record.date, "date").map_err(|error| {
            crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            }
        })?;
        if day.year() != year || day.month() != month {
            continue;
        }
        record
            .validate()
            .map_err(|error| crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            })?;
        match by_day.get(&day) {
            None => {
                by_day.insert(day, record.clone());
            }
            Some(prior) => {
                // Freshest issuance wins for a given day; missing issuance sorts last.
                if record.forecast_issued_at_utc > prior.forecast_issued_at_utc {
                    by_day.insert(day, record.clone());
                }
            }
        }
    }

    if by_day.is_empty() {
        return Ok(None);
    }

    let days: Vec<_> = by_day.values().collect();
    let mean = days.iter().map(|r| r.value_sfu).sum::<f64>() / days.len() as f64;
    if !mean.is_finite() || mean <= 0.0 {
        return Err(crate::error::NsbError::DataParse {
            file: "f107_store",
            message: "45-day monthly aggregation produced a non-positive mean".into(),
        });
    }

    let (month_start, month_end) = month_bounds_for(requested);
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

    Ok(Some(F107Record {
        date: month_start.format("%Y-%m-%d").to_string(),
        value_sfu: mean,
        kind: F107Kind::Forecast,
        provider,
        product: "45-day-forecast-monthly-mean".into(),
        observation_date: None,
        forecast_issued_at_utc: issued,
        retrieved_at_utc: retrieved,
        valid_from: Some(month_start.format("%Y-%m-%d").to_string()),
        valid_through: Some(month_end.format("%Y-%m-%d").to_string()),
        cadence: Some("monthly".into()),
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: Some(format!(
            "{} (operational calendar-month mean of available 45-day daily forecasts; approximates Noll/SkyCalc msolflux, not a measured monthly mean; {} day(s) in month)",
            source_locator.unwrap_or_else(|| "45-day-forecast".into()),
            days.len()
        )),
    }))
}

fn select_monthly_cycle_forecast(
    store: &F107Store,
    requested: NaiveDate,
) -> crate::Result<Option<F107Record>> {
    let mut matches =
        store
            .forecasts_covering(requested)
            .map_err(|error| crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            })?;
    matches.retain(|record| {
        record.cadence.as_deref() == Some("monthly") && !record.product.contains("45-day")
    });
    if matches.is_empty() {
        return Ok(None);
    }
    // Deterministic precedence: prefer records with issuance when present (freshest),
    // but never invent issuance. Missing issuance sorts after dated issuance for
    // freshness but still participates via product/value tie-breaks.
    matches.sort_by(|a, b| {
        b.forecast_issued_at_utc
            .cmp(&a.forecast_issued_at_utc)
            .then_with(|| a.product.cmp(&b.product))
            .then_with(|| a.value_sfu.to_bits().cmp(&b.value_sfu.to_bits()))
    });
    Ok(Some(matches[0].clone()))
}

fn month_bounds_for(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start");
    let end = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
    .expect("valid next month")
        - chrono::Duration::days(1);
    (start, end)
}

fn validity_span_days(record: &F107Record) -> Option<u32> {
    let (from, through) = record.validity_window().ok()?;
    Some((through - from).num_days().max(0) as u32)
}

/// UTC calendar date for a `Time<UTC>`.
pub fn utc_calendar_date(time: Time<UTC>) -> NaiveDate {
    time.to_chrono()
        .expect("F10.7 resolution requires a chrono-representable UTC instant")
        .date_naive()
}
