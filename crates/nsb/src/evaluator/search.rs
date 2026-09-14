use crate::error::Result;
use log::{debug, trace};
use qtty::{Quantity, Unit};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const MAX_CROSSING_REFINEMENTS: usize = 24;
const CROSSING_TOLERANCE: Days = Days::new(1.0e-5);
const MAX_ADAPTIVE_SUBDIVISIONS: usize = 18;
const MAX_ADAPTIVE_ACCEPT_SPAN: Days = Days::new(1.0);
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
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| diagnostics.fallback_intervals += 1);
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

/// Discover crossings with a cheap signal, then validate and refine every
/// retained crossing against the authoritative signal.
pub(crate) fn validated_above_threshold_periods<V, F, E>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    approximate: &F,
    exact: &E,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if window.start >= window.end || step <= Days::new(0.0) {
        return Ok(Vec::new());
    }

    let exact_start = exact(window.start)?;
    let mut above = exact_start > threshold;
    let mut open_start = above.then_some(window.start);
    let mut periods = Vec::new();
    let mut t0 = window.start;
    let mut approximate_above0 = approximate(t0)? > threshold;

    while t0 < window.end {
        let t1 = add_days_clamped(t0, step, window.end);
        if t1 <= t0 {
            break;
        }
        let approximate_above1 = approximate(t1)? > threshold;
        if approximate_above0 != approximate_above1 {
            let mut bracket_lo = t0;
            let mut bracket_hi = t1;
            let mut exact_lo = exact(bracket_lo)?;
            let mut exact_hi = exact(bracket_hi)?;
            if (exact_lo > threshold) == (exact_hi > threshold) {
                bracket_lo = ModifiedJulianDate::new(
                    (t0.raw().value() - step.value()).max(window.start.raw().value()),
                );
                bracket_hi = ModifiedJulianDate::new(
                    (t1.raw().value() + step.value()).min(window.end.raw().value()),
                );
                exact_lo = exact(bracket_lo)?;
                exact_hi = exact(bracket_hi)?;
            }

            let exact_lo_above = exact_lo > threshold;
            let exact_hi_above = exact_hi > threshold;
            if exact_lo_above != exact_hi_above {
                let crossing = refine_threshold_crossing(
                    bracket_lo, exact_lo, bracket_hi, exact_hi, exact, threshold,
                )?;
                if above != exact_hi_above {
                    if above {
                        if let Some(start) = open_start.take() {
                            push_non_empty_period(&mut periods, start, crossing);
                        }
                    } else {
                        open_start = Some(crossing);
                    }
                    above = exact_hi_above;
                }
            }
        }
        t0 = t1;
        approximate_above0 = approximate_above1;
    }

    if let Some(start) = open_start {
        push_non_empty_period(&mut periods, start, window.end);
    }
    coalesce_periods(&mut periods);
    Ok(periods)
}

pub(super) fn adaptive_above_threshold_periods<V, F, E>(
    window: TimePeriod<ModifiedJulianDate>,
    fallback_step: Days,
    f: &F,
    exact_f: &E,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| diagnostics.adaptive_intervals += 1);
    if window.start >= window.end || fallback_step <= Days::new(0.0) {
        debug!(
            "skipping adaptive threshold search: non-positive window or step; start_mjd={}, end_mjd={}, fallback_step_days={}",
            window.start.raw().value(),
            window.end.raw().value(),
            fallback_step.value()
        );
        return Ok(Vec::new());
    }
    if interval_width_days(window.start, window.end) <= fallback_step * 4.0 {
        debug!(
            "falling back to scan threshold search for short interval: width_days={}, fallback_step_days={}",
            interval_width_days(window.start, window.end).value(),
            fallback_step.value()
        );
        return above_threshold_periods(window, fallback_step, exact_f, threshold);
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
        AdaptiveSearchConfig {
            fallback_step,
            threshold,
        },
        f,
        exact_f,
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

#[derive(Clone, Copy)]
struct AdaptiveSearchConfig<V: Unit> {
    fallback_step: Days,
    threshold: Quantity<V>,
}

fn collect_adaptive_above<V, F, E>(
    lo: ThresholdSample<V>,
    hi: ThresholdSample<V>,
    config: AdaptiveSearchConfig<V>,
    f: &F,
    exact_f: &E,
    depth: usize,
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
) -> Result<()>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let width = interval_width_days(lo.time, hi.time);
    if width <= Days::new(0.0) {
        return Ok(());
    }
    if width <= config.fallback_step {
        collect_terminal_pair(lo, hi, exact_f, config.threshold, periods)?;
        return Ok(());
    }

    let mid_time = midpoint_mjd(lo.time, hi.time);
    if mid_time <= lo.time || mid_time >= hi.time {
        collect_terminal_pair(lo, hi, exact_f, config.threshold, periods)?;
        return Ok(());
    }

    let mid = threshold_sample(mid_time, f, config.threshold)?;
    if depth >= MAX_ADAPTIVE_SUBDIVISIONS || width <= CROSSING_TOLERANCE * 2.0 {
        trace!(
            "adaptive threshold search reached refinement limit: depth={}, width_days={}",
            depth,
            width.value()
        );
        collect_terminal_pair(lo, mid, exact_f, config.threshold, periods)?;
        collect_terminal_pair(mid, hi, exact_f, config.threshold, periods)?;
        return Ok(());
    }

    let same_side = lo.above == mid.above && mid.above == hi.above;
    if same_side
        && width <= MAX_ADAPTIVE_ACCEPT_SPAN
        && samples_are_smooth_and_clear(lo, mid, hi, config.threshold)
    {
        let exact_lo = threshold_sample(lo.time, exact_f, config.threshold)?;
        let exact_mid = threshold_sample(mid.time, exact_f, config.threshold)?;
        let exact_hi = threshold_sample(hi.time, exact_f, config.threshold)?;
        if exact_lo.above == exact_mid.above
            && exact_mid.above == exact_hi.above
            && samples_are_smooth_and_clear(exact_lo, exact_mid, exact_hi, config.threshold)
        {
            #[cfg(feature = "window-search-diagnostics")]
            super::diagnostics::update(|diagnostics| diagnostics.accepted_smooth_intervals += 1);
            trace!(
                "adaptive threshold search accepted exact-validated smooth interval: depth={}, width_days={}, above={}",
                depth,
                width.value(),
                exact_lo.above
            );
            if exact_lo.above {
                push_non_empty_period(periods, lo.time, hi.time);
            }
            return Ok(());
        }
    }

    collect_adaptive_above(lo, mid, config, f, exact_f, depth + 1, periods)?;
    collect_adaptive_above(mid, hi, config, f, exact_f, depth + 1, periods)
}

fn collect_terminal_pair<V, E>(
    lo: ThresholdSample<V>,
    hi: ThresholdSample<V>,
    exact_f: &E,
    threshold: Quantity<V>,
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
) -> Result<()>
where
    V: Unit,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    if hi.time <= lo.time {
        return Ok(());
    }
    let mid_time = midpoint_mjd(lo.time, hi.time);
    let exact_lo = exact_f(lo.time)?;
    let exact_mid = exact_f(mid_time)?;
    let exact_hi = exact_f(hi.time)?;
    collect_exact_pair(
        lo.time, exact_lo, mid_time, exact_mid, exact_f, threshold, periods,
    )?;
    collect_exact_pair(
        mid_time, exact_mid, hi.time, exact_hi, exact_f, threshold, periods,
    )?;
    Ok(())
}

fn collect_exact_pair<V, E>(
    lo: ModifiedJulianDate,
    y_lo: Quantity<V>,
    hi: ModifiedJulianDate,
    y_hi: Quantity<V>,
    exact_f: &E,
    threshold: Quantity<V>,
    periods: &mut Vec<TimePeriod<ModifiedJulianDate>>,
) -> Result<()>
where
    V: Unit,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    let lo_above = y_lo > threshold;
    let hi_above = y_hi > threshold;
    if lo_above == hi_above {
        if lo_above {
            push_non_empty_period(periods, lo, hi);
        }
        return Ok(());
    }
    let crossing = refine_threshold_crossing(lo, y_lo, hi, y_hi, exact_f, threshold)?;
    if lo_above {
        push_non_empty_period(periods, lo, crossing);
    } else {
        push_non_empty_period(periods, crossing, hi);
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
    y_lo: Quantity<V>,
    mut hi: ModifiedJulianDate,
    y_hi: Quantity<V>,
    f: &F,
    threshold: Quantity<V>,
) -> Result<ModifiedJulianDate>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    #[cfg(feature = "window-search-diagnostics")]
    super::diagnostics::update(|diagnostics| diagnostics.threshold_crossings += 1);
    let threshold_value = threshold.value();
    let mut f_lo = y_lo.value() - threshold_value;
    let mut f_hi = y_hi.value() - threshold_value;
    debug_assert!(f_lo.signum() != f_hi.signum());
    let mut refinements = 0usize;
    let mut retained_lo = 0usize;
    let mut retained_hi = 0usize;
    for _ in 0..MAX_CROSSING_REFINEMENTS {
        if (hi.raw() - lo.raw()).abs() <= CROSSING_TOLERANCE {
            break;
        }

        let lo_raw = lo.raw().value();
        let hi_raw = hi.raw().value();
        let denom = f_hi - f_lo;
        let secant_raw = hi_raw - f_hi * (hi_raw - lo_raw) / denom;
        let guard = (hi_raw - lo_raw) * 1.0e-6;
        let candidate = if denom.is_finite()
            && secant_raw.is_finite()
            && secant_raw > lo_raw + guard
            && secant_raw < hi_raw - guard
        {
            ModifiedJulianDate::new(secant_raw)
        } else {
            midpoint_mjd(lo, hi)
        };
        let y_candidate = f(candidate)?;
        #[cfg(feature = "window-search-diagnostics")]
        super::diagnostics::update(|diagnostics| {
            diagnostics.crossing_refinement_evaluations += 1;
        });
        refinements += 1;
        let f_candidate = y_candidate.value() - threshold_value;
        if f_candidate == 0.0 {
            trace!("threshold crossing exactly sampled after {refinements} refinements");
            return Ok(candidate);
        }

        if f_candidate.signum() == f_lo.signum() {
            lo = candidate;
            f_lo = f_candidate;
            retained_hi += 1;
            retained_lo = 0;
            if retained_hi > 1 {
                f_hi *= 0.5;
            }
        } else {
            hi = candidate;
            f_hi = f_candidate;
            retained_lo += 1;
            retained_hi = 0;
            if retained_lo > 1 {
                f_lo *= 0.5;
            }
        }
    }

    trace!("threshold crossing refined with {refinements} samples");
    Ok(midpoint_mjd(lo, hi))
}

fn midpoint_mjd(lo: ModifiedJulianDate, hi: ModifiedJulianDate) -> ModifiedJulianDate {
    ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()))
}

fn interval_width_days(start: ModifiedJulianDate, end: ModifiedJulianDate) -> Days {
    end.raw() - start.raw()
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
        let adaptive_f = |time: ModifiedJulianDate| {
            adaptive_calls.set(adaptive_calls.get() + 1);
            let dt = time.raw().value() - 60_000.5;
            Ok(BandPhotonRadiance::new(0.2 + 0.001 * dt * dt))
        };
        let adaptive =
            adaptive_above_threshold_periods(window, step, &adaptive_f, &adaptive_f, threshold)
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

        let f =
            |time: ModifiedJulianDate| Ok(BandPhotonRadiance::new(time.raw().value() - 60_000.0));
        let periods = adaptive_above_threshold_periods(window, step, &f, &f, threshold).unwrap();

        assert_eq!(periods.len(), 1);
        assert!((periods[0].start.raw().value() - 60_000.5).abs() <= CROSSING_TOLERANCE.value());
        assert_eq!(periods[0].end, window.end);
    }

    #[test]
    fn approximate_signal_cannot_hide_exact_extrema_or_crossings() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0 / 144.0);
        let approximate = |_time| Ok(BandPhotonRadiance::new(0.1));
        let exact = |time: ModifiedJulianDate| {
            let x = (time.raw().value() - 60_000.5) * 4.0;
            Ok(BandPhotonRadiance::new(1.0 - x * x))
        };

        let adaptive =
            adaptive_above_threshold_periods(window, step, &approximate, &exact, threshold)
                .unwrap();
        let scan = above_threshold_periods(window, step, &exact, threshold).unwrap();

        assert_eq!(adaptive.len(), 1);
        assert_eq!(scan.len(), 1);
        assert!((adaptive[0].start.raw().value() - scan[0].start.raw().value()).abs() < 2.0e-5);
        assert!((adaptive[0].end.raw().value() - scan[0].end.raw().value()).abs() < 2.0e-5);
    }

    #[test]
    fn approximate_crossings_are_rejected_when_exact_signal_stays_below() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0 / 144.0);
        let approximate = |time: ModifiedJulianDate| {
            let x = (time.raw().value() - 60_000.5) * 4.0;
            Ok(BandPhotonRadiance::new(1.0 - x * x))
        };
        let exact = |_time| Ok(BandPhotonRadiance::new(0.49));

        let periods =
            adaptive_above_threshold_periods(window, step, &approximate, &exact, threshold)
                .unwrap();
        assert!(periods.is_empty());
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
            adaptive_above_threshold_periods(window, step, &f, &f, BandPhotonRadiance::new(0.999))
                .unwrap();
        let above_max =
            adaptive_above_threshold_periods(window, step, &f, &f, BandPhotonRadiance::new(1.001))
                .unwrap();

        assert_eq!(below_max.len(), 1);
        assert!(below_max[0].start < ModifiedJulianDate::new(60_000.5));
        assert!(below_max[0].end > ModifiedJulianDate::new(60_000.5));
        assert!(above_max.is_empty());
    }
}
