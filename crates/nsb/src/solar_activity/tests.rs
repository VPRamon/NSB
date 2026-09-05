//! Unit tests for offline F10.7 resolution.

use super::monthly::{days_in_month, is_finalized_monthly_observation};
use super::resolve::utc_calendar_date;
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
      "retrieved_at_utc": "2026-08-01T00:00:00Z",
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
      "retrieved_at_utc": "2026-08-01T00:00:00Z",
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

fn complete_september_forecast_store() -> F107Store {
    let mut records = String::from("[");
    for day in 1..=30 {
        if day > 1 {
            records.push(',');
        }
        records.push_str(&format!(
            r#"{{
      "date": "2026-09-{day:02}",
      "value_sfu": {value}.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "45-day-forecast",
      "forecast_issued_at_utc": "2026-08-27T00:00:00Z",
      "valid_from": "2026-09-{day:02}",
      "valid_through": "2026-09-{day:02}",
      "cadence": "daily"
    }}"#,
            value = 100 + day
        ));
    }
    records.push(']');
    let json = format!(
        r#"{{
  "schema_version": 1,
  "dataset_id": "complete-sep",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "retrieved_at_utc": "2026-08-27T08:00:00Z",
  "records": {records}
}}"#
    );
    F107Store::from_json_str(&json).unwrap()
}

#[test]
fn explicit_override_wins() {
    let source = SolarActivitySource::Explicit(SolarFluxUnits::new(99.0));
    let resolved = resolve_f107(t("2024-06-01T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 99.0);
    assert_eq!(resolved.record.kind, F107Kind::Explicit);
    assert_eq!(resolved.resolution_step, "explicit-override");
    assert!(
        !resolved.is_degraded_planning_input(),
        "caller-explicit F10.7 is not a degraded planning substitute"
    );
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
fn historical_prefers_completed_monthly_observation() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    // Store retrieved 2026-08-01; June 2024 monthly is finalized and covers June dates.
    let resolved = resolve_f107(t("2024-06-15T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 175.0);
    assert_eq!(resolved.record.kind, F107Kind::Observed);
    assert_eq!(resolved.record.product, "observed-solar-cycle-indices");
    assert_eq!(
        resolved.monthly_completeness,
        Some(MonthlyCompleteness::CompleteObserved)
    );
    assert_eq!(resolved.resolution_step, "monthly-observed-complete");
    assert!(
        !resolved.is_degraded_planning_input(),
        "finalized monthly observed F10.7 must not be labelled degraded planning input"
    );
}

#[test]
fn incomplete_month_two_forecast_days_does_not_become_msolflux() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    // Only Sep 1–2 present; must fall back to predicted-solar-cycle, not 155.
    let resolved = resolve_f107(t("2026-09-15T00:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 133.0);
    assert_eq!(resolved.record.product, "predicted-solar-cycle");
    assert_eq!(
        resolved.monthly_completeness,
        Some(MonthlyCompleteness::OfficialMonthlyPrediction)
    );
    assert!(resolved.is_degraded_planning_input());
}

#[test]
fn complete_30_day_month_may_use_45_day_monthly_mean() {
    let store = std::sync::Arc::new(complete_september_forecast_store());
    let source = SolarActivitySource::Dataset(store);
    // No observations present → complete 45-day month mean is admissible.
    let resolved = resolve_f107(t("2026-09-15T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.record.product, "45-day-forecast-monthly-mean");
    assert_eq!(
        resolved.monthly_completeness,
        Some(MonthlyCompleteness::CompleteForecast)
    );
    assert_eq!(resolved.total_days, Some(30));
    assert_eq!(resolved.forecast_days, Some(30));
    let expected_mean = (101..=130).sum::<u32>() as f64 / 30.0;
    assert!((resolved.value.value() - expected_mean).abs() < 1e-9);
}

#[test]
fn current_month_prefers_observed_plus_forecast_over_pure_45_day() {
    // Full September 45-day coverage + observed days 1–15. Mid-month evaluation
    // must prefer provisional observed+forecast, not the all-forecast month mean.
    let mut records = Vec::new();
    for day in 1..=30 {
        records.push(format!(
            r#"{{
      "date": "2026-09-{day:02}",
      "value_sfu": 200.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "45-day-forecast",
      "forecast_issued_at_utc": "2026-08-27T00:00:00Z",
      "valid_from": "2026-09-{day:02}",
      "valid_through": "2026-09-{day:02}",
      "cadence": "daily"
    }}"#
        ));
    }
    for day in 1..=15 {
        records.push(format!(
            r#"{{
      "date": "2026-09-{day:02}",
      "value_sfu": 100.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "daily-solar-indices",
      "observation_date": "2026-09-{day:02}",
      "valid_from": "2026-09-{day:02}",
      "valid_through": "2026-09-{day:02}",
      "cadence": "daily"
    }}"#
        ));
    }
    let json = format!(
        r#"{{
  "schema_version": 1,
  "dataset_id": "current-pref",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "retrieved_at_utc": "2026-09-15T12:00:00Z",
  "records": [{}]
}}"#,
        records.join(",")
    );
    let store = F107Store::from_json_str(&json).unwrap();
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    let resolved = resolve_f107(t("2026-09-15T12:00:00Z"), &source).unwrap();
    assert_eq!(
        resolved.record.product,
        "current-month-observed-plus-forecast-mean"
    );
    assert_eq!(
        resolved.monthly_completeness,
        Some(MonthlyCompleteness::ProvisionalObservedPlusForecast)
    );
    // 15 obs @100 + 15 forecast @200 → mean 150, not pure-forecast 200.
    assert!((resolved.value.value() - 150.0).abs() < 1e-9);
    assert_eq!(resolved.observed_days, Some(15));
    assert_eq!(resolved.forecast_days, Some(15));
}

#[test]
fn predicted_solar_cycle_rejects_future_retrieval_as_evidence() {
    let store = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "future-pred",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "retrieved_at_utc": "2026-08-27T08:00:00Z",
  "records": [
    {
      "date": "2026-08-01",
      "value_sfu": 140.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "predicted-solar-cycle",
      "retrieved_at_utc": "2026-08-27T08:00:00Z",
      "valid_from": "2026-08-01",
      "valid_through": "2026-08-31",
      "cadence": "monthly"
    }
  ]
}"#,
    )
    .unwrap();
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    // Evaluation before the prediction was retrieved into the store.
    let resolved = resolve_f107(t("2026-08-01T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
}

#[test]
fn february_non_leap_requires_28_days() {
    assert_eq!(
        days_in_month(NaiveDate::from_ymd_opt(2027, 2, 1).unwrap()),
        28
    );
    assert_eq!(
        days_in_month(NaiveDate::from_ymd_opt(2028, 2, 1).unwrap()),
        29
    );
}

#[test]
fn forecast_issued_after_requested_time_does_not_leak() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    // Aug 1 evaluation must not use Aug 27-issued September 45-day rows.
    let resolved = resolve_f107(t("2026-08-01T12:00:00Z"), &source).unwrap();
    assert_ne!(resolved.record.product, "45-day-forecast-monthly-mean");
    // Falls through to predicted-solar-cycle for August if present, else climatology.
    // Sample store has no August predicted row → climatology.
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
}

#[test]
fn current_incomplete_monthly_observed_does_not_count_as_final() {
    let store = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "mtd",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "retrieved_at_utc": "2026-08-27T08:00:00Z",
  "records": [
    {
      "date": "2026-08-01",
      "value_sfu": 140.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "observed-solar-cycle-indices-month-to-date",
      "observation_date": "2026-08-01",
      "valid_from": "2026-08-01",
      "valid_through": "2026-08-31",
      "cadence": "monthly"
    },
    {
      "date": "2026-08-01",
      "value_sfu": 133.7,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "predicted-solar-cycle",
      "retrieved_at_utc": "2026-08-01T00:00:00Z",
      "valid_from": "2026-08-01",
      "valid_through": "2026-08-31",
      "cadence": "monthly"
    }
  ]
}"#,
    )
    .unwrap();
    let as_of = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
    assert!(!is_finalized_monthly_observation(&store.records[0], as_of));
    // Even after the month ends, month-to-date product must not become CompleteObserved.
    assert!(!is_finalized_monthly_observation(
        &store.records[0],
        NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()
    ));
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    let resolved = resolve_f107(t("2026-08-20T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.record.product, "predicted-solar-cycle");
    assert_ne!(resolved.record.kind, F107Kind::Observed);
}

#[test]
fn provisional_current_month_observed_plus_forecast_when_complete() {
    // August 2026 has 31 days. Evaluation Aug 10: need obs days 1–10 + forecast 11–31.
    let mut records = Vec::new();
    for day in 1..=10 {
        records.push(format!(
            r#"{{
      "date": "2026-08-{day:02}",
      "value_sfu": 120.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "daily-solar-indices",
      "observation_date": "2026-08-{day:02}",
      "valid_from": "2026-08-{day:02}",
      "valid_through": "2026-08-{day:02}",
      "cadence": "daily"
    }}"#
        ));
    }
    for day in 11..=31 {
        records.push(format!(
            r#"{{
      "date": "2026-08-{day:02}",
      "value_sfu": 130.0,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "45-day-forecast",
      "forecast_issued_at_utc": "2026-08-09T00:00:00Z",
      "valid_from": "2026-08-{day:02}",
      "valid_through": "2026-08-{day:02}",
      "cadence": "daily"
    }}"#
        ));
    }
    let json = format!(
        r#"{{
  "schema_version": 1,
  "dataset_id": "prov",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "records": [{}]
}}"#,
        records.join(",")
    );
    let store = F107Store::from_json_str(&json).unwrap();
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    let resolved = resolve_f107(t("2026-08-10T12:00:00Z"), &source).unwrap();
    assert_eq!(
        resolved.record.product,
        "current-month-observed-plus-forecast-mean"
    );
    assert_eq!(resolved.record.kind, F107Kind::Forecast);
    assert_eq!(
        resolved.monthly_completeness,
        Some(MonthlyCompleteness::ProvisionalObservedPlusForecast)
    );
    assert_eq!(resolved.observed_days, Some(10));
    assert_eq!(resolved.forecast_days, Some(21));
    assert_eq!(resolved.total_days, Some(31));
}

#[test]
fn provisional_gap_falls_back_to_monthly_prediction() {
    let store = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "gap",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.20671119074768,
  "records": [
    {
      "date": "2026-08-01",
      "value_sfu": 120.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "daily-solar-indices",
      "observation_date": "2026-08-01",
      "valid_from": "2026-08-01",
      "valid_through": "2026-08-01",
      "cadence": "daily"
    },
    {
      "date": "2026-08-01",
      "value_sfu": 133.7,
      "kind": "forecast",
      "provider": "noaa-swpc",
      "product": "predicted-solar-cycle",
      "retrieved_at_utc": "2026-08-01T00:00:00Z",
      "valid_from": "2026-08-01",
      "valid_through": "2026-08-31",
      "cadence": "monthly"
    }
  ]
}"#,
    )
    .unwrap();
    let source = SolarActivitySource::Dataset(std::sync::Arc::new(store));
    let resolved = resolve_f107(t("2026-08-10T12:00:00Z"), &source).unwrap();
    assert_eq!(resolved.record.product, "predicted-solar-cycle");
}

#[test]
fn longer_future_uses_monthly_forecast_without_fabricated_issuance() {
    let store = std::sync::Arc::new(sample_store());
    let source = SolarActivitySource::Dataset(store);
    let resolved = resolve_f107(t("2027-01-15T00:00:00Z"), &source).unwrap();
    assert_eq!(resolved.value.value(), 130.0);
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
    let far = resolve_f107(t("2040-01-01T00:00:00Z"), &source).unwrap();
    assert_eq!(far.record.kind, F107Kind::Climatology);
}

#[test]
fn store_rejects_unsupported_schema_and_empty_identity() {
    let schema = F107Store::from_json_str(
        r#"{
  "schema_version": 2,
  "dataset_id": "x",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.2,
  "records": []
}"#,
    )
    .unwrap_err();
    assert!(schema.0.contains("unsupported F10.7 store schema"));

    let empty_id = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "   ",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.2,
  "records": []
}"#,
    )
    .unwrap_err();
    assert!(empty_id.0.contains("dataset_id"));
}

#[test]
fn store_rejects_conflicting_duplicate_identity() {
    let err = F107Store::from_json_str(
        r#"{
  "schema_version": 1,
  "dataset_id": "dup",
  "snapshot_id": "s",
  "convention": "test",
  "climatology_sfu": 129.2,
  "records": [
    {
      "date": "2024-06-01",
      "value_sfu": 180.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "observed-solar-cycle-indices",
      "observation_date": "2024-06-01",
      "valid_from": "2024-06-01",
      "valid_through": "2024-06-30",
      "cadence": "monthly"
    },
    {
      "date": "2024-06-01",
      "value_sfu": 190.0,
      "kind": "observed",
      "provider": "noaa-swpc",
      "product": "observed-solar-cycle-indices",
      "observation_date": "2024-06-01",
      "valid_from": "2024-06-01",
      "valid_through": "2024-06-30",
      "cadence": "monthly"
    }
  ]
}"#,
    )
    .unwrap_err();
    assert!(err.0.contains("conflicting F10.7 records"));
}

#[test]
fn automatic_resolution_uses_bundled_store_identity() {
    let resolved =
        resolve_f107(t("2040-01-01T00:00:00Z"), &SolarActivitySource::Automatic).unwrap();
    let bundled = bundled_f107_store().unwrap();
    assert_eq!(
        resolved.dataset_id.as_deref(),
        Some(bundled.dataset_id.as_str())
    );
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
    assert!(resolved.is_degraded_planning_input());
}

#[test]
fn degraded_planning_input_flags_kind_or_completeness_independently() {
    // Public maturity helper is intentionally fail-open on either signal so a
    // future resolver pairing change cannot silently drop the degraded label.
    let mut observed_but_provisional = resolve_f107(
        t("2024-06-15T12:00:00Z"),
        &SolarActivitySource::Dataset(std::sync::Arc::new(sample_store())),
    )
    .unwrap();
    assert_eq!(observed_but_provisional.record.kind, F107Kind::Observed);
    observed_but_provisional.monthly_completeness =
        Some(MonthlyCompleteness::ProvisionalObservedPlusForecast);
    assert!(
        observed_but_provisional.is_degraded_planning_input(),
        "degraded monthly completeness must flag planning maturity even when kind stays Observed"
    );

    let mut forecast_without_completeness = resolve_f107(
        t("2026-09-15T00:00:00Z"),
        &SolarActivitySource::Dataset(std::sync::Arc::new(sample_store())),
    )
    .unwrap();
    assert_eq!(forecast_without_completeness.record.kind, F107Kind::Forecast);
    forecast_without_completeness.monthly_completeness = None;
    assert!(
        forecast_without_completeness.is_degraded_planning_input(),
        "Forecast/Climatology kind must flag planning maturity even without completeness metadata"
    );
}

#[test]
fn dataset_resolution_does_not_silently_use_bundled_store() {
    let store = std::sync::Arc::new(sample_store());
    let resolved = resolve_f107(
        t("2035-01-01T00:00:00Z"),
        &SolarActivitySource::Dataset(store.clone()),
    )
    .unwrap();
    assert_eq!(resolved.dataset_id.as_deref(), Some("test-f107"));
    assert_ne!(
        resolved.dataset_id.as_deref(),
        Some(bundled_f107_store().unwrap().dataset_id.as_str())
    );
    assert_eq!(resolved.record.kind, F107Kind::Climatology);
}

#[test]
fn utc_calendar_date_matches_chrono() {
    assert_eq!(
        utc_calendar_date(t("2024-06-01T23:59:59Z")),
        NaiveDate::from_ymd_opt(2024, 6, 1).unwrap()
    );
}
