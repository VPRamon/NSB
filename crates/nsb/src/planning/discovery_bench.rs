// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Complete schedule benchmark; the historical certificate feasibility harness
//! remains unchanged. Run the ignored test in release with two Rayon threads.
use super::{
    bounds::prepared_bounds,
    discovery::{self, Costs},
    filters, prepare, scan, threshold,
};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, MoonlightModel, SiteWindowContext, Target, ThresholdQuery, DEG};
use rayon::prelude::*;
use siderust::qtty::{Degrees, Meters, Seconds};
use siderust::time::{Interval, ModifiedJulianDate};
use std::{cell::Cell, time::Instant};
use tempoch::{Period, Time, UTC};

fn run(
    evaluator: &crate::NsbEvaluator,
    context: &SiteWindowContext,
    queries: &[ThresholdQuery],
    adaptive: bool,
    step: f64,
) -> (f64, Costs, Vec<Vec<Interval<ModifiedJulianDate>>>) {
    let started = Instant::now();
    let mut costs = Costs::default();
    let mut output = Vec::new();
    for q in queries {
        let p = prepare::prepare_target_threshold(evaluator, context, q).unwrap();
        let windows = if adaptive {
            filters::adaptive_threshold_windows(&p).unwrap()
        } else {
            filters::smooth_threshold_windows(&p)
        };
        let parts: Vec<_> = if adaptive {
            vec![discovery::search_windows(
                &windows,
                q.threshold,
                None,
                |t| threshold::evaluate_integrated(evaluator, &p, t),
                |w| prepared_bounds(&p, w),
                false,
            )
            .unwrap_or_else(|e| panic!("query {q:?}: {e}"))]
        } else {
            windows
                .into_par_iter()
                .map(|w| {
                    let count = Cell::new(0);
                    let f = |t| {
                        count.set(count.get() + 1);
                        threshold::evaluate_integrated(evaluator, &p, t)
                    };
                    let above = scan::authoritative_above_threshold_periods(
                        w,
                        siderust::qtty::Days::new(step / 86400.0),
                        &f,
                        q.threshold,
                    )
                    .unwrap();
                    (
                        scan::complement_periods(w, &above),
                        Costs {
                            exact: count.get(),
                            ..Costs::default()
                        },
                    )
                })
                .collect()
        };
        let mut periods = Vec::new();
        for (part, c) in parts {
            periods.extend(part);
            costs.exact += c.exact;
            costs.bounds += c.bounds;
            costs.fits += c.fits;
            costs.subdivisions += c.subdivisions;
            costs.refinements += c.refinements;
            costs.extrema += c.extrema;
            costs.certified += c.certified;
            costs.peak_samples = costs.peak_samples.max(c.peak_samples);
            costs.peak_pending = costs.peak_pending.max(c.peak_pending);
            costs.peak_depth = costs.peak_depth.max(c.peak_depth);
            costs.bracket_seconds += c.bracket_seconds;
        }
        scan::coalesce_periods(&mut periods);
        // Exercise the native output projection too: these must be usable
        // schedules, rather than unexportable internal crossing candidates.
        for &period in &periods {
            if adaptive {
                scan::certified_tt_mjd_period_to_utc(period, p.tt_window, q.window).unwrap();
            } else {
                scan::tt_mjd_period_to_utc(period, p.tt_window, q.window);
            }
        }
        output.push(periods);
    }
    (started.elapsed().as_secs_f64(), costs, output)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compare(
    evaluator: &crate::NsbEvaluator,
    context: &SiteWindowContext,
    q: &ThresholdQuery,
    actual: &[Interval<ModifiedJulianDate>],
    reference: &[Interval<ModifiedJulianDate>],
    label: &str,
    allow_new_events: bool,
) -> usize {
    let p = prepare::prepare_target_threshold(evaluator, context, q).unwrap();
    let windows = filters::smooth_threshold_windows(&p);
    let mut used = vec![false; actual.len()];
    let mut artifacts = 0;
    if allow_new_events {
        for r in reference {
            if r.length().value() * 86400.0 <= 0.432001 {
                let boundary = if windows.iter().any(|w| w.start == r.start) {
                    Some(r.start)
                } else if windows.iter().any(|w| w.end == r.end) {
                    Some(r.end)
                } else {
                    None
                };
                if boundary.is_some_and(|t| {
                    threshold::evaluate_integrated(evaluator, &p, t).unwrap() <= q.threshold
                }) && [0.1, 0.25, 0.5, 0.75, 0.9].into_iter().all(|u| {
                    threshold::evaluate_integrated(
                        evaluator,
                        &p,
                        ModifiedJulianDate::new(r.start.raw().value() + r.length().value() * u),
                    )
                    .unwrap()
                        > q.threshold
                }) {
                    artifacts += 1;
                    continue;
                }
            }
            for (t, start) in [(r.start, true), (r.end, false)] {
                if actual
                    .iter()
                    .flat_map(|a| [a.start, a.end])
                    .any(|u| (t.raw().value() - u.raw().value()).abs() * 86400.0 <= 0.533)
                {
                    continue;
                }
                let physical = windows
                    .iter()
                    .any(|w| if start { w.start == t } else { w.end == t });
                let just_inside = ModifiedJulianDate::new(
                    t.raw().value() + (if start { 0.001 } else { -0.001 }) / 86400.0,
                );
                let singleton = physical
                    && threshold::evaluate_integrated(evaluator, &p, t).unwrap() <= q.threshold
                    && threshold::evaluate_integrated(evaluator, &p, just_inside).unwrap()
                        > q.threshold;
                assert!(
                    singleton,
                    "lost coarse boundary {label}: {}",
                    t.raw().value()
                );
                artifacts += 1;
            }
        }
        return artifacts;
    }
    for r in reference {
        if let Some((i, _)) = actual.iter().enumerate().find(|(i, a)| {
            !used[*i]
                && (a.start.raw().value() - r.start.raw().value()).abs() * 86400.0 <= 0.533
                && (a.end.raw().value() - r.end.raw().value()).abs() * 86400.0 <= 0.533
        }) {
            used[i] = true;
            continue;
        }
        // The legacy scanner places a midpoint into a sign-transition bracket.
        // At twilight it can manufacture a positive-duration period from an
        // accepted singleton endpoint. Check the actual model, not just length.
        let at_start = windows.iter().any(|w| w.start == r.start);
        let at_end = windows.iter().any(|w| w.end == r.end);
        let short = r.length().value() * 86400.0 <= 0.432001;
        let boundary = if at_start { r.start } else { r.end };
        let endpoint_below =
            threshold::evaluate_integrated(evaluator, &p, boundary).unwrap() <= q.threshold;
        let interior_above = [0.1, 0.25, 0.5, 0.75, 0.9].into_iter().all(|u| {
            let t = ModifiedJulianDate::new(r.start.raw().value() + r.length().value() * u);
            threshold::evaluate_integrated(evaluator, &p, t).unwrap() > q.threshold
        });
        if short && (at_start || at_end) && endpoint_below && interior_above {
            artifacts += 1;
            continue;
        }
        panic!("unmatched reference period {label}: start={}, end={}, duration_s={}, at_start={at_start}, at_end={at_end}, endpoint_below={endpoint_below}, interior_above={interior_above}",r.start.raw().value(),r.end.raw().value(),r.length().value()*86400.0);
    }
    assert!(
        allow_new_events || used.iter().all(|x| *x),
        "additional adaptive periods need investigation: {label}"
    );
    artifacts
}

#[allow(clippy::too_many_arguments)]
fn measure(
    label: &str,
    north: bool,
    moon: MoonlightModel,
    days: i64,
    bits: u8,
    targets: usize,
    threshold_value: f64,
    start_label: &str,
) {
    let filter = std::env::var("NSB_DISCOVERY_FILTER").unwrap_or_default();
    if !label.contains(&filter) {
        return;
    }
    let evaluator = super::adaptive_bench::evaluator_with_moon(north, bits & 2 != 0, moon);
    let observer = if north {
        siderust::coordinates::centers::Geodetic::new_raw(
            Degrees::new(-17.892),
            Degrees::new(28.762),
            Meters::new(2200.0),
        )
    } else {
        siderust::coordinates::centers::Geodetic::new_raw(
            Degrees::new(-70.31634444444444),
            Degrees::new(-24.683427777777776),
            Meters::new(2184.6),
        )
    };
    let start = Time::<UTC>::from_chrono(
        start_label
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap(),
    );
    let seed = ThresholdQuery::new(
        observer,
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        Period::new(start, start + Seconds::new(days as f64 * 86400.0)),
        Radiance::new(threshold_value),
    )
    .with_components(ComponentMask::from_bits(bits).unwrap())
    .with_target_altitude_floor(Some(Degrees::new(20.0)));
    let prep = Instant::now();
    let context = evaluator.prepare_site_window_context(&seed).unwrap();
    let preparation_seconds = prep.elapsed().as_secs_f64();
    let mut queries: Vec<_> = (0..targets)
        .map(|i| {
            let mut q = seed.clone();
            if i > 0 {
                q.target = Target::new(
                    (83.6331 + i as f64 * 137.507764).rem_euclid(360.0) * DEG,
                    (-60.0 + (i % 25) as f64 * 5.0) * DEG,
                );
            }
            q
        })
        .collect();
    if bits & ComponentMask::STARLIGHT.bits() != 0 && bits != ComponentMask::STARLIGHT.bits() {
        for q in &mut queries {
            let p = prepare::prepare_target_threshold(&evaluator, &context, q).unwrap();
            q.threshold += p.starlight_integrated;
        }
    }
    let repeats: usize = std::env::var("NSB_DISCOVERY_REPEATS")
        .ok()
        .map(|s| s.parse().unwrap())
        .unwrap_or(3);
    assert!(repeats >= 3);
    let mut old_times = Vec::new();
    let mut new_times = Vec::new();
    let mut old = run(&evaluator, &context, &queries, false, 600.0);
    let mut new = run(&evaluator, &context, &queries, true, 600.0);
    for i in 0..repeats {
        for adaptive in if i % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            if adaptive {
                new = run(&evaluator, &context, &queries, true, 600.0);
                new_times.push(new.0);
            } else {
                old = run(&evaluator, &context, &queries, false, 600.0);
                old_times.push(old.0);
            }
        }
    }
    let fine_step = if targets >= 10 { 120.0 } else { 30.0 };
    let fine = run(&evaluator, &context, &queries, false, fine_step);
    let mut coarse_disagreements = 0;
    let mut fine_artifacts = 0;
    let mut coarse_artifacts = 0;
    for (i, query) in queries.iter().enumerate() {
        fine_artifacts += compare(
            &evaluator,
            &context,
            query,
            &new.2[i],
            &fine.2[i],
            &format!("{label}, target {i}, fine"),
            false,
        );
        coarse_artifacts += compare(
            &evaluator,
            &context,
            query,
            &new.2[i],
            &old.2[i],
            &format!("{label}, target {i}, coarse"),
            true,
        );
        coarse_disagreements += usize::from(new.2[i].len() != old.2[i].len());
    }
    let mut report = serde_json::json!({
        "scenario":label,"site":if north {"CTAO-N"}else{"CTAO-S"},"moonlight_model":moon.as_str(),"days":days,"components_bits":bits,"targets":targets,"threshold":threshold_value,"start":start_label,"repeats":repeats,
        "preparation_seconds":preparation_seconds,"scan_seconds":old_times,"adaptive_seconds":new_times,"reference_seconds":fine.0,"reference_step_seconds":fine_step,
        "scan_exact":old.1.exact,"adaptive_exact":new.1.exact,"reference_exact":fine.1.exact,"bounds":new.1.bounds,"fits":new.1.fits,"subdivisions":new.1.subdivisions,"root_refinements":new.1.refinements,"extrema":new.1.extrema,"certified_intervals":new.1.certified,"peak_samples_per_worker":new.1.peak_samples,
        "fallbacks":0,"unresolved_seconds":0,"excluded_root_bracket_seconds":new.1.bracket_seconds,"complete_usable":true,"coarse_count_disagreements":coarse_disagreements,"fine_count_disagreements":0,"reference_boundary_tolerance_seconds":0.533,
    "accepted_periods":new.2.iter().map(Vec::len).sum::<usize>(),"fine_singleton_jump_artifacts":fine_artifacts,"coarse_singleton_jump_artifacts":coarse_artifacts
    });
    let extra = serde_json::json!({
            "adaptive_end_to_end_seconds":new_times.iter().map(|t|t+preparation_seconds).collect::<Vec<_>>(),
            "scan_end_to_end_seconds":old_times.iter().map(|t|t+preparation_seconds).collect::<Vec<_>>(),
            "peak_pending_intervals":new.1.peak_pending,"peak_recursion_depth":new.1.peak_depth,
            "peak_sample_cache_bytes_per_worker":new.1.peak_samples * std::mem::size_of::<(f64,f64)>(),
            "process_peak_rss_kib":std::fs::read_to_string("/proc/self/status").ok().and_then(|s|s.lines().find(|l|l.starts_with("VmHWM:")).map(str::to_owned)),
            "budget_exhaustions":0,
            "accepted_duration_seconds":new.2.iter().flatten().map(|w|w.length().value()*86400.0).sum::<f64>(),
            "scan_duration_seconds":old.2.iter().flatten().map(|w|w.length().value()*86400.0).sum::<f64>(),
            "reference_duration_seconds":fine.2.iter().flatten().map(|w|w.length().value()*86400.0).sum::<f64>(),
    });
    report
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    println!("DISCOVERY_BENCH {report}");
}

#[test]
#[ignore = "release complete adaptive schedules, scan600 and fine reference"]
fn complete_adaptive_schedule_matrix() {
    for north in [false, true] {
        for moon in [
            MoonlightModel::Jones2013Spectral,
            MoonlightModel::KrisciunasSchaefer1991,
        ] {
            let prefix = format!(
                "{}/{}",
                if north { "north" } else { "south" },
                moon.as_str()
            );
            for days in [1, 7, 30, 365] {
                for (name, bits) in [
                    ("starlight", 2),
                    ("zodiacal", 1),
                    ("airglow", 4),
                    ("moon", 8),
                    ("ALL", ComponentMask::ALL.bits()),
                    ("four", 15),
                ] {
                    measure(
                        &format!("{prefix}/{name}/{days}d/1"),
                        north,
                        moon,
                        days,
                        bits,
                        1,
                        0.25,
                        "2026-01-01T00:00:00Z",
                    );
                }
            }
            for bits in 0..=15 {
                measure(
                    &format!("{prefix}/masks/{bits}/7d"),
                    north,
                    moon,
                    7,
                    bits,
                    1,
                    0.25,
                    "2026-01-03T00:00:00Z",
                );
            }
            for days in [1, 7, 30, 365] {
                for targets in [10, 100] {
                    measure(
                        &format!("{prefix}/batch/{days}d/{targets}"),
                        north,
                        moon,
                        days,
                        ComponentMask::ALL.bits(),
                        targets,
                        0.25,
                        "2026-01-01T00:00:00Z",
                    );
                }
            }
            for (name, value, start) in [
                ("far", 1e6, "2026-01-03T00:00:00Z"),
                ("near", 0.13, "2026-01-03T00:00:00Z"),
                ("moon-down", 0.25, "2026-01-14T00:00:00Z"),
            ] {
                measure(
                    &format!("{prefix}/{name}"),
                    north,
                    moon,
                    7,
                    ComponentMask::ALL.bits(),
                    1,
                    value,
                    start,
                );
            }
        }
    }
}

/// Run unchanged before/after the dependency update. Values and boundaries use
/// the scanner in both runs, isolating astronomy/time changes from discovery.
#[test]
#[ignore = "dependency-upgrade scientific reference export"]
fn dependency_upgrade_reference() {
    for north in [false, true] {
        for moon in [
            MoonlightModel::Jones2013Spectral,
            MoonlightModel::KrisciunasSchaefer1991,
        ] {
            let evaluator = super::adaptive_bench::evaluator_with_moon(north, true, moon);
            let observer = siderust::coordinates::centers::Geodetic::new_raw(
                Degrees::new(if north { -17.892 } else { -70.31634444444444 }),
                Degrees::new(if north { 28.762 } else { -24.683427777777776 }),
                Meters::new(if north { 2200.0 } else { 2184.6 }),
            );
            for bits in 0..=15 {
                let start = Time::<UTC>::from_chrono(
                    "2026-01-03T00:00:00Z"
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .unwrap(),
                );
                let q = ThresholdQuery::new(
                    observer,
                    Target::new(83.6331 * DEG, 22.0145 * DEG),
                    Period::new(start, start + Seconds::new(7.0 * 86400.0)),
                    Radiance::new(0.25),
                )
                .with_components(ComponentMask::from_bits(bits).unwrap())
                .with_target_altitude_floor(Some(Degrees::new(20.0)));
                let context = evaluator.prepare_site_window_context(&q).unwrap();
                let p = prepare::prepare_target_threshold(&evaluator, &context, &q).unwrap();
                let windows = filters::smooth_threshold_windows(&p);
                let values: Vec<_> = windows
                    .iter()
                    .flat_map(|w| {
                        [0.0, 0.25, 0.5, 0.75, 1.0].map(|u| {
                            let t = ModifiedJulianDate::new(
                                w.start.raw().value() + w.length().value() * u,
                            );
                            (
                                t.raw().value(),
                                threshold::evaluate_integrated(&evaluator, &p, t)
                                    .unwrap()
                                    .value(),
                            )
                        })
                    })
                    .collect();
                let result = evaluator
                    .periods_below_threshold_with_context(&context, &q)
                    .unwrap();
                println!(
                    "DEPENDENCY_REFERENCE {}",
                    serde_json::json!({
                        "site":if north {"N"} else {"S"},"moon":moon.as_str(),"bits":bits,
                        "windows":windows.iter().map(|w| [w.start.raw().value(),w.end.raw().value()]).collect::<Vec<_>>(),
                        "values":values,
                        "periods":result.periods.iter().map(|w| [w.start.to_chrono().unwrap().to_rfc3339(),w.end.to_chrono().unwrap().to_rfc3339()]).collect::<Vec<_>>()
                    })
                );
            }
        }
    }
    for label in [
        "2016-12-31T23:59:59Z",
        "2017-01-01T00:00:00Z",
        "2026-10-12T12:34:56.123456Z",
    ] {
        let t = Time::<UTC>::from_chrono(label.parse::<chrono::DateTime<chrono::Utc>>().unwrap());
        let tt = scan::utc_time_to_tt_mjd(t);
        println!(
            "DEPENDENCY_TIME {}",
            serde_json::json!({"utc":label,"tt_mjd":tt.raw().value(),"roundtrip":scan::tt_mjd_to_utc_time(tt).to_chrono().unwrap().to_rfc3339()})
        );
    }
}
