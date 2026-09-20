use crate::error::Result;
use log::{debug, trace};
use qtty::{Quantity, Unit};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Period, Time, MJD, UTC};

const MAX_CROSSING_REFINEMENTS: usize = 24;
const CROSSING_TOLERANCE: Days = Days::new(1.0e-5);

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

pub(crate) fn above_threshold_periods<V, F>(
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
    super::diagnostics::update(|diagnostics| diagnostics.authoritative_scan_windows += 1);
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

/// Scan the authoritative signal at the caller-selected resolution.
///
/// No discovery signal may classify or prune an interval. Consequently every
/// interval is visited by the exact model at `step`, and every bracketed
/// crossing is refined against that same model. Features narrower than the
/// explicitly selected resolution remain outside this discrete search
/// contract; callers can reduce `step` when they need finer completeness.
pub(crate) fn authoritative_above_threshold_periods<V, E>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    exact_f: &E,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    E: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    above_threshold_periods(window, step, exact_f, threshold)
}

pub(crate) fn complement_periods(
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
    let lo_above = f_lo > 0.0;
    debug_assert_ne!(lo_above, f_hi > 0.0);
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
        if (f_candidate > 0.0) == lo_above {
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

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;

    fn test_window() -> TimePeriod<ModifiedJulianDate> {
        TimePeriod::new(
            ModifiedJulianDate::new(60_000.0),
            ModifiedJulianDate::new(60_001.0),
        )
    }

    #[test]
    fn authoritative_search_refines_bracketed_crossings() {
        let window = test_window();
        let threshold = BandPhotonRadiance::new(0.5);
        let step = Days::new(1.0);

        let f =
            |time: ModifiedJulianDate| Ok(BandPhotonRadiance::new(time.raw().value() - 60_000.0));
        let periods = authoritative_above_threshold_periods(window, step, &f, threshold).unwrap();

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

        let authoritative =
            authoritative_above_threshold_periods(window, step, &exact, threshold).unwrap();
        let exact_scan = above_threshold_periods(window, step, &exact, threshold).unwrap();

        assert_eq!(
            exact_scan.len(),
            1,
            "the exact oracle must resolve the bump"
        );
        assert_eq!(authoritative, exact_scan);
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
            authoritative_above_threshold_periods(window, step, &f, BandPhotonRadiance::new(0.999))
                .unwrap();
        let above_max =
            authoritative_above_threshold_periods(window, step, &f, BandPhotonRadiance::new(1.001))
                .unwrap();

        assert_eq!(below_max.len(), 1);
        assert!(below_max[0].start < ModifiedJulianDate::new(60_000.5));
        assert!(below_max[0].end > ModifiedJulianDate::new(60_000.5));
        assert!(above_max.is_empty());
    }
}
