//! Fail-safe F10.7 store update / freeze / import / verify / resolve helpers.

use super::providers::{
    parse_45_day_forecast_json, parse_daily_solar_indices, parse_observed_solar_cycle_indices,
    parse_predicted_solar_cycle,
};
use crate::platform::artifact_store::atomic_write;
use crate::platform::checksum_io::sha256_bytes;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use nsb::{resolve_f107, F107Kind, F107Store, ResolvedSolarActivity, SolarActivitySource};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempoch::{Time, UTC};

const DAILY_URL: &str = "https://services.swpc.noaa.gov/text/daily-solar-indices.txt";
const FORECAST_45_URL: &str = "https://services.swpc.noaa.gov/json/45-day-forecast.json";
const PREDICTED_URL: &str =
    "https://services.swpc.noaa.gov/json/solar-cycle/predicted-solar-cycle.json";
const OBSERVED_MONTHLY_URL: &str =
    "https://services.swpc.noaa.gov/json/solar-cycle/observed-solar-cycle-indices.json";

/// Operational freshness windows for F10.7 cache `status` (#109).
///
/// These are **cache refresh** thresholds for distinguishing `fresh` /
/// `forecast` / `stale`; they do not change resolver science.
///
/// - 45-day forecasts are issued frequently → refresh within a week.
/// - `predicted-solar-cycle` is a monthly-cadence SWPC product → refresh within
///   one calendar month even when the published horizon still covers `now`.
/// - Observation-only caches: newest observation lag of a few days.
const FORECAST_45_RETRIEVAL_MAX_AGE_DAYS: i64 = 7;
const MONTHLY_CYCLE_FORECAST_RETRIEVAL_MAX_AGE_DAYS: i64 = 31;
const OBSERVATION_MAX_LAG_DAYS: i64 = 3;

/// How an update obtains provider bytes.
#[derive(Debug, Clone)]
pub enum UpdateMode {
    /// Fetch authoritative SWPC products over the network.
    Online,
    /// Read pinned fixtures from a directory (CI / offline).
    FixtureDir(PathBuf),
}

/// Outcome of an update or freeze attempt.
#[derive(Debug, Clone)]
pub struct UpdateReport {
    pub active_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub dataset_id: String,
    pub snapshot_id: String,
    pub checksum_sha256: String,
    pub record_count: usize,
    pub status: &'static str,
    pub notes: Vec<String>,
}

/// Parameters for deterministic scientific-asset freeze (no wall clock, no merge).
#[derive(Debug, Clone)]
pub struct FreezeParams {
    pub fixture_dir: PathBuf,
    pub store_path: PathBuf,
    pub dataset_id: String,
    pub snapshot_id: String,
    pub retrieved_at_utc: String,
}

/// Refresh a local **operational** F10.7 store from online products or fixtures.
///
/// Uses wall-clock retrieval time and may merge with an existing store. This is
/// **not** the scientific asset freeze workflow — see [`freeze_store`].
pub fn update_store(store_path: &Path, mode: UpdateMode, dataset_id: &str) -> Result<UpdateReport> {
    let retrieved_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut notes = Vec::new();
    let incoming = match &mode {
        UpdateMode::Online => {
            notes.push("network acquisition from NOAA/NWS SWPC".into());
            fetch_and_parse_online(&retrieved_at)?
        }
        UpdateMode::FixtureDir(dir) => {
            notes.push(format!("fixture acquisition from {}", dir.display()));
            parse_fixtures(dir, &retrieved_at)?
        }
    };

    let existing = if store_path.exists() {
        let bytes = fs::read(store_path)
            .with_context(|| format!("read existing store {}", store_path.display()))?;
        Some(F107Store::from_json_bytes(&bytes).map_err(|error| anyhow::anyhow!(error.0))?)
    } else {
        None
    };

    let snapshot_id = format!("snap-{}", retrieved_at.replace(':', ""));
    let base = existing.unwrap_or_else(|| empty_store(dataset_id, &snapshot_id, &retrieved_at));
    let merged = base
        .merge_with(&incoming, snapshot_id.clone(), Some(retrieved_at.clone()))
        .map_err(|error| anyhow::anyhow!(error.0))?;
    activate_store(store_path, merged, "fresh", notes)
}

/// Deterministically materialize an immutable scientific F10.7 store from fixtures.
///
/// Always starts from a **clean** empty store (never merges an existing destination).
/// Retrieval timestamp and snapshot id are caller-supplied — no wall clock.
pub fn freeze_store(params: &FreezeParams) -> Result<UpdateReport> {
    validate_retrieved_at(&params.retrieved_at_utc)?;
    let mut notes = vec![
        format!(
            "deterministic freeze from fixtures {}",
            params.fixture_dir.display()
        ),
        format!("retrieved_at_utc={}", params.retrieved_at_utc),
        format!("snapshot_id={}", params.snapshot_id),
        "starts from clean store (no merge with destination)".into(),
    ];
    let incoming = parse_fixtures(&params.fixture_dir, &params.retrieved_at_utc)?;
    notes.push(format!("parsed {} fixture records", incoming.len()));
    let base = empty_store(
        &params.dataset_id,
        &params.snapshot_id,
        &params.retrieved_at_utc,
    );
    let frozen = base
        .merge_with(
            &incoming,
            params.snapshot_id.clone(),
            Some(params.retrieved_at_utc.clone()),
        )
        .map_err(|error| anyhow::anyhow!(error.0))?;
    activate_store(&params.store_path, frozen, "frozen", notes)
}

fn activate_store(
    store_path: &Path,
    store: F107Store,
    status: &'static str,
    mut notes: Vec<String>,
) -> Result<UpdateReport> {
    let bytes = store
        .to_json_bytes()
        .map_err(|error| anyhow::anyhow!(error.0))?;
    let verified = F107Store::from_json_bytes(&bytes).map_err(|error| anyhow::anyhow!(error.0))?;
    let checksum = verified
        .checksum_sha256
        .clone()
        .context("verified store missing checksum")?;
    let snapshot_id = verified.snapshot_id.clone();

    if let Some(parent) = store_path.parent() {
        let snapshots = parent.join("snapshots");
        fs::create_dir_all(&snapshots)?;
        if store_path.exists() {
            let previous = fs::read(store_path)?;
            let previous_checksum = sha256_bytes(&previous);
            let prior_id = F107Store::from_json_bytes(&previous)
                .map(|s| s.snapshot_id)
                .unwrap_or_else(|_| "previous".into());
            let archive = snapshots.join(format!(
                "{}-{previous_checksum}.json",
                prior_id.replace(':', "")
            ));
            atomic_write(&archive, &previous)?;
            notes.push(format!("retained previous snapshot {}", archive.display()));
        }
        let snap_out = snapshots.join(format!("{snapshot_id}-{checksum}.json"));
        atomic_write(&snap_out, &bytes)?;
        atomic_write(store_path, &bytes)?;
        Ok(UpdateReport {
            active_path: store_path.to_path_buf(),
            snapshot_path: snap_out,
            dataset_id: verified.dataset_id,
            snapshot_id: verified.snapshot_id,
            checksum_sha256: checksum,
            record_count: verified.records.len(),
            status,
            notes,
        })
    } else {
        bail!("store path has no parent directory");
    }
}

fn validate_retrieved_at(value: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("--retrieved-at must be RFC3339 UTC, got {value}"))?;
    Ok(())
}

/// Import a caller-provided store file after validation.
pub fn import_store(source: &Path, destination: &Path) -> Result<UpdateReport> {
    let bytes = fs::read(source).with_context(|| format!("read {}", source.display()))?;
    let store = F107Store::from_json_bytes(&bytes).map_err(|error| anyhow::anyhow!(error.0))?;
    let checksum = store
        .checksum_sha256
        .clone()
        .context("imported store missing checksum")?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
        let snapshots = parent.join("snapshots");
        fs::create_dir_all(&snapshots)?;
        let snap_out = snapshots.join(format!("{}-{checksum}.json", store.snapshot_id));
        atomic_write(&snap_out, &bytes)?;
        atomic_write(destination, &bytes)?;
        Ok(UpdateReport {
            active_path: destination.to_path_buf(),
            snapshot_path: snap_out,
            dataset_id: store.dataset_id,
            snapshot_id: store.snapshot_id,
            checksum_sha256: checksum,
            record_count: store.records.len(),
            status: "imported",
            notes: vec![format!("imported from {}", source.display())],
        })
    } else {
        bail!("destination has no parent");
    }
}

/// Verify a store asset parses and matches an optional expected checksum.
pub fn verify_store(path: &Path, expected_sha256: Option<&str>) -> Result<F107Store> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let store = F107Store::from_json_bytes(&bytes).map_err(|error| anyhow::anyhow!(error.0))?;
    if let Some(expected) = expected_sha256 {
        let actual = store
            .checksum_sha256
            .as_deref()
            .context("store missing checksum")?;
        if actual != expected {
            bail!("checksum mismatch: expected {expected}, got {actual}");
        }
    }
    Ok(store)
}

/// Structured freshness status for an F10.7 store at an injected evaluation time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStatus {
    pub status: &'static str,
    pub path: PathBuf,
    pub dataset_id: Option<String>,
    pub snapshot_id: Option<String>,
    pub checksum_sha256: Option<String>,
    pub retrieved_at_utc: Option<String>,
    pub retrieval_age_days: Option<i64>,
    pub newest_observation_date: Option<String>,
    pub forecast_45_issued_at_utc: Option<String>,
    pub forecast_45_valid_through: Option<String>,
    pub monthly_forecast_horizon: Option<String>,
    pub record_count: usize,
    pub notes: Vec<String>,
}

impl StoreStatus {
    /// Single-line CLI rendering.
    pub fn render(&self) -> String {
        let mut line = format!("status={} path={}", self.status, self.path.display());
        if let Some(id) = &self.dataset_id {
            line.push_str(&format!(" dataset={id}"));
        }
        if let Some(id) = &self.snapshot_id {
            line.push_str(&format!(" snapshot={id}"));
        }
        if let Some(sum) = &self.checksum_sha256 {
            line.push_str(&format!(" checksum={sum}"));
        }
        if let Some(retrieved) = &self.retrieved_at_utc {
            line.push_str(&format!(" retrieved_at={retrieved}"));
        }
        if let Some(age) = self.retrieval_age_days {
            line.push_str(&format!(" retrieval_age_days={age}"));
        }
        if let Some(obs) = &self.newest_observation_date {
            line.push_str(&format!(" newest_observation={obs}"));
        }
        if let Some(issued) = &self.forecast_45_issued_at_utc {
            line.push_str(&format!(" forecast_45_issued_at={issued}"));
        }
        if let Some(through) = &self.forecast_45_valid_through {
            line.push_str(&format!(" forecast_45_valid_through={through}"));
        }
        if let Some(horizon) = &self.monthly_forecast_horizon {
            line.push_str(&format!(" monthly_forecast_horizon={horizon}"));
        }
        line.push_str(&format!(" records={}", self.record_count));
        for note in &self.notes {
            line.push_str(&format!(" note={note}"));
        }
        line
    }
}

/// Status using wall-clock `Utc::now()`. Prefer [`status_report_at`] in tests.
pub fn status_report(path: &Path) -> Result<String> {
    Ok(status_report_at(path, Utc::now())?.render())
}

/// Deterministic freshness report at injected `now`.
pub fn status_report_at(path: &Path, now: DateTime<Utc>) -> Result<StoreStatus> {
    if !path.exists() {
        return Ok(StoreStatus {
            status: "missing",
            path: path.to_path_buf(),
            dataset_id: None,
            snapshot_id: None,
            checksum_sha256: None,
            retrieved_at_utc: None,
            retrieval_age_days: None,
            newest_observation_date: None,
            forecast_45_issued_at_utc: None,
            forecast_45_valid_through: None,
            monthly_forecast_horizon: None,
            record_count: 0,
            notes: vec!["run: nsb-data solar f107 update|freeze".into()],
        });
    }

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Ok(StoreStatus {
                status: "invalid",
                path: path.to_path_buf(),
                dataset_id: None,
                snapshot_id: None,
                checksum_sha256: None,
                retrieved_at_utc: None,
                retrieval_age_days: None,
                newest_observation_date: None,
                forecast_45_issued_at_utc: None,
                forecast_45_valid_through: None,
                monthly_forecast_horizon: None,
                record_count: 0,
                notes: vec![format!("read error: {error}")],
            });
        }
    };
    let store = match F107Store::from_json_bytes(&bytes) {
        Ok(store) => store,
        Err(error) => {
            return Ok(StoreStatus {
                status: "invalid",
                path: path.to_path_buf(),
                dataset_id: None,
                snapshot_id: None,
                checksum_sha256: None,
                retrieved_at_utc: None,
                retrieval_age_days: None,
                newest_observation_date: None,
                forecast_45_issued_at_utc: None,
                forecast_45_valid_through: None,
                monthly_forecast_horizon: None,
                record_count: 0,
                notes: vec![error.0],
            });
        }
    };

    let now_date = now.date_naive();
    let retrieved_at = store.retrieved_at_utc.clone();
    let retrieval_age_days = retrieved_at.as_deref().and_then(|value| {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| (now.naive_utc().date() - dt.naive_utc().date()).num_days())
    });

    let newest_observation_date = store
        .records
        .iter()
        .filter(|r| r.kind == F107Kind::Observed)
        .filter_map(|r| r.observation_date.clone().or_else(|| Some(r.date.clone())))
        .max();

    let mut forecast_45_issued = None;
    let mut forecast_45_through: Option<NaiveDate> = None;
    for record in &store.records {
        if record.product != "45-day-forecast" {
            continue;
        }
        if let Some(issued) = &record.forecast_issued_at_utc {
            if forecast_45_issued
                .as_ref()
                .map(|prior: &String| issued > prior)
                .unwrap_or(true)
            {
                forecast_45_issued = Some(issued.clone());
            }
        }
        if let Ok((_, through)) = record.validity_window() {
            forecast_45_through = Some(
                forecast_45_through
                    .map(|prior| prior.max(through))
                    .unwrap_or(through),
            );
        }
    }

    let monthly_forecast_horizon = store
        .records
        .iter()
        .filter(|r| {
            r.kind == F107Kind::Forecast
                && r.cadence.as_deref() == Some("monthly")
                && r.product.contains("predicted-solar-cycle")
        })
        .filter_map(|r| r.valid_through.clone())
        .max();

    let has_obs = store.records.iter().any(|r| r.kind == F107Kind::Observed);
    let has_forecast = store.records.iter().any(|r| r.kind == F107Kind::Forecast);
    let only_climatology_path = !has_obs && !has_forecast;

    let mut notes = Vec::new();
    let status = if only_climatology_path {
        notes.push("fallback=climatology-only".into());
        "stale"
    } else if let Some(through) = forecast_45_through {
        if through < now_date {
            notes.push("45-day forecast horizon ended".into());
            "stale"
        } else if retrieval_age_days
            .map(|age| age > FORECAST_45_RETRIEVAL_MAX_AGE_DAYS)
            .unwrap_or(true)
        {
            notes.push("45-day forecast horizon covers now".into());
            notes.push(format!(
                "retrieval_age_days>{FORECAST_45_RETRIEVAL_MAX_AGE_DAYS} (reproducible but aging)"
            ));
            "stale"
        } else {
            notes.push("45-day forecast horizon covers now".into());
            "fresh"
        }
    } else if has_forecast {
        let monthly_horizon = monthly_forecast_horizon
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        if let Some(horizon) = monthly_horizon {
            if horizon < now_date {
                notes.push("monthly forecast horizon ended".into());
                "stale"
            } else if retrieval_age_days
                .map(|age| age > MONTHLY_CYCLE_FORECAST_RETRIEVAL_MAX_AGE_DAYS)
                .unwrap_or(true)
            {
                notes.push("monthly forecast horizon covers now".into());
                notes.push(format!(
                    "retrieval_age_days>{MONTHLY_CYCLE_FORECAST_RETRIEVAL_MAX_AGE_DAYS} (predicted-solar-cycle monthly cadence)"
                ));
                "stale"
            } else {
                notes.push("forecast".into());
                "forecast"
            }
        } else if retrieval_age_days
            .map(|age| age > MONTHLY_CYCLE_FORECAST_RETRIEVAL_MAX_AGE_DAYS)
            .unwrap_or(true)
        {
            notes.push(format!(
                "retrieval_age_days>{MONTHLY_CYCLE_FORECAST_RETRIEVAL_MAX_AGE_DAYS} (forecast cache aging)"
            ));
            "stale"
        } else {
            notes.push("forecast".into());
            "forecast"
        }
    } else if has_obs {
        let stale_obs = newest_observation_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .map(|d| (now_date - d).num_days() > OBSERVATION_MAX_LAG_DAYS)
            .unwrap_or(true);
        if stale_obs {
            notes.push("observations lag now".into());
            "stale"
        } else {
            "fresh"
        }
    } else {
        "stale"
    };

    if status == "stale" && store.dataset_id.contains("bundled") {
        notes.push("reproducible-but-stale".into());
    }

    Ok(StoreStatus {
        status,
        path: path.to_path_buf(),
        dataset_id: Some(store.dataset_id),
        snapshot_id: Some(store.snapshot_id),
        checksum_sha256: store.checksum_sha256,
        retrieved_at_utc: retrieved_at,
        retrieval_age_days,
        newest_observation_date,
        forecast_45_issued_at_utc: forecast_45_issued,
        forecast_45_valid_through: forecast_45_through.map(|d| d.format("%Y-%m-%d").to_string()),
        monthly_forecast_horizon,
        record_count: store.records.len(),
        notes,
    })
}

/// Resolve F10.7 for a UTC time against a local store (or bundled automatic).
pub fn resolve_against_store(
    time: Time<UTC>,
    store_path: Option<&Path>,
) -> Result<ResolvedSolarActivity> {
    let source = if let Some(path) = store_path {
        let store = verify_store(path, None)?;
        SolarActivitySource::Dataset(Arc::new(store))
    } else {
        SolarActivitySource::Automatic
    };
    resolve_f107(time, &source).map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn empty_store(dataset_id: &str, snapshot_id: &str, retrieved_at: &str) -> F107Store {
    F107Store {
        schema_version: nsb::F107_STORE_SCHEMA_VERSION,
        dataset_id: dataset_id.into(),
        snapshot_id: snapshot_id.into(),
        convention: "penticton-f107-sfu-as-reported-by-noaa-swpc".into(),
        convention_notes: (
            "Values are Penticton/DRAO 10.7 cm solar radio flux in sfu as republished by NOAA/NWS SWPC. \
             NSB does not convert between Earth-observed and 1-AU-adjusted variants; product identity is retained. \
             Airglow applies the Noll/SkyCalc monthly-averaged F10.7 quantity (msolflux)."
        ).into(),
        climatology_sfu: nsb::DEFAULT_SOLAR_RADIO_FLUX.value(),
        climatology_notes: (
            "Noll/SkyCalc-compatible climatological fallback equal to the Airglow neutralizing \
             F10.7 (DEFAULT_SOLAR_RADIO_FLUX ≈ 129.207 sfu), aligned with the ~129 sfu reference \
             mean used with the continuum solar-activity coefficients. Deterministic planning \
             fallback only — not an observation or forecast."
        ).into(),
        retrieved_at_utc: Some(retrieved_at.into()),
        records: Vec::new(),
        checksum_sha256: None,
    }
}

fn parse_fixtures(dir: &Path, retrieved_at: &str) -> Result<Vec<nsb::F107Record>> {
    let mut records = Vec::new();
    let daily = dir.join("daily-solar-indices.txt");
    if daily.exists() {
        records.extend(parse_daily_solar_indices(
            &fs::read_to_string(daily)?,
            retrieved_at,
        )?);
    }
    let forecast = dir.join("45-day-forecast.json");
    if forecast.exists() {
        records.extend(parse_45_day_forecast_json(
            &fs::read(forecast)?,
            retrieved_at,
        )?);
    }
    let predicted = dir.join("predicted-solar-cycle.json");
    if predicted.exists() {
        records.extend(parse_predicted_solar_cycle(
            &fs::read(predicted)?,
            retrieved_at,
        )?);
    }
    let observed = dir.join("observed-solar-cycle-indices-sample.json");
    if observed.exists() {
        records.extend(parse_observed_solar_cycle_indices(
            &fs::read(observed)?,
            retrieved_at,
        )?);
    }
    if records.is_empty() {
        bail!(
            "fixture directory {} produced no F10.7 records",
            dir.display()
        );
    }
    Ok(records)
}

fn fetch_and_parse_online(retrieved_at: &str) -> Result<Vec<nsb::F107Record>> {
    let mut records = Vec::new();
    let daily = http_get_text(DAILY_URL)?;
    records.extend(parse_daily_solar_indices(&daily, retrieved_at)?);
    let forecast = http_get_bytes(FORECAST_45_URL)?;
    records.extend(parse_45_day_forecast_json(&forecast, retrieved_at)?);
    let predicted = http_get_bytes(PREDICTED_URL)?;
    records.extend(parse_predicted_solar_cycle(&predicted, retrieved_at)?);
    match http_get_bytes(OBSERVED_MONTHLY_URL) {
        Ok(bytes) => match parse_observed_solar_cycle_indices(&bytes, retrieved_at) {
            Ok(extra) => records.extend(extra),
            Err(error) => {
                eprintln!("warning: observed monthly indices skipped: {error}");
            }
        },
        Err(error) => eprintln!("warning: observed monthly indices download failed: {error}"),
    }
    Ok(records)
}

fn http_get_text(url: &str) -> Result<String> {
    let response = reqwest::blocking::get(url)
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?;
    Ok(response.text()?)
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>> {
    let response = reqwest::blocking::get(url)
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?;
    Ok(response.bytes()?.to_vec())
}
