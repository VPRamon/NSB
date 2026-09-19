//! Criterion benchmarks for Airglow geometry and end-to-end evaluation.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, Criterion};
use nsb::components::airglow::{
    AirglowGeometryModel, AirglowWavelengthApplicability, ValidatedZenithDomain,
    VerticalEmissionProfile, VerticalEmissionProfileDefinition,
};
use nsb::{ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, Target, DEG};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use tempoch::{Time, UTC};

fn profile() -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(
        VerticalEmissionProfileDefinition::new(
            "benchmark-synthetic-broad",
            [70.0, 78.0, 86.0, 94.0, 103.0, 115.0]
                .into_iter()
                .map(Kilometers::new)
                .collect(),
            vec![0.0, 0.25, 0.8, 1.0, 0.4, 0.0],
            AirglowWavelengthApplicability::new(
                Nanometers::new(300.0),
                Nanometers::new(650.0),
                "synthetic-300-650-nm",
            ),
            ValidatedZenithDomain::new(Degrees::new(0.0), Degrees::new(90.0)),
        )
        .with_assumptions("synthetic benchmark shape; not observational data")
        .with_provenance("NSB Airglow geometry benchmark")
        .with_license("CC0-1.0 synthetic fixture"),
    )
    .expect("static benchmark profile")
}

fn time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target() -> Target {
    Target::new(80.0 * DEG, -20.0 * DEG)
}

fn bench_airglow_evaluation(c: &mut Criterion) {
    let observer: Geodetic<ECEF> = observatories::EL_PARANAL.geodetic();
    let van = NsbEvaluator::new().unwrap();
    let vertical = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_airglow_geometry(AirglowGeometryModel::VerticalProfile(profile())),
    )
    .unwrap();
    let query =
        || PointQuery::new(observer, time(), target()).with_components(ComponentMask::AIRGLOW);

    let mut group = c.benchmark_group("airglow_evaluation");
    group.bench_function("default_van_rhijn", |b| {
        b.iter(|| van.evaluate(&query()).unwrap())
    });
    group.bench_function("vertical_profile", |b| {
        b.iter(|| vertical.evaluate(&query()).unwrap())
    });
    group.finish();
}

criterion_group!(benches, bench_airglow_evaluation);
criterion_main!(benches);
