// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Ignored release-mode harness: internal measurements without a public feature.
//! Run via scripts/benchmark-adaptive-threshold.sh. Emits JSONL to stdout.

use super::adaptive::{discover, Controls, State, StopReason};
use super::bounds::{prepared_bounds, BoundOutcome};
use super::{filters, prepare, scan, threshold};
use crate::components::starlight::{StarlightMap, StarlightProvenance};
use crate::site::{AtmosphericConditions, SiteProfile, SiteProfileTag};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{
    ComponentMask, NsbEvaluator, NsbModelConfig, SiteWindowContext, StarlightProduct, Target,
    ThresholdQuery, DEG,
};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use siderust::qtty::{Days, Degrees, Hectopascals, Kilometers, Meters};
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;
use tempoch::{Period, Time, UTC};

#[derive(Default)]
struct Costs {
    exact: usize,
    bounds: usize,
    fits: usize,
    failed_fits: usize,
    candidates: usize,
    subdivisions: usize,
    refinements: usize,
    fallbacks: usize,
    unresolved: usize,
    accepted: usize,
    rejected: usize,
    unresolved_days: f64,
    bounds_seconds: f64,
    fits_seconds: f64,
    refinement_seconds: f64,
    peak_pending: usize,
    unsupported: usize,
    exact_budget_exhausted: usize,
    fit_budget_limited: usize,
    interval_budget_unresolved: usize,
}

fn run(
    evaluator: &NsbEvaluator,
    context: &SiteWindowContext,
    queries: &[ThresholdQuery],
    adaptive: bool,
) -> (f64, Costs) {
    let start = Instant::now();
    let mut total = Costs::default();
    for query in queries {
        // Same target preparation and Rayon orchestration as the public search.
        let prepared = prepare::prepare_target_threshold(evaluator, context, query).unwrap();
        let windows = filters::smooth_threshold_windows(&prepared);
        let costs: Vec<Costs> = if adaptive {
            // One cap for the entire target query, never reset per window.
            let out = discover(
                &windows,
                query.threshold,
                |t| threshold::evaluate_integrated(evaluator, &prepared, t),
                |w| prepared_bounds(&prepared, w),
                Controls {
                    max_intervals: 2_048,
                    max_evaluations: 0,
                    fit_depth: 0,
                    ..Controls::default()
                },
            )
            .unwrap();
            let d = &out.diagnostics;
            let mut c = Costs {
                exact: d.exact_evaluations,
                bounds: d.bound_computations,
                fits: d.fits,
                failed_fits: d.failed_fits,
                candidates: d.polynomial_candidates,
                subdivisions: d.subdivisions,
                refinements: d.root_refinements,
                fallbacks: d.fallback_intervals,
                unresolved: d.unresolved_intervals,
                bounds_seconds: d.bounds_time.as_secs_f64(),
                fits_seconds: d.fits_time.as_secs_f64(),
                refinement_seconds: d.refinement_time.as_secs_f64(),
                peak_pending: d.peak_pending_intervals,
                unsupported: d.unsupported_intervals,
                exact_budget_exhausted: usize::from(d.exact_budget_exhausted),
                fit_budget_limited: usize::from(d.fit_budget_limited),
                ..Costs::default()
            };
            for r in &out.regions {
                match r.state {
                    State::CertifiedBelow => c.accepted += 1,
                    State::CertifiedAbove => c.rejected += 1,
                    State::Unresolved(reason) => {
                        c.unresolved_days += r.interval.length().value();
                        c.interval_budget_unresolved +=
                            usize::from(reason == StopReason::IntervalBudget);
                    }
                }
            }
            black_box(&out);
            vec![c]
        } else {
            windows
                .into_par_iter()
                .map(|w| {
                    let count = Cell::new(0);
                    let f = |t| {
                        count.set(count.get() + 1);
                        threshold::evaluate_integrated(evaluator, &prepared, t)
                    };
                    let brighter = scan::authoritative_above_threshold_periods(
                        w,
                        Days::new(600.0 / 86_400.0),
                        &f,
                        query.threshold,
                    )
                    .unwrap();
                    black_box(scan::complement_periods(w, &brighter));
                    Costs {
                        exact: count.get(),
                        ..Costs::default()
                    }
                })
                .collect()
        };
        for c in costs {
            total.unsupported += c.unsupported;
            total.exact_budget_exhausted += c.exact_budget_exhausted;
            total.fit_budget_limited += c.fit_budget_limited;
            total.interval_budget_unresolved += c.interval_budget_unresolved;
            total.exact += c.exact;
            total.bounds += c.bounds;
            total.fits += c.fits;
            total.failed_fits += c.failed_fits;
            total.candidates += c.candidates;
            total.subdivisions += c.subdivisions;
            total.refinements += c.refinements;
            total.fallbacks += c.fallbacks;
            total.unresolved += c.unresolved;
            total.accepted += c.accepted;
            total.rejected += c.rejected;
            total.unresolved_days += c.unresolved_days;
            total.bounds_seconds += c.bounds_seconds;
            total.fits_seconds += c.fits_seconds;
            total.refinement_seconds += c.refinement_seconds;
            total.peak_pending = total.peak_pending.max(c.peak_pending);
        }
    }
    (start.elapsed().as_secs_f64(), total)
}

fn parse(s: &str) -> Time<UTC> {
    Time::<UTC>::from_chrono(DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc))
}

#[test]
fn real_starlight_certificate_reuses_the_authoritative_cached_scalar() {
    let evaluator = evaluator(false, true);
    let start = parse("2026-12-30T00:00:00Z");
    let end = parse("2027-01-02T00:00:00Z");
    let query = ThresholdQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, end),
        Radiance::new(100.0),
    )
    .with_components(ComponentMask::STARLIGHT)
    .with_sun_altitude_ceiling(None)
    .with_target_altitude_floor(None);
    let context = evaluator.prepare_site_window_context(&query).unwrap();
    let prepared = prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
    let exact = |t| threshold::evaluate_integrated(&evaluator, &prepared, t);
    let BoundOutcome::Enclosure {
        range: enclosure, ..
    } = prepared_bounds(&prepared, prepared.tt_window).unwrap()
    else {
        panic!("missing Starlight certificate")
    };
    assert_eq!(enclosure.lower, enclosure.upper);
    for i in 0..=100 {
        let t = siderust::time::ModifiedJulianDate::new(
            prepared.tt_window.start.raw().value()
                + i as f64 / 100.0 * prepared.tt_window.length().value(),
        );
        assert_eq!(
            exact(t).unwrap().value().to_bits(),
            enclosure.lower.value().to_bits()
        );
    }
    let result = discover(
        &[prepared.tt_window],
        enclosure.upper,
        exact,
        |w| prepared_bounds(&prepared, w),
        Controls::default(),
    )
    .unwrap();
    assert!(result.complete());
    assert_eq!(result.regions[0].state, State::CertifiedBelow);
    assert_eq!(result.diagnostics.exact_evaluations, 0);
    let mut strict_query = query.clone();
    strict_query.threshold = enclosure.upper;
    assert_eq!(
        evaluator
            .periods_below_threshold_certified(&strict_query)
            .unwrap()
            .periods,
        vec![query.window]
    );
    assert!(enclosure.lower.value() > 0.0);
    strict_query.threshold = Radiance::new(0.0);
    assert!(
        evaluator
            .periods_below_threshold_certified(&strict_query)
            .unwrap()
            .periods
            .is_empty(),
        "a proven rejection is a valid empty schedule, unlike incomplete coverage"
    );
    // A constant partial contribution cannot certify a selected dynamic sum.
    let mut dynamic = prepared;
    dynamic.components |= ComponentMask::ZODIACAL;
    assert!(matches!(
        prepared_bounds(&dynamic, dynamic.tt_window).unwrap(),
        BoundOutcome::Unsupported
    ));
}

fn evaluator(north: bool, starlight: bool) -> NsbEvaluator {
    evaluator_with_moon(north, starlight, crate::MoonlightModel::Jones2013Spectral)
}

pub(super) fn evaluator_with_moon(
    north: bool,
    starlight: bool,
    moon: crate::MoonlightModel,
) -> NsbEvaluator {
    struct North;
    impl SiteProfileTag for North {
        const NAME: &'static str = "ctao-north-planning";
    }
    struct South;
    impl SiteProfileTag for South {
        const NAME: &'static str = "ctao-south-planning";
    }
    let config = if north {
        NsbModelConfig::generic_clear_sky().with_site_profile(
            SiteProfile::<North>::planning(
                Kilometers::new(2.2),
                AtmosphericConditions::clear_sky_with_pressure(Hectopascals::new(770.0)).unwrap(),
                "CTAO-N benchmark planning preset",
            )
            .unwrap(),
        )
    } else {
        NsbModelConfig::generic_clear_sky().with_site_profile(
            SiteProfile::<South>::planning(
                Kilometers::new(2.1),
                AtmosphericConditions::paranal_average(),
                "CTAO-S benchmark planning preset",
            )
            .unwrap(),
        )
    };
    let config = if starlight {
        let p = StarlightProvenance::new(
            "synthetic benchmark fixture",
            "fixture",
            "2026-06-21",
            "synthetic",
            "test-only",
            "test-only",
            "integrated 300-650 nm photon radiance",
            "HEALPix nside=1 ring 12 pixels",
            None::<String>,
        );
        let map = StarlightMap::from_csv_str(
            include_str!("../../tests/data/starlight_fixture_map.csv"),
            p,
        )
        .unwrap();
        config.with_starlight_product(StarlightProduct::with_experimental_map(map))
    } else {
        config
    };
    NsbEvaluator::with_config(config.with_moonlight_model(moon)).unwrap()
}

fn measure(
    label: &str,
    north: bool,
    start: &str,
    days: i64,
    components: ComponentMask,
    threshold: f64,
    targets: usize,
) {
    measure_with_moon(
        label,
        north,
        start,
        days,
        components,
        threshold,
        targets,
        crate::MoonlightModel::Jones2013Spectral,
    );
}

#[allow(clippy::too_many_arguments)]
fn measure_with_moon(
    label: &str,
    north: bool,
    start: &str,
    days: i64,
    components: ComponentMask,
    threshold: f64,
    targets: usize,
    moon: crate::MoonlightModel,
) {
    let evaluator = evaluator_with_moon(north, components.contains(ComponentMask::STARLIGHT), moon);
    let observer = if north {
        siderust::coordinates::centers::Geodetic::new_raw(
            Degrees::new(-17.892),
            Degrees::new(28.762),
            Meters::new(2_200.0),
        )
    } else {
        siderust::coordinates::centers::Geodetic::new_raw(
            Degrees::new(-70.316_344_444_444_44),
            Degrees::new(-24.683_427_777_777_776),
            Meters::new(2_184.6),
        )
    };
    let start = parse(start);
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(days));
    let seed = ThresholdQuery::new(
        observer,
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, end),
        Radiance::new(threshold),
    )
    .with_components(components)
    .with_target_altitude_floor(Some(Degrees::new(20.0)));
    let preparation = Instant::now();
    let context = evaluator.prepare_site_window_context(&seed).unwrap();
    let preparation_seconds = preparation.elapsed().as_secs_f64();
    let queries: Vec<_> = (0..targets)
        .map(|i| {
            let mut q = seed.clone();
            if i > 0 {
                q.target = Target::new(
                    (83.6331 + i as f64 * 137.507_764).rem_euclid(360.0) * DEG,
                    (-60.0 + (i % 25) as f64 * 5.0) * DEG,
                );
            }
            q
        })
        .collect();
    let repeats: usize = std::env::var("NSB_ADAPTIVE_REPEATS")
        .ok()
        .map(|x| x.parse().unwrap())
        .unwrap_or(3);
    assert!(repeats >= 3);
    // Both paths warm up; timed pairs alternate their order.
    run(&evaluator, &context, &queries, false);
    run(&evaluator, &context, &queries, true);
    let mut old_times = Vec::new();
    let mut new_times = Vec::new();
    let mut old = Costs::default();
    let mut new = Costs::default();
    for i in 0..repeats {
        for adaptive in if i % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let (seconds, costs) = run(&evaluator, &context, &queries, adaptive);
            if adaptive {
                new_times.push(seconds);
                new = costs;
            } else {
                old_times.push(seconds);
                old = costs;
            }
        }
    }
    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    println!(
        "ADAPTIVE_BENCH {}",
        serde_json::json!({
            "scenario": label, "site": if north { "CTAO-N" } else { "CTAO-S" }, "days": days, "targets": targets,
            "components_bits": components.bits(), "threshold": threshold, "repeats": repeats,
            "moonlight_model": moon.as_str(),
            "preparation_seconds": preparation_seconds,
            "previous_seconds": old_times, "prototype_seconds": new_times,
            "previous_exact": old.exact, "prototype_exact": new.exact,
            "unsupported_intervals": new.unsupported,
            "exact_budget_exhausted_queries": new.exact_budget_exhausted,
            "fit_budget_limited_queries": new.fit_budget_limited,
            "interval_budget_unresolved": new.interval_budget_unresolved,
            "bound_computations": new.bounds, "fits": new.fits, "failed_fits": new.failed_fits,
            "polynomial_candidates": new.candidates, "subdivisions": new.subdivisions, "root_refinements": new.refinements,
            "fallback_intervals": new.fallbacks, "unresolved_intervals": new.unresolved,
            "unresolved_days": new.unresolved_days, "certified_below": new.accepted, "certified_above": new.rejected,
            "bounds_worker_elapsed_seconds": new.bounds_seconds, "fits_worker_elapsed_seconds": new.fits_seconds,
            "refinement_worker_elapsed_seconds": new.refinement_seconds, "peak_pending_per_worker": new.peak_pending,
            "prototype_complete": new.unresolved == 0,
            "comparable_certification": components == ComponentMask::STARLIGHT
        })
    );
}

#[test]
#[ignore = "release-mode performance experiment; run scripts/benchmark-adaptive-threshold.sh"]
fn adaptive_threshold_benchmark_matrix() {
    let filter = std::env::var("NSB_ADAPTIVE_FILTER").unwrap_or_default();
    for (name, threshold) in [("far", 1.0), ("close-pair", 1.0e-8), ("tangent", 0.0)] {
        let label = format!("synthetic/{name}");
        if label.contains(&filter) {
            synthetic_measure(&label, threshold);
        }
    }
    let case = |label: String, north, start, days, mask, limit, count| {
        if label.contains(&filter) {
            measure(&label, north, start, days, mask, limit, count);
        }
    };
    for north in [false, true] {
        let site = if north { "north" } else { "south" };
        for (name, mask) in [
            ("all", ComponentMask::ALL),
            ("zodiacal", ComponentMask::ZODIACAL),
            ("airglow", ComponentMask::AIRGLOW),
            ("moon", ComponentMask::MOON),
            ("starlight", ComponentMask::STARLIGHT),
        ] {
            for days in [1, 7, 30, 365] {
                case(
                    format!("{site}/{name}/{days}d"),
                    north,
                    "2026-01-01T00:00:00Z",
                    days,
                    mask,
                    0.25,
                    1,
                );
            }
        }
        for count in [10, 100] {
            case(
                format!("{site}/all/year/{count}targets"),
                north,
                "2026-01-01T00:00:00Z",
                365,
                ComponentMask::ALL,
                0.25,
                count,
            );
        }
        for (name, start, mask, threshold) in [
            (
                "bright-moon",
                "2023-09-29T00:00:00Z",
                ComponentMask::MOON,
                0.02,
            ),
            (
                "moon-down",
                "2023-09-15T00:00:00Z",
                ComponentMask::MOON,
                0.02,
            ),
            (
                "near-threshold",
                "2026-01-01T00:00:00Z",
                ComponentMask::ALL,
                0.205,
            ),
            (
                "far-threshold",
                "2026-01-01T00:00:00Z",
                ComponentMask::ALL,
                100.0,
            ),
        ] {
            case(
                format!("{site}/{name}"),
                north,
                start,
                30,
                mask,
                threshold,
                1,
            );
        }
        // The analytic lunar model is an independently supported path, not a
        // numerical stand-in for Jones. Exercise its component and ALL sums.
        for (name, mask) in [("moon", ComponentMask::MOON), ("all", ComponentMask::ALL)] {
            for days in [1, 7, 30, 365] {
                let label = format!("{site}/ks1991/{name}/{days}d");
                if label.contains(&filter) {
                    measure_with_moon(
                        &label,
                        north,
                        "2026-01-01T00:00:00Z",
                        days,
                        mask,
                        0.25,
                        1,
                        crate::MoonlightModel::KrisciunasSchaefer1991,
                    );
                }
            }
        }
        for count in [10, 100] {
            let label = format!("{site}/ks1991/all/year/{count}targets");
            if label.contains(&filter) {
                measure_with_moon(
                    &label,
                    north,
                    "2026-01-01T00:00:00Z",
                    365,
                    ComponentMask::ALL,
                    0.25,
                    count,
                    crate::MoonlightModel::KrisciunasSchaefer1991,
                );
            }
        }
        for (name, start, mask, limit) in [
            (
                "bright-moon",
                "2023-09-29T00:00:00Z",
                ComponentMask::MOON,
                0.02,
            ),
            (
                "moon-down",
                "2023-09-15T00:00:00Z",
                ComponentMask::MOON,
                0.02,
            ),
            (
                "near-threshold",
                "2026-01-01T00:00:00Z",
                ComponentMask::ALL,
                0.205,
            ),
            (
                "far-threshold",
                "2026-01-01T00:00:00Z",
                ComponentMask::ALL,
                100.0,
            ),
        ] {
            let label = format!("{site}/ks1991/{name}");
            if label.contains(&filter) {
                measure_with_moon(
                    &label,
                    north,
                    start,
                    30,
                    mask,
                    limit,
                    1,
                    crate::MoonlightModel::KrisciunasSchaefer1991,
                );
            }
        }
    }
}

fn synthetic_measure(label: &str, limit: f64) {
    use super::adaptive_validation::{quadratic, quadratic_bounds};
    use siderust::time::{Interval, ModifiedJulianDate as Mjd};
    let w = Interval::new(Mjd::new(60_000.0), Mjd::new(60_001.0));
    let f = |t| quadratic(t, 0.301_234, 1.0, 0.0);
    let bound = |w| quadratic_bounds(w, 0.301_234, 1.0, 0.0);
    let threshold = Radiance::new(limit);
    // A separate experiment can compare sound bound-only subdivision with
    // fit assistance on the identical analytic signal and uncertainty contract.
    let fit_depth = std::env::var("NSB_ADAPTIVE_SYNTHETIC_FIT_DEPTH")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(1);
    let mut old_times = Vec::new();
    let mut new_times = Vec::new();
    let mut old_count = 0;
    let mut last = None;
    let repeats: usize = std::env::var("NSB_ADAPTIVE_REPEATS")
        .ok()
        .map(|x| x.parse().unwrap())
        .unwrap_or(3);
    assert!(repeats >= 3);
    const BATCH: usize = 100;
    for repeat in 0..=repeats {
        for adaptive in if repeat % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = Instant::now();
            for _ in 0..BATCH {
                if adaptive {
                    last = Some(black_box(
                        discover(
                            &[w],
                            threshold,
                            f,
                            bound,
                            Controls {
                                fit_depth,
                                ..Controls::default()
                            },
                        )
                        .unwrap(),
                    ));
                } else {
                    let count = Cell::new(0);
                    let exact = |t| {
                        count.set(count.get() + 1);
                        f(t)
                    };
                    black_box(
                        scan::authoritative_above_threshold_periods(
                            w,
                            Days::new(600.0 / 86_400.0),
                            &exact,
                            threshold,
                        )
                        .unwrap(),
                    );
                    old_count = count.get();
                }
            }
            let seconds = start.elapsed().as_secs_f64() / BATCH as f64;
            if repeat > 0 {
                if adaptive {
                    new_times.push(seconds);
                } else {
                    old_times.push(seconds);
                }
            }
        }
    }
    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    let out = last.unwrap();
    let d = &out.diagnostics;
    println!(
        "ADAPTIVE_BENCH {}",
        serde_json::json!({
            "scenario": label, "site": "analytic", "days": 1, "targets": 1, "threshold": limit,
            "repeats": repeats, "batch": BATCH, "preparation_seconds": 0.0,
            "synthetic_fit_depth": fit_depth,
            "previous_seconds": old_times, "prototype_seconds": new_times,
            "previous_exact": old_count, "prototype_exact": d.exact_evaluations,
            "bound_computations": d.bound_computations, "fits": d.fits, "failed_fits": d.failed_fits,
            "polynomial_candidates": d.polynomial_candidates, "subdivisions": d.subdivisions,
            "root_refinements": d.root_refinements, "fallback_intervals": d.fallback_intervals,
            "unresolved_intervals": d.unresolved_intervals,
            "unresolved_days": out.regions.iter().filter(|r| matches!(r.state, State::Unresolved(_))).map(|r| r.interval.length().value()).sum::<f64>(),
            "certified_below": out.regions.iter().filter(|r| r.state == State::CertifiedBelow).count(),
            "certified_above": out.regions.iter().filter(|r| r.state == State::CertifiedAbove).count(),
            "bounds_worker_elapsed_seconds": d.bounds_time.as_secs_f64(), "fits_worker_elapsed_seconds": d.fits_time.as_secs_f64(),
            "refinement_worker_elapsed_seconds": d.refinement_time.as_secs_f64(), "peak_pending_per_worker": d.peak_pending_intervals,
            "prototype_complete": out.complete(), "comparable_certification": out.complete()
        })
    );
}
