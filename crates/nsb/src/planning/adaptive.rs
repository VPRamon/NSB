// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Bounded, proof-aware discovery prototype for #234; no public API changes.
//!
//! Only an enclosure can accept/reject a closed interval. Chebyshev roots and
//! exact sign brackets guide splitting; neither can certify their complement.
//! Unsupported envelopes, contacts, jumps and budget exhaustion retain explicit
//! uncertainty. In particular a small unresolved interval is never accepted.

use super::bounds::RadianceBounds;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{NsbError, Result};
use cheby::fit_dyn_from_fn;
use siderust::qtty::Days;
use siderust::time::{Interval, ModifiedJulianDate as Mjd};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) struct Controls {
    pub time_tolerance: Days,
    pub min_width: Days,
    pub max_intervals: usize,
    pub max_evaluations: usize,
    pub max_depth: usize,
    /// Fits near the top only; deeper ambiguous regions use bisection.
    pub fit_depth: usize,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            time_tolerance: Days::new(0.1 / 86_400.0),
            min_width: Days::new(0.1 / 86_400.0),
            max_intervals: 2_048,
            max_evaluations: 1_024,
            max_depth: 40,
            fit_depth: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    CertifiedBelow,
    CertifiedAbove,
    Unresolved(StopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StopReason {
    IntervalBudget,
    Resolution,
    Depth,
    FloatingPoint,
}

#[derive(Debug)]
pub(super) struct Region {
    pub interval: Interval<Mjd>,
    pub state: State,
}

/// Exact opposite-side endpoints, with no assertion of uniqueness or
/// continuity. A zero is enclosed only if the model is continuous here;
/// across a jump this is a sign-transition bracket. Never an accepted period.
#[derive(Debug)]
pub(super) struct Bracket {
    pub interval: Interval<Mjd>,
    pub left: Radiance,
    pub right: Radiance,
    pub tolerance_met: bool,
}

#[derive(Debug, Default, Clone)]
pub(super) struct Diagnostics {
    pub exact_evaluations: usize,
    pub bound_computations: usize,
    pub fits: usize,
    pub failed_fits: usize,
    pub polynomial_candidates: usize,
    pub subdivisions: usize,
    pub root_refinements: usize,
    pub fallback_intervals: usize,
    pub unresolved_intervals: usize,
    pub bounds_time: Duration,
    /// Includes exact fitting-node evaluations and polynomial root solving.
    pub fits_time: Duration,
    pub refinement_time: Duration,
    pub peak_pending_intervals: usize,
}

#[derive(Debug, Default)]
pub(super) struct Discovery {
    /// Ordered closed certificates/unresolved regions. Neighbours share endpoints.
    /// Do not coalesce across any unresolved gap, however small.
    pub regions: Vec<Region>,
    /// Candidate diagnostics, possibly repeated in nested intervals.
    pub brackets: Vec<Bracket>,
    pub diagnostics: Diagnostics,
}

impl Discovery {
    pub fn complete(&self) -> bool {
        self.diagnostics.unresolved_intervals == 0
    }
}

pub(super) fn discover<E, B>(
    windows: &[Interval<Mjd>],
    threshold: Radiance,
    exact: E,
    bounds: B,
    controls: Controls,
) -> Result<Discovery>
where
    E: Fn(Mjd) -> Result<Radiance>,
    B: Fn(Interval<Mjd>) -> Result<Option<RadianceBounds>>,
{
    if !threshold.is_finite()
        || !controls.time_tolerance.is_finite()
        || controls.time_tolerance.value() <= 0.0
        || !controls.min_width.is_finite()
        || controls.min_width.value() <= 0.0
        || windows.iter().any(|w| {
            !w.start.raw().value().is_finite()
                || !w.end.raw().value().is_finite()
                || w.end < w.start
                || !(w.end.raw().value() - w.start.raw().value()).is_finite()
        })
        || windows.windows(2).any(|w| w[0].end > w[1].start)
    {
        return Err(NsbError::OutOfRange(
            "invalid adaptive search inputs".into(),
        ));
    }
    let mut out = Discovery::default();
    let mut pending: Vec<_> = windows.iter().rev().map(|&w| (w, 0)).collect();
    let mut sampler = Sampler {
        exact,
        limit: controls.max_evaluations,
        count: 0,
    };
    out.diagnostics.peak_pending_intervals = pending.len();

    while let Some((window, depth)) = pending.pop() {
        let state = if out.diagnostics.bound_computations >= controls.max_intervals {
            Some(State::Unresolved(StopReason::IntervalBudget))
        } else {
            let start = Instant::now();
            let enclosure = bounds(window)?;
            out.diagnostics.bounds_time += start.elapsed();
            out.diagnostics.bound_computations += 1;
            match enclosure {
                Some(b) => {
                    // Also validate providers constructed via field literals.
                    RadianceBounds::new(b.lower, b.upper)?;
                    if b.upper <= threshold {
                        Some(State::CertifiedBelow)
                    } else if b.lower > threshold {
                        Some(State::CertifiedAbove)
                    } else {
                        None
                    }
                }
                None => None,
            }
        };
        let width = window.end.raw().value() - window.start.raw().value();
        let state = state.or_else(|| {
            if width <= controls.min_width.value() {
                Some(State::Unresolved(StopReason::Resolution))
            } else if depth >= controls.max_depth {
                Some(State::Unresolved(StopReason::Depth))
            } else {
                None
            }
        });
        if let Some(state) = state {
            if matches!(state, State::Unresolved(_)) {
                out.diagnostics.unresolved_intervals += 1;
            }
            out.regions.push(Region {
                interval: window,
                state,
            });
            continue;
        }

        let mut split = None;
        if depth < controls.fit_depth {
            split = candidate_split(window, threshold, &mut sampler, controls, &mut out)?;
        }
        if split.is_none() {
            out.diagnostics.fallback_intervals += 1;
        }
        let mid = split.unwrap_or_else(|| Mjd::new(window.start.raw().value() + width * 0.5));
        if mid <= window.start || mid >= window.end {
            out.diagnostics.unresolved_intervals += 1;
            out.regions.push(Region {
                interval: window,
                state: State::Unresolved(StopReason::FloatingPoint),
            });
            continue;
        }
        out.diagnostics.subdivisions += 1;
        pending.push((Interval::new(mid, window.end), depth + 1));
        pending.push((Interval::new(window.start, mid), depth + 1));
        out.diagnostics.peak_pending_intervals =
            out.diagnostics.peak_pending_intervals.max(pending.len());
    }
    out.diagnostics.exact_evaluations = sampler.count;
    Ok(out)
}

struct Sampler<E> {
    exact: E,
    limit: usize,
    count: usize,
}

impl<E: Fn(Mjd) -> Result<Radiance>> Sampler<E> {
    fn evaluate(&mut self, t: Mjd) -> Result<Option<Radiance>> {
        if self.count >= self.limit {
            return Ok(None);
        }
        self.count += 1;
        let value = (self.exact)(t)?;
        if !value.is_finite() {
            return Err(NsbError::OutOfRange("non-finite exact radiance".into()));
        }
        Ok(Some(value))
    }
}

fn candidate_split<E: Fn(Mjd) -> Result<Radiance>>(
    window: Interval<Mjd>,
    threshold: Radiance,
    sampler: &mut Sampler<E>,
    controls: Controls,
    out: &mut Discovery,
) -> Result<Option<Mjd>> {
    const DEGREE: usize = 8;
    // Reserve a whole fit and its two endpoints; budget exhaustion cannot
    // substitute fabricated values for unvisited nodes.
    if sampler.limit.saturating_sub(sampler.count) < DEGREE + 3 {
        return Ok(None);
    }
    let start = Instant::now();
    out.diagnostics.fits += 1;
    let a = window.start.raw().value();
    let width = window.end.raw().value() - a;
    let time_at =
        |x: f64| Mjd::new((a + width * ((x + 1.0) * 0.5)).clamp(a, window.end.raw().value()));
    let mut samples = Vec::with_capacity(DEGREE + 3);
    let mut error = None;
    let fit = fit_dyn_from_fn(DEGREE, |x| {
        if error.is_some() {
            return f64::NAN;
        }
        let t = time_at(x);
        match sampler.evaluate(t) {
            Ok(Some(y)) => {
                samples.push((t, y));
                y.value() - threshold.value()
            }
            Err(e) => {
                error = Some(e);
                f64::NAN
            }
            Ok(None) => unreachable!("fit budget reserved"),
        }
    });
    if let Some(error) = error {
        return Err(error);
    }
    let mut predicted = Vec::new();
    match fit {
        Ok(poly) if poly.coeffs().iter().all(|c| c.is_finite()) => {
            // No tail-norm/residual test is used as a coverage certificate.
            predicted = poly
                .roots()
                .into_iter()
                .filter(|x| x.is_finite() && *x > -1.0 && *x < 1.0)
                .map(time_at)
                .collect();
            out.diagnostics.polynomial_candidates += predicted.len();
        }
        Ok(_) | Err(_) => out.diagnostics.failed_fits += 1,
    }
    for t in [window.start, window.end] {
        if let Some(y) = sampler.evaluate(t)? {
            samples.push((t, y));
        }
    }
    // Validate predictions with the full signal. A failed prediction cannot
    // suppress the enclosure-driven subdivision.
    for &t in &predicted {
        if let Some(y) = sampler.evaluate(t)? {
            samples.push((t, y));
        }
    }
    samples.sort_by(|a, b| a.0.raw().value().total_cmp(&b.0.raw().value()));
    samples.dedup_by_key(|p| p.0.raw().value().to_bits());
    out.diagnostics.fits_time += start.elapsed();

    let start = Instant::now();
    for pair in samples.windows(2) {
        let [(mut lo, mut y_lo), (mut hi, mut y_hi)] = [pair[0], pair[1]];
        if (y_lo > threshold) == (y_hi > threshold) {
            continue;
        }
        out.diagnostics.root_refinements += 1;
        while hi.raw().value() - lo.raw().value() > controls.time_tolerance.value() {
            let mid = Mjd::new(lo.raw().value() + (hi.raw().value() - lo.raw().value()) * 0.5);
            if mid <= lo || mid >= hi {
                break;
            }
            let Some(y) = sampler.evaluate(mid)? else {
                break;
            };
            if (y > threshold) == (y_lo > threshold) {
                lo = mid;
                y_lo = y;
            } else {
                hi = mid;
                y_hi = y;
            }
        }
        out.brackets.push(Bracket {
            interval: Interval::new(lo, hi),
            left: y_lo,
            right: y_hi,
            tolerance_met: hi.raw().value() - lo.raw().value() <= controls.time_tolerance.value(),
        });
    }
    out.diagnostics.refinement_time += start.elapsed();
    // Guard progress: a prediction may change efficiency, never coverage.
    Ok(predicted
        .into_iter()
        .filter(|t| {
            let u = (t.raw().value() - a) / width;
            (0.125..=0.875).contains(&u)
        })
        .min_by(|a, b| {
            let center = window.start.raw().value() + width * 0.5;
            (a.raw().value() - center)
                .abs()
                .total_cmp(&(b.raw().value() - center).abs())
        }))
}
