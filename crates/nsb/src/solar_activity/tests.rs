//! Unit tests for offline F10.7 resolution.

use super::*;
use crate::components::airglow::{SolarFluxUnits, DEFAULT_SOLAR_RADIO_FLUX};
use chrono::{DateTime, NaiveDate, Utc};
use tempoch::{Time, UTC};

fn t(input: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339(input)
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn sample_store() -> F107Store {
    F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "test-f107",
  "snapshot_id": "snap-a",
  "convention": "penticton-f107-sfu-as-reported-by-noaa-swpc",
  "convention_notes": "test",
  "climatology_sfu": 129.20671119074768,
  "climatology_notes": "Noll/SkyCalc neutralizing reference",
  "retrieved_at_utc": "2026-08-01T00:00:00Z",
  "records": [
    {
      "date": "2024-06-01",
      "value_sfu": 180.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "daily-solar-indices",
      "observation_date": "2024-06-01",
      "valid_from": "2024-06-01",
      "valid_through": "2024-06-01",
      "cadence": "daily"
    },
    {
      "date": "2024-06-01",
      "value_sfu": 175.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "observed-solar-cycle-indices",
      "observation_date": "2024-06-01",
      "valid_from": "2024-06-01",
      "valid_through": "2024-06-30",
      "cadence": "monthly"
    },
    {
      "date": "2026-09-01",
      "value_sfu": 150.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "45-day-forecast",
      "forecast_issued_at_utc": "2026-08-27T00:00:00Z",
      "valid_from": "2026-09-01",
      "valid_through": "2026-09-01",
      "cadence": "daily"
    },
    {
      "date": "2026-09-02",
      "value_sfu": 160.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "45-day-forecast",
      "forecast_issued_at_utc": "2026-08-27T00:00:00Z",
      "valid_from": "2026-09-02",
      "valid_through": "2026-09-02",
      "cadence": "daily"
    },
    {
      "date": "2026-09-01",
      "value_sfu": 133.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "predicted-solar-cycle",
      "valid_from": "2026-09-01",
      "valid_through": "2026-09-30",
      "cadence": "monthly",
      "range_low_sfu": 120.0,
      "range_high_sfu": 145.0
    },
    {
      "date": "2027-01-01",
      "value_sfu": 130.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "predicted-solar-cycle",
      "valid_from": "2027-01-01",
      "valid_through": "2027-01-31",
      "cadence": "monthly",
      "range_low_sfu": 110.0,
      "range_high_sfu": 150.0
    }
  ]
}"#,
    )
    .unwrap()
}

#[test]
fn explicit_override_wins() {
    let store = sample_store();
    let source = SolarActivitySource::Explicit(SolarFluxUnits::new(99.0));
    let resolved = resolve_f107(t("2024-06-01T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 99.0);
    assert_eq!(resolved.record.kind, F107Kind::Explicit);
    assert_eq!(resolved.resolution_step, "explicit-override");
    let _ = store;
}

#[test]
fn explicit_override_rejects_nan_and_non_positive() {
    let nan = SolarActivitySource::Explicit(SolarFluxUnits::new(f64::NAN));
    assert!(resolve_f107(t("2024-06-01T12:00:00Z"), &nan).is_err());
    let neg = SolarActivitySource::Explicit(SolarFluxUnits::new(-1.0));
    assert!(resolve_f107(t("2024-06-01T12:00:00Z"), &neg).is_err());
    let zero = SolarActivitySource::Explicit(SolarFluxUnits::new(0.0));
    assert!(resolve_f107(t("2024-06-01T12:00:00Z"), &zero).is_err());
}

#[test]
fn historical_prefers_monthly_observation_over_daily_and_forecast() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    let resolved = resolve_f107(t("2024-06-01T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 175.0);
    assert_eq!(resolved.record.kind, F107Kind::Observed);
    assert_eq!(resolved.record.product, "observed-solar-cycle-indices");
    assert_eq!(resolved.record.cadence.as_deref(), Some("monthly"));
    assert_eq!(resolved.resolution_step, "monthly-observed-local-store");
}

#[test]
fn daily_only_observation_does_not_drive_airglow_input() {
    let store = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "daily-only",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "records": [
    {
      "date": "2024-03-15",
      "value_sfu": 200.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "daily-solar-indices",
      "observation_date": "2024-03-15",
      "valid_from": "2024-03-15",
      "valid_through": "2024-03-15",
      "cadence": "daily"
    }
  ]
}"#,
    )
    .unwrap();
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    let resolved = resolve_f107(t("2024-03-15T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
    assert!((resolved.value.value() - DEFAULT_SOLAR_RADIO_FLUX.value()).abs() < 1e-9);
}

#[test]
fn near_future_uses_45_day_calendar_month_mean() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    let resolved = resolve_f107(t("2026-09-01T00:00:00Z"), &source).unwrap();
    // Freshest 2026-09-01 = 150; 2026-09-02 = 160 → mean 155.
    assert!((resolved.value.value() - 155.0).abs() < 1e-9);
    assert_eq!(resolved.record.kind, F107Kind::Forecast);
    assert_eq!(resolved.record.product, "45-day-forecast-monthly-mean");
    assert_eq!(resolved.record.cadence.as_deref(), Some("monthly"));
    assert_eq!(
        resolved.record.forecast_issued_at_utc.as_deref(),
        Some("2026-08-27T00:00:00Z")
    );
    assert_eq!(
        resolved.resolution_step,
        "short-range-forecast-monthly-mean"
    );
}

#[test]
fn longer_future_uses_monthly_forecast_without_fabricated_issuance() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    let resolved = resolve_f107(t("2027-01-15T00:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 130.0);
    assert_eq!(resolved.record.kind, F107Kind::Forecast);
    assert!(resolved.record.forecast_issued_at_utc.is_none());
    assert_eq!(resolved.resolution_step, "monthly-solar-cycle-forecast");
}

#[test]
fn beyond_horizon_uses_climatology() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    let resolved = resolve_f107(t("2035-01-01T00:00:00Z"), &source).unwrap();
    assert!((resolved.value.value() - DEFAULT_SOLAR_RADIO_FLUX.value()).abs() < 1e-9);
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
    assert_eq!(resolved.resolution_step, "climatology-fallback");
}

#[test]
fn rejects_nan_and_non_positive() {
    let err = F107Record {
        date: "2024-01-01".into(),
        value_sfu: f64::NAN,
        kind: F107Kind::Observed,
        provider: "x".into(),
        product: "y".into(),
        observation_date: Some("2024-01-01".into()),
        forecast_issued_at_utc: None,
        retrieved_at_utc: None,
        valid_from: None,
        valid_through: None,
        cadence: None,
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: None,
    }
    .validate()
    .unwrap_err();
    assert!(err.0.contains("finite"));
}

#[test]
fn forecast_may_omit_issuance_for_cycle_products() {
    F107Record {
        date: "2027-01-01".into(),
        value_sfu: 130.0,
        kind: F107Kind::Forecast,
        provider: "noaa-swpc".into(),
        product: "predicted-solar-cycle".into(),
        observation_date: None,
        forecast_issued_at_utc: None,
        retrieved_at_utc: Some("2026-08-27T08:00:00Z".into()),
        valid_from: Some("2027-01-01".into()),
        valid_through: Some("2027-01-31".into()),
        cadence: Some("monthly".into()),
        uncertainty_sfu: None,
        range_low_sfu: None,
        range_high_sfu: None,
        source_locator: None,
    }
    .validate()
    .unwrap();
}

#[test]
fn bundled_store_loads_and_resolves_offline() {
    let store = bundled_f107_store().unwrap();
    assert_eq!(store.schema_version, 1);
    assert!(store.checksum_sha256.is_some());
    assert!((store.climatology_sfu - DEFAULT_SOLAR_RADIO_FLUX.value()).abs() < 1e-6);
    let source = SolarActivitySource::Automatic;
    let historical = resolve_f107(t("2026-08-20T12:00:00Z"), &source).unwrap();
    assert_eq!(historical.record.kind, F107Kind::Observed);
    assert_eq!(historical.record.cadence.as_deref(), Some("monthly"));
    let near = resolve_f107(t("2026-09-05T12:00:00Z"), &source).unwrap();
    assert_eq!(near.record.kind, F107Kind::Forecast);
    assert_eq!(near.record.product, "45-day-forecast-monthly-mean");
    let far = resolve_f107(t("2040-01-01T00:00:00Z"), &source).unwrap();
    assert_eq!(far.record.kind, F107Kind::Climatology);
}

#[test]
fn snapshot_pin_reproduces_after_merge() {
    let original = sample_store();
    let bytes_a = original.to_json_bytes().unwrap();
    let pinned = F107Store::from_json_bytes(&bytes_a).unwrap();
    let checksum = pinned.checksum_sha256.clone().unwrap();

    let merged = pinned
        .merge_with(
            &[F107Record {
                date: "2024-06-02".into(),
                value_sfu: 181.0,
                kind: F107Kind::Observed,
                provider: "noaa-swpc".into(),
                product: "daily-solar-indices".into(),
                observation_date: Some("2024-06-02".into()),
                forecast_issued_at_utc: None,
                retrieved_at_utc: None,
                valid_from: Some("2024-06-02".into()),
                valid_through: Some("2024-06-02".into()),
                cadence: Some("daily".into()),
                uncertainty_sfu: None,
                range_low_sfu: None,
                range_high_sfu: None,
                source_locator: None,
            }],
            "snap-b",
            Some("2026-08-28T00:00:00Z".into()),
        )
        .unwrap();
    assert_ne!(merged.snapshot_id, pinned.snapshot_id);

    let pinned_again = F107Store::from_json_bytes(&bytes_a).unwrap();
    assert_eq!(
        pinned_again.checksum_sha256.as_deref(),
        Some(checksum.as_str())
    );
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(pinned_again));
    let resolved = resolve_f107(t("2024-06-01T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 175.0);
    assert_eq!(resolved.checksum_sha256.as_deref(), Some(checksum.as_str()));
}

#[test]
fn utc_calendar_date_matches_chrono() {
    assert_eq!(
        utc_calendar_date(t("2024-06-01T23:59:59Z")),
        NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()
    );
}
