// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Public evaluator integration against analytical limits and pinned external RTE.
use nsb::components::starlight::StarlightMap;
use nsb::transport::multiple::starlight::ProjectedSettings;
use nsb::transport::multiple::MultipleScatteringSettings;
use nsb::transport::single::starlight::{
    HealpixSettings, StarlightGeometry, WavelengthSettings, WithinBinSpectrum,
};
use nsb::transport::single::AerosolPhase;
use nsb::units::qtty::{Albedos, Degrees, Meters, Nanometers, Quantity};
use nsb::{NsbEvaluator, NsbModelConfig, Observer, StarlightProduct};
use nsb::{SpectralStarlightMode as Mode, SpectralStarlightResult as Output};
use sha2::{Digest, Sha256};
use siderust::atmosphere::{AtmosphereProfile, AtmosphericOptics, LayeredAtmosphere};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use std::sync::Arc;
use tempoch::JulianDate;

const FIXTURE: &[u8] = include_bytes!("data/starlight-transport/uniform.json");
const CHECKSUM: &str = "35058caa944e51096e5d9ff8ca33eb19be08ef0b6c727ec37192758a64bdc24f";
const FLAT: WithinBinSpectrum = WithinBinSpectrum::FlatPhotonDensity;
fn source() -> Arc<StarlightMap> {
    Arc::new(StarlightMap::from_json_bytes(FIXTURE, CHECKSUM).unwrap())
}
fn evaluator(map: Arc<StarlightMap>) -> NsbEvaluator {
    NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_starlight_product(StarlightProduct::with_shared_experimental_map(map)),
    )
    .unwrap()
}
fn geometry() -> StarlightGeometry {
    StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.),
        ),
        tt: JulianDate::new(2459680.8),
        ut1: JulianDate::new(2459680.7992),
    }
}
fn view() -> Horizontal {
    Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.))
}
fn band() -> [Nanometers; 2] {
    [360., 390.].map(Nanometers::new)
}
fn medium(rayleigh: f64, aerosol: f64, ssa: f64, absorption: f64) -> LayeredAtmosphere {
    LayeredAtmosphere::try_new(
        // Deliberately distinct atmospheric datum and ellipsoidal height.
        &[Meters::new(2000.), Meters::new(3000.)],
        [360., 570.].map(Nanometers::new),
        vec![AtmosphericOptics::from_extinction(
            Quantity::new(rayleigh / 1000.),
            Quantity::new(aerosol / 1000.),
            Albedos::new(ssa),
            Quantity::new(absorption / 1000.),
        )
        .unwrap()],
    )
    .unwrap()
}
fn phase(g: f64) -> AerosolPhase {
    AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(g).unwrap())
}
fn wavelength() -> WavelengthSettings {
    WavelengthSettings {
        refinements: 1,
        ..WavelengthSettings::default()
    }
}
fn angular() -> HealpixSettings {
    HealpixSettings {
        subdivision: 8,
        refinements: 4,
        ..HealpixSettings::default()
    }
}
fn projected() -> ProjectedSettings {
    ProjectedSettings {
        solver: MultipleScatteringSettings {
            mu_nodes: 8,
            azimuth_nodes: 32,
            cells_per_layer: 2,
            max_orders: 32,
            angular_tolerance: 0.05,
            vertical_tolerance: 0.05,
            ..MultipleScatteringSettings::default()
        },
        boundary: angular(),
        wavelength: wavelength(),
        kernel_mu: 128,
        kernel_azimuth: 64,
    }
}
fn close(actual: f64, expected: f64, relative: f64) {
    assert!(
        (actual - expected).abs() <= relative * expected.abs().max(1e-15),
        "{actual} vs {expected}"
    );
}

#[test]
fn exact_identity_retains_shared_product_and_has_no_ground_prediction() {
    assert!(StarlightMap::from_json_bytes(FIXTURE, &"0".repeat(64)).is_err());
    let map = source();
    let e = evaluator(Arc::clone(&map));
    for (lo, hi, expected) in [
        (360., 390., 0.003),
        (390., 420., 0.006),
        (420., 450., 0.009),
    ] {
        let r = e
            .evaluate_spectral_starlight(
                geometry(),
                view(),
                [lo, hi].map(Nanometers::new),
                Mode::Identity,
            )
            .unwrap();
        assert_eq!(r.toa().value(), expected);
        assert!(r.direct().is_none());
        assert!(r.single().is_none());
        assert!(r.higher_orders().is_none());
        assert!(r.total_scattered().is_none());
        assert!(r.ground_total().is_none());
        assert!(r.uncertainty().is_none());
        let Output::Identity(r) = r else {
            panic!("wrong mode")
        };
        assert!(Arc::ptr_eq(&r.source, &map));
        assert_eq!(r.source.sha256(), Some(CHECKSUM));
        assert_eq!(r.source.maturity(), Some("experimental"));
        assert_eq!(r.source.grid(), map.grid());
        assert_eq!(r.geometry.tt, geometry().tt);
    }
}

#[test]
fn vacuum_absorption_and_scattering_direct_match_beer_lambert_without_angular_work() {
    let e = evaluator(source());
    for (ray, aer, ssa, absorption) in [
        (0., 0., 0., 0.),
        (0., 0., 0., 0.16),
        (0.08, 0.03, 0.9, 0.02),
    ] {
        let a = medium(ray, aer, ssa, absorption);
        let r = e
            .evaluate_spectral_starlight(
                geometry(),
                view(),
                band(),
                Mode::Direct {
                    atmosphere: &a,
                    assumption: FLAT,
                    wavelength: wavelength(),
                },
            )
            .unwrap();
        close(r.toa().value(), 0.003, 1e-14);
        close(
            r.direct().unwrap().value(),
            0.003 * (-(ray + aer + absorption) / 0.8_f64).exp(),
            1e-14,
        );
        assert!(r.single().is_none() && r.higher_orders().is_none());
        assert!(r.ground_total().is_none() && r.total_scattered().is_none());
        let Output::Direct(d) = r else {
            panic!("wrong mode")
        };
        assert_eq!(d.wavelength_order, 4);
        assert!(d.wavelength_change.value() < 1e-16);
        assert_eq!(d.planned_work, 652);
        assert_eq!(d.atmosphere.edges(), a.edges());
        assert_eq!(d.assumption, FLAT);
    }
}

#[test]
fn single_and_multiple_match_independent_disort_and_preserve_decomposition() {
    let e = evaluator(source());
    let a = medium(0.08, 0., 0., 0.08);
    let p = phase(0.65);
    let s = e
        .evaluate_spectral_starlight(
            geometry(),
            view(),
            band(),
            Mode::Single {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                angular: angular(),
                wavelength: wavelength(),
            },
        )
        .unwrap();
    let m = e
        .evaluate_spectral_starlight(
            geometry(),
            view(),
            band(),
            Mode::Multiple {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                settings: projected(),
            },
        )
        .unwrap();
    assert_eq!(s.toa().value(), 0.003);
    assert_eq!(m.toa(), s.toa());
    assert!(s.uncertainty().is_none() && m.uncertainty().is_none());
    // Pinned independent 256-stream PythonicDISORT, uniform TOA spectral .025,
    // Rayleigh .08, absorption .08, mu_view=.8; linear scaling to fixture .0001/nm.
    let independent_single = 0.003 / 0.025 * 0.0008873157461357191;
    let independent_higher = 0.003 / 0.025 * 0.00010378276034380166;
    close(s.single().unwrap().value(), independent_single, 1e-4);
    close(m.single().unwrap().value(), independent_single, 0.01);
    close(m.higher_orders().unwrap().value(), independent_higher, 0.01);
    close(
        s.direct().unwrap().value(),
        0.003 * (-0.16_f64 / 0.8).exp(),
        1e-14,
    );
    assert_eq!(s.direct(), m.direct());
    assert!(s.higher_orders().is_none() && s.ground_total().is_none());
    assert!(s.total_scattered().is_none());
    close(
        m.ground_total().unwrap().value(),
        m.direct().unwrap().value()
            + m.single().unwrap().value()
            + m.higher_orders().unwrap().value(),
        1e-14,
    );
    assert_eq!(
        m.total_scattered(),
        Some(m.single().unwrap() + m.higher_orders().unwrap())
    );
    let Output::Single(s) = s else {
        panic!("wrong mode")
    };
    let Output::Multiple(m) = m else {
        panic!("wrong mode")
    };
    assert!(s.uncertainty.is_none() && m.transport.uncertainty.is_none());
    assert_eq!(s.provenance.map_checksum.as_deref(), Some(CHECKSUM));
    assert_eq!(m.transport.provenance, s.provenance);
    assert!(s.samples.iter().all(|s| s.healpix_convergence.is_some()));
    assert!(m.transport.samples.iter().all(|s| s.convergence.orders > 1));
    assert!(m.workspace_bytes < 16 * 1024 * 1024);
    assert!(m.transport.planned_work < 32_000_000_000);
}

#[test]
fn scattering_modes_report_physical_zeros_only_when_computed() {
    let e = evaluator(source());
    let p = phase(0.);
    let settings = ProjectedSettings {
        kernel_mu: 8,
        kernel_azimuth: 16,
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
        ..projected()
    };
    for depth in [0., 0.2] {
        let a = medium(0., 0., 0., depth);
        for mode in [
            Mode::Single {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                angular: HealpixSettings::default(),
                wavelength: wavelength(),
            },
            Mode::Multiple {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                settings,
            },
        ] {
            let r = e
                .evaluate_spectral_starlight(geometry(), view(), band(), mode)
                .unwrap();
            assert_eq!(r.single().unwrap().value(), 0.);
            if let Some(higher) = r.higher_orders() {
                assert_eq!(higher.value(), 0.);
            }
            close(
                r.direct().unwrap().value(),
                0.003 * (-depth / 0.8_f64).exp(),
                1e-14,
            );
        }
    }
}

#[test]
fn unsupported_bins_geometry_settings_and_products_fail() {
    let e = evaluator(source());
    for b in [
        [360., 375.],
        [350., 390.],
        [360., 420.],
        [450., 480.],
        [390., 360.],
        [f64::NAN, 390.],
    ] {
        assert!(e
            .evaluate_spectral_starlight(geometry(), view(), b.map(Nanometers::new), Mode::Identity)
            .is_err());
    }
    for bad_geometry in [
        StarlightGeometry {
            tt: JulianDate::new(f64::INFINITY),
            ..geometry()
        },
        StarlightGeometry {
            observer: Observer::new(
                Degrees::new(0.),
                Degrees::new(31.),
                Meters::new(f64::INFINITY),
            ),
            ..geometry()
        },
    ] {
        assert!(e
            .evaluate_spectral_starlight(bad_geometry, view(), band(), Mode::Identity)
            .is_err());
    }
    assert!(e
        .evaluate_spectral_starlight(
            geometry(),
            Horizontal::new(Degrees::new(f64::NAN), Degrees::new(0.)),
            band(),
            Mode::Identity
        )
        .is_err());
    let a = medium(0., 0., 0., 0.1);
    for (alt, settings) in [
        (9., wavelength()),
        (-1., wavelength()),
        (
            50.,
            WavelengthSettings {
                nodes: usize::MAX,
                ..wavelength()
            },
        ),
        (
            50.,
            WavelengthSettings {
                relative_tolerance: 0.,
                ..wavelength()
            },
        ),
    ] {
        assert!(e
            .evaluate_spectral_starlight(
                geometry(),
                Horizontal::new(Degrees::new(alt), Degrees::new(0.)),
                band(),
                Mode::Direct {
                    atmosphere: &a,
                    assumption: FLAT,
                    wavelength: settings
                }
            )
            .is_err());
    }
    let legacy = Arc::new(
        StarlightMap::from_csv_str(
            include_str!("data/starlight_fixture_map.csv"),
            nsb::components::starlight::StarlightProvenance::new(
                "fixture",
                "1",
                "2026-01-01",
                "synthetic",
                "AGPL-3.0-only",
                "all",
                "300-650 nm",
                "synthetic",
                None::<String>,
            ),
        )
        .unwrap(),
    );
    assert!(evaluator(legacy)
        .evaluate_spectral_starlight(geometry(), view(), band(), Mode::Identity)
        .is_err());
}

#[test]
fn missing_coverage_nonconvergence_and_budgets_fail_closed() {
    let e = evaluator(source());
    let narrow = LayeredAtmosphere::try_new(
        &[Meters::new(0.), Meters::new(1000.)],
        [370., 390.].map(Nanometers::new),
        vec![AtmosphericOptics::from_extinction(
            Quantity::new(0.),
            Quantity::new(0.),
            Albedos::new(0.),
            Quantity::new(0.),
        )
        .unwrap()],
    )
    .unwrap();
    let p = phase(0.);
    for mode in [
        Mode::Direct {
            atmosphere: &narrow,
            assumption: FLAT,
            wavelength: wavelength(),
        },
        Mode::Single {
            atmosphere: &narrow,
            phase: &p,
            assumption: FLAT,
            angular: HealpixSettings::default(),
            wavelength: wavelength(),
        },
        Mode::Multiple {
            atmosphere: &narrow,
            phase: &p,
            assumption: FLAT,
            settings: projected(),
        },
    ] {
        assert!(e
            .evaluate_spectral_starlight(geometry(), view(), band(), mode)
            .unwrap_err()
            .to_string()
            .contains("cover"));
    }
    let a = medium(0.08, 0., 0., 0.08);
    for (settings, expected_error) in [
        (
            ProjectedSettings {
                kernel_mu: usize::MAX,
                ..projected()
            },
            "grid",
        ),
        (
            ProjectedSettings {
                solver: MultipleScatteringSettings {
                    max_orders: 2,
                    ..projected().solver
                },
                ..projected()
            },
            "order",
        ),
        (
            ProjectedSettings {
                solver: MultipleScatteringSettings {
                    angular_tolerance: 1e-12,
                    vertical_tolerance: 1e-12,
                    ..projected().solver
                },
                ..projected()
            },
            "converg",
        ),
        (
            ProjectedSettings {
                kernel_mu: 256,
                kernel_azimuth: 512,
                ..projected()
            },
            "work",
        ),
    ] {
        let error = e
            .evaluate_spectral_starlight(
                geometry(),
                view(),
                band(),
                Mode::Multiple {
                    atmosphere: &a,
                    phase: &p,
                    assumption: FLAT,
                    settings,
                },
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected_error), "{error}");
    }
    assert!(e
        .evaluate_spectral_starlight(
            geometry(),
            view(),
            band(),
            Mode::Single {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                angular: HealpixSettings {
                    subdivision: 64,
                    refinements: 10,
                    ..angular()
                },
                wavelength: wavelength()
            }
        )
        .unwrap_err()
        .to_string()
        .contains("work"));
    let variable = LayeredAtmosphere::from_profile(
        AtmosphereProfile::EL_PARANAL,
        &[Meters::new(2635.), Meters::new(12000.)],
        [360., 570.].map(Nanometers::new),
        &[1.],
        &[1.],
        Albedos::new(0.),
    )
    .unwrap();
    let strict = WavelengthSettings {
        relative_tolerance: 1e-12,
        absolute_tolerance: Quantity::new(0.),
        ..wavelength()
    };
    assert!(e
        .evaluate_spectral_starlight(
            geometry(),
            view(),
            band(),
            Mode::Direct {
                atmosphere: &variable,
                assumption: FLAT,
                wavelength: strict
            }
        )
        .unwrap_err()
        .to_string()
        .contains("convergence"));
}

#[test]
fn bright_and_horizon_pixels_reach_the_existing_independent_hg_reference() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "data/starlight-transport/angular-independent.json"
    ))
    .unwrap();
    let a = medium(0.1, 0.03, 0.9, 0.02);
    let p = phase(0.65);
    for (pixel, case) in [(1, 0), (4, 2)] {
        let mut fixture: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        for cell in fixture["pixels"].as_array_mut().unwrap() {
            cell["radiance"][0] = serde_json::json!(if cell["pixel"] == pixel { 30. } else { 0. });
        }
        let bytes = serde_json::to_vec(&fixture).unwrap();
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let e = evaluator(Arc::new(
            StarlightMap::from_json_bytes(&bytes, &hash).unwrap(),
        ));
        let r = e
            .evaluate_spectral_starlight(
                geometry(),
                view(),
                band(),
                Mode::Single {
                    atmosphere: &a,
                    phase: &p,
                    assumption: FLAT,
                    angular: HealpixSettings {
                        subdivision: 16,
                        refinements: 4,
                        ..angular()
                    },
                    wavelength: wavelength(),
                },
            )
            .unwrap();
        close(
            r.single().unwrap().value(),
            30. * reference["cases"][case]["single"].as_f64().unwrap(),
            1e-3,
        );
    }
}

#[test]
fn nonabsorbing_isotropic_scattering_has_the_independent_thin_column_limit() {
    let e = evaluator(source());
    let tau = 1e-8;
    let a = medium(0., tau, 1., 0.);
    let p = phase(0.);
    let r = e
        .evaluate_spectral_starlight(
            geometry(),
            view(),
            band(),
            Mode::Single {
                atmosphere: &a,
                phase: &p,
                assumption: FLAT,
                angular: HealpixSettings {
                    absolute_tolerance: Quantity::new(0.),
                    ..angular()
                },
                wavelength: wavelength(),
            },
        )
        .unwrap();
    // Isotropic phase integrates to 1/2 over the illuminated hemisphere.
    // In the optically thin limit the path length is tau/mu_view; the omitted
    // attenuation correction is O(tau*abs(log(tau))) relative, well below 1e-4.
    close(r.single().unwrap().value(), 0.003 * tau / (2. * 0.8), 1e-4);
    close(
        r.direct().unwrap().value(),
        0.003 * (-tau / 0.8).exp(),
        1e-14,
    );
    assert!(r.higher_orders().is_none() && r.uncertainty().is_none());
}
