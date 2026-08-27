//! Fail-safe F10.7 store update / import / verify / resolve helpers.

use super::providers::{
    parse_45_day_forecast_json, parse_daily_solar_indices, parse_observed_solar_cycle_indices,
    parse_predicted_solar_cycle,
};
use crate::platform::artifact_store::atomic_write;
use crate::platform::checksum_io::sha256_bytes;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use nsb::{resolve_f107, F107Store, ResolvedSolarActivity, SolarActivitySource};
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

/// How an update obtains provider bytes.
#[derive(Debug, Clone)]
pub enum UpdateMode {
    /// Fetch authoritative SWPC products over the network.
    Online,
    /// Read pinned fixtures from a directory (CI / offline).
    FixtureDir(PathBuf),
}

/// Outcome of an update attempt.
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

/// Refresh a local F10.7 store from online products or fixtures.
///
/// Validated bytes are written atomically. Previous active snapshots are copied
/// into a `snapshots/` directory before replacement so pinned runs remain
/// reproducible.
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
    let bytes = merged
        .to_json_bytes()
        .map_err(|error| anyhow::anyhow!(error.0))?;
    // Re-parse to attach checksum and fail closed before activation.
    let verified = F107Store::from_json_bytes(&bytes).map_err(|error| anyhow::anyhow!(error.0))?;
    let checksum = verified
        .checksum_sha256
        .clone()
        .context("verified store missing checksum")?;

    if let Some(parent) = store_path.parent() {
        let snapshots = parent.join("snapshots");
        fs::create_dir_all(&snapshots)?;
        if store_path.exists() {
            let previous = fs::read(store_path)?;
            let previous_checksum = sha256_bytes(&previous);
            let archive = snapshots.join(format!(
                "{}-{previous_checksum}.json",
                base.snapshot_id.replace(':', "")
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
            status: "fresh",
            notes,
        })
    } else {
        bail!("store path has no parent directory");
    }
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

/// Human-oriented status for an active store path.
pub fn status_report(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(format!(
            "status=missing path={} (run: nsb-data solar f107 update)",
            path.display()
        ));
    }
    let store = verify_store(path, None)?;
    let observed = store
        .records
        .iter()
        .filter(|r| r.kind == nsb::F107Kind::Observed)
        .count();
    let forecast = store
        .records
        .iter()
        .filter(|r| r.kind == nsb::F107Kind::Forecast)
        .count();
    Ok(format!(
        "status=present path={} dataset={} snapshot={} checksum={} records={} observed={} forecast={} climatology_sfu={} convention={}",
        path.display(),
        store.dataset_id,
        store.snapshot_id,
        store.checksum_sha256.as_deref().unwrap_or("n/a"),
        store.records.len(),
        observed,
        forecast,
        store.climatology_sfu,
        store.convention
    ))
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
        // Noll/SkyCalc neutralizing reference: solar_corr = 0.2068 + 0.006139*F10.7 = 1
        // at DEFAULT_SOLAR_RADIO_FLUX ≈ 129.207 sfu (compatible with the ~129 sfu
        // 1954–2007 reference mean used with the Airglow continuum coefficients).
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
    // Monthly observed history is large; keep optional / best-effort.
    match http_get_bytes(OBSERVED_MONTHLY_URL) {
        Ok(bytes) => match parse_observed_solar_cycle_indices(&bytes, retrieved_at) {
            Ok(extra) => records.extend(extra),
            Err(error) => {
                // Fail soft for this secondary product; primary products already validated.
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
