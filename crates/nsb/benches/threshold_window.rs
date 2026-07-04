//! Criterion benchmarks for representative production and experimental paths.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};
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

fn high_arctic() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        siderust::qtty::Degrees::new(0.0),
        siderust::qtty::Degrees::new(89.0),
        siderust::qtty::Meters::new(0.0),
    )
}

fn target_sgr_a() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn north_pole_target() -> Target {
    Target::new(0.0 * DEG, 89.0 * DEG)
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
    let cases = [
        window_case(
            "1d",
            "2023-09-04T00:00:00Z",
            1,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
        ),
        window_case(
            "1w",
            "2023-09-04T00:00:00Z",
            7,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
        ),
        window_case(
            "1mo",
            "2023-09-04T00:00:00Z",
            30,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
        ),
        window_case(
            "moon_low",
            "2023-09-15T00:00:00Z",
            3,
            paranal(),
            target_sgr_a(),
            ComponentMask::MOON,
        ),
        window_case(
            "moon_up_bright",
            "2023-09-29T00:00:00Z",
            3,
            paranal(),
            target_sgr_a(),
            ComponentMask::MOON,
        ),
        window_case(
            "target_never_visible",
            "2023-09-04T00:00:00Z",
            7,
            paranal(),
            north_pole_target(),
            ComponentMask::ALL,
        ),
        window_case(
            "long_astronomical_night",
            "2023-12-01T00:00:00Z",
            30,
            high_arctic(),
            north_pole_target(),
            ComponentMask::AIRGLOW,
        ),
    ];
    let mut group = c.benchmark_group("threshold_window");
    group.sample_size(10);

    for case in cases {
        group.throughput(Throughput::Elements(case.days as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(case.label),
            &case.query,
            |b, query| {
                b.iter(|| evaluator.periods_below_threshold(query).expect("window"));
            },
        );
    }
    group.finish();
}

struct WindowBenchCase {
    label: &'static str,
    days: i64,
    query: ThresholdQuery,
}

fn window_case(
    label: &'static str,
    start: &str,
    days: i64,
    observer: Geodetic<ECEF>,
    target: Target,
    components: ComponentMask,
) -> WindowBenchCase {
    let start = parse(start);
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(days));
    WindowBenchCase {
        label,
        days,
        query: ThresholdQuery {
            observer,
            target,
            window: Period::new(start, end),
            threshold: BandPhotonRadiance::new(0.21),
            components,
            sample_step: Second::new(600.0),
            sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
            target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
        },
    }
}

fn smoke_test() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::hours(2));
    let query = ThresholdQuery {
        observer: paranal(),
        target: target_sgr_a(),
        window: Period::new(start, end),
        threshold: BandPhotonRadiance::new(0.21),
        components: ComponentMask::ALL,
        sample_step: Second::new(3_600.0),
        sun_altitude_ceiling: Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING),
        target_altitude_floor: Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR),
    };
    evaluator
        .periods_below_threshold(&query)
        .expect("window smoke");
}

fn main() {
    if std::env::args().any(|arg| arg == "--bench") {
        benches();
        Criterion::default().configure_from_args().final_summary();
    } else {
        smoke_test();
    }
}

criterion_group!(benches, bench_point_components, bench_window);
