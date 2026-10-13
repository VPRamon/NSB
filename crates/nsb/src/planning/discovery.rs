// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Practical adaptive discovery, deliberately separate from interval certificates.
//!
//! On each physical regime we test a degree-four Chebyshev interpolant at
//! interlaced nodes, upgrading to degree eight before subdivision. Roots AND
//! extrema generate authoritative probes. This is a resolved-model assumption,
//! not a bound on unvisited values. See adaptive-threshold-production-status.md.

use super::bounds::{BoundOutcome, RadianceBounds};
use super::scan::coalesce_periods;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{NsbError, Result};
use cheby::ChebySeriesDyn;
use siderust::time::{Interval, ModifiedJulianDate as Mjd};

pub(super) const TOLERANCE: f64 = 0.1 / 86_400.0;
const INSET: f64 = 0.001 / 86_400.0;
const MAX_SPAN: f64 = 0.25; // six hours: resolve diurnal geometry before fitting

#[derive(Default, Debug, Clone)]
pub(super) struct Costs {
    pub exact: usize,
    pub bounds: usize,
    pub fits: usize,
    pub subdivisions: usize,
    pub refinements: usize,
    pub extrema: usize,
    pub certified: usize,
    pub peak_samples: usize,
    pub peak_pending: usize,
    pub peak_depth: usize,
    pub bracket_seconds: f64,
}

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub exact: usize,
    pub fits: usize,
    pub depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            exact: 4096,
            fits: 256,
            depth: 32,
        }
    }
}

struct Search<E> {
    exact: E,
    window: Interval<Mjd>,
    threshold: f64,
    minimum: Option<f64>,
    limits: Limits,
    // One bounded cache per initial physical interval, also reused by roots.
    samples: Vec<(f64, f64)>,
    costs: Costs,
    periods: Vec<Interval<Mjd>>,
}

fn incomplete(reason: &str) -> NsbError {
    NsbError::Unsupported(format!(
        "adaptive threshold discovery unresolved: {reason}; no partial schedule returned"
    ))
}

impl<E: Fn(Mjd) -> Result<Radiance>> Search<E> {
    fn time(&self, offset_days: f64) -> Mjd {
        if offset_days == 0.0 {
            self.window.start
        } else if offset_days == self.window.length().value() {
            self.window.end
        } else {
            self.window.start + siderust::qtty::Days::new(offset_days)
        }
    }

    fn probe(&mut self, t: f64) -> Result<f64> {
        match self.samples.binary_search_by(|p| p.0.total_cmp(&t)) {
            Ok(i) => Ok(self.samples[i].1),
            Err(i) => {
                if self.costs.exact >= self.limits.exact {
                    return Err(incomplete("authoritative evaluation budget"));
                }
                let value = (self.exact)(self.time(t))?.value();
                if !value.is_finite() {
                    return Err(NsbError::OutOfRange(
                        "non-finite authoritative radiance".into(),
                    ));
                }
                self.costs.exact += 1;
                self.samples.insert(i, (t, value));
                self.costs.peak_samples = self.costs.peak_samples.max(self.samples.len());
                Ok(value)
            }
        }
    }

    fn split(&mut self, a: f64, b: f64, depth: usize) -> Result<()> {
        let mid = a + (b - a) * 0.5;
        if depth >= self.limits.depth || mid <= a || mid >= b || b - a <= TOLERANCE {
            return Err(incomplete(
                "unresolved fit at time/depth/floating-point limit",
            ));
        }
        self.costs.subdivisions += 1;
        self.interval(a, mid, depth + 1)?;
        self.interval(mid, b, depth + 1)
    }

    fn interval(&mut self, a: f64, b: f64, depth: usize) -> Result<()> {
        self.costs.peak_depth = self.costs.peak_depth.max(depth);
        if b - a > MAX_SPAN {
            return self.split(a, b, depth);
        }
        if self.costs.fits >= self.limits.fits {
            return Err(incomplete("polynomial fit budget"));
        }
        // Fit open interiors; exact endpoints are included separately below.
        // This avoids fitting a smooth polynomial through a known regime jump.
        let inset = INSET.min((b - a) * 0.01);
        let left = a + inset;
        let right = b - inset;
        if left >= right {
            return Err(incomplete("unrepresentable physical interval"));
        }
        let time = |x: f64| left + (right - left) * (x + 1.0) * 0.5;
        let times: [f64; 9] =
            std::array::from_fn(|i| time((std::f64::consts::PI * i as f64 / 8.0).cos()));
        if times.windows(2).any(|p| p[0] <= p[1]) {
            return Err(incomplete("fitting nodes collapse at time precision"));
        }
        // Never accept a low fit after checking only its endpoint-adjacent
        // interlaced nodes: a compact excursion can alias those seven probes.
        let mut ys = [0.0; 9];
        for (i, y) in ys.iter_mut().enumerate() {
            *y = self.probe(times[i])?;
        }
        let scale = ys
            .iter()
            .fold(self.threshold.abs().max(1e-12), |s, y| s.max(y.abs()))
            .max(self.minimum.unwrap_or(0.0).abs());
        let normalized: Vec<_> = ys
            .iter()
            .map(|y| y / scale - self.threshold / scale)
            .collect();
        let low = lobatto(&normalized.iter().step_by(2).copied().collect::<Vec<_>>())?;
        self.costs.fits += 1;
        // Two non-Lobatto interior checks complement ALL four interlaced
        // checks of the low fit. Expensive high-fit validation is lazy.
        let mut validation = Vec::with_capacity(6);
        for x in [-1.0 / 3.0, 1.0 / 3.0] {
            validation.push((x, self.probe(time(x))? / scale - self.threshold / scale));
        }
        let residual_at = |poly: &ChebySeriesDyn<f64>, validation: &[(f64, f64)]| {
            validation
                .iter()
                .map(|&(x, y)| (poly.evaluate(x) - y).abs())
                .chain((1..8).step_by(2).map(|i| {
                    (poly.evaluate((std::f64::consts::PI * i as f64 / 8.0).cos()) - normalized[i])
                        .abs()
                }))
                .fold(0.0, f64::max)
        };
        let mut residual = residual_at(&low, &validation);
        let poly = if residual <= 1e-5 {
            low
        } else {
            if self.costs.fits >= self.limits.fits {
                return Err(incomplete("polynomial fit budget"));
            }
            self.costs.fits += 1;
            let high = lobatto(&normalized)?;
            // Independent validation of the high fit cannot reuse its own
            // interpolation nodes as evidence. Retain every probe for roots.
            for x in [-0.83, -0.31, 0.27, 0.79] {
                validation.push((x, self.probe(time(x))? / scale - self.threshold / scale));
            }
            residual = residual_at(&high, &validation);
            if residual > 1e-5 {
                return self.split(a, b, depth);
            }
            high
        };
        let mut probes: Vec<f64> = ys
            .iter()
            .enumerate()
            .filter(|(_, y)| y.is_finite())
            .map(|(i, _)| time((std::f64::consts::PI * i as f64 / 8.0).cos()))
            .collect();
        probes.extend([a, b]);
        let mut candidates = poly.roots();
        if let Some(minimum) = self.minimum {
            // Same physical fit and cache, roots for both levels. Subtracting
            // contracted below-minimum schedules would widen the band unsafely.
            let mut coefficients = poly.coeffs().to_vec();
            coefficients[0] += self.threshold / scale - minimum / scale;
            let lower = ChebySeriesDyn::new(coefficients)
                .map_err(|_| incomplete("lower threshold polynomial"))?;
            candidates.extend(lower.roots());
        }
        let mut extrema: Vec<_> = poly
            .derivative()
            .roots()
            .into_iter()
            .filter(|x| x.is_finite() && *x > -1.0 && *x < 1.0)
            .collect();
        extrema.sort_by(f64::total_cmp);
        for (i, &x) in extrema.iter().enumerate() {
            self.costs.extrema += 1;
            let t = time(x);
            let y = self.probe(t)?;
            candidates.push(x);
            // Resolve grazing extrema with the exact model when their height
            // is close to threshold relative to the observed approximation error.
            let distance = (y / scale - self.threshold / scale).abs().min(
                self.minimum
                    .map_or(f64::INFINITY, |v| (y / scale - v / scale).abs()),
            );
            if distance <= (16.0 * residual).max(1e-8) {
                let lo = time(if i == 0 {
                    -1.0
                } else {
                    (extrema[i - 1] + x) * 0.5
                });
                let hi = time(if i + 1 == extrema.len() {
                    1.0
                } else {
                    (x + extrema[i + 1]) * 0.5
                });
                let maximum = poly.evaluate_derivative((x - 1e-5).max(-1.0)) > 0.0;
                let extremum = self.extremum(lo, hi, maximum)?;
                let height = self.probe(extremum)? / scale - self.threshold / scale;
                let lower_height = self.minimum.map_or(f64::INFINITY, |v| {
                    height + self.threshold / scale - v / scale
                });
                if height.abs().min(lower_height.abs()) <= 128.0 * f64::EPSILON {
                    return Err(incomplete(
                        "extremum indistinguishable from threshold at radiance precision",
                    ));
                }
                probes.push(extremum);
            }
        }
        for x in candidates {
            if x.is_finite() && x > -1.0 && x < 1.0 {
                let t = time(x);
                let y = self.probe(t)?;
                if (y / scale - self.threshold / scale - poly.evaluate(x)).abs() > 1e-5 {
                    return self.split(a, b, depth);
                }
                probes.push(t);
            }
        }
        // Retain every validation and extremum probe, including upgraded fits.
        probes.extend(
            self.samples
                .iter()
                .filter(|p| p.0 >= a && p.0 <= b)
                .map(|p| p.0),
        );
        probes.sort_by(f64::total_cmp);
        probes.dedup();
        for pair in probes.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            let (yl, yr) = (self.probe(lo)?, self.probe(hi)?);
            let mut accepted_lo = lo;
            let mut accepted_hi = hi;
            for (level, lower) in [(Some(self.threshold), false), (self.minimum, true)] {
                let Some(level) = level else { continue };
                let rejected = |y: f64| if lower { y < level } else { y > level };
                let (left, right) = (rejected(yl), rejected(yr));
                if left && right {
                    accepted_hi = accepted_lo;
                    break;
                }
                if left != right {
                    let (l, r) = self.crossing(lo, hi, yl, level, lower)?;
                    if left {
                        accepted_lo = accepted_lo.max(r);
                    } else {
                        accepted_hi = accepted_hi.min(l);
                    }
                }
            }
            if accepted_lo < accepted_hi {
                // Both threshold brackets contract toward their accepted side.
                // Check the final endpoints before exposing a positive duration.
                let left_value = self.probe(accepted_lo)?;
                let right_value = self.probe(accepted_hi)?;
                if !self.accepted(left_value) || !self.accepted(right_value) {
                    return Err(incomplete("band endpoints outside accepted radiance"));
                }
                self.push(accepted_lo, accepted_hi);
            }
        }
        Ok(())
    }

    fn extremum(&mut self, mut lo: f64, mut hi: f64, maximum: bool) -> Result<f64> {
        // Golden section on the candidate's local basin. Unimodality is an
        // explicit resolved-model assumption, not inferred as a theorem.
        let ratio = (5.0_f64.sqrt() - 1.0) * 0.5;
        let mut l = hi - ratio * (hi - lo);
        let mut r = lo + ratio * (hi - lo);
        let mut yl = self.probe(l)?;
        let mut yr = self.probe(r)?;
        while hi - lo > 0.0001 / 86_400.0 {
            let previous = (lo, hi);
            if (yl < yr) == maximum {
                lo = l;
                l = r;
                yl = yr;
                r = lo + ratio * (hi - lo);
                yr = self.probe(r)?;
            } else {
                hi = r;
                r = l;
                yr = yl;
                l = hi - ratio * (hi - lo);
                yl = self.probe(l)?;
            }
            if previous == (lo, hi) {
                return Err(incomplete("extremum floating-point limit"));
            }
        }
        Ok(if (yl > yr) == maximum { l } else { r })
    }

    fn accepted(&self, value: f64) -> bool {
        value <= self.threshold && self.minimum.is_none_or(|min| value >= min)
    }

    fn crossing(
        &mut self,
        mut lo: f64,
        mut hi: f64,
        yl: f64,
        level: f64,
        lower: bool,
    ) -> Result<(f64, f64)> {
        self.costs.refinements += 1;
        let rejected = |y: f64| if lower { y < level } else { y > level };
        let above = rejected(yl);
        let yr = self.probe(hi)?;
        let scale = yl.abs().max(yr.abs()).max(level.abs()).max(1e-300);
        let mut fl = yl / scale - level / scale;
        let mut fr = yr / scale - level / scale;
        let mut retained_left = 0;
        let mut retained_right = 0;
        let mut iteration = 0;
        while hi - lo > TOLERANCE {
            let secant = lo + (hi - lo) * (fl / (fl - fr));
            let guard = (hi - lo) * 0.001;
            // A forced bisection every fourth iteration bounds progress even
            // across jumps or poor secant predictions. Illinois weights only
            // accelerate proposals; authoritative signs own bracket updates.
            let mid = if iteration % 4 != 3
                && secant.is_finite()
                && secant > lo + guard
                && secant < hi - guard
            {
                secant
            } else {
                lo + (hi - lo) * 0.5
            };
            if mid <= lo || mid >= hi {
                return Err(incomplete("crossing floating-point limit"));
            }
            let y = self.probe(mid)?;
            if rejected(y) == above {
                lo = mid;
                fl = y / scale - level / scale;
                retained_right += 1;
                retained_left = 0;
                if retained_right > 1 {
                    fr *= 0.5;
                }
            } else {
                hi = mid;
                fr = y / scale - level / scale;
                retained_left += 1;
                retained_right = 0;
                if retained_left > 1 {
                    fl *= 0.5;
                }
            }
            iteration += 1;
        }
        self.costs.bracket_seconds += (hi - lo) * 86400.0;
        // Export the known accepted side, never the middle of an uncertain bracket.
        Ok((lo, hi))
    }

    fn push(&mut self, a: f64, b: f64) {
        if a < b {
            self.periods.push(Interval::new(self.time(a), self.time(b)));
        }
    }
}

/// DCT-I on Lobatto values (ordered cos(k*pi/n)). cheby's existing root and
/// derivative implementation owns polynomial event solving.
fn lobatto(values: &[f64]) -> Result<ChebySeriesDyn<f64>> {
    let n = values.len() - 1;
    let coefficients = (0..=n)
        .map(|j| {
            let sum = values
                .iter()
                .enumerate()
                .map(|(k, y)| {
                    let weight = if k == 0 || k == n { 0.5 } else { 1.0 };
                    weight * y * (std::f64::consts::PI * (j * k) as f64 / n as f64).cos()
                })
                .sum::<f64>();
            sum * (if j == 0 || j == n { 1.0 } else { 2.0 }) / n as f64
        })
        .collect();
    ChebySeriesDyn::new(coefficients).map_err(|_| incomplete("polynomial construction"))
}

pub(super) fn search<E: Fn(Mjd) -> Result<Radiance>>(
    window: Interval<Mjd>,
    threshold: Radiance,
    exact: E,
    bounds: BoundOutcome,
    limits: Limits,
) -> Result<(Vec<Interval<Mjd>>, Costs)> {
    search_band(window, threshold, None, exact, bounds, limits)
}

pub(super) fn search_band<E: Fn(Mjd) -> Result<Radiance>>(
    window: Interval<Mjd>,
    threshold: Radiance,
    minimum: Option<Radiance>,
    exact: E,
    bounds: BoundOutcome,
    limits: Limits,
) -> Result<(Vec<Interval<Mjd>>, Costs)> {
    if minimum.is_some_and(|v| !v.is_finite() || v > threshold)
        || !threshold.is_finite()
        || !window.start.raw().value().is_finite()
        || !window.end.raw().value().is_finite()
        || window.start >= window.end
    {
        return Err(incomplete("invalid search inputs"));
    }
    let mut search = Search {
        exact,
        window,
        threshold: threshold.value(),
        minimum: minimum.map(|v| v.value()),
        limits,
        samples: Vec::new(),
        costs: Costs {
            bounds: 1,
            ..Costs::default()
        },
        periods: Vec::new(),
    };
    if let BoundOutcome::Enclosure { range: b, .. } = bounds {
        RadianceBounds::new(b.lower, b.upper)?;
        if b.upper <= threshold && minimum.is_none_or(|min| b.lower >= min) {
            search.costs.certified = 1;
            return Ok((vec![window], search.costs));
        }
        if b.lower > threshold || minimum.is_some_and(|min| b.upper < min) {
            search.costs.certified = 1;
            return Ok((Vec::new(), search.costs));
        }
    }
    // A scalar absolute MJD loses the split coordinate's low part. In a
    // mixture of certified and fitted pieces that created sub-microsecond
    // artificial gaps at UTC midnight. Local offsets retain the original
    // endpoints and the upstream time representation throughout the search.
    search.interval(0.0, window.length().value(), 0)?;
    coalesce_periods(&mut search.periods);
    Ok((search.periods, search.costs))
}

/// Deterministic batches reclaim unused local work without racing a global
/// atomic counter. Limits cover the whole query; no successful partial result.
/// A batch has at most 64 pending physical intervals, plus bounded worker stacks.
pub(super) fn search_windows<E, B>(
    windows: &[Interval<Mjd>],
    maximum: Radiance,
    minimum: Option<Radiance>,
    exact: E,
    bounds: B,
    certified: bool,
) -> Result<(Vec<Interval<Mjd>>, Costs)>
where
    E: Fn(Mjd) -> Result<Radiance> + Sync,
    B: Fn(Interval<Mjd>) -> Result<BoundOutcome> + Sync,
{
    use rayon::prelude::*;
    if windows.len() > 25_000 {
        return Err(incomplete("initial physical interval budget (25000)"));
    }
    let complexity = windows
        .iter()
        .map(|w| (w.length().value() / MAX_SPAN).ceil().max(1.0) as usize)
        .fold(0usize, usize::saturating_add);
    // Initial classification is reserved separately from local refinements.
    // Annual hourly partitions have ample initial work without allowing every
    // interval to independently spend 4096 exact calls.
    let mut remaining_exact = (complexity.saturating_mul(64).saturating_add(65_536)).min(2_000_000);
    let mut remaining_fits = (complexity.saturating_mul(8).saturating_add(4_096)).min(250_000);
    let mut total = Costs::default();
    let mut periods = Vec::new();
    for batch in windows.chunks(64) {
        total.peak_pending = total.peak_pending.max(batch.len());
        let limits = Limits {
            exact: Limits::default().exact.min(remaining_exact / batch.len()),
            fits: Limits::default().fits.min(remaining_fits / batch.len()),
            ..Limits::default()
        };
        let parts = batch
            .par_iter()
            .map(|&window| {
                let bound = bounds(window)?;
                if certified {
                    if let BoundOutcome::Enclosure { range, .. } = bound {
                        RadianceBounds::new(range.lower, range.upper)?;
                        let inside =
                            range.upper <= maximum && minimum.is_none_or(|min| range.lower >= min);
                        let outside =
                            range.lower > maximum || minimum.is_some_and(|min| range.upper < min);
                        if inside || outside {
                            return Ok((
                                if inside { vec![window] } else { Vec::new() },
                                Costs {
                                    bounds: 1,
                                    certified: 1,
                                    ..Costs::default()
                                },
                            ));
                        }
                    }
                    return Err(incomplete(
                        "no interval certificate for the complete radiance band",
                    ));
                }
                if minimum.is_none() {
                    search(window, maximum, &exact, bound, limits)
                } else {
                    search_band(window, maximum, minimum, &exact, bound, limits)
                }
            })
            // Result's Rayon collector returns whichever error wins a mutex.
            // Preserve indexed physical order, then select errors sequentially.
            // All work in this batch remains bounded by its assigned limits.
            .collect::<Vec<_>>();
        for part in parts {
            let (part, c) = part?;
            remaining_exact -= c.exact;
            remaining_fits -= c.fits;
            total.exact += c.exact;
            total.bounds += c.bounds;
            total.fits += c.fits;
            total.subdivisions += c.subdivisions;
            total.refinements += c.refinements;
            total.extrema += c.extrema;
            total.certified += c.certified;
            total.peak_samples = total.peak_samples.max(c.peak_samples);
            total.peak_depth = total.peak_depth.max(c.peak_depth);
            total.bracket_seconds += c.bracket_seconds;
            periods.extend(part);
        }
    }
    coalesce_periods(&mut periods);
    Ok((periods, total))
}
