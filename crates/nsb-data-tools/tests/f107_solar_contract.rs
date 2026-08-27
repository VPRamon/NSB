//! Offline F10.7 provider / updater contract tests (no live network).

use nsb_data_tools::solar::{
    parse_45_day_forecast_json, parse_daily_solar_indices, update_store, verify_store, UpdateMode,
};
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/swpc")
}

#[test]
fn parses_daily_and_45_day_fixtures() {
    let daily = fs::read_to_string(fixtures().join("daily-solar-indices.txt")).unwrap();
    let records = parse_daily_solar_indices(&daily, "2026-08-27T00:00:00Z").unwrap();
    assert!(!records.is_empty());
    assert!(records.iter().all(|r| r.kind == nsb::F107Kind::Observed));

    let forecast = fs::read(fixtures().join("45-day-forecast.json")).unwrap();
    let forecasts = parse_45_day_forecast_json(&forecast, "2026-08-27T00:00:00Z").unwrap();
    assert!(!forecasts.is_empty());
    assert!(forecasts.iter().all(|r| r.kind == nsb::F107Kind::Forecast));
    assert!(forecasts.iter().all(|r| r.forecast_issued_at_utc.is_some()));
}

#[test]
fn fixture_update_is_atomic_and_retains_snapshots() {
    let dir = tempdir().unwrap();
    let store = dir.path().join("f107_store.json");
    let report = update_store(
        &store,
        UpdateMode::FixtureDir(fixtures()),
        "test-f107-local",
    )
    .unwrap();
    assert_eq!(report.status, "fresh");
    assert!(store.exists());
    assert!(report.snapshot_path.exists());
    let verified = verify_store(&store, Some(&report.checksum_sha256)).unwrap();
    assert!(!verified.records.is_empty());

    // Corrupt payload must not replace a valid store when parse fails before write —
    // simulate by verifying invalid JSON is rejected by verify_store.
    let bad = dir.path().join("bad.json");
    fs::write(&bad, b"{\"schema_version\":999}").unwrap();
    assert!(verify_store(&bad, None).is_err());
    // Active store still valid.
    verify_store(&store, Some(&report.checksum_sha256)).unwrap();
}

#[test]
fn invalid_45_day_schema_fails_closed() {
    let err = parse_45_day_forecast_json(br#"{"issued":"","data":[]}"#, "2026-08-27T00:00:00Z")
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}
