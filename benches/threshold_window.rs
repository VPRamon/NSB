//! Criterion benches for `NsbEvaluator::periods_below_threshold`.
//!
//! Compares the legacy uniform-scan path against the optimized
//! event-driven pipeline across a range of window sizes.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nsb::{ComponentMask, Location, NsbEvaluator, PointQuery, Site, Target, ThresholdQuery, DEG};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use tempoch::{Period, Time, UTC};

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

fn target_sgr_a() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn default_components() -> ComponentMask {
    ComponentMask::ZODIACAL | ComponentMask::AIRGLOW
}

fn make_query(
    start: Time<UTC>,
    end: Time<UTC>,
    components: ComponentMask,
    legacy: bool,
) -> ThresholdQuery {
    ThresholdQuery {
        location: Location::NamedSite(Site::Paranal),
        target: target_sgr_a(),
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(0.21),
        components,
        sample_step: if legacy {
            Second::new(300.0)
        } else {
            ThresholdQuery::DEFAULT_SAMPLE_STEP
        },
        sun_altitude_ceiling: if legacy {
            None
        } else {
            Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING)
        },
        target_altitude_floor: if legacy {
            None
        } else {
            Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR)
        },
    }
}

fn bench_point_eval(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let query = PointQuery {
        location: Location::NamedSite(Site::Paranal),
        time: parse("2023-09-04T01:48:00Z"),
        target: target_sgr_a(),
        components: default_components(),
    };
    c.bench_function("point_eval", |b| {
        b.iter(|| evaluator.evaluate(&query).expect("eval"));
    });
}

fn bench_window(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");

    let cases = [
        ("1d", 1i64),
        ("1w", 7),
        ("1mo", 30),
        // 1-year case: only run for the optimized path by default — the
        // legacy uniform-scan path is too slow to bench routinely.
        ("1y_opt_only", 365),
    ];

    let mut group = c.benchmark_group("threshold_window");
    group.sample_size(10);
    for (label, days) in cases {
        let end_dt = start.to_chrono().unwrap() + chrono::Duration::days(days);
        let end = Time::<UTC>::from_chrono(end_dt);
        group.throughput(Throughput::Elements(days as u64));

        let q_opt = make_query(start, end, default_components(), false);
        group.bench_with_input(BenchmarkId::new("optimized", label), &q_opt, |b, q| {
            b.iter(|| evaluator.periods_below_threshold(q).expect("opt"));
        });

        if !label.starts_with("1y") {
            let q_legacy = make_query(start, end, default_components(), true);
            group.bench_with_input(BenchmarkId::new("legacy", label), &q_legacy, |b, q| {
                b.iter(|| evaluator.periods_below_threshold_legacy(q).expect("legacy"));
            });
        }
    }
    group.finish();
}

fn bench_window_with_moon(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(7));
    let components = ComponentMask::ZODIACAL | ComponentMask::AIRGLOW | ComponentMask::MOON;

    let mut group = c.benchmark_group("threshold_window_moon_1w");
    group.sample_size(10);

    let q_opt = make_query(start, end, components, false);
    group.bench_function("optimized", |b| {
        b.iter(|| evaluator.periods_below_threshold(&q_opt).expect("opt"));
    });
    let q_legacy = make_query(start, end, components, true);
    group.bench_function("legacy", |b| {
        b.iter(|| {
            evaluator
                .periods_below_threshold_legacy(&q_legacy)
                .expect("legacy")
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_point_eval,
    bench_window,
    bench_window_with_moon
);
criterion_main!(benches);
