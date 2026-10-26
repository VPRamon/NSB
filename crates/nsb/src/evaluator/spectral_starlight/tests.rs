// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Public workflow regression within the library as well as external consumers.
use super::*;
use crate::transport::multiple::MultipleScatteringSettings;
use crate::units::qtty::{Albedos, Degrees, Meters, Quantity};
use crate::{NsbModelConfig, Observer, StarlightProduct};
use siderust::atmosphere::AtmosphericOptics;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use tempoch::JulianDate;

#[test]
fn public_vacuum_workflow_preserves_the_component_availability_matrix() {
    let map = StarlightMap::from_json_bytes(
        include_bytes!("../../../tests/data/starlight-transport/uniform.json"),
        "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f",
    )
    .unwrap();
    let evaluator = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_starlight_product(StarlightProduct::with_experimental_map(map)),
    )
    .unwrap();
    let geometry = StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.),
        ),
        tt: JulianDate::new(2459680.8),
        ut1: JulianDate::new(2459680.7992),
    };
    let atmosphere = LayeredAtmosphere::try_new(
        &[Meters::new(2000.), Meters::new(3000.)],
        [360., 570.].map(Nanometers::new),
        vec![AtmosphericOptics::from_extinction(
            Quantity::new(0.),
            Quantity::new(0.),
            Albedos::new(0.),
            Quantity::new(0.),
        )
        .unwrap()],
    )
    .unwrap();
    let phase = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.).unwrap());
    let assumption = WithinBinSpectrum::FlatPhotonDensity;
    let wavelength = WavelengthSettings {
        refinements: 1,
        ..WavelengthSettings::default()
    };
    let settings = ProjectedSettings {
        solver: MultipleScatteringSettings {
            mu_nodes: 2,
            azimuth_nodes: 4,
            cells_per_layer: 2,
            max_orders: 16,
            angular_tolerance: 0.05,
            vertical_tolerance: 0.05,
            ..MultipleScatteringSettings::default()
        },
        boundary: HealpixSettings::default(),
        wavelength,
        kernel_mu: 8,
        kernel_azimuth: 16,
    };
    // A reporting consumer may select the public component accessors dynamically.
    // The same physical vacuum has different availability in truncated modes.
    let accessors: [fn(&SpectralStarlightResult) -> Option<BandRadiance>; 5] = [
        SpectralStarlightResult::direct,
        SpectralStarlightResult::single,
        SpectralStarlightResult::higher_orders,
        SpectralStarlightResult::total_scattered,
        SpectralStarlightResult::ground_total,
    ];
    for (mode, expected) in [
        (SpectralStarlightMode::Identity, [None; 5]),
        (
            SpectralStarlightMode::Direct {
                atmosphere: &atmosphere,
                assumption,
                wavelength,
            },
            [Some(0.003), None, None, None, None],
        ),
        (
            SpectralStarlightMode::Single {
                atmosphere: &atmosphere,
                phase: &phase,
                assumption,
                angular: HealpixSettings::default(),
                wavelength,
            },
            [Some(0.003), Some(0.), None, None, None],
        ),
        (
            SpectralStarlightMode::Multiple {
                atmosphere: &atmosphere,
                phase: &phase,
                assumption,
                settings,
            },
            [Some(0.003), Some(0.), Some(0.), Some(0.), Some(0.003)],
        ),
    ] {
        let r = evaluator
            .evaluate_spectral_starlight(
                geometry,
                Horizontal::new(Degrees::new(60.), Degrees::new(0.)),
                [360., 390.].map(Nanometers::new),
                mode,
            )
            .unwrap();
        assert_eq!(r.toa().value(), 0.003);
        assert!(r.uncertainty().is_none());
        for (accessor, expected) in accessors.into_iter().zip(expected) {
            let actual = std::hint::black_box(accessor)(&r);
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!((actual.value() - expected).abs() < 1e-16);
            }
        }
    }
}
