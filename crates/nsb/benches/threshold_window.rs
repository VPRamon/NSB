//! Criterion benchmarks for representative production and experimental paths.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use nsb::{
    ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightModel, Target,
    ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Period, Time, UTC};

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn target_sgr_a() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn point_query(components: ComponentMask) -> PointQuery {
    PointQuery {
        observer: paranal(),
        time: parse("2023-09-04T01:48:00Z"),
        target: target_sgr_a(),
        components,
    }
}

fn bench_point_components(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let experimental = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_starlight_model(StarlightModel::bundled_experimental_seed()),
    )
    .expect("experimental evaluator");
    let cases = [
        ("zodiacal", ComponentMask::ZODIACAL),
        ("airglow", ComponentMask::AIRGLOW),
        ("moonlight", ComponentMask::MOON),
        ("full_default", ComponentMask::ALL),
    ];

    let mut group = c.benchmark_group("point_evaluation");
    for (name, components) in cases {
        let query = point_query(components);
        group.bench_with_input(BenchmarkId::from_parameter(name), &query, |b, query| {
            b.iter(|| evaluator.evaluate(query).expect("evaluation"));
        });
    }
    let starlight = point_query(ComponentMask::STARLIGHT);
    group.bench_function("experimental_starlight_lookup", |b| {
        b.iter(|| experimental.evaluate(&starlight).expect("starlight"));
    });
    group.finish();
}

fn bench_window(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");
    let cases = [("1d", 1i64), ("1w", 7), ("1mo", 30)];
    let mut group = c.benchmark_group("threshold_window");
    group.sample_size(10);

    for (label, days) in cases {
        let end =
            Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(days));
        let query = ThresholdQuery {
            observer: paranal(),
            target: target_sgr_a(),
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(0.21),
            components: ComponentMask::ALL,
            sample_step: Second::new(600.0),
            sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
            target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
        };
        group.throughput(Throughput::Elements(days as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), &query, |b, query| {
            b.iter(|| evaluator.periods_below_threshold(query).expect("window"));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_point_components, bench_window);
criterion_main!(benches);
