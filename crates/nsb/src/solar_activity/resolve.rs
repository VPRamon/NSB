//! Offline F10.7 resolver (no network I/O).
//!
//! Airglow's Noll/SkyCalc solar correction expects a **monthly-averaged** F10.7
//! quantity (`msolflux`). This resolver therefore never feeds a raw daily
//! observation or daily forecast value into Airglow, and never promotes a
//! partial-month average to a completed monthly mean.

use super::monthly::{resolve_monthly_evidence, MonthlyCompleteness, MonthlyF107Evidence};
use super::record::{explicit_record, F107Kind, F107Record};
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
    /// Monthly completeness / method classification when store-resolved.
    pub monthly_completeness: Option<MonthlyCompleteness>,
    /// Observed day count contributing to a derived monthly estimate.
    pub observed_days: Option<u32>,
    /// Forecast day count contributing to a derived monthly estimate.
    pub forecast_days: Option<u32>,
    /// Total calendar days in the resolved month.
    pub total_days: Option<u32>,
}

impl ResolvedSolarActivity {
    /// Compact provenance fragment for component metadata strings.
    pub fn provenance_fragment(&self) -> String {
        format!(
            "F10.7 {} sfu kind={} provider={} product={} requested_date={} observation_date={} forecast_issued_at={} dataset={} snapshot={} checksum={} resolution={} monthly_completeness={} observed_days={:?} forecast_days={:?} total_days={:?} uncertainty_sfu={:?} range=[{:?}, {:?}]; measured F10.7 does not make Airglow site-calibrated",
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
            self.monthly_completeness
                .map(MonthlyCompleteness::as_str)
                .unwrap_or("n/a"),
            self.observed_days,
            self.forecast_days,
            self.total_days,
            self.record.uncertainty_sfu,
            self.record.range_low_sfu,
            self.record.range_high_sfu,
        )
    }

    /// Whether this resolution should be treated as degraded planning maturity
    /// relative to a measured historical observation.
    pub fn is_degraded_planning_input(&self) -> bool {
        matches!(self.record.kind, F107Kind::Forecast | F107Kind::Climatology)
            || matches!(
                self.monthly_completeness,
                Some(
                    MonthlyCompleteness::CompleteForecast
                        | MonthlyCompleteness::ProvisionalObservedPlusForecast
                        | MonthlyCompleteness::OfficialMonthlyPrediction
                        | MonthlyCompleteness::Climatology
                )
            )
    }
}

/// Resolve F10.7 for `requested_time` against `source`.
///
/// Precedence (Noll/SkyCalc monthly-averaged quantity):
/// 1. explicit caller override (validated finite and positive)
/// 2. finalized monthly observed covering a completed month
/// 3. current month: provisional observed+forecast mean (full coverage only)
/// 4. future month: complete calendar-month 45-day forecast mean (time-valid)
/// 5. official monthly solar-cycle prediction (issued_at or retrieved_at ≤ requested)
/// 6. documented climatological fallback
/// 7. legacy neutralizing constant only via [`SolarActivitySource::LegacyDefault`]
///
/// Partial-month averages are never selected. Forecasts issued (or, when
/// issuance is absent, retrieved) after the requested evaluation instant never
/// participate. Raw daily values are never selected as the Airglow input.
/// This function performs no network I/O.
pub fn resolve_f107(
    requested_time: Time<UTC>,
    source: &SolarActivitySource,
) -> crate::Result<ResolvedSolarActivity> {
    let requested_date = utc_calendar_date(requested_time);
    let requested_at = requested_time
        .to_chrono()
        .expect("F10.7 resolution requires a chrono-representable UTC instant")
        .naive_utc();
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
                monthly_completeness: None,
                observed_days: None,
                forecast_days: None,
                total_days: None,
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
            monthly_completeness: None,
            observed_days: None,
            forecast_days: None,
            total_days: None,
        }),
        SolarActivitySource::Dataset(store) => {
            resolve_from_store(requested_date, requested_at, store)
        }
        SolarActivitySource::Automatic => {
            let store = super::bundled::bundled_f107_store()?;
            resolve_from_store(requested_date, requested_at, store)
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
    requested_at: chrono::NaiveDateTime,
    store: &F107Store,
) -> crate::Result<ResolvedSolarActivity> {
    let evidence =
        resolve_monthly_evidence(store, requested_date, requested_at).map_err(|message| {
            crate::error::NsbError::DataParse {
                file: "f107_store",
                message,
            }
        })?;
    Ok(from_evidence(store, requested_date, evidence))
}

fn from_evidence(
    store: &F107Store,
    requested_date: NaiveDate,
    evidence: MonthlyF107Evidence,
) -> ResolvedSolarActivity {
    ResolvedSolarActivity {
        value: SolarFluxUnits::new(evidence.value_sfu),
        record: evidence.record,
        dataset_id: Some(store.dataset_id.clone()),
        snapshot_id: Some(store.snapshot_id.clone()),
        checksum_sha256: store.checksum_sha256.clone(),
        requested_date,
        resolution_step: evidence.resolution_step,
        monthly_completeness: Some(evidence.method),
        observed_days: Some(evidence.observed_days),
        forecast_days: Some(evidence.forecast_days),
        total_days: Some(evidence.total_days),
    }
}

/// UTC calendar date for a `Time<UTC>`.
pub fn utc_calendar_date(time: Time<UTC>) -> NaiveDate {
    time.to_chrono()
        .expect("F10.7 resolution requires a chrono-representable UTC instant")
        .date_naive()
}
