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
    threshold: f64,
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
    fn probe(&mut self, t: f64) -> Result<f64> {
        match self.samples.binary_search_by(|p| p.0.total_cmp(&t)) {
            Ok(i) => Ok(self.samples[i].1),
            Err(i) => {
                if self.costs.exact >= self.limits.exact {
                    return Err(incomplete("authoritative evaluation budget"));
                }
                let value = (self.exact)(Mjd::new(t))?.value();
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
        let mut ys = [f64::NAN; 9];
        for (i, y) in ys.iter_mut().enumerate() {
            if i % 2 == 0 || i == 1 || i == 7 {
                *y = self.probe(times[i])?;
            }
        }
        let mut scale = ys
            .iter()
            .fold(self.threshold.abs().max(1e-12), |s, y| s.max(y.abs()));
        // Normalize before DCT/root solving, preventing overflow from finite
        // radiances. This normalization never changes authoritative comparison.
        let mut normalized: Vec<_> = ys
            .iter()
            .map(|y| y / scale - self.threshold / scale)
            .collect();
        let low = lobatto(&normalized.iter().step_by(2).copied().collect::<Vec<_>>())?;
        self.costs.fits += 1;
        let mut residual: f64 = [1, 7]
            .into_iter()
            .map(|i| {
                (low.evaluate((std::f64::consts::PI * i as f64 / 8.0).cos()) - normalized[i]).abs()
            })
            .fold(0.0, f64::max);
        let poly = if residual <= 1e-5 {
            low
        } else {
            if self.costs.fits >= self.limits.fits {
                return Err(incomplete("polynomial fit budget"));
            }
            self.costs.fits += 1;
            for i in [3, 5] {
                ys[i] = self.probe(times[i])?;
            }
            scale = ys.iter().fold(scale, |s, y| s.max(y.abs()));
            normalized = ys
                .iter()
                .map(|y| y / scale - self.threshold / scale)
                .collect();
            let high = lobatto(&normalized)?;
            residual = 0.0;
            // Independent, non-Lobatto checks; empirical agreement only.
            for x in [-0.83, -0.31, 0.27, 0.79] {
                let y = self.probe(time(x))? / scale - self.threshold / scale;
                residual = residual.max((high.evaluate(x) - y).abs());
            }
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
            if (y / scale - self.threshold / scale).abs() <= (16.0 * residual).max(1e-8) {
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
                if height.abs() <= 128.0 * f64::EPSILON {
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
            if (yl > self.threshold) != (yr > self.threshold) {
                let accepted = self.crossing(lo, hi, yl)?;
                if yl <= self.threshold {
                    self.push(lo, accepted);
                } else {
                    self.push(accepted, hi);
                }
            } else if yl <= self.threshold {
                self.push(lo, hi);
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

    fn crossing(&mut self, mut lo: f64, mut hi: f64, yl: f64) -> Result<f64> {
        self.costs.refinements += 1;
        let above = yl > self.threshold;
        let yr = self.probe(hi)?;
        let scale = yl.abs().max(yr.abs()).max(self.threshold.abs()).max(1e-300);
        let mut fl = yl / scale - self.threshold / scale;
        let mut fr = yr / scale - self.threshold / scale;
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
            if (y > self.threshold) == above {
                lo = mid;
                fl = y / scale - self.threshold / scale;
                retained_right += 1;
                retained_left = 0;
                if retained_right > 1 {
                    fr *= 0.5;
                }
            } else {
                hi = mid;
                fr = y / scale - self.threshold / scale;
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
        Ok(if above { hi } else { lo })
    }

    fn push(&mut self, a: f64, b: f64) {
        if a < b {
            self.periods.push(Interval::new(Mjd::new(a), Mjd::new(b)));
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
    if !threshold.is_finite()
        || !window.start.raw().value().is_finite()
        || !window.end.raw().value().is_finite()
        || window.start >= window.end
    {
        return Err(incomplete("invalid search inputs"));
    }
    let mut search = Search {
        exact,
        threshold: threshold.value(),
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
        if b.upper <= threshold {
            search.costs.certified = 1;
            return Ok((vec![window], search.costs));
        }
        if b.lower > threshold {
            search.costs.certified = 1;
            return Ok((Vec::new(), search.costs));
        }
    }
    search.interval(window.start.raw().value(), window.end.raw().value(), 0)?;
    coalesce_periods(&mut search.periods);
    Ok((search.periods, search.costs))
}
