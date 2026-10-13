// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Complete schedule benchmark and dependency reference exports.
//! Run the ignored matrix in release with two Rayon threads.
use super::{
    bounds::prepared_bounds,
    discovery::{self, Costs},
    filters, intervals as scan, prepare, threshold,
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
    // Each measured schedule starts with an empty astronomical cache.
    let mut fresh_context = context.clone();
    fresh_context.astronomy = std::sync::Arc::default();
    let context = &fresh_context;
    let mut costs = Costs::default();
    let mut output = Vec::new();
    for q in queries {
        let p = prepare::prepare_target_threshold(evaluator, context, q).unwrap();
        let windows = if adaptive {
            filters::adaptive_threshold_windows(&p).unwrap()
        } else {
            super::filters::threshold_windows(&p, true)
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
                        super::reference_scan::evaluate_integrated(evaluator, &p, t)
                    };
                    let above = super::reference_scan::above_threshold_periods(
                        w,
                        siderust::qtty::Days::new(step / 86400.0),
                        &f,
                        q.threshold,
                    )
                    .unwrap();
                    (
                        super::reference_scan::complement_periods(w, &above),
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
                scan::inward_tt_mjd_period_to_utc(period, p.tt_window, q.window).unwrap();
            } else {
                scan::tt_mjd_period_to_utc(period, p.tt_window, q.window);
            }
        }
        output.push(periods);
    }
    (started.elapsed().as_secs_f64(), costs, output)
}

#[derive(Default)]
pub(super) struct Comparison {
    pub singleton_artifacts: usize,
    pub fine_missed_excursions: usize,
    pub fine_missed_accepted_excursions: usize,
}

/// Independently resolve a newly discovered gap in an otherwise accepted
/// reference interval. A finer scan remains empirical, but must witness the
/// excursion and agree with BOTH adaptive boundaries at the original tolerance.
fn confirm_gap(
    evaluator: &crate::NsbEvaluator,
    prepared: &super::types::PreparedThresholdQuery,
    q: &ThresholdQuery,
    left: ModifiedJulianDate,
    right: ModifiedJulianDate,
) -> bool {
    let width = (right - left).value();
    if width <= 0.0 || width > 120.0 / 86400.0 {
        return false;
    }
    let padding = width.min(1.0 / 86400.0);
    let local: Interval<ModifiedJulianDate> = Interval::new(
        left - siderust::qtty::Days::new(padding),
        right + siderust::qtty::Days::new(padding),
    );
    let f = |t| threshold::evaluate_integrated(evaluator, prepared, t);
    if f(left + siderust::qtty::Days::new(width * 0.5)).unwrap() <= q.threshold {
        return false;
    }
    let step = (width / 100.0).clamp(0.000001 / 86400.0, 0.01 / 86400.0);
    let above = super::reference_scan::above_threshold_periods(
        local,
        siderust::qtty::Days::new(step),
        &f,
        q.threshold,
    )
    .unwrap();
    above.len() == 1
        && (above[0].start - left).value().abs() * 86400.0 <= 0.106
        && (above[0].end - right).value().abs() * 86400.0 <= 0.106
}

/// Confirm a short accepted island missed entirely by the discrete reference.
/// Preserve eligibility constraints while independently resolving both edges.
fn confirm_accepted_island(
    evaluator: &crate::NsbEvaluator,
    prepared: &super::types::PreparedThresholdQuery,
    q: &ThresholdQuery,
    island: Interval<ModifiedJulianDate>,
) -> bool {
    let seconds = island.length().value() * 86400.0;
    if seconds <= 0.0 || seconds > 120.0 {
        return false;
    }
    let f = |t| threshold::evaluate_integrated(evaluator, prepared, t);
    if [0.1, 0.5, 0.9]
        .into_iter()
        .any(|u| f(island.start + island.length() * u).unwrap() > q.threshold)
    {
        return false;
    }
    let local: Interval<ModifiedJulianDate> = Interval::new(
        island.start - Seconds::new(1.0).to::<siderust::qtty::Day>(),
        island.end + Seconds::new(1.0).to::<siderust::qtty::Day>(),
    );
    let step = Seconds::new((seconds / 1000.0).clamp(0.0001, 0.01)).to::<siderust::qtty::Day>();
    let mut accepted = Vec::new();
    for candidate in &prepared.candidate_windows {
        let w = Interval::new(
            local.start.max(candidate.start),
            local.end.min(candidate.end),
        );
        if w.start >= w.end {
            continue;
        }
        let above =
            super::reference_scan::above_threshold_periods(w, step, &f, q.threshold).unwrap();
        accepted.extend(super::reference_scan::complement_periods(w, &above));
    }
    scan::coalesce_periods(&mut accepted);
    accepted.len() == 1
        && (accepted[0].start - island.start).value().abs() * 86400.0 <= 0.106
        && (accepted[0].end - island.end).value().abs() * 86400.0 <= 0.106
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
) -> Comparison {
    let p = prepare::prepare_target_threshold(evaluator, context, q).unwrap();
    let windows = super::filters::threshold_windows(&p, true);
    let mut used = vec![false; actual.len()];
    let mut artifacts = 0;
    let mut fine_missed_excursions = 0;
    let mut fine_missed_accepted_excursions = 0;
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
        return Comparison {
            singleton_artifacts: artifacts,
            fine_missed_excursions,
            fine_missed_accepted_excursions,
        };
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
        // A reference scanner can bridge a real short rejected excursion.
        // Reconcile one-to-many topology only after an independent dense local
        // scan confirms every additional gap. Outer boundary tolerances remain
        // unchanged; no arbitrary period-count or duration filter is admitted.
        let pieces: Vec<_> = actual
            .iter()
            .enumerate()
            .filter(|(i, a)| !used[*i] && a.end > r.start && a.start < r.end)
            .collect();
        let mut confirmed = Vec::new();
        let compatible = if let (Some(first), Some(last)) = (pieces.first(), pieces.last()) {
            let start_close = (first.1.start - r.start).value().abs() * 86400.0 <= 0.533;
            let end_close = (last.1.end - r.end).value().abs() * 86400.0 <= 0.533;
            let start_resolved = start_close
                || (first.1.start > r.start
                    && windows.iter().any(|w| w.start == r.start)
                    && confirm_gap(evaluator, &p, q, r.start, first.1.start));
            let end_resolved = end_close
                || (last.1.end < r.end
                    && windows.iter().any(|w| w.end == r.end)
                    && confirm_gap(evaluator, &p, q, last.1.end, r.end));
            if !start_close && start_resolved {
                confirmed.push((r.start, first.1.start, "physical-start"));
            }
            for pair in pieces.windows(2) {
                confirmed.push((pair[0].1.end, pair[1].1.start, "interior"));
            }
            if !end_close && end_resolved {
                confirmed.push((last.1.end, r.end, "physical-end"));
            }
            start_resolved
                && end_resolved
                && !confirmed.is_empty()
                && pieces
                    .windows(2)
                    .all(|pair| confirm_gap(evaluator, &p, q, pair[0].1.end, pair[1].1.start))
        } else {
            false
        };
        if compatible {
            for &(left, right, kind) in &confirmed {
                println!(
                    "CONFIRMED_FINE_MISS {}",
                    serde_json::json!({"scenario":label,"kind":kind,"start_tt_mjd":left.raw().value(),"end_tt_mjd":right.raw().value(),"duration_seconds":(right-left).value()*86400.0,"local_reference_max_step_seconds":0.01,"boundary_allowance_seconds":0.106})
                );
            }
            fine_missed_excursions += confirmed.len();
            for (i, _) in pieces {
                used[i] = true;
            }
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
        eprintln!(
            "ADAPTIVE_PERIODS {label}: {:?}",
            actual
                .iter()
                .map(|w| [w.start.raw().value(), w.end.raw().value()])
                .collect::<Vec<_>>()
        );
        panic!("unmatched reference period {label}: start={}, end={}, duration_s={}, at_start={at_start}, at_end={at_end}, endpoint_below={endpoint_below}, interior_above={interior_above}",r.start.raw().value(),r.end.raw().value(),r.length().value()*86400.0);
    }
    for (_, island) in actual.iter().enumerate().filter(|(i, _)| !used[*i]) {
        assert!(
            reference
                .iter()
                .all(|r| island.end <= r.start || island.start >= r.end)
                && confirm_accepted_island(evaluator, &p, q, *island),
            "additional adaptive period needs investigation: {label}: {island:?}"
        );
        println!(
            "CONFIRMED_FINE_MISS {}",
            serde_json::json!({"scenario":label,"kind":"accepted-island","start_tt_mjd":island.start.raw().value(),"end_tt_mjd":island.end.raw().value(),"duration_seconds":island.length().value()*86400.0,"local_reference_max_step_seconds":0.01,"boundary_allowance_seconds":0.106})
        );
        fine_missed_accepted_excursions += 1;
    }
    Comparison {
        singleton_artifacts: artifacts,
        fine_missed_excursions,
        fine_missed_accepted_excursions,
    }
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
    let exclude = std::env::var("NSB_DISCOVERY_EXCLUDE").unwrap_or_default();
    if !label.contains(&filter) || (!exclude.is_empty() && label.contains(&exclude)) {
        return;
    }
    // Resume an expensive acceptance run without retiming already validated
    // scenarios. The default still executes the complete matrix.
    if let Ok(path) = std::env::var("NSB_DISCOVERY_COMPLETED") {
        let completed = std::fs::read_to_string(path).unwrap();
        if completed
            .lines()
            .filter_map(|line| {
                line.strip_prefix("DISCOVERY_BENCH ")
                    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            })
            .any(|row| row["scenario"].as_str() == Some(label) && row["complete_usable"] == true)
        {
            return;
        }
    }
    let evaluator = super::test_fixtures::evaluator_with_moon(north, bits & 2 != 0, moon);
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
    println!("DISCOVERY_PHASE {label}: scan warmup");
    let mut old = run(&evaluator, &context, &queries, false, 600.0);
    println!("DISCOVERY_PHASE {label}: adaptive warmup");
    let mut new = run(&evaluator, &context, &queries, true, 600.0);
    let fine_step = if targets >= 10 { 120.0 } else { 30.0 };
    println!("DISCOVERY_PHASE {label}: independent fine reference");
    let fine = run(&evaluator, &context, &queries, false, fine_step);
    let mut coarse_disagreements = 0;
    let mut fine_artifacts = 0;
    let mut coarse_artifacts = 0;
    let mut fine_missed_excursions = 0;
    let mut fine_missed_accepted_excursions = 0;
    let mut fine_raw_count_disagreements = 0;
    for (i, query) in queries.iter().enumerate() {
        let comparison = compare(
            &evaluator,
            &context,
            query,
            &new.2[i],
            &fine.2[i],
            &format!("{label}, target {i}, fine"),
            false,
        );
        fine_artifacts += comparison.singleton_artifacts;
        fine_missed_excursions += comparison.fine_missed_excursions;
        fine_missed_accepted_excursions += comparison.fine_missed_accepted_excursions;
        fine_raw_count_disagreements += usize::from(new.2[i].len() != fine.2[i].len());
        coarse_artifacts += compare(
            &evaluator,
            &context,
            query,
            &new.2[i],
            &old.2[i],
            &format!("{label}, target {i}, coarse"),
            true,
        )
        .singleton_artifacts;
        coarse_disagreements += usize::from(new.2[i].len() != old.2[i].len());
    }
    let validated_old = old.2.clone();
    let validated_new = new.2.clone();
    for i in 0..repeats {
        for adaptive in if i % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            if adaptive {
                println!("DISCOVERY_PHASE {label}: adaptive repetition {}", i + 1);
                new = run(&evaluator, &context, &queries, true, 600.0);
                assert_eq!(
                    new.2, validated_new,
                    "adaptive timed schedule changed: {label}"
                );
                new_times.push(new.0);
            } else {
                println!("DISCOVERY_PHASE {label}: scan repetition {}", i + 1);
                old = run(&evaluator, &context, &queries, false, 600.0);
                assert_eq!(old.2, validated_old, "scan timed schedule changed: {label}");
                old_times.push(old.0);
            }
        }
    }
    let mut report = serde_json::json!({
        "scenario":label,"site":if north {"CTAO-N"}else{"CTAO-S"},"moonlight_model":moon.as_str(),"days":days,"components_bits":bits,"targets":targets,"threshold":threshold_value,"start":start_label,"repeats":repeats,
        "preparation_seconds":preparation_seconds,"cold_cache_each_schedule":true,"scan_seconds":old_times,"adaptive_seconds":new_times,"reference_seconds":fine.0,"reference_step_seconds":fine_step,
        "scan_exact":old.1.exact,"adaptive_exact":new.1.exact,"reference_exact":fine.1.exact,"bounds":new.1.bounds,"fits":new.1.fits,"subdivisions":new.1.subdivisions,"root_refinements":new.1.refinements,"extrema":new.1.extrema,"certified_intervals":new.1.certified,"peak_samples_per_worker":new.1.peak_samples,
        "fallbacks":0,"unresolved_seconds":0,"excluded_root_bracket_seconds":new.1.bracket_seconds,"complete_usable":true,"coarse_count_disagreements":coarse_disagreements,"fine_count_disagreements":0,"reference_boundary_tolerance_seconds":0.533,
    "accepted_periods":new.2.iter().map(Vec::len).sum::<usize>(),"fine_singleton_jump_artifacts":fine_artifacts,"coarse_singleton_jump_artifacts":coarse_artifacts
    });
    let extra = serde_json::json!({
        "fine_missed_excursions":fine_missed_excursions,"fine_missed_accepted_excursions":fine_missed_accepted_excursions,"fine_raw_count_disagreements":fine_raw_count_disagreements,
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
            // Establish the largest required usable workload first. Timers
            // still warm up each engine independently within every scenario.
            measure(
                &format!("{prefix}/batch/365d/100"),
                north,
                moon,
                365,
                ComponentMask::ALL.bits(),
                100,
                0.25,
                "2026-01-01T00:00:00Z",
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
                    if days == 365 && targets == 100 {
                        continue;
                    }
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
