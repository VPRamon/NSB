// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Deterministic representative fits, point/band and one-hour planning benchmarks.
use chrono::{DateTime, Utc};
use criterion::{criterion_group, criterion_main, Criterion};
use nsb::site::airglow_profile::{AirglowCalibrationProfile, AirglowCalibrationRequest};
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use nsb::{
    AirglowModel, ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, Target,
    ThresholdQuery,
};
use siderust::coordinates::centers::Geodetic;
use siderust::qtty::{Degrees, Meters, Nanometers};
use tempoch::{Period, Time, UTC};
fn benchmarks(c: &mut Criterion) {
    let req: AirglowCalibrationRequest =
        toml::from_str(include_str!("../examples/airglow-calibration/request.toml")).unwrap();
    let fit = AirglowCalibrationProfile::fit(req).unwrap();
    let raw = fit.to_toml_string().unwrap();
    let location = Geodetic::new_raw(
        Degrees::new(-70.4044),
        Degrees::new(-24.6275),
        Meters::new(2635.),
    );
    let time = Time::<UTC>::from_chrono(
        DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    let end = Time::<UTC>::from_chrono(time.to_chrono().unwrap() + chrono::Duration::hours(1));
    let target = Target::new(Degrees::new(266.41683), Degrees::new(-29.00781));
    let query = PointQuery::new(location, time, target).with_components(ComponentMask::AIRGLOW);
    let window = ThresholdQuery::new(location, target, Period::new(time, end), Radiance::new(0.2))
        .with_components(ComponentMask::AIRGLOW)
        .with_target_altitude_floor(Some(Degrees::new(30.)));
    let base = NsbModelConfig::generic_clear_sky().with_solar_radio_flux(SolarFluxUnits::new(100.));
    let configs = [
        ("continuum", base.clone()),
        (
            "oxygen",
            base.clone()
                .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1),
        ),
        (
            "registered",
            base.clone().with_airglow_calibration(fit.clone()).unwrap(),
        ),
        (
            "registered_oxygen",
            base.with_airglow_calibration(fit)
                .unwrap()
                .with_airglow_model(AirglowModel::ParanalPalaceOxygenV1),
        ),
    ];
    let mut g = c.benchmark_group("airglow_calibration");
    g.sample_size(10);
    g.bench_function("artifact_load_replay", |b| {
        b.iter(|| AirglowCalibrationProfile::from_toml_str(&raw).unwrap())
    });
    for (name, config) in configs {
        let evaluator = NsbEvaluator::with_config(config.clone()).unwrap();
        g.bench_function(format!("{name}_init"), |b| {
            b.iter(|| NsbEvaluator::with_config(config.clone()).unwrap())
        });
        g.bench_function(format!("{name}_point"), |b| {
            b.iter(|| evaluator.evaluate(&query).unwrap())
        });
        g.bench_function(format!("{name}_band"), |b| {
            b.iter(|| {
                evaluator
                    .integrate_band(&query, Nanometers::new(365.), Nanometers::new(650.))
                    .unwrap()
            })
        });
        g.bench_function(format!("{name}_window_1h"), |b| {
            b.iter(|| evaluator.periods_below_threshold(&window).unwrap())
        });
    }
    g.finish();
}
criterion_group!(benches, benchmarks);
criterion_main!(benches);
