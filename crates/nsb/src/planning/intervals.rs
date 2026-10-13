// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::error::Result;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

pub(crate) fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

pub(crate) fn tt_mjd_to_utc_time(time: ModifiedJulianDate) -> Time<UTC> {
    tempoch::Time::<TT>::from(time).to::<UTC>()
}

pub(crate) fn utc_period_to_tt_mjd(window: Period<UTC>) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        utc_time_to_tt_mjd(window.start),
        utc_time_to_tt_mjd(window.end),
    )
}

pub(crate) fn coalesce_periods(periods: &mut Vec<TimePeriod<ModifiedJulianDate>>) {
    if periods.len() <= 1 {
        return;
    }

    periods.sort_by(|lhs, rhs| lhs.start.raw().value().total_cmp(&rhs.start.raw().value()));
    let mut out: Vec<TimePeriod<ModifiedJulianDate>> = Vec::with_capacity(periods.len());
    for period in periods.drain(..) {
        if let Some(last) = out.last_mut() {
            if period.start <= last.end {
                last.end = last.end.max(period.end);
                continue;
            }
        }
        out.push(period);
    }
    *periods = out;
}

pub(crate) fn tt_mjd_period_to_utc(
    window: TimePeriod<ModifiedJulianDate>,
    query_window_tt: TimePeriod<ModifiedJulianDate>,
    query_window_utc: Period<UTC>,
) -> Period<UTC> {
    let start = if window.start == query_window_tt.start {
        query_window_utc.start
    } else {
        tt_mjd_to_utc_time(window.start)
    };
    let end = if window.end == query_window_tt.end {
        query_window_utc.end
    } else {
        tt_mjd_to_utc_time(window.end)
    };
    Period::new(start, end)
}

/// Convert without expanding a closed TT interval. This checks native Rust
/// time semantics; each language/output adapter owns its representation limits.
pub(crate) fn inward_tt_mjd_period_to_utc(
    window: TimePeriod<ModifiedJulianDate>,
    query_window_tt: TimePeriod<ModifiedJulianDate>,
    query_window_utc: Period<UTC>,
) -> Result<Period<UTC>> {
    if window.start >= window.end {
        return Err(crate::NsbError::Unsupported(
            "empty TT interval cannot be exported".into(),
        ));
    }
    let converted = tt_mjd_period_to_utc(window, query_window_tt, query_window_utc);
    let error = || {
        crate::NsbError::Unsupported(
            "UTC endpoint cannot be represented inside its TT interval".into(),
        )
    };
    let endpoint = |original: Time<UTC>, start: bool| -> Result<Time<UTC>> {
        for i in 0..=1000 {
            let candidate = original
                + crate::units::Second::new(if start { 1.0 } else { -1.0 } * i as f64 * 0.0001);
            let tt = utc_time_to_tt_mjd(candidate);
            if tt >= window.start
                && tt <= window.end
                && candidate >= query_window_utc.start
                && candidate <= query_window_utc.end
            {
                return Ok(candidate);
            }
        }
        Err(error())
    };
    let result = Period::new(
        endpoint(converted.start, true)?,
        endpoint(converted.end, false)?,
    );
    if result.start >= result.end {
        return Err(error());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::reference_scan::{above_threshold_periods, CROSSING_TOLERANCE};
    use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
    use siderust::qtty::Days;

    fn test_window() -> TimePeriod<ModifiedJulianDate> {
        TimePeriod::new(
            ModifiedJulianDate::new(60_000.0),
            ModifiedJulianDate::new(60_001.0),
        )
    }

    #[test]
    fn certified_native_utc_conversion_is_inward_and_has_no_python_year_limit() {
        use chrono::TimeZone;
        for civil in [
            "2026-01-14T01:00:00.000000999Z".parse().unwrap(),
            "2016-12-31T23:59:59Z".parse().unwrap(),
            "2017-01-01T00:00:00Z".parse().unwrap(),
            chrono::Utc.with_ymd_and_hms(10000, 1, 1, 0, 0, 0).unwrap(),
        ] {
            let start = Time::<UTC>::from_chrono(civil);
            let utc = Period::new(start, start + crate::units::Second::new(1.0));
            let tt = utc_period_to_tt_mjd(utc);
            let result = inward_tt_mjd_period_to_utc(tt, tt, utc).unwrap();
            assert!(utc_time_to_tt_mjd(result.start) >= tt.start);
            assert!(utc_time_to_tt_mjd(result.end) <= tt.end);
            assert!(result.start >= utc.start && result.end <= utc.end);
            assert!((result.start - utc.start).value() <= 0.1);
            assert!((utc.end - result.end).value() <= 0.1);
            let point = TimePeriod::new(tt.start, tt.start);
            assert!(inward_tt_mjd_period_to_utc(point, tt, utc).is_err());
        }
    }

    #[test]
    fn authoritative_search_refines_bracketed_crossings() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0);

        let f =
            |time: ModifiedJulianDate| Ok(BandPhotonRadiance::new(time.raw().value() - 60_000.0));
        let periods = above_threshold_periods(window, step, &f, threshold).unwrap();

        assert_eq!(periods.len(), 1);
        assert!((periods[0].start.raw().value() - 60_000.5).abs() <= CROSSING_TOLERANCE.value());
        assert_eq!(periods[0].end, window.end);
    }

    #[test]
    fn exact_narrow_bump_between_validation_samples_is_not_pruned() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0 / 144.0);
        let exact = |time: ModifiedJulianDate| {
            let distance = (time.raw().value() - 60_000.25).abs();
            let value = if distance < 0.02 {
                1.0 - 2_500.0 * distance * distance
            } else {
                0.1
            };
            Ok(BandPhotonRadiance::new(value))
        };

        let actual = above_threshold_periods(window, step, &exact, threshold).unwrap();
        assert_eq!(
            actual.len(),
            1,
            "the independent oracle must resolve the bump"
        );
        let radius = (0.5_f64 / 2500.0).sqrt();
        for (boundary, expected) in [
            (actual[0].start, 60000.25 - radius),
            (actual[0].end, 60000.25 + radius),
        ] {
            assert!((boundary.raw().value() - expected).abs() <= CROSSING_TOLERANCE.value());
        }
    }

    #[test]
    fn thresholds_around_a_grazing_extremum_are_distinguished() {
        let window = test_window();
        let step = Days::new(1.0 / 144.0);
        let f = |time: ModifiedJulianDate| {
            let x = (time.raw().value() - 60_000.5) * 4.0;
            Ok(BandPhotonRadiance::new(1.0 - x * x))
        };

        let below_max =
            above_threshold_periods(window, step, &f, BandPhotonRadiance::new(0.999)).unwrap();
        let above_max =
            above_threshold_periods(window, step, &f, BandPhotonRadiance::new(1.001)).unwrap();

        assert_eq!(below_max.len(), 1);
        assert!(below_max[0].start < ModifiedJulianDate::new(60_000.5));
        assert!(below_max[0].end > ModifiedJulianDate::new(60_000.5));
        assert!(above_max.is_empty());
    }
}
