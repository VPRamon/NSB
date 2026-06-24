use crate::error::Result;
use qtty::{Quantity, Unit};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const MAX_CROSSING_REFINEMENTS: usize = 24;
const CROSSING_TOLERANCE_DAYS: f64 = 1.0e-5;

pub(super) fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

pub(super) fn tt_mjd_to_utc_time(time: ModifiedJulianDate) -> Time<UTC> {
    tempoch::Time::<TT>::from(time).to::<UTC>()
}

pub(super) fn utc_period_to_tt_mjd(window: Period<UTC>) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        utc_time_to_tt_mjd(window.start),
        utc_time_to_tt_mjd(window.end),
    )
}

pub(super) fn above_threshold_periods<V, F>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    f: &F,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if window.start >= window.end || step <= Days::new(0.0) {
        return Ok(Vec::new());
    }

    let mut periods = Vec::new();
    let mut t0 = window.start;
    let mut y0 = f(t0)?;
    let mut above0 = y0 > threshold;
    let mut open_start = above0.then_some(window.start);

    while t0 < window.end {
        let t1 = add_days_clamped(t0, step, window.end);
        if t1 <= t0 {
            break;
        }

        let y1 = f(t1)?;
        let above1 = y1 > threshold;
        if above0 != above1 {
            let crossing = refine_threshold_crossing(t0, y0, t1, y1, f, threshold)?;
            if above0 {
                if let Some(start) = open_start.take() {
                    push_non_empty_period(&mut periods, start, crossing);
                }
            } else {
                open_start = Some(crossing);
            }
        }

        t0 = t1;
        y0 = y1;
        above0 = above1;
    }

    if let Some(start) = open_start {
        push_non_empty_period(&mut periods, start, window.end);
    }

    Ok(periods)
}

pub(super) fn complement_periods(
    window: TimePeriod<ModifiedJulianDate>,
    periods: &[TimePeriod<ModifiedJulianDate>],
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let mut out = Vec::new();
    let mut cursor = window.start;

    for period in periods {
        let start = period.start.max(window.start);
        let end = period.end.min(window.end);
        if end <= window.start || start >= window.end || start >= end {
            continue;
        }
        push_non_empty_period(&mut out, cursor, start);
        cursor = cursor.max(end);
    }

    push_non_empty_period(&mut out, cursor, window.end);
    out
}

fn refine_threshold_crossing<V, F>(
    mut lo: ModifiedJulianDate,
    mut y_lo: Quantity<V>,
    mut hi: ModifiedJulianDate,
    mut y_hi: Quantity<V>,
    f: &F,
    threshold: Quantity<V>,
) -> Result<ModifiedJulianDate>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let lo_above = y_lo > threshold;
    for _ in 0..MAX_CROSSING_REFINEMENTS {
        let mid = midpoint_mjd(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = f(mid)?;
        if y_mid == threshold {
            return Ok(mid);
        }
        if (y_mid > threshold) == lo_above {
            lo = mid;
            y_lo = y_mid;
        } else {
            hi = mid;
            y_hi = y_mid;
        }
        if (hi.raw() - lo.raw()).abs() <= Days::new(CROSSING_TOLERANCE_DAYS) {
            break;
        }
    }

    Ok(linear_crossing_estimate(lo, y_lo, hi, y_hi, threshold))
}

fn linear_crossing_estimate<V>(
    lo: ModifiedJulianDate,
    y_lo: Quantity<V>,
    hi: ModifiedJulianDate,
    y_hi: Quantity<V>,
    threshold: Quantity<V>,
) -> ModifiedJulianDate
where
    V: Unit,
{
    let denom = y_hi.value() - y_lo.value();
    if !denom.is_finite() || denom == 0.0 {
        return midpoint_mjd(lo, hi);
    }
    let frac = ((threshold.value() - y_lo.value()) / denom).clamp(0.0, 1.0);
    let lo_raw = lo.raw().value();
    let hi_raw = hi.raw().value();
    ModifiedJulianDate::new(lo_raw + (hi_raw - lo_raw) * frac)
}

fn midpoint_mjd(lo: ModifiedJulianDate, hi: ModifiedJulianDate) -> ModifiedJulianDate {
    ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()))
}

fn add_days_clamped(
    time: ModifiedJulianDate,
    delta: Days,
    end: ModifiedJulianDate,
) -> ModifiedJulianDate {
    let next = ModifiedJulianDate::new(time.raw().value() + delta.value());
    next.min(end)
}

fn push_non_empty_period(
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
    start: ModifiedJulianDate,
    end: ModifiedJulianDate,
) {
    if start < end {
        periods.push(TimePeriod::new(start, end));
    }
}

pub(super) fn tt_mjd_period_to_utc(
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
