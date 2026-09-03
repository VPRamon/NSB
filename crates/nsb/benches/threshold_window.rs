//! Criterion benchmarks for representative production and experimental paths.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, BenchmarkId, Criterion, Throughput};
use nsb::{
    ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightMap, StarlightModel,
    StarlightProvenance, Target, ThresholdQuery, DEG,
};
use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use qtty::Second;
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
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

fn experimental_starlight_model() -> StarlightModel {
    let map =
        StarlightMap::from_csv_str(HEALPIX_FIXTURE, StarlightProvenance::test_fixture()).unwrap();
    StarlightModel::with_experimental_map(map)
}

fn bench_point_components(c: &mut Criterion) {
    let evaluator = NsbEvaluator::new().expect("evaluator");
    let experimental = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_starlight_model(experimental_starlight_model()),
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
        query: ThresholdQuery::new(
            observer,
            target,
            Period::new(start, end),
            BandPhotonRadiance::new(0.21),
        )
        .with_components(components)
        .with_sample_step(Second::new(600.0))
        .with_sun_altitude_ceiling(Some(ThresholdQuery::DEFAULT_SUN_ALTITUDE_CEILING))
        .with_target_altitude_floor(Some(ThresholdQuery::DEFAULT_TARGET_ALTITUDE_FLOOR)),
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

criterion_group!(benches, bench_point_components, bench_window);
