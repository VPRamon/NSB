// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Frozen sampled oracle for scientific comparisons; never compiled in runtime builds.
use crate::error::Result;
use crate::units::{Quantity, Unit};
use log::{debug, trace};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};
const MAX_CROSSING_REFINEMENTS: usize = 24;
pub(crate) const CROSSING_TOLERANCE: Days = Days::new(1.0e-5);

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

pub(super) fn sampled_schedule(
    evaluator: &crate::NsbEvaluator,
    context: &super::types::SiteWindowContext,
    query: &super::types::ThresholdQuery,
) -> Result<super::types::ThresholdQueryResult> {
    use rayon::prelude::*;
    let p = super::prepare::prepare_target_threshold(evaluator, context, query)?;
    let parts = smooth_threshold_windows(&p)
        .into_par_iter()
        .map(|w| {
            let above = authoritative_above_threshold_periods(
                w,
                query.sample_step.to::<siderust::qtty::Day>(),
                &|t| super::threshold::evaluate_integrated(evaluator, &p, t),
                query.threshold,
            )?;
            Ok(complement_periods(w, &above))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut periods: Vec<_> = parts.into_iter().flatten().collect();
    super::intervals::coalesce_periods(&mut periods);
    Ok(super::types::ThresholdQueryResult {
        threshold: query.threshold,
        periods: periods
            .into_iter()
            .map(|w| super::intervals::tt_mjd_period_to_utc(w, p.tt_window, query.window))
            .collect(),
    })
}

/// Preserve the original scanner's physical partition and grid phase.
pub(super) fn smooth_threshold_windows(
    prepared: &super::types::PreparedThresholdQuery,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    super::filters::threshold_windows(prepared, true)
}
