// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Repeated warm-evaluator restricted-band timing; no observed-sky comparison.
use nsb::units::length::Nanometers;
use nsb::{AirglowModel, ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, Target, DEG};
use std::hint::black_box;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let time = tempoch::Time::<tempoch::UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339("2023-09-15T05:00:00Z")?.with_timezone(&chrono::Utc),
    );
    let query = PointQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        time,
        Target::new(10.0 * DEG, -20.0 * DEG),
    )
    .with_components(ComponentMask::ZODIACAL | ComponentMask::AIRGLOW);
    let start = Instant::now();
    let evaluator = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky().with_airglow_model(AirglowModel::ParanalPalaceV1),
    )?;
    let construction_seconds = start.elapsed().as_secs_f64();
    for edges in [
        vec![360.0, 570.0],
        (0..=7).map(|i| 360.0 + f64::from(i) * 30.0).collect(),
    ] {
        // Warm the selected path. This is not a cold OS-page-cache benchmark.
        for edge in edges.windows(2) {
            black_box(evaluator.integrate_band_components(
                &query,
                Nanometers::new(edge[0]),
                Nanometers::new(edge[1]),
            )?);
        }
        for repetition in 0..10 {
            let start = Instant::now();
            let mut total = 0.0;
            for edge in edges.windows(2) {
                let result = evaluator.integrate_band_components(
                    &query,
                    Nanometers::new(edge[0]),
                    Nanometers::new(edge[1]),
                )?;
                total += black_box(result.integrated.value());
            }
            println!(
                "{}",
                serde_json::json!({
                    "schema": "nsb-restricted-band-timing-v1", "repetition": repetition,
                    "band_edges_nm": edges, "seconds": start.elapsed().as_secs_f64(),
                    "radiance_photons_cm2_ns_sr": total,
                    "evaluator_construction_seconds": construction_seconds,
                    "cache": "warm evaluator and OS pages; no solver LUT",
                    "components": ["zodiacal", "airglow-continuum"],
                    "observational_prediction": false,
                })
            );
        }
    }
    Ok(())
}
