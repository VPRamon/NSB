use crate::error::Result;
use log::{debug, trace};
use qtty::{Quantity, Unit};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const MAX_CROSSING_REFINEMENTS: usize = 24;
const CROSSING_TOLERANCE_DAYS: f64 = 1.0e-5;
const MAX_ADAPTIVE_SUBDIVISIONS: usize = 18;
const MAX_ADAPTIVE_ACCEPT_SPAN_DAYS: f64 = 1.0;
const SMOOTHNESS_SAFETY_FACTOR: f64 = 8.0;
const SMOOTHNESS_RELATIVE_MARGIN: f64 = 1.0e-8;

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
        debug!(
            "skipping scan threshold search: non-positive window or step; start_mjd={}, end_mjd={}, step_days={}",
            window.start.raw().value(),
            window.end.raw().value(),
            step.value()
        );
        return Ok(Vec::new());
    }

    debug!(
        "running scan threshold search: start_mjd={}, end_mjd={}, step_days={}, threshold={}",
        window.start.raw().value(),
        window.end.raw().value(),
        step.value(),
        threshold.value()
    );

    let mut periods = Vec::new();
    let mut t0 = window.start;
    let mut y0 = f(t0)?;
    let mut above0 = y0 > threshold;
    let mut open_start = above0.then_some(window.start);
    let mut samples = 1usize;
    let mut crossings = 0usize;

    while t0 < window.end {
        let t1 = add_days_clamped(t0, step, window.end);
        if t1 <= t0 {
            break;
        }

        let y1 = f(t1)?;
        samples += 1;
        let above1 = y1 > threshold;
        if above0 != above1 {
            crossings += 1;
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

    debug!(
        "completed scan threshold search: samples={}, crossings={}, above_periods={}",
        samples,
        crossings,
        periods.len()
    );

    Ok(periods)
}

pub(super) fn adaptive_above_threshold_periods<V, F>(
    window: TimePeriod<ModifiedJulianDate>,
    fallback_step: Days,
    f: &F,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if window.start >= window.end || fallback_step <= Days::new(0.0) {
        debug!(
            "skipping adaptive threshold search: non-positive window or step; start_mjd={}, end_mjd={}, fallback_step_days={}",
            window.start.raw().value(),
            window.end.raw().value(),
            fallback_step.value()
        );
        return Ok(Vec::new());
    }
    if interval_width_days(window.start, window.end) <= 4.0 * fallback_step.value() {
        debug!(
            "falling back to scan threshold search for short interval: width_days={}, fallback_step_days={}",
            interval_width_days(window.start, window.end),
            fallback_step.value()
        );
        return above_threshold_periods(window, fallback_step, f, threshold);
    }

    debug!(
        "running adaptive threshold search: start_mjd={}, end_mjd={}, fallback_step_days={}, threshold={}",
        window.start.raw().value(),
        window.end.raw().value(),
        fallback_step.value(),
        threshold.value()
    );

    let start = threshold_sample(window.start, f, threshold)?;
    let end = threshold_sample(window.end, f, threshold)?;
    let mut periods = Vec::new();
    collect_adaptive_above(
        start,
        end,
        fallback_step.value(),
        f,
        threshold,
        0,
        &mut periods,
    )?;
    coalesce_periods(&mut periods);
    debug!(
        "completed adaptive threshold search: above_periods={}",
        periods.len()
    );
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

#[derive(Clone, Copy)]
struct ThresholdSample<V: Unit> {
    time: ModifiedJulianDate,
    value: Quantity<V>,
    above: bool,
}

fn collect_adaptive_above<V, F>(
    lo: ThresholdSample<V>,
    hi: ThresholdSample<V>,
    fallback_step_days: f64,
    f: &F,
    threshold: Quantity<V>,
    depth: usize,
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
) -> Result<()>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let width_days = interval_width_days(lo.time, hi.time);
    if width_days <= 0.0 {
        return Ok(());
    }
    if width_days <= fallback_step_days {
        collect_terminal_pair(lo, hi, f, threshold, periods)?;
        return Ok(());
    }

    let mid_time = midpoint_mjd(lo.time, hi.time);
    if mid_time <= lo.time || mid_time >= hi.time {
        collect_terminal_pair(lo, hi, f, threshold, periods)?;
        return Ok(());
    }

    let mid = threshold_sample(mid_time, f, threshold)?;
    if depth >= MAX_ADAPTIVE_SUBDIVISIONS || width_days <= 2.0 * CROSSING_TOLERANCE_DAYS {
        trace!(
            "adaptive threshold search reached refinement limit: depth={}, width_days={}",
            depth,
            width_days
        );
        collect_terminal_pair(lo, mid, f, threshold, periods)?;
        collect_terminal_pair(mid, hi, f, threshold, periods)?;
        return Ok(());
    }

    let same_side = lo.above == mid.above && mid.above == hi.above;
    if same_side
        && width_days <= MAX_ADAPTIVE_ACCEPT_SPAN_DAYS
        && samples_are_smooth_and_clear(lo, mid, hi, threshold)
    {
        trace!(
            "adaptive threshold search accepted smooth interval: depth={}, width_days={}, above={}",
            depth,
            width_days,
            lo.above
        );
        if lo.above {
            push_non_empty_period(periods, lo.time, hi.time);
        }
        return Ok(());
    }

    collect_adaptive_above(
        lo,
        mid,
        fallback_step_days,
        f,
        threshold,
        depth + 1,
        periods,
    )?;
    collect_adaptive_above(
        mid,
        hi,
        fallback_step_days,
        f,
        threshold,
        depth + 1,
        periods,
    )
}

fn collect_terminal_pair<V, F>(
    lo: ThresholdSample<V>,
    hi: ThresholdSample<V>,
    f: &F,
    threshold: Quantity<V>,
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
) -> Result<()>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if hi.time <= lo.time {
        return Ok(());
    }
    if lo.above != hi.above {
        let crossing =
            refine_threshold_crossing(lo.time, lo.value, hi.time, hi.value, f, threshold)?;
        if lo.above {
            push_non_empty_period(periods, lo.time, crossing);
        } else {
            push_non_empty_period(periods, crossing, hi.time);
        }
    } else if lo.above {
        push_non_empty_period(periods, lo.time, hi.time);
    }
    Ok(())
}

fn threshold_sample<V, F>(
    time: ModifiedJulianDate,
    f: &F,
    threshold: Quantity<V>,
) -> Result<ThresholdSample<V>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let value = f(time)?;
    Ok(ThresholdSample {
        time,
        value,
        above: value > threshold,
    })
}

fn samples_are_smooth_and_clear<V>(
    lo: ThresholdSample<V>,
    mid: ThresholdSample<V>,
    hi: ThresholdSample<V>,
    threshold: Quantity<V>,
) -> bool
where
    V: Unit,
{
    let lo_value = lo.value.value();
    let mid_value = mid.value.value();
    let hi_value = hi.value.value();
    if !lo_value.is_finite() || !mid_value.is_finite() || !hi_value.is_finite() {
        return false;
    }

    let linear_mid = 0.5 * (lo_value + hi_value);
    let curvature = (mid_value - linear_mid).abs();
    let sample_min = lo_value.min(mid_value).min(hi_value);
    let sample_max = lo_value.max(mid_value).max(hi_value);
    let threshold_value = threshold.value();
    let margin = if lo.above {
        sample_min - threshold_value
    } else {
        threshold_value - sample_max
    };
    let required_margin = SMOOTHNESS_SAFETY_FACTOR * curvature
        + SMOOTHNESS_RELATIVE_MARGIN * threshold_value.abs().max(1.0);

    margin.is_finite() && margin > required_margin
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
    let mut refinements = 0usize;
    for _ in 0..MAX_CROSSING_REFINEMENTS {
        let mid = midpoint_mjd(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = f(mid)?;
        refinements += 1;
        if y_mid == threshold {
            trace!("threshold crossing exactly sampled after {refinements} refinements");
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

    trace!("threshold crossing refined with {refinements} samples");
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

fn interval_width_days(start: ModifiedJulianDate, end: ModifiedJulianDate) -> f64 {
    end.raw().value() - start.raw().value()
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

pub(super) fn coalesce_periods(periods: &mut Vec<TimePeriod<ModifiedJulianDate>>) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
    use std::cell::Cell;

    fn test_window() -> TimePeriod<ModifiedJulianDate> {
        TimePeriod::new(
            ModifiedJulianDate::new(60_000.0),
            ModifiedJulianDate::new(60_001.0),
        )
    }

    #[test]
    fn adaptive_search_accepts_clear_smooth_intervals_with_fewer_samples() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(1.0);
        let step = Days::new(1.0 / 144.0);

        let adaptive_calls = Cell::new(0);
        let adaptive = adaptive_above_threshold_periods(
            window,
            step,
            &|time| {
                adaptive_calls.set(adaptive_calls.get() + 1);
                let dt = time.raw().value() - 60_000.5;
                Ok(BandPhotonRadiance::new(0.2 + 0.001 * dt * dt))
            },
            threshold,
        )
        .unwrap();

        let scan_calls = Cell::new(0);
        let scan = above_threshold_periods(
            window,
            step,
            &|time| {
                scan_calls.set(scan_calls.get() + 1);
                let dt = time.raw().value() - 60_000.5;
                Ok(BandPhotonRadiance::new(0.2 + 0.001 * dt * dt))
            },
            threshold,
        )
        .unwrap();

        assert!(adaptive.is_empty());
        assert!(scan.is_empty());
        assert!(
            adaptive_calls.get() * 10 < scan_calls.get(),
            "adaptive calls {}, scan calls {}",
            adaptive_calls.get(),
            scan_calls.get()
        );
    }

    #[test]
    fn adaptive_search_refines_bracketed_crossings_with_exact_evaluations() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0);

        let periods = adaptive_above_threshold_periods(
            window,
            step,
            &|time| Ok(BandPhotonRadiance::new(time.raw().value() - 60_000.0)),
            threshold,
        )
        .unwrap();

        assert_eq!(periods.len(), 1);
        assert!((periods[0].start.raw().value() - 60_000.5).abs() <= CROSSING_TOLERANCE_DAYS);
        assert_eq!(periods[0].end, window.end);
    }
}
