// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Discrete references with the original grid phase. Legacy mode retains
//! 0.864 s Illinois roots for historical scan600 comparisons; precise mode
//! uses millisecond roots and one-sided physical partition endpoints.
use super::intervals::tt_mjd_to_utc_time;
use super::types::PreparedThresholdQuery;
use crate::error::Result;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use crate::units::{Quantity, Unit};
use crate::ComponentMask;
use crate::NsbEvaluator;
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
    above_threshold_periods_impl(window, step, f, threshold, CROSSING_TOLERANCE, false)
}

/// Independent fine reference: millisecond roots and one-sided values at
/// physical partition edges. A lone accepted endpoint is not a time interval.
pub(crate) fn above_threshold_periods_precise<V, F>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    f: &F,
    threshold: Quantity<V>,
) -> Result<Vec<TimePeriod<ModifiedJulianDate>>>
where
    V: Unit,
    F: Fn(ModifiedJulianDate) -> Result<Quantity<V>>,
{
    above_threshold_periods_impl(window, step, f, threshold, Days::new(0.001 / 86400.0), true)
}

fn above_threshold_periods_impl<V, F>(
    window: TimePeriod<ModifiedJulianDate>,
    step: Days,
    f: &F,
    threshold: Quantity<V>,
    tolerance: Days,
    one_sided: bool,
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
    let inset = Days::new((0.000001 / 86400.0_f64).min(window.length().value() * 0.25));
    let mut y0 = f(if one_sided { t0 + inset } else { t0 })?;
    let mut above0 = y0 > threshold;
    let mut open_start = above0.then_some(window.start);

    while t0 < window.end {
        let t1 = ModifiedJulianDate::new(t0.raw().value() + step.value()).min(window.end);
        if t1 <= t0 {
            break;
        }

        let y1 = f(if one_sided && t1 == window.end {
            t1 - inset
        } else {
            t1
        })?;
        let above1 = y1 > threshold;
        if above0 != above1 {
            let crossing = refine_threshold_crossing(t0, y0, t1, y1, f, threshold, tolerance)?;
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
    tolerance: Days,
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
    let mut retained = [0usize; 2];
    for _ in 0..if tolerance == CROSSING_TOLERANCE {
        MAX_CROSSING_REFINEMENTS
    } else {
        48
    } {
        if (hi.raw() - lo.raw()).abs() <= tolerance {
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
        let f_candidate = y_candidate.value() - threshold_value;
        if (f_candidate > 0.0) == lo_above {
            lo = candidate;
            f_lo = f_candidate;
            retained[1] += 1;
            retained[0] = 0;
            if retained[1] > 1 {
                f_hi *= 0.5;
            }
        } else {
            hi = candidate;
            f_hi = f_candidate;
            retained[0] += 1;
            retained[1] = 0;
            if retained[0] > 1 {
                f_lo *= 0.5;
            }
        }
    }

    Ok(midpoint_mjd(lo, hi))
}

fn midpoint_mjd(lo: ModifiedJulianDate, hi: ModifiedJulianDate) -> ModifiedJulianDate {
    ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()))
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

// Independent pre-optimization radiance path for scanner and cache equivalence.
pub(crate) fn evaluate_integrated(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    mjd_tt: ModifiedJulianDate,
) -> Result<BandPhotonRadiance> {
    let time = tt_mjd_to_utc_time(mjd_tt);
    let target = super::threshold::resolve_prepared_target(prepared, mjd_tt)?;
    let mut total = BandPhotonRadiance::new(0.0);

    if prepared.components.contains(ComponentMask::ZODIACAL) {
        let out =
            evaluator
                .zodiacal()
                .compute_observed(time, prepared.observer, target)?;
        total += out.integrated;
    }
    if prepared.components.contains(ComponentMask::STARLIGHT) {
        match prepared.starlight {
            super::types::PreparedStarlight::Constant(value) => total += value,
            super::types::PreparedStarlight::Dynamic => {
                total += evaluator.evaluate_starlight(target)?.integrated;
            }
            super::types::PreparedStarlight::Disabled => {}
        }
    }
    if prepared.components.contains(ComponentMask::AIRGLOW) {
        let solar = prepared
            .solar_activity_cache
            .as_ref()
            .expect("solar activity cache is prepared with airglow")
            .value_at(time)?;
        let airglow = prepared
            .airglow_model
            .as_ref()
            .expect("airglow model is prepared when airglow is selected");
        if let Some(phase) = super::filters::airglow_night_phase(prepared, mjd_tt) {
            total += airglow.compute_integrated_with_night_phase(time, target, phase, solar)?;
        } else {
            // Outside night contributes physical zero, but invalid inputs must
            // still fail rather than masquerading as inactivity (#151/#175).
            airglow.validate_inputs_for_query(time, target, solar)?;
        }
    }
    if prepared.components.contains(ComponentMask::MOON) {
        let moon_visible = prepared
            .moon_visible_periods
            .as_ref()
            .is_none_or(|periods| super::filters::contains_time(periods, mjd_tt));
        if moon_visible {
            let out = evaluator.evaluate_moonlight(prepared.observer, time, target)?;
            total += out.integrated;
        }
    }

    Ok(total)
}

#[test]
fn precise_reference_does_not_expand_an_isolated_endpoint_into_a_window() {
    let start = ModifiedJulianDate::new(60000.0);
    let window = TimePeriod::new(start, start + siderust::qtty::Seconds::new(30.0));
    let f = |time| {
        Ok(BandPhotonRadiance::new(if time == start {
            0.0
        } else {
            1.0
        }))
    };
    let above = above_threshold_periods_precise(
        window,
        Days::new(30.0 / 86400.0),
        &f,
        BandPhotonRadiance::new(0.5),
    )
    .unwrap();
    assert_eq!(above, vec![window]);
    assert!(complement_periods(window, &above).is_empty());
}

#[test]
fn precise_reference_resolves_a_known_crossing_to_one_millisecond() {
    let start = ModifiedJulianDate::new(60000.0);
    let window = TimePeriod::new(start, start + siderust::qtty::Seconds::new(60.0));
    let f =
        |time: ModifiedJulianDate| Ok(BandPhotonRadiance::new((time - start).value() * 86400.0));
    let above = above_threshold_periods_precise(
        window,
        Days::new(30.0 / 86400.0),
        &f,
        BandPhotonRadiance::new(33.123),
    )
    .unwrap();
    assert_eq!(above.len(), 1);
    assert!(((above[0].start - start).value() * 86400.0 - 33.123).abs() <= 0.001);
    assert_eq!(above[0].end, window.end);
}
