//! Criterion benchmarks for representative production and experimental paths.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};
use nsb::{
    ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightMap, StarlightProduct,
    StarlightProvenance, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use std::hint::black_box;
use tempoch::{Period, Time, UTC};

const HEALPIX_FIXTURE: &str = r#"# map_type=healpix
# nside=1
# ordering=ring
# coordinate_frame=galactic
# s10_diagnostics=not_provided
# dataset_name=NSB synthetic bench-only HEALPix starlight fixture
# version=fixture
# generation_date_utc=2026-06-21T00:00:00Z
# source_catalogue=synthetic bench fixture
# source_catalogue_release=test
# source_catalogue_license=test-only
# source_catalogue_checksum=sha256:fixture
# magnitude_limit=test-only
# map_resolution=HEALPix nside=1 ring 12 pixels
# photometry_model=fixture
# band_definition=integrated 300-650 nm photon radiance
# smoothing_fwhm_deg=none
# generated_by=bench
healpix_index,integrated_ph_cm2_ns_sr,statistical_uncertainty_ph_cm2_ns_sr,systematic_uncertainty_ph_cm2_ns_sr,total_uncertainty_ph_cm2_ns_sr
0,1.0,0.1,0.2,0.25
1,2.0,0.2,0.4,0.5
2,3.0,0.3,0.6,0.75
3,4.0,0.4,0.8,1.0
4,5.0,0.5,1.0,1.25
5,6.0,0.6,1.2,1.5
6,7.0,0.7,1.4,1.75
7,8.0,0.8,1.6,2.0
8,9.0,0.9,1.8,2.25
9,10.0,1.0,2.0,2.5
10,11.0,1.1,2.2,2.75
11,12.0,1.2,2.4,3.0
"#;

fn parse(s: &str) -> Time<UTC> {
    let dt = DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}

fn paranal() -> Geodetic<ECEF> {
    observatories::EL_PARANAL.geodetic()
}

fn cta_south() -> Geodetic<ECEF> {
    Geodetic::new_raw(
        siderust::qtty::Degrees::new(-70.316_344_444_444_44),
        siderust::qtty::Degrees::new(-24.683_427_777_777_776),
        siderust::qtty::Meters::new(2_184.6),
    )
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
    PointQuery::new(paranal(), parse("2023-09-04T01:48:00Z"), target_sgr_a())
        .with_components(components)
}

fn experimental_starlight_product() -> StarlightProduct {
    let map =
        StarlightMap::from_csv_str(HEALPIX_FIXTURE, StarlightProvenance::test_fixture()).unwrap();
    StarlightProduct::with_experimental_map(map)
}

fn bench_point_components(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let experimental = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_starlight_product(experimental_starlight_product()),
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

fn bench_window_duration_components(c: &mut Criterion) {
    let evaluator = NsbEvaluator::with_config(NsbModelConfig::cta_s_planning()).expect("evaluator");
    let durations = [("1d", 1), ("1w", 7), ("1mo", 30), ("1y", 365)];
    let components = [
        ("all", ComponentMask::ALL),
        ("airglow", ComponentMask::AIRGLOW),
        ("moon", ComponentMask::MOON),
        ("zodiacal", ComponentMask::ZODIACAL),
    ];
    let mut group = c.benchmark_group("threshold_window_duration_component");
    group.sample_size(10);

    for (component_name, component_mask) in components {
        for (duration_name, days) in durations {
            let case = window_case(
                duration_name,
                "2026-01-01T00:00:00Z",
                days,
                cta_south(),
                Target::new(83.6331 * DEG, 22.0145 * DEG),
                component_mask,
                0.25,
            );
            group.throughput(Throughput::Elements(days as u64));
            group.bench_with_input(
                BenchmarkId::new(component_name, duration_name),
                &case.query,
                |b, query| {
                    b.iter(|| evaluator.periods_below_threshold(query).expect("window"));
                },
            );
        }
    }
    group.finish();
}

fn bench_physical_regimes(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let cases = [
        window_case(
            "moon_down",
            "2023-09-15T00:00:00Z",
            3,
            paranal(),
            target_sgr_a(),
            ComponentMask::MOON,
            0.21,
        ),
        window_case(
            "bright_moon",
            "2023-09-29T00:00:00Z",
            3,
            paranal(),
            target_sgr_a(),
            ComponentMask::MOON,
            0.21,
        ),
        window_case(
            "target_never_visible",
            "2023-09-04T00:00:00Z",
            7,
            paranal(),
            north_pole_target(),
            ComponentMask::ALL,
            0.21,
        ),
        window_case(
            "long_astronomical_night",
            "2023-12-01T00:00:00Z",
            30,
            high_arctic(),
            north_pole_target(),
            ComponentMask::AIRGLOW,
            0.21,
        ),
        window_case(
            "ordinary_target",
            "2026-01-01T00:00:00Z",
            30,
            cta_south(),
            Target::new(83.6331 * DEG, 22.0145 * DEG),
            ComponentMask::ALL,
            0.25,
        ),
        window_case(
            "near_threshold_crossing",
            "2026-01-01T00:00:00Z",
            30,
            cta_south(),
            Target::new(83.6331 * DEG, 22.0145 * DEG),
            ComponentMask::ALL,
            0.205,
        ),
    ];
    let mut group = c.benchmark_group("threshold_window_physical_regime");
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

fn bench_regression_workloads(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let mut cases = [
        window_case(
            "1d",
            "2023-09-04T00:00:00Z",
            1,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
            0.21,
        ),
        window_case(
            "1w",
            "2023-09-04T00:00:00Z",
            7,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
            0.21,
        ),
        window_case(
            "1mo",
            "2023-09-04T00:00:00Z",
            30,
            paranal(),
            target_sgr_a(),
            ComponentMask::ALL,
            0.21,
        ),
    ];
    for case in &mut cases {
        case.query.target_altitude_floor = Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR);
    }

    let mut group = c.benchmark_group("threshold_window_regression");
    group.sample_size(10);
    for case in cases {
        group.throughput(Throughput::Elements(case.days as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(case.label),
            &case.query,
            |b, query| b.iter(|| evaluator.periods_below_threshold(query).expect("window")),
        );
    }
    group.finish();
}

fn bench_site_context_and_multi_target(c: &mut Criterion) {
    let evaluator = NsbEvaluator::with_config(NsbModelConfig::cta_s_planning()).expect("evaluator");
    let seed = window_case(
        "seed",
        "2026-01-01T00:00:00Z",
        365,
        cta_south(),
        Target::new(83.6331 * DEG, 22.0145 * DEG),
        ComponentMask::ALL,
        0.25,
    )
    .query;

    let mut preparation = c.benchmark_group("site_window_context_preparation");
    preparation.sample_size(10);
    preparation.bench_function("all/1y", |b| {
        b.iter(|| {
            evaluator
                .prepare_site_window_context(black_box(&seed))
                .expect("context")
        });
    });
    preparation.finish();

    let context = evaluator
        .prepare_site_window_context(&seed)
        .expect("shared site/year context");
    let queries: Vec<_> = (0..100)
        .map(|index| {
            let ra = (83.6331 + index as f64 * 137.507_764).rem_euclid(360.0);
            let dec = -60.0 + (index % 25) as f64 * 5.0;
            ThresholdQuery::new(
                seed.observer,
                Target::new(ra * DEG, dec * DEG),
                seed.window,
                seed.threshold,
            )
            .with_components(seed.components)
            .with_sample_step(seed.sample_step)
            .with_sun_altitude_ceiling(seed.sun_altitude_ceiling)
            .with_target_altitude_floor(seed.target_altitude_floor)
        })
        .collect();

    // Evaluator and site/year preparation are deliberately excluded here.
    // Each iteration includes target visibility, target-static starlight, and
    // authoritative threshold searches while reusing site/Moon/Sun state.
    let mut group = c.benchmark_group("multi_target_reused_site_year");
    group.sample_size(10);
    for count in [1_usize, 10, 100] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &count| {
            b.iter(|| {
                queries[..count]
                    .iter()
                    .map(|query| {
                        evaluator
                            .periods_below_threshold_with_context(&context, black_box(query))
                            .expect("window")
                    })
                    .collect::<Vec<_>>()
            });
        });
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
    threshold: f64,
) -> WindowBenchCase {
    let start = parse(start);
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(days));
    WindowBenchCase {
        label,
        days,
        query: ThresholdQuery::new(
            observer,
            target,
            Period::new(start, end),
            BandPhotonRadiance::new(threshold),
        )
        .with_components(components)
        .with_sample_step(Second::new(600.0))
        .with_sun_altitude_ceiling(Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING))
        .with_target_altitude_floor(Some(siderust::qtty::Degrees::new(20.0))),
    }
}

fn smoke_test() {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let start = parse("2023-09-04T00:00:00Z");
    let end = Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::hours(2));
    let query = ThresholdQuery::new(
        paranal(),
        target_sgr_a(),
        Period::new(start, end),
        BandPhotonRadiance::new(0.21),
    )
    .with_components(ComponentMask::ALL)
    .with_sample_step(Second::new(3_600.0))
    .with_sun_altitude_ceiling(Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING))
    .with_target_altitude_floor(Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR));
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

criterion_group!(
    benches,
    bench_point_components,
    bench_window_duration_components,
    bench_physical_regimes,
    bench_regression_workloads,
    bench_site_context_and_multi_target
);
