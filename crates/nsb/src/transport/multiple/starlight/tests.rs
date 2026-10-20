// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::components::starlight::StarlightMap;
use crate::units::qtty::{Albedos, Degrees, Meters, Quantity};
use crate::Observer;
use sha2::{Digest, Sha256};
use siderust::atmosphere::AtmosphericOptics;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;
use tempoch::{JulianDate, TT, UT1};

fn map(bright: Option<u64>, ordering: &str) -> StarlightMap {
    let p = serde_json::json!({"schema":"nsb-starlight-spectral-map-v2","product_id":"synthetic-multiple-v1","band_nm":[360,390],"bin_edges_nm":[360,390],"nside":1,"ordering":ordering,"coordinate_frame":"galactic","domain":"top-of-atmosphere","units":"photon/(cm2 ns sr)","wavelength_convention":"vacuum","maturity":"experimental","provenance":{"source_catalogue":"synthetic","source_license":"AGPL-3.0-only","source_selection":"all","generation_method":"offline fixture","limitations":["synthetic"]},"catalogue_rows":12,"admitted_sources":12,"excluded_sources":{},"pixels":(0..12).map(|i|serde_json::json!({"pixel":i,"sources":1,"radiance":[if bright.is_none_or(|p|p==i){0.75}else{0.}]})).collect::<Vec<_>>()});
    let bytes = serde_json::to_vec(&p).unwrap();
    let hash = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    StarlightMap::from_json_bytes(&bytes, &hash).unwrap()
}
fn sky(map: &StarlightMap) -> StarlightSky<'_> {
    StarlightSky::new(
        map,
        StarlightGeometry {
            observer: Observer::new(
                Degrees::new(-111.59989),
                Degrees::new(31.96403),
                Meters::new(2097.),
            ),
            tt: JulianDate::<TT>::new(2459680.8),
            ut1: JulianDate::<UT1>::new(2459680.7992),
        },
        [Nanometers::new(360.), Nanometers::new(390.)],
        WithinBinSpectrum::FlatPhotonDensity,
    )
    .unwrap()
}
fn medium(depth: f64) -> LayeredAtmosphere {
    let c = AtmosphericOptics::from_extinction(
        Quantity::new(depth / 1000.),
        Quantity::new(0.),
        Albedos::new(0.),
        Quantity::new(depth / 1000.),
    )
    .unwrap();
    LayeredAtmosphere::try_new(
        &[Meters::new(2097.), Meters::new(3097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        vec![c],
    )
    .unwrap()
}
fn phase() -> AerosolPhase {
    AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.).unwrap())
}
fn view() -> Horizontal {
    Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.))
}
fn settings() -> MultipleScatteringSettings {
    MultipleScatteringSettings {
        mu_nodes: 2,
        azimuth_nodes: 4,
        cells_per_layer: 2,
        max_orders: 16,
        angular_tolerance: 0.05,
        vertical_tolerance: 0.05,
        ..MultipleScatteringSettings::default()
    }
}

#[test]
fn vacuum_native_bin_provenance_exact_direct_and_independent_grids() {
    {
        let ordering = "ring";
        let map = map(None, ordering);
        let sky = sky(&map);
        let r = starlight_multiple_scattering(
            &medium(0.),
            &phase(),
            &sky,
            view(),
            settings(),
            HealpixSettings::default(),
            WavelengthSettings {
                refinements: 1,
                ..WavelengthSettings::default()
            },
        )
        .unwrap();
        assert!((r.direct.value() - 0.75).abs() < 1e-14);
        assert_eq!(r.single.value(), 0.);
        assert_eq!(r.higher_orders.value(), 0.);
        assert_eq!(r.total, r.direct);
        assert!(r.uncertainty.is_none());
        assert_eq!(r.grid, map.grid());
        assert_eq!(r.assumption, WithinBinSpectrum::FlatPhotonDensity);
        assert_eq!(r.source, sky.coverage());
        assert_eq!(r.wavelength_order, 4);
        assert_eq!(r.boundary_nside, vec![4; 4]);
        assert_eq!(r.samples.len(), 4);
        assert!(r.wavelength_change.iter().all(|v| v.value() < 1e-14));
    }
}
#[test]
fn child_areas_preserve_isolated_parent_integrals_and_horizon() {
    {
        let ordering = "ring";
        for pixel in 0..12 {
            let m = map(Some(pixel), ordering);
            let s = sky(&m);
            for nside in [1, 2, 4, 8] {
                let fine = grid(nside).unwrap();
                let mut integrated = 0.;
                let mut children = 0;
                for i in 0..fine.npix() {
                    let d = fine.pixel_center::<Galactic>(HealpixIndex::new(i)).unwrap();
                    if m.grid().direction_to_pixel(d).unwrap().get() == pixel {
                        integrated += fine.pixel_area_sr() * 0.025;
                        children += 1;
                    }
                }
                assert_eq!(children, nside * nside);
                assert!((integrated - 0.025 * TAU / 6.).abs() < 1e-14);
                let b = boundary(&s, nside).unwrap();
                assert!(b.iter().all(|d| d.mu > 0. && d.mu <= 1.));
                assert!((b.iter().map(|d| d.weight).sum::<f64>() - TAU).abs() < 1e-12);
                assert!(b.iter().map(|d| d.incoming * d.weight).sum::<f64>() <= integrated + 1e-14);
            }
        }
    }
}
#[test]
fn nonzero_scattering_native_bin_and_boundary_refinement() {
    let m = map(None, "ring");
    let s = sky(&m);
    let r = starlight_multiple_scattering(
        &medium(0.08),
        &phase(),
        &s,
        view(),
        MultipleScatteringSettings {
            mu_nodes: 4,
            max_orders: 8,
            ..settings()
        },
        HealpixSettings {
            subdivision: 8,
            refinements: 4,
            ..HealpixSettings::default()
        },
        WavelengthSettings {
            refinements: 1,
            ..WavelengthSettings::default()
        },
    )
    .unwrap();
    // Frozen materialized #269 results at identical optics and quadrature.
    assert!((r.single.value() / 0.02658873913628157 - 1.).abs() < 2e-14);
    assert!((r.higher_orders.value() / 0.0031324449896208877 - 1.).abs() < 2e-14);
    assert!(
        (r.samples[0].convergence.flux_balance_relative - (-0.0001266470069952773)).abs() < 1e-13
    );
    assert!(r.total_scattered.value() > r.single.value());
    // Independent 256-stream continuous-height DISORT reference, same uniform
    // native bin and optical depths; finite cells dominate this 1% allowance.
    assert!((r.single.value() / (30. * 0.0008873157461357191) - 1.).abs() < 0.01);
    assert!((r.higher_orders.value() / (30. * 0.00010378276034380166) - 1.).abs() < 0.01);
    eprintln!(
        "native-bin first={} higher={} nside={:?}",
        r.single.value(),
        r.higher_orders.value(),
        r.boundary_nside
    );
}
#[test]
fn invalid_and_excessive_spectral_and_boundary_plans_fail() {
    let m = map(None, "ring");
    let s = sky(&m);
    let run = |a, w, solver| {
        starlight_multiple_scattering(&medium(0.), &phase(), &s, view(), solver, a, w)
    };
    let w = WavelengthSettings {
        nodes: usize::MAX,
        ..WavelengthSettings::default()
    };
    assert!(run(HealpixSettings::default(), w, settings()).is_err());
    let mut a = HealpixSettings {
        subdivision: u32::MAX,
        ..HealpixSettings::default()
    };
    assert!(run(a, WavelengthSettings::default(), settings()).is_err());
    a = HealpixSettings::default();
    a.subdivision = 64;
    a.refinements = 4;
    assert!(run(a, WavelengthSettings::default(), settings()).is_err());
    assert!(run(
        HealpixSettings::default(),
        WavelengthSettings::default(),
        MultipleScatteringSettings::default()
    )
    .is_err());
}

#[test]
fn wavelength_refinement_is_independent_and_cannot_accept_bin_centers() {
    use crate::units::qtty::{Hectopascals, OpticalDepths};
    use siderust::atmosphere::{AtmosphereProfile, MieParams};
    let m = map(None, "ring");
    let s = sky(&m);
    let profile = AtmosphereProfile {
        surface_pressure: Hectopascals::new(0.),
        observer_altitude: Meters::new(2097.).to(),
        mie_params: MieParams {
            tau0: OpticalDepths::new(0.08),
            lambda_ref: Nanometers::new(550.),
            alpha: 4.,
        },
        ..AtmosphereProfile::EL_PARANAL
    };
    let atm = LayeredAtmosphere::from_profile(
        profile,
        &[Meters::new(2097.), Meters::new(3097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[1.],
        &[1.],
        Albedos::new(0.),
    )
    .unwrap();
    let solver = MultipleScatteringSettings {
        max_orders: 2,
        ..settings()
    };
    let strict = WavelengthSettings {
        nodes: 2,
        refinements: 1,
        relative_tolerance: 1e-12,
        absolute_tolerance: BandRadiance::new(0.),
    };
    let failed = starlight_multiple_scattering(
        &atm,
        &phase(),
        &s,
        view(),
        solver,
        HealpixSettings::default(),
        strict,
    )
    .unwrap_err();
    assert!(failed.to_string().contains("wavelength refinement"));
    let r = starlight_multiple_scattering(
        &atm,
        &phase(),
        &s,
        view(),
        solver,
        HealpixSettings::default(),
        WavelengthSettings {
            refinements: 3,
            ..strict
        },
    )
    .unwrap();
    assert!(r.wavelength_order > 4);
    let mut oracle = 0.;
    for (x, w) in single::quadrature::gauss(64).unwrap() {
        oracle += single::direct_illumination(
            &atm,
            &phase(),
            &s,
            Nanometers::new(360. + 30. * x),
            view(),
        )
        .unwrap()
        .0 * w
            * 30.;
    }
    assert!((r.direct.value() / oracle - 1.).abs() < 1e-12);
    let center = single::direct_illumination(&atm, &phase(), &s, Nanometers::new(375.), view())
        .unwrap()
        .0
        * 30.;
    assert!((r.direct.value() / center - 1.).abs() > 1e-5);
}

#[test]
fn child_grid_preserves_every_parent_at_poles_and_seams() {
    for native in [1, 2, 4, 8] {
        let parent = grid(native).unwrap();
        for factor in [2, 4] {
            let fine = grid(native * factor).unwrap();
            let mut counts = vec![0_u32; parent.npix() as usize];
            for i in 0..fine.npix() {
                let d = fine.pixel_center::<Galactic>(HealpixIndex::new(i)).unwrap();
                counts[parent.direction_to_pixel(d).unwrap().get() as usize] += 1;
            }
            assert!(counts.iter().all(|n| *n == factor * factor));
        }
    }
}

#[test]
fn isolated_bright_parent_is_integrated_before_internal_scattering() {
    let uniform = map(None, "ring");
    let context = sky(&uniform);
    let zenith = siderust::coordinates::cartesian::Direction::<Galactic>::new_unchecked(
        context.basis[2][0],
        context.basis[2][1],
        context.basis[2][2],
    );
    let pixel = uniform.grid().direction_to_pixel(zenith).unwrap().get();
    let m = map(Some(pixel), "ring");
    let s = sky(&m);
    let optics = AtmosphericOptics::from_extinction(
        Quantity::new(0.),
        Quantity::new(0.16 / 1000.),
        Albedos::new(0.5),
        Quantity::new(0.),
    )
    .unwrap();
    let atm = LayeredAtmosphere::try_new(
        &[Meters::new(2097.), Meters::new(3097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        vec![optics],
    )
    .unwrap();
    let r = starlight_multiple_scattering(
        &atm,
        &phase(),
        &s,
        view(),
        MultipleScatteringSettings {
            mu_nodes: 4,
            max_orders: 8,
            ..settings()
        },
        HealpixSettings {
            subdivision: 8,
            refinements: 4,
            ..HealpixSettings::default()
        },
        WavelengthSettings {
            refinements: 1,
            ..WavelengthSettings::default()
        },
    )
    .unwrap();
    assert!(r.single.value() > 0.);
    assert!(r.higher_orders.value() > 0.);
    assert!(r.total.value() > r.direct.value());
    assert_eq!(r.toa, s.toa(view()).unwrap());
    eprintln!(
        "bright pixel {pixel}: single={} higher={} boundary_nside={:?}",
        r.single.value(),
        r.higher_orders.value(),
        r.boundary_nside
    );
}

#[test]
fn wavelength_dependent_higher_orders_match_independent_full_bin_reference() {
    use crate::units::qtty::{Hectopascals, OpticalDepths};
    use siderust::atmosphere::{AtmosphereProfile, MieParams};
    let m = map(None, "ring");
    let s = sky(&m);
    let profile = AtmosphereProfile {
        surface_pressure: Hectopascals::new(200.),
        observer_altitude: Meters::new(2097.).to(),
        mie_params: MieParams {
            tau0: OpticalDepths::new(0.04),
            lambda_ref: Nanometers::new(550.),
            alpha: 1.38,
        },
        ..AtmosphereProfile::EL_PARANAL
    };
    let atm = LayeredAtmosphere::from_profile(
        profile,
        &[Meters::new(2097.), Meters::new(3097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[1.],
        &[1.],
        Albedos::new(0.),
    )
    .unwrap();
    // Export matched optics only, not predicted radiance, for the independent
    // solver. NumPy separately checks these GL nodes and weights.
    let optics: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/data/multiple-scattering/spectral-optics-v1.json"
    ))
    .unwrap();
    for ((x, weight), expected) in single::quadrature::gauss(64)
        .unwrap()
        .into_iter()
        .zip(optics["samples"].as_array().unwrap())
    {
        let w = Nanometers::new(360. + 30. * x);
        let c = atm.coefficients(w, Meters::new(2097.)).unwrap();
        assert!((w.value() - expected[0].as_f64().unwrap()).abs() < 1e-12);
        assert!((weight - expected[1].as_f64().unwrap()).abs() < 1e-13);
        assert!(
            (1000. * c.rayleigh().sigma_s.value() - expected[2].as_f64().unwrap()).abs() < 1e-13
        );
        assert!(
            (1000. * (c.aerosol().sigma_s.value() + c.aerosol().sigma_a.value())
                - expected[3].as_f64().unwrap())
            .abs()
                < 1e-13
        );
        eprintln!(
            "OPTICS {},{weight},{},{}",
            w.value(),
            1000. * c.rayleigh().sigma_s.value(),
            1000. * (c.aerosol().sigma_s.value() + c.aerosol().sigma_a.value())
        );
    }
    let r = starlight_multiple_scattering(
        &atm,
        &phase(),
        &s,
        view(),
        MultipleScatteringSettings {
            mu_nodes: 4,
            max_orders: 24,
            ..settings()
        },
        HealpixSettings {
            subdivision: 8,
            refinements: 4,
            ..HealpixSettings::default()
        },
        WavelengthSettings {
            nodes: 4,
            refinements: 1,
            ..WavelengthSettings::default()
        },
    )
    .unwrap();
    eprintln!(
        "profile native-bin direct={} single={} higher={} wavelength_order={} work={}",
        r.direct.value(),
        r.single.value(),
        r.higher_orders.value(),
        r.wavelength_order,
        r.planned_work
    );
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/data/multiple-scattering/spectral-v1.json"
    ))
    .unwrap();
    let reference = fixture["results"].as_array().unwrap().last().unwrap();
    assert!((r.direct.value() / reference["direct"].as_f64().unwrap() - 1.).abs() < 1e-10);
    assert!((r.single.value() / reference["single"].as_f64().unwrap() - 1.).abs() < 0.01);
    assert!((r.higher_orders.value() / reference["higher"].as_f64().unwrap() - 1.).abs() < 0.01);
}

// Materialized #269 boundary retained only as a small-fixture oracle.
fn boundary(sky: &StarlightSky<'_>, nside: u32) -> Result<Vec<Direction>> {
    let fine = grid(nside)?;
    let bin = sky
        .map
        .bin_edges_nm()
        .iter()
        .position(|e| f64::from(*e) == sky.band[0].value())
        .ok_or_else(|| invalid("missing native bin"))?;
    let width = (sky.band[1] - sky.band[0]).value();
    let mut samples = Vec::with_capacity(fine.npix() as usize);
    for i in 0..fine.npix() {
        let gal = fine
            .pixel_center::<Galactic>(HealpixIndex::new(i))
            .map_err(|e| invalid(&e.to_string()))?;
        let local: [f64; 3] = std::array::from_fn(|j| {
            sky.basis[j]
                .iter()
                .zip(gal.as_array())
                .map(|(a, b)| a * b)
                .sum()
        });
        if local[2] > 0.0 {
            let parent = sky
                .map
                .grid()
                .direction_to_pixel(gal)
                .map_err(|e| invalid(&e.to_string()))?;
            let incoming = sky.map.spectral_bin_at_index(parent, bin)?.value() / width;
            samples.push(Direction {
                mu: local[2],
                vector: local,
                weight: fine.pixel_area_sr(),
                incoming,
            });
        }
    }
    Ok(samples)
}

struct StoredBoundary(Vec<Direction>);
impl IncidentBoundary for StoredBoundary {
    fn visit(&self, visitor: &mut dyn FnMut(Direction) -> Result<()>) -> Result<()> {
        for d in &self.0 {
            visitor(*d)?;
        }
        Ok(())
    }
}

#[test]
fn streamed_boundary_is_identical_to_materialized_ring_oracle() {
    for bright in [None, Some(0), Some(5), Some(11)] {
        let map = map(bright, "ring");
        let sky = sky(&map);
        for nside in [1, 2, 8, 32] {
            let expected = boundary(&sky, nside).unwrap();
            let mut seen = 0;
            HealpixBoundary { sky: &sky, nside }
                .visit(&mut |d| {
                    let old = expected[seen];
                    assert_eq!(d.mu, old.mu);
                    assert_eq!(d.vector, old.vector);
                    assert_eq!(d.weight, old.weight);
                    assert_eq!(d.incoming, old.incoming);
                    seen += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(seen, expected.len());
        }
    }
}

#[test]
fn streamed_scattering_preserves_components_tail_and_photon_flux() {
    for bright in [None, Some(5)] {
        let map = map(bright, "ring");
        let sky = sky(&map);
        let stored = StoredBoundary(boundary(&sky, 32).unwrap());
        let streamed = HealpixBoundary {
            sky: &sky,
            nside: 32,
        };
        for depth in [0.0, 0.08] {
            for (a, z) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
                let run = |boundary| {
                    solve(
                        &medium(depth),
                        &phase(),
                        &sky,
                        Nanometers::new(375.),
                        view(),
                        settings(),
                        (a, z, Some(boundary)),
                    )
                    .unwrap()
                };
                let old = run(&stored as &dyn IncidentBoundary);
                let new = run(&streamed as &dyn IncidentBoundary);
                // Identical direction traversal and recurrence make radiances
                // bit-identical; no relaxed continuum tolerance is needed.
                assert_eq!(new.first, old.first);
                assert_eq!(new.higher, old.higher);
                assert_eq!(new.orders, old.orders);
                assert_eq!(new.bound, old.bound);
                assert_eq!(new.contraction, old.contraction);
                assert_eq!(new.flux_balance, old.flux_balance);
            }
        }
    }
}

#[test]
fn streamed_boundary_propagates_errors_without_visiting_more_pixels() {
    let map = map(None, "ring");
    let sky = sky(&map);
    let mut seen = 0;
    let error = HealpixBoundary {
        sky: &sky,
        nside: 512,
    }
    .visit(&mut |_| {
        seen += 1;
        Err(invalid("visitor stopped"))
    })
    .unwrap_err();
    assert_eq!(seen, 1);
    assert!(error.to_string().contains("visitor stopped"));
    assert!(HealpixBoundary {
        sky: &sky,
        nside: 0
    }
    .visit(&mut |_| Ok(()))
    .is_err());
}

#[test]
fn streaming_workspace_does_not_scale_with_boundary_nside() {
    let bytes = workspace_bytes(128, 4).unwrap();
    assert!(bytes < MEMORY_LIMIT);
    assert_eq!(bytes, 292288);
    assert!(workspace_bytes(usize::MAX, 4).is_err());
    assert!(workspace_bytes(128, usize::MAX).is_err());
    assert!(workspace_bytes(2048, 4).unwrap() > MEMORY_LIMIT);
}

#[test]
fn complete_nside512_stream_has_hemisphere_measure_without_retained_directions() {
    let map = map(None, "ring");
    let sky = sky(&map);
    let mut count = 0;
    let mut area = 0.0;
    let mut radiance = 0.0;
    let start = std::time::Instant::now();
    HealpixBoundary {
        sky: &sky,
        nside: 512,
    }
    .visit(&mut |d| {
        count += 1;
        area += d.weight;
        radiance += d.incoming * d.weight;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, 12 * 512 * 512 / 2);
    assert!((area / TAU - 1.0).abs() < 5e-11);
    assert!((radiance / (0.025 * TAU) - 1.0).abs() < 5e-11);
    eprintln!(
        "streamed NSIDE512: {count} visible cells in {:?}",
        start.elapsed()
    );
}
