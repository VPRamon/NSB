//! Offline F10.7 provider / freeze / status contract tests (no live network).

use chrono::{TimeZone, Utc};
use nsb_data_tools::solar::{
    freeze_store, parse_45_day_forecast_json, parse_daily_solar_indices,
    parse_predicted_solar_cycle, status_report_at, update_store, verify_store, FreezeParams,
    UpdateMode,
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
fn predicted_solar_cycle_does_not_fabricate_issuance_from_retrieval() {
    let bytes = fs::read(fixtures().join("predicted-solar-cycle.json")).unwrap();
    let retrieved = "2026-08-27T08:00:00Z";
    let records = parse_predicted_solar_cycle(&bytes, retrieved).unwrap();
    assert!(!records.is_empty());
    assert!(records.iter().all(|r| r.forecast_issued_at_utc.is_none()));
    assert!(records
        .iter()
        .all(|r| r.retrieved_at_utc.as_deref() == Some(retrieved)));
    assert!(records
        .iter()
        .all(|r| r.cadence.as_deref() == Some("monthly")));
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

    let bad = dir.path().join("bad.json");
    fs::write(&bad, b"{\"schema_version\":999}").unwrap();
    assert!(verify_store(&bad, None).is_err());
    verify_store(&store, Some(&report.checksum_sha256)).unwrap();
}

#[test]
fn invalid_45_day_schema_fails_closed() {
    let err = parse_45_day_forecast_json(br#"{"issued":"","data":[]}"#, "2026-08-27T00:00:00Z")
        .unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn freeze_reproduces_bundled_store_and_manifest_sha_bit_for_bit() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("f107_store.json");
    let report = freeze_store(&FreezeParams {
        fixture_dir: fixtures(),
        store_path: out.clone(),
        dataset_id: "nsb-f107-bundled-offline".into(),
        snapshot_id: "bundled-2026-08-27".into(),
        retrieved_at_utc: "2026-08-27T08:00:00Z".into(),
    })
    .unwrap();
    assert_eq!(report.status, "frozen");

    let generated = fs::read(&out).unwrap();
    let checked_in =
        fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/f107_store.json"))
            .unwrap();
    assert_eq!(
        generated, checked_in,
        "freeze output must match checked-in f107_store.json bytes"
    );

    let generated_sha = sha256_hex(&generated);
    let checked_in_sha = sha256_hex(&checked_in);
    assert_eq!(generated_sha, checked_in_sha);
    assert_eq!(generated_sha, report.checksum_sha256);

    let manifest = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml"),
    )
    .unwrap();
    assert!(
        manifest.contains(&format!("sha256 = \"{generated_sha}\"")),
        "manifest.toml must record the frozen SHA-256"
    );
    assert!(manifest.contains("nsb-data solar f107 freeze"));
    assert!(manifest.contains("--retrieved-at 2026-08-27T08:00:00Z"));
}

#[test]
fn status_missing_invalid_fresh_and_stale_are_deterministic() {
    let missing = status_report_at(
        Path::new("/tmp/nsb-f107-definitely-missing.json"),
        Utc.with_ymd_and_hms(2026, 8, 28, 0, 0, 0).unwrap(),
    )
    .unwrap();
    assert_eq!(missing.status, "missing");

    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad.json");
    fs::write(&bad, b"{not-json").unwrap();
    let invalid =
        status_report_at(&bad, Utc.with_ymd_and_hms(2026, 8, 28, 0, 0, 0).unwrap()).unwrap();
    assert_eq!(invalid.status, "invalid");

    let store = dir.path().join("f107_store.json");
    freeze_store(&FreezeParams {
        fixture_dir: fixtures(),
        store_path: store.clone(),
        dataset_id: "nsb-f107-bundled-offline".into(),
        snapshot_id: "bundled-2026-08-27".into(),
        retrieved_at_utc: "2026-08-27T08:00:00Z".into(),
    })
    .unwrap();

    // Same day as retrieval + horizon still covers → fresh.
    let fresh =
        status_report_at(&store, Utc.with_ymd_and_hms(2026, 8, 27, 12, 0, 0).unwrap()).unwrap();
    assert_eq!(fresh.status, "fresh");
    assert!(fresh.forecast_45_valid_through.is_some());

    // Long after 45-day horizon → stale.
    let stale =
        status_report_at(&store, Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap()).unwrap();
    assert_eq!(stale.status, "stale");
    assert!(stale
        .notes
        .iter()
        .any(|n| n.contains("horizon") || n.contains("stale") || n.contains("reproducible")));

    // Climatology-only store.
    let climate = dir.path().join("climate.json");
    fs::write(
        &climate,
        br#"{
  "schema_version": 1,
  "dataset_id": "climate-only",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "records": []
}"#,
    )
    .unwrap();
    let climate_status = status_report_at(
        &climate,
        Utc.with_ymd_and_hms(2026, 8, 28, 0, 0, 0).unwrap(),
    )
    .unwrap();
    assert_eq!(climate_status.status, "stale");
    assert!(climate_status
        .notes
        .iter()
        .any(|n| n.contains("climatology")));
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

use std::path::Path;
