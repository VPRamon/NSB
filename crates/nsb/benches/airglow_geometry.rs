//! Criterion benchmarks for Airglow geometry and end-to-end evaluation.

use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, Criterion};
use nsb::{
    Airglow, AirglowGeometryModel, AirglowWavelengthApplicability, ValidatedZenithDomain,
    VerticalEmissionProfile, VerticalEmissionProfileDefinition, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::coordinates::spherical::Direction;
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use tempoch::{Time, UTC};

fn profile() -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: "benchmark-synthetic-broad".into(),
        altitude_km: [70.0, 78.0, 86.0, 94.0, 103.0, 115.0]
            .into_iter()
            .map(Kilometers::new)
            .collect(),
        relative_emissivity: vec![0.0, 0.25, 0.8, 1.0, 0.4, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic-300-650-nm".into(),
        },
        assumptions: "synthetic benchmark shape; not observational data".into(),
        provenance: "NSB Airglow geometry benchmark".into(),
        license: "CC0-1.0 synthetic fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .expect("static benchmark profile")
}

fn time() -> Time<UTC> {
    Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
}

fn target() -> Direction<nsb::EquatorialMeanJ2000> {
    Direction::<nsb::EquatorialMeanJ2000>::new(Degrees::new(80.0), Degrees::new(-20.0))
}

fn bench_geometry(c: &mut Criterion) {
    let observer: Geodetic<ECEF> = observatories::EL_PARANAL.geodetic();
    let zenith = Degrees::new(85.0);
    let van = AirglowGeometryModel::default();
    let vertical_profile = profile();
    let vertical = AirglowGeometryModel::VerticalProfile(vertical_profile.clone());

    let mut group = c.benchmark_group("airglow_geometry");
    group.bench_function("van_rhijn", |b| {
        b.iter(|| van.geometry_factor(observer, zenith).unwrap())
    });
    group.bench_function("vertical_profile_reference", |b| {
        b.iter(|| vertical.geometry_factor(observer, zenith).unwrap())
    });
    group.bench_function("vertical_profile_reference_128_substeps", |b| {
        b.iter(|| {
            vertical_profile
                .geometry_factor_with_substeps(observer, zenith, 128)
                .unwrap()
        })
    });
    group.finish();
}

fn bench_airglow_evaluation(c: &mut Criterion) {
    let observer: Geodetic<ECEF> = observatories::EL_PARANAL.geodetic();
    let van = Airglow::standard_clear_sky(observer).unwrap();
    let vertical = Airglow::standard_clear_sky(observer)
        .unwrap()
        .with_geometry(AirglowGeometryModel::VerticalProfile(profile()));

    let mut group = c.benchmark_group("airglow_evaluation");
    group.bench_function("default_van_rhijn", |b| {
        b.iter(|| van.compute(time(), target()).unwrap())
    });
    group.bench_function("vertical_profile", |b| {
        b.iter(|| vertical.compute(time(), target()).unwrap())
    });
    group.finish();
}

criterion_group!(benches, bench_geometry, bench_airglow_evaluation);
criterion_main!(benches);
