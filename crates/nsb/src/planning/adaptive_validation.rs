// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Executable correctness and performance evidence for the internal #234 spike.

use super::adaptive::{discover, Controls, Discovery, State};
use super::bounds::{prepared_bounds, BoundOutcome, RadianceBounds};
use super::scan::authoritative_above_threshold_periods;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{NsbError, Result};
use siderust::qtty::Days;
use siderust::time::{Interval, ModifiedJulianDate as Mjd};

fn window(a: f64, b: f64) -> Interval<Mjd> {
    Interval::new(Mjd::new(a), Mjd::new(b))
}

fn value(t: Mjd) -> f64 {
    t.raw().value()
}

// Directed interval arithmetic for the synthetic polynomial tests only. Each
// operation expands by one representable value to enclose the rounded point
// operation. These tests use finite, small operands (no overflow/underflow).
#[derive(Clone, Copy)]
struct Enclosure(f64, f64);

impl Enclosure {
    fn subtract(self, x: f64) -> Self {
        Self((self.0 - x).next_down(), (self.1 - x).next_up())
    }
    fn square(self) -> Self {
        let a = self.0 * self.0;
        let b = self.1 * self.1;
        let lo = if self.0 <= 0.0 && self.1 >= 0.0 {
            0.0
        } else {
            a.min(b).next_down()
        };
        Self(lo, a.max(b).next_up())
    }
    fn scale(self, x: f64) -> Self {
        let a = self.0 * x;
        let b = self.1 * x;
        Self(a.min(b).next_down(), a.max(b).next_up())
    }
    fn add(self, x: f64) -> Self {
        Self((self.0 + x).next_down(), (self.1 + x).next_up())
    }
    fn bounds(self) -> Result<BoundOutcome> {
        Ok(BoundOutcome::enclosure(
            RadianceBounds::new(Radiance::new(self.0), Radiance::new(self.1))?,
            true,
        ))
    }
}

pub(super) fn quadratic_bounds(
    w: Interval<Mjd>,
    center: f64,
    scale: f64,
    bias: f64,
) -> Result<BoundOutcome> {
    Enclosure(value(w.start), value(w.end))
        .subtract(60_000.0)
        .subtract(center)
        .square()
        .scale(scale)
        .add(bias)
        .bounds()
}

pub(super) fn quadratic(t: Mjd, center: f64, scale: f64, bias: f64) -> Result<Radiance> {
    let x = (value(t) - 60_000.0) - center;
    Ok(Radiance::new((x * x) * scale + bias))
}

fn assert_partition(result: &Discovery, extent: Interval<Mjd>) {
    assert_eq!(result.regions.first().unwrap().interval.start, extent.start);
    assert_eq!(result.regions.last().unwrap().interval.end, extent.end);
    for pair in result.regions.windows(2) {
        assert_eq!(pair[0].interval.end, pair[1].interval.start);
    }
}

fn assert_certificates(
    result: &Discovery,
    f: impl Fn(Mjd) -> Result<Radiance>,
    threshold: Radiance,
) {
    // Diagnostic probes supplement the analytic enclosure, not its proof.
    for r in &result.regions {
        if matches!(r.state, State::Unresolved(_)) {
            continue;
        }
        for i in 0..=8 {
            let t = Mjd::new(
                value(r.interval.start)
                    + (value(r.interval.end) - value(r.interval.start)) * i as f64 / 8.0,
            );
            let y = f(t).unwrap();
            match r.state {
                State::CertifiedBelow => assert!(y <= threshold),
                State::CertifiedAbove => assert!(y > threshold),
                State::Unresolved(_) => (),
            }
        }
    }
}

#[test]
fn certified_discovery_finds_two_crossings_inside_one_scan_interval() {
    let extent = window(60_000.0, 60_001.0);
    let threshold = Radiance::new(0.0001);
    let f = |t| quadratic(t, 0.3, 1.0, 0.0);
    let old = authoritative_above_threshold_periods(extent, Days::new(1.0), &f, threshold).unwrap();
    assert_eq!(
        old,
        vec![extent],
        "the coarse scan misses the interior accepted dip"
    );
    let result = discover(
        &[extent],
        threshold,
        f,
        |w| quadratic_bounds(w, 0.3, 1.0, 0.0),
        Controls::default(),
    )
    .unwrap();
    assert_partition(&result, extent);
    assert_certificates(&result, f, threshold);
    assert!(result
        .regions
        .iter()
        .any(|r| r.state == State::CertifiedBelow));
    for root in [60_000.29, 60_000.31] {
        assert!(result
            .brackets
            .iter()
            .any(|b| value(b.interval.start) <= root
                && root <= value(b.interval.end)
                && b.tolerance_met));
    }
    for b in &result.brackets {
        assert_ne!(b.left > threshold, b.right > threshold);
        assert_eq!(b.left, f(b.interval.start).unwrap());
        assert_eq!(b.right, f(b.interval.end).unwrap());
    }
    assert!(
        !result.complete(),
        "crossing neighbourhoods remain unresolved"
    );
}

#[test]
fn hidden_narrow_spike_cannot_be_accepted_even_when_the_fit_misses_it() {
    let extent = window(60_000.0, 60_001.0);
    let center = 60_000.301_234;
    let radius = 0.000_001; // 0.0864 s half-width inside a one-day coarse cell.
    let f = |t: Mjd| {
        Ok(Radiance::new(if (value(t) - center).abs() < radius {
            1.0
        } else {
            0.0
        }))
    };
    let b = |w: Interval<Mjd>| {
        if value(w.end) < center - radius || value(w.start) > center + radius {
            Ok(BoundOutcome::enclosure(
                RadianceBounds::constant(Radiance::new(0.0))?,
                false,
            ))
        } else {
            // Deliberately loose, but valid even at the two jumps.
            Ok(BoundOutcome::enclosure(
                RadianceBounds::new(Radiance::new(0.0), Radiance::new(1.0))?,
                true,
            ))
        }
    };
    // Disable fitting across this known discontinuous signal. A second run
    // below exercises an unknown black box whose probes all miss the spike.
    let result = discover(
        &[extent],
        Radiance::new(0.5),
        f,
        b,
        Controls {
            fit_depth: 0,
            ..Controls::default()
        },
    )
    .unwrap();
    assert_partition(&result, extent);
    assert_certificates(&result, f, Radiance::new(0.5));
    assert!(!result
        .regions
        .iter()
        .any(|r| r.state == State::CertifiedBelow
            && r.interval.start <= Mjd::new(center)
            && Mjd::new(center) <= r.interval.end));
    let unknown = discover(
        &[extent],
        Radiance::new(0.5),
        f,
        |_| Ok(BoundOutcome::NeedsSubdivision),
        Controls {
            max_intervals: 64,
            ..Controls::default()
        },
    )
    .unwrap();
    assert!(unknown
        .regions
        .iter()
        .all(|r| matches!(r.state, State::Unresolved(_))));
    assert_eq!(unknown.diagnostics.polynomial_candidates, 0);
    assert!(unknown.brackets.is_empty());
}

#[test]
fn smooth_narrow_peak_requires_bounds_even_with_zero_polynomial_candidates() {
    let extent = window(60_000.0, 60_001.0);
    // A smooth rational spike; reciprocal is monotone over this positive range.
    let f = |t: Mjd| {
        let x = (value(t) - 60_000.0) - 0.301_234;
        Ok(Radiance::new(1.0 / (1.0 + (x * x) * 1.0e12)))
    };
    let bounds = |w: Interval<Mjd>| {
        let d = Enclosure(value(w.start), value(w.end))
            .subtract(60_000.0)
            .subtract(0.301_234)
            .square()
            .scale(1.0e12)
            .add(1.0);
        Enclosure((1.0 / d.1).next_down(), (1.0 / d.0).next_up()).bounds()
    };
    let result = discover(
        &[extent],
        Radiance::new(0.5),
        f,
        bounds,
        Controls {
            min_width: Days::new(1.0e-8),
            ..Controls::default()
        },
    )
    .unwrap();
    assert_certificates(&result, f, Radiance::new(0.5));
    assert!(result
        .regions
        .iter()
        .any(|r| r.state == State::CertifiedAbove));
    assert!(!result.complete());
}

#[test]
fn tangent_and_near_flat_signals_retain_uncertainty() {
    let extent = window(60_000.0, 60_001.0);
    let tangent = discover(
        &[extent],
        Radiance::new(0.0),
        |t| quadratic(t, 0.3, 1.0, 0.0),
        |w| quadratic_bounds(w, 0.3, 1.0, 0.0),
        Controls::default(),
    )
    .unwrap();
    assert!(!tangent.complete());
    assert!(tangent
        .regions
        .iter()
        .filter(|r| r.interval.start <= Mjd::new(60_000.3) && Mjd::new(60_000.3) <= r.interval.end)
        .all(|r| matches!(r.state, State::Unresolved(_))));
    let flat = discover(
        &[extent],
        Radiance::new(1.0),
        |_| Ok(Radiance::new(1.0)),
        |_| {
            RadianceBounds::new(
                Radiance::new(1.0 - f64::EPSILON),
                Radiance::new(1.0 + f64::EPSILON),
            )
            .map(|b| BoundOutcome::enclosure(b, true))
        },
        Controls {
            max_intervals: 32,
            ..Controls::default()
        },
    )
    .unwrap();
    assert!(flat
        .regions
        .iter()
        .all(|r| matches!(r.state, State::Unresolved(_))));
    let equal = discover(
        &[extent],
        Radiance::new(1.0),
        |_| panic!("constant certificate needs no probes"),
        |_| RadianceBounds::constant(Radiance::new(1.0)).map(|b| BoundOutcome::enclosure(b, true)),
        Controls::default(),
    )
    .unwrap();
    assert!(equal.complete());
    assert_eq!(equal.regions[0].state, State::CertifiedBelow);
}

#[test]
fn budgets_and_machine_precision_never_widen_certificates() {
    let extent = window(60_000.0, 60_001.0);
    for evaluations in [0, 10, 11, 12, 20] {
        let controls = Controls {
            max_intervals: 20,
            max_evaluations: evaluations,
            ..Controls::default()
        };
        let result = discover(
            &[extent],
            Radiance::new(0.1),
            |t| quadratic(t, 0.3, 1.0, 0.0),
            |_| Ok(BoundOutcome::NeedsSubdivision),
            controls,
        )
        .unwrap();
        assert_partition(&result, extent);
        assert!(!result.complete());
        assert!(result.diagnostics.bound_computations <= controls.max_intervals);
        assert!(result.diagnostics.exact_evaluations <= evaluations);
        assert!(result
            .regions
            .iter()
            .all(|r| matches!(r.state, State::Unresolved(_))));
        for b in result.brackets {
            assert!(
                !b.tolerance_met
                    || (value(b.interval.end) - value(b.interval.start))
                        <= controls.time_tolerance.value()
            );
        }
    }
    let adjacent = window(60_000.0, 60_000.0_f64.next_up());
    let result = discover(
        &[adjacent],
        Radiance::new(0.0),
        |_| Ok(Radiance::new(1.0)),
        |_| Ok(BoundOutcome::NeedsSubdivision),
        Controls {
            min_width: Days::new(1.0e-20),
            fit_depth: 0,
            ..Controls::default()
        },
    )
    .unwrap();
    assert!(!result.complete());
    assert_eq!(result.regions.len(), 1);
}

#[test]
fn invalid_values_and_model_errors_fail_closed() {
    let extent = window(60_000.0, 60_001.0);
    for y in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(discover(
            &[extent],
            Radiance::new(0.0),
            |_| Ok(Radiance::new(y)),
            |_| Ok(BoundOutcome::NeedsSubdivision),
            Controls::default()
        )
        .is_err());
    }
    assert!(matches!(
        discover(
            &[extent],
            Radiance::new(0.0),
            |_| Err(NsbError::Unsupported("test model".into())),
            |_| Ok(BoundOutcome::NeedsSubdivision),
            Controls::default()
        ),
        Err(NsbError::Unsupported(_))
    ));
    assert!(discover(
        &[extent],
        Radiance::new(0.0),
        |_| panic!(),
        |_| Ok(BoundOutcome::enclosure(
            RadianceBounds {
                lower: Radiance::new(2.0),
                upper: Radiance::new(1.0)
            },
            true
        )),
        Controls::default()
    )
    .is_err());
    // Tempoch rejects NaN at construction, before the search can receive it.
    for bad in [window(0.0, f64::INFINITY), window(1.0, 0.0)] {
        assert!(discover(
            &[bad],
            Radiance::new(0.0),
            |_| panic!(),
            |_| panic!(),
            Controls::default()
        )
        .is_err());
    }
}

#[test]
fn finite_exact_values_with_overflowing_fit_fall_back_without_acceptance() {
    // Subtracting the finite threshold overflows, but authoritative values
    // remain valid. A numerical approximation failure isn't a model error.
    let result = discover(
        &[window(60_000.0, 60_001.0)],
        Radiance::new(-f64::MAX),
        |_| Ok(Radiance::new(f64::MAX)),
        |_| Ok(BoundOutcome::NeedsSubdivision),
        Controls {
            max_intervals: 4,
            ..Controls::default()
        },
    )
    .unwrap();
    assert_eq!(result.diagnostics.failed_fits, 1);
    assert!(result.diagnostics.fallback_intervals > 0);
    assert!(result
        .regions
        .iter()
        .all(|r| matches!(r.state, State::Unresolved(_))));
}

#[test]
fn known_jump_is_partitioned_and_equality_is_not_a_root_precision_claim() {
    let edge = 60_000.3;
    let windows = [window(60_000.0, edge), window(edge, 60_001.0)];
    let f = |t: Mjd| Ok(Radiance::new(if value(t) < edge { 0.0 } else { 1.0 }));
    let bounds = |w: Interval<Mjd>| {
        let lo = if value(w.start) < edge { 0.0 } else { 1.0 };
        let hi = if value(w.end) < edge { 0.0 } else { 1.0 };
        RadianceBounds::new(Radiance::new(lo), Radiance::new(hi))
            .map(|b| BoundOutcome::enclosure(b, true))
    };
    let result = discover(&windows, Radiance::new(0.5), f, bounds, Controls::default()).unwrap();
    assert_partition(&result, window(60_000.0, 60_001.0));
    assert_certificates(&result, f, Radiance::new(0.5));
    assert!(!result.complete()); // the left closed endpoint includes the jump
    assert!(result
        .brackets
        .iter()
        .any(|b| b.interval.end == Mjd::new(edge)));
}

#[test]
fn seeded_quadratic_cases_preserve_analytic_coverage() {
    let mut seed = 234_u64;
    for _ in 0..64 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let center = 0.1 + (seed >> 32) as f64 / u32::MAX as f64 * 0.8;
        let radius = 0.0001 + (seed & 0xffff) as f64 / 65535.0 * 0.02;
        let extent = window(60_000.0, 60_001.0);
        let f = |t| quadratic(t, center, 1.0, 0.0);
        let threshold = Radiance::new(radius * radius);
        let result = discover(
            &[extent],
            threshold,
            f,
            |w| quadratic_bounds(w, center, 1.0, 0.0),
            Controls::default(),
        )
        .unwrap();
        assert_partition(&result, extent);
        assert_certificates(&result, f, threshold);
        // Analytic real roots must never be inside a strict above certificate.
        for root in [60_000.0 + center - radius, 60_000.0 + center + radius] {
            assert!(!result
                .regions
                .iter()
                .any(|r| r.state == State::CertifiedAbove
                    && value(r.interval.start) < root
                    && root < value(r.interval.end)));
        }
    }
}

#[test]
fn real_prepared_moon_down_enclosure_respects_inclusive_boundaries() {
    use super::{prepare, scan, types::ThresholdQuery};
    use crate::{ComponentMask, NsbEvaluator, Target, DEG};
    use chrono::{DateTime, Utc};
    use tempoch::{Period, Time, UTC};
    let evaluator = NsbEvaluator::new().unwrap();
    let start = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2026-01-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let query = ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, start + siderust::qtty::Seconds::new(3.0 * 86_400.0)),
        Radiance::new(0.0),
    )
    .with_components(ComponentMask::MOON)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let context = evaluator.prepare_site_window_context(&query).unwrap();
    let prepared = prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
    let visible = prepared.moon_visible_periods.as_ref().unwrap();
    let rise = visible[0].start;
    assert!(matches!(
        prepared_bounds(&prepared, Interval::new(rise, rise)).unwrap(),
        BoundOutcome::Unsupported
    ));
    let exact = |t| super::threshold::evaluate_integrated(&evaluator, &prepared, t);
    let gap = visible.windows(2).find(|p| p[0].end < p[1].start).unwrap();
    let gap = Interval::new(gap[0].end, gap[1].start);
    let result = discover(
        &[gap],
        query.threshold,
        exact,
        |w| prepared_bounds(&prepared, w),
        Controls {
            fit_depth: 0,
            max_intervals: 256,
            ..Controls::default()
        },
    )
    .unwrap();
    assert_partition(&result, gap);
    assert!(!result.complete());
    assert!(result
        .regions
        .iter()
        .any(|r| r.state == State::CertifiedBelow));
    assert_certificates(&result, exact, query.threshold);
    // Preserve the original query endpoints when certificates meet them.
    let tt = scan::utc_period_to_tt_mjd(query.window);
    let roundtrip = scan::tt_mjd_period_to_utc(tt, tt, query.window);
    assert_eq!(roundtrip, query.window);
}

#[test]
fn unsupported_and_fixed_uncertainty_stop_without_point_search() {
    use super::adaptive::StopReason;
    for (provider, reason) in [
        (BoundOutcome::Unsupported, StopReason::UnsupportedBounds),
        (
            BoundOutcome::enclosure(
                RadianceBounds::new(Radiance::new(0.0), Radiance::new(2.0)).unwrap(),
                false,
            ),
            StopReason::InconclusiveFixedBounds,
        ),
    ] {
        let out = discover(
            &[window(60_000.0, 60_365.0)],
            Radiance::new(1.0),
            |_| panic!("unsupported/fixed envelopes must not trigger probes"),
            |_| Ok(provider),
            Controls::default(),
        )
        .unwrap();
        assert_eq!(out.regions[0].state, State::Unresolved(reason));
        assert_eq!(out.regions.len(), 1);
        assert_eq!(out.diagnostics.subdivisions, 0);
        assert_eq!(out.diagnostics.bound_computations, 1);
        assert!(
            out.certified_periods().is_err(),
            "uncertainty is never a valid empty schedule"
        );
    }
}

#[test]
fn whole_query_budgets_preserve_later_bound_only_certificates() {
    let windows = [window(60_000.0, 60_001.0), window(60_002.0, 60_003.0)];
    let out = discover(
        &windows,
        // Wide enough for fitting nodes to supply opposite-side endpoints
        // even when the reserved eleven probes exhaust the exact budget.
        Radiance::new(0.01),
        |t| quadratic(t, 0.3, 1.0, 0.0),
        |w| {
            if w.start == windows[1].start {
                Ok(BoundOutcome::enclosure(
                    RadianceBounds::constant(Radiance::new(0.0))?,
                    false,
                ))
            } else {
                quadratic_bounds(w, 0.3, 1.0, 0.0)
            }
        },
        Controls {
            max_evaluations: 11,
            max_intervals: 64,
            fit_depth: 3,
            ..Controls::default()
        },
    )
    .unwrap();
    assert_eq!(out.diagnostics.exact_evaluations, 11);
    assert!(out.diagnostics.exact_budget_exhausted);
    assert!(out.diagnostics.fit_budget_limited);
    assert!(out.diagnostics.bound_computations <= 64);
    assert!(out
        .regions
        .iter()
        .any(|r| r.interval == windows[1] && r.state == State::CertifiedBelow));
    assert!(!out.brackets.is_empty());
    assert!(out.brackets.iter().all(|b| !b.tolerance_met));
    assert!(out.certified_periods().is_err());

    // The bound cap is global too: it does not reset for the second window.
    let limited = discover(
        &windows,
        Radiance::new(1.0),
        |_| panic!(),
        |_| {
            Ok(BoundOutcome::enclosure(
                RadianceBounds::constant(Radiance::new(0.0))?,
                false,
            ))
        },
        Controls {
            max_intervals: 1,
            ..Controls::default()
        },
    )
    .unwrap();
    assert_eq!(limited.diagnostics.bound_computations, 1);
    assert!(!limited.complete());
    assert!(limited.certified_periods().is_err());
}

#[test]
fn bound_numerical_failure_is_not_unsupported_or_budget_exhaustion() {
    let result = discover(
        &[window(60_000.0, 60_001.0)],
        Radiance::new(1.0),
        |_| panic!(),
        |_| {
            Err(NsbError::Interpolation(
                "invalid enclosure arithmetic".into(),
            ))
        },
        Controls::default(),
    );
    assert!(matches!(result, Err(NsbError::Interpolation(_))));
}

#[test]
fn production_planning_preserves_discrete_results_for_every_mask_and_lunar_model() {
    use super::{filters, prepare, scan, threshold};
    use crate::{ComponentMask, MoonlightModel, Target, ThresholdQuery, DEG};
    use chrono::{DateTime, Utc};
    use tempoch::{Period, Time, UTC};
    let start = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2026-01-14T01:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let base = ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, start + siderust::qtty::Seconds::new(6.0 * 3_600.0)),
        Radiance::new(0.25),
    );
    for moon in [
        MoonlightModel::KrisciunasSchaefer1991,
        MoonlightModel::Jones2013Spectral,
    ] {
        let evaluator = super::adaptive_bench::evaluator_with_moon(false, true, moon);
        for bits in 0..=15 {
            let query = base
                .clone()
                .with_components(ComponentMask::from_bits(bits).unwrap());
            let context = evaluator.prepare_site_window_context(&query).unwrap();
            let prepared = prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
            let mut reference = Vec::new();
            for w in filters::smooth_threshold_windows(&prepared) {
                let brighter = authoritative_above_threshold_periods(
                    w,
                    Days::new(600.0 / 86_400.0),
                    &|t| threshold::evaluate_integrated(&evaluator, &prepared, t),
                    query.threshold,
                )
                .unwrap();
                reference.extend(scan::complement_periods(w, &brighter));
            }
            scan::coalesce_periods(&mut reference);
            let actual = evaluator
                .periods_below_threshold_with_context(&context, &query)
                .unwrap();
            assert_eq!(
                actual.periods.len(),
                reference.len(),
                "mask {bits}, model {moon:?}"
            );
            for (actual, expected) in actual.periods.iter().zip(reference) {
                let expected =
                    scan::tt_mjd_period_to_utc(expected, prepared.tt_window, query.window);
                // Regression evidence for the compatibility path, not proof of
                // continuous-time discovery. Both refine to about 0.864 s.
                assert!(
                    (actual.start - expected.start)
                        .to::<crate::units::unit::Second>()
                        .value()
                        .abs()
                        <= 2.0,
                    "mask {bits}, model {moon:?}"
                );
                assert!(
                    (actual.end - expected.end)
                        .to::<crate::units::unit::Second>()
                        .value()
                        .abs()
                        <= 2.0,
                    "mask {bits}, model {moon:?}"
                );
            }
            let out = discover(
                &filters::smooth_threshold_windows(&prepared),
                query.threshold,
                |_| panic!("real certificates need no exact probes"),
                |w| prepared_bounds(&prepared, w),
                Controls {
                    fit_depth: 0,
                    max_evaluations: 0,
                    ..Controls::default()
                },
            )
            .unwrap();
            let strict = evaluator.periods_below_threshold_certified_with_context(&context, &query);
            assert_eq!(
                strict.is_ok(),
                out.complete(),
                "mask {bits}, model {moon:?}"
            );
            if bits == 0 || bits == ComponentMask::STARLIGHT.bits() {
                assert!(strict.is_ok());
                let strict = strict.unwrap();
                assert_eq!(strict.periods.len(), actual.periods.len());
                for (certified, sampled) in strict.periods.iter().zip(&actual.periods) {
                    assert!(certified.start >= sampled.start);
                    assert!(certified.end <= sampled.end);
                    assert!((certified.start - sampled.start).value() <= 0.1);
                    assert!((sampled.end - certified.end).value() <= 0.1);
                }
            } else if !out.complete() {
                assert!(matches!(strict, Err(NsbError::Unsupported(_))));
            }
        }
        // No eligible target time is a complete empty result even when the
        // selected dynamic models would be unsupported on a nonempty interval.
        let mut invisible = base.clone();
        invisible.target = Target::new(0.0 * DEG, 89.0 * DEG);
        invisible.components = ComponentMask::ALL;
        assert!(evaluator
            .periods_below_threshold_certified(&invisible)
            .unwrap()
            .periods
            .is_empty());
    }
}
