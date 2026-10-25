// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Runnable, offline evaluator workflow and serial steady-state measurements.
use nsb::components::starlight::StarlightMap;
use nsb::transport::multiple::starlight::ProjectedSettings;
use nsb::transport::multiple::MultipleScatteringSettings;
use nsb::transport::single::starlight::{
    HealpixSettings, StarlightGeometry, WavelengthSettings, WithinBinSpectrum,
};
use nsb::transport::single::AerosolPhase;
use nsb::units::qtty::{Albedos, Degrees, Meters, Nanometers, Quantity};
use nsb::{
    NsbEvaluator, NsbModelConfig, Observer, SpectralStarlightMode as Mode,
    SpectralStarlightResult as Output, StarlightProduct,
};
use siderust::atmosphere::{AtmosphericOptics, LayeredAtmosphere};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use tempoch::JulianDate;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    // For an external product use from_json_file(path, expected_sha256).
    // Obtain the expected hash from trusted product metadata, independently of loading.
    let map = StarlightMap::from_json_bytes(
        include_bytes!("../tests/data/starlight-transport/uniform.json"),
        "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f",
    )?;
    eprintln!("asset_load_seconds={:.9}", start.elapsed().as_secs_f64());
    let start = std::time::Instant::now();
    let evaluator = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_starlight_product(StarlightProduct::with_experimental_map(map)),
    )?;
    eprintln!(
        "evaluator_construction_seconds={:.9}",
        start.elapsed().as_secs_f64()
    );
    let geometry = StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.),
        ),
        // Explicit caller-declared time scales for the same physical instant.
        tt: JulianDate::new(2459680.8),
        ut1: JulianDate::new(2459680.7992),
    };
    let view = Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.));
    let band = [360., 390.].map(Nanometers::new);
    // Model column heights are independently declared. No geoid conversion
    // between this atmospheric datum and the ellipsoidal site height is implied.
    let atmosphere = LayeredAtmosphere::try_new(
        &[Meters::new(2000.), Meters::new(3000.)],
        [360., 570.].map(Nanometers::new),
        vec![AtmosphericOptics::from_extinction(
            Quantity::new(0.08 / 1000.),
            Quantity::new(0.),
            Albedos::new(0.),
            Quantity::new(0.08 / 1000.),
        )?],
    )?;
    let phase = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.)?);
    let assumption = WithinBinSpectrum::FlatPhotonDensity;
    let wavelength = WavelengthSettings {
        refinements: 1,
        ..WavelengthSettings::default()
    };
    let angular = HealpixSettings {
        subdivision: 8,
        refinements: 4,
        ..HealpixSettings::default()
    };
    let settings = ProjectedSettings {
        solver: MultipleScatteringSettings {
            mu_nodes: 8,
            azimuth_nodes: 32,
            cells_per_layer: 2,
            max_orders: 32,
            angular_tolerance: 0.05,
            vertical_tolerance: 0.05,
            ..MultipleScatteringSettings::default()
        },
        boundary: angular,
        wavelength,
        kernel_mu: 128,
        kernel_azimuth: 64,
    };
    for (name, mode) in [
        ("identity", Mode::Identity),
        (
            "direct",
            Mode::Direct {
                atmosphere: &atmosphere,
                assumption,
                wavelength,
            },
        ),
        (
            "single",
            Mode::Single {
                atmosphere: &atmosphere,
                phase: &phase,
                assumption,
                angular,
                wavelength,
            },
        ),
        (
            "multiple",
            Mode::Multiple {
                atmosphere: &atmosphere,
                phase: &phase,
                assumption,
                settings,
            },
        ),
    ] {
        let start = std::time::Instant::now();
        let r = evaluator.evaluate_spectral_starlight(geometry, view, band, mode)?;
        let elapsed = start.elapsed().as_secs_f64();
        let (work, workspace) = match &r {
            Output::Direct(r) => (r.planned_work, None),
            Output::Single(r) => (r.planned_work, None),
            Output::Multiple(r) => (r.transport.planned_work, Some(r.workspace_bytes)),
            _ => (0, None),
        };
        println!(
            "{}",
            serde_json::json!({"mode":name,"units":"photon/(cm2 ns sr)","toa":r.toa().value(),
            "direct":r.direct().map(|x|x.value()),"single":r.single().map(|x|x.value()),"higher":r.higher_orders().map(|x|x.value()),
            "scattered":r.total_scattered().map(|x|x.value()),"ground_total":r.ground_total().map(|x|x.value()),
            "physical_uncertainty":null,"elapsed_seconds":elapsed,"planned_work":work,"workspace_bound_bytes":workspace})
        );
    }
    let unsupported = evaluator.evaluate_spectral_starlight(
        geometry,
        view,
        [360., 375.].map(Nanometers::new),
        Mode::Identity,
    );
    assert!(unsupported.is_err());
    eprintln!(
        "expected partial-bin rejection: {}",
        unsupported.unwrap_err()
    );
    Ok(())
}
