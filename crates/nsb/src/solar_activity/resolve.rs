//! Offline F10.7 resolver (no network I/O).

use super::record::{climatology_record, explicit_record, F107Kind, F107Record};
use super::store::F107Store;
use crate::components::airglow::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use chrono::NaiveDate;
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
/// Precedence:
/// 1. explicit caller override
/// 2. exact measured observation in the selected local/pinned store
/// 3. valid official forecast covering the requested date (freshest issuance;
///    short-range daily before longer-range monthly when both cover)
/// 4. bundled/pinned offline forecast (same store path when Automatic)
/// 5. documented climatological fallback
/// 6. legacy neutralizing constant only via [`SolarActivitySource::LegacyDefault`]
///
/// This function performs no network I/O.
pub fn resolve_f107(
    requested_time: Time<UTC>,
    source: &SolarActivitySource,
) -> crate::Result<ResolvedSolarActivity> {
    let requested_date = utc_calendar_date(requested_time);
    match source {
        SolarActivitySource::Explicit(flux) => Ok(ResolvedSolarActivity {
            value: *flux,
            record: explicit_record(requested_date, flux.value()),
            dataset_id: None,
            snapshot_id: None,
            checksum_sha256: None,
            requested_date,
            resolution_step: "explicit-override",
        }),
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

fn resolve_from_store(
    requested_date: NaiveDate,
    store: &F107Store,
) -> crate::Result<ResolvedSolarActivity> {
    if let Some(record) = select_observed(store, requested_date)? {
        return Ok(resolved(
            store,
            requested_date,
            record,
            "observed-local-store",
        ));
    }
    if let Some(record) = select_forecast(store, requested_date)? {
        let step = if record.product.contains("45-day") {
            "short-range-forecast"
        } else if record.cadence.as_deref() == Some("monthly") {
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

fn select_observed(store: &F107Store, requested: NaiveDate) -> crate::Result<Option<F107Record>> {
    let mut matches =
        store
            .observed_covering(requested)
            .map_err(|error| crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            })?;
    if matches.is_empty() {
        return Ok(None);
    }
    // Prefer daily cadence over monthly means when both cover the day.
    matches.sort_by_key(|record| {
        let cadence_rank = match record.cadence.as_deref() {
            Some("daily") => 0u8,
            Some("monthly") => 1,
            _ => 2,
        };
        (
            cadence_rank,
            // Prefer more specific (narrower) validity windows.
            validity_span_days(record).unwrap_or(u32::MAX),
            std::cmp::Reverse(record.product.as_str()),
        )
    });
    Ok(Some(matches[0].clone()))
}

fn select_forecast(store: &F107Store, requested: NaiveDate) -> crate::Result<Option<F107Record>> {
    let mut matches =
        store
            .forecasts_covering(requested)
            .map_err(|error| crate::error::NsbError::DataParse {
                file: "f107_store",
                message: error.0,
            })?;
    if matches.is_empty() {
        return Ok(None);
    }
    // Prefer short-range daily forecasts over monthly cycle predictions, then
    // freshest issuance, then deterministic product name.
    matches.sort_by(|a, b| {
        let rank = |record: &F107Record| -> (u8, u8) {
            let product_rank = if record.product.contains("45-day") {
                0
            } else if record.cadence.as_deref() == Some("monthly") {
                2
            } else {
                1
            };
            let cadence_rank = match record.cadence.as_deref() {
                Some("daily") => 0,
                Some("monthly") => 1,
                _ => 2,
            };
            (product_rank, cadence_rank)
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| {
                // Freshest issuance wins (lexicographic RFC3339 UTC is chronological).
                b.forecast_issued_at_utc.cmp(&a.forecast_issued_at_utc)
            })
            .then_with(|| a.product.cmp(&b.product))
            .then_with(|| a.value_sfu.to_bits().cmp(&b.value_sfu.to_bits()))
    });
    Ok(Some(matches[0].clone()))
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
