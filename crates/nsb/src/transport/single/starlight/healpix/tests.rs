// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::super::tests::{close, geometry, load, medium, payload, phase, sky, solver, view};
use super::*;

#[test]
fn ring_subdivision_preserves_every_parent_area_and_antipodal_partition() {
    for n in [1, 2, 3, 4] {
        let parent = HealpixGrid::ring(Nside::new(n).unwrap()).unwrap();
        for factor in [1, 2, 4] {
            let fine = HealpixGrid::ring(Nside::new(n * factor).unwrap()).unwrap();
            let mut counts = vec![0; parent.npix() as usize];
            for i in 0..fine.npix() / 2 {
                let d = fine.pixel_center::<Galactic>(HealpixIndex::new(i)).unwrap();
                for sign in [1.0, -1.0] {
                    let p = parent
                        .direction_to_pixel(Direction::<Galactic>::new_unchecked(
                            sign * d.x(),
                            sign * d.y(),
                            sign * d.z(),
                        ))
                        .unwrap();
                    counts[p.get() as usize] += 1;
                }
            }
            assert!(counts.iter().all(|&v| v == factor * factor));
            close(
                fine.pixel_area_sr() * fine.npix() as f64 / 2.0,
                2.0 * PI,
                1e-14,
            );
        }
    }
}

#[test]
fn uniform_pixel_kernel_matches_authoritative_gl_path() {
    let map = load(&payload(&[360, 390], 1, "ring", false));
    let s = sky(&map, [360., 390.]);
    let m = medium(0.1, 0.03, 0.9, 0.02, 2);
    let p = phase(0.65);
    let w = Nanometers::new(375.);
    let reference = single_scattering(&m, &p, &s, w, view(), solver()).unwrap();
    let angular = HealpixSettings {
        refinements: 8,
        ..HealpixSettings::default()
    };
    estimate(&s, 2, angular, &p).unwrap();
    let (direct, single, c) = solve(&m, &p, &s, w, view(), angular, None).unwrap();
    close(direct, reference.direct.value(), 1e-14);
    close(single, reference.single.value(), 1e-4);
    println!(
        "uniform HEALPix {:?}; single={single}; GL={}",
        c,
        reference.single.value()
    );
}

#[test]
#[ignore = "optional unchanged frozen-map diagnostic; set NSB_FROZEN_MAP"]
fn frozen_subpixel_probe() {
    use crate::units::qtty::{Albedos, Hectopascals, OpticalDepths};
    use siderust::atmosphere::{AtmosphereProfile, MieParams};
    let path = std::env::var("NSB_FROZEN_MAP").unwrap();
    let map = StarlightMap::from_json_file(
        std::path::Path::new(&path),
        "fe331343cb9797a1d871ae798c88d2ae8865ae39358e8d82b464a91b3cf87d5f",
    )
    .unwrap();
    let mut g = geometry();
    g.tt = JulianDate::new(2459679.9675055384);
    g.ut1 = JulianDate::new(2459679.966703672);
    let profile = AtmosphereProfile {
        surface_pressure: Hectopascals::new(779.5),
        observer_altitude: Meters::new(2097.).to(),
        mie_params: MieParams {
            tau0: OpticalDepths::new(0.03),
            lambda_ref: Nanometers::new(550.),
            alpha: 1.38,
        },
        ..AtmosphereProfile::EL_PARANAL
    };
    let m = LayeredAtmosphere::from_profile(
        profile,
        &[Meters::new(2097.), Meters::new(12097.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[1.],
        &[1.],
        Albedos::new(1.),
    )
    .unwrap();
    let view = Horizontal::new(Degrees::new(76.9256911), Degrees::new(271.8975544));
    for e in map.bin_edges_nm().windows(2) {
        let s = StarlightSky::new(
            &map,
            g,
            [e[0], e[1]].map(|v| Nanometers::new(f64::from(v))),
            WithinBinSpectrum::FlatPhotonDensity,
        )
        .unwrap();
        println!("pilot_basis={:?}", s.basis);
        let w = Nanometers::new(f64::from(e[0] + e[1]) / 2.);
        let p = phase(0.65);
        let kernel = quadrature::Kernel {
            atmosphere: &m,
            phase: &p,
            sky: &s,
            wavelength: w,
            view,
        };
        let layers = kernel.layer_responses().unwrap();
        let mut prev = None;
        for n in [128_u32, 256, 512, 1024] {
            let settings = HealpixSettings {
                refinements: n.ilog2() as usize - 7,
                ..HealpixSettings::default()
            };
            // Each reported monochromatic diagnostic is individually bounded.
            if n >= 512 {
                estimate(&s, 1, settings, &p).unwrap();
            }
            let t = std::time::Instant::now();
            let value = integrate(&s, &kernel, &layers, n).unwrap();
            println!(
                "bin={e:?} nside={n} bin-center-single={} relative_change={:?} elapsed={:?}",
                value * 30.,
                prev.map(|a: f64| (value / a - 1.).abs()),
                t.elapsed()
            );
            prev = Some(value);
        }
    }
}

#[test]
fn sparse_and_anisotropic_pixels_match_independent_boundary_quadrature() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/data/starlight-transport/angular-independent.json"
    ))
    .unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let mut input = payload(&[360, 390], 1, "ring", case["pixel"].is_null());
        if let Some(pixel) = case["pixel"].as_u64() {
            for (i, p) in input["pixels"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                p["radiance"] = serde_json::json!([if i as u64 == pixel { 30.0 } else { 0.0 }]);
            }
        }
        let map = load(&input);
        let s = sky(&map, [360., 390.]);
        let m = medium(0.1, 0.03, 0.9, 0.02, 1);
        let p = phase(case["g"].as_f64().unwrap());
        let settings = HealpixSettings {
            refinements: 9,
            ..HealpixSettings::default()
        };
        estimate(&s, 1, settings, &p).unwrap();
        let (_, value, c) =
            solve(&m, &p, &s, Nanometers::new(375.), view(), settings, None).unwrap();
        let expected = case["single"].as_f64().unwrap();
        if expected == 0. {
            assert_eq!(value, 0.);
        } else {
            close(value, expected, 1e-4);
        }
        assert!(c
            .phase_normalization
            .iter()
            .flatten()
            .all(|v| (v - 1.0).abs() <= settings.relative_tolerance));
        println!(
            "independent {}: value={value:.17e} expected={expected:.17e} nside={} relative={:.3e}",
            case["name"],
            c.nside,
            if expected == 0. {
                0.
            } else {
                value / expected - 1.
            }
        );
    }
}

#[test]
fn independent_horizon_areas_and_rotated_seam_partition() {
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/data/starlight-transport/angular-independent.json"
    ))
    .unwrap();
    let map = load(&payload(&[360, 390], 1, "ring", false));
    let s = sky(&map, [360., 390.]);
    let mut areas = [0.; 12];
    visit_hemisphere(&s, 256, |gal, _, area| {
        let i = map.grid().direction_to_pixel(gal).unwrap().get() as usize;
        areas[i] += area.value();
        Ok(())
    })
    .unwrap();
    close(areas.iter().sum(), 2. * PI, 1e-12);
    for case in reference["cases"].as_array().unwrap() {
        if let Some(i) = case["pixel"].as_u64() {
            let expected = case["visible_area_sr"].as_f64().unwrap();
            assert!(
                (areas[i as usize] - expected).abs() < 2e-4,
                "{areas:?} {case}"
            );
        }
    }
    // An exact quarter-turn is a symmetry of RING pixels, including the seam.
    let input = payload(&[360, 390], 2, "ring", true);
    let original = load(&input);
    let mut rotated = input.clone();
    for i in 0..original.grid().npix() {
        let d = original
            .grid()
            .pixel_center::<Galactic>(HealpixIndex::new(i))
            .unwrap();
        let j = original
            .grid()
            .direction_to_pixel(Direction::<Galactic>::new_unchecked(-d.y(), d.x(), d.z()))
            .unwrap()
            .get();
        rotated["pixels"][j as usize]["radiance"] = input["pixels"][i as usize]["radiance"].clone();
    }
    let rotated = load(&rotated);
    let a = sky(&original, [360., 390.]);
    let mut b = sky(&rotated, [360., 390.]);
    for c in &mut b.basis {
        *c = [-c[1], c[0], c[2]];
    }
    let m = medium(0.1, 0.03, 0.9, 0.02, 2);
    let p = phase(0.65);
    let eval = |s: &StarlightSky<'_>, n| {
        let k = quadrature::Kernel {
            atmosphere: &m,
            phase: &p,
            sky: s,
            wavelength: Nanometers::new(375.),
            view: view(),
        };
        integrate(s, &k, &k.layer_responses().unwrap(), n).unwrap()
    };
    close(eval(&a, 64), eval(&b, 64), 1e-12);
    // Refining the representation must preserve each original constant mean.
    let mut finer = payload(&[360, 390], 4, "ring", false);
    let grid = HealpixGrid::ring(Nside::new(4).unwrap()).unwrap();
    for i in 0..grid.npix() {
        let d = grid.pixel_center::<Galactic>(HealpixIndex::new(i)).unwrap();
        let parent = original.grid().direction_to_pixel(d).unwrap().get();
        finer["pixels"][i as usize]["radiance"] =
            input["pixels"][parent as usize]["radiance"].clone();
    }
    let finer = load(&finer);
    close(eval(&a, 64), eval(&sky(&finer, [360., 390.]), 64), 1e-14);
    assert_eq!(eval(&a, 64).to_bits(), eval(&a, 64).to_bits());
}

#[test]
fn bounded_settings_phase_resolution_and_spectral_modes() {
    use crate::units::qtty::Quantity;
    use siderust::optica::phase::ScatteringFactor;
    let map = load(&payload(&[360, 390], 1, "ring", false));
    let s = sky(&map, [360., 390.]);
    let p = phase(0.65);
    for angular in [
        HealpixSettings {
            subdivision: 0,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            subdivision: 3,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            subdivision: 128,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            refinements: 1,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            refinements: 11,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            relative_tolerance: f64::NAN,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            relative_tolerance: 0.,
            ..HealpixSettings::default()
        },
        HealpixSettings {
            absolute_tolerance: Radiance::new(-1.),
            ..HealpixSettings::default()
        },
        HealpixSettings {
            absolute_tolerance: Radiance::new(1e-11),
            ..HealpixSettings::default()
        },
        HealpixSettings {
            subdivision: 64,
            refinements: 10,
            ..HealpixSettings::default()
        },
    ] {
        assert!(estimate(&s, 1, angular, &p).is_err());
    }
    assert!(estimate(&s, usize::MAX, HealpixSettings::default(), &p).is_err());
    assert!(estimate(&s, 100_000, HealpixSettings::default(), &p).is_err());
    let m = medium(0., 0.1, 1., 0., 1);
    // Successive tiny values cannot certify an unresolved delta-like HG lobe.
    assert!(solve(
        &m,
        &phase(0.999999),
        &s,
        Nanometers::new(375.),
        view(),
        HealpixSettings::default(),
        None
    )
    .unwrap_err()
    .to_string()
    .contains("convergence"));
    let constant = Quantity::<ScatteringFactor>::new(1. / (4. * PI));
    let table = AerosolPhase::Tabulated(Box::new(
        NormalizedPhaseTable::new(
            &[Nanometers::new(360.), Nanometers::new(570.)],
            &[Radians::new(0.), Radians::new(PI)],
            &[constant; 4],
        )
        .unwrap(),
    ));
    let double = AerosolPhase::DoubleHenyeyGreenstein(
        DoubleHenyeyGreensteinPhaseFunction::try_new(0., 0., 0.7).unwrap(),
    );
    let angular = HealpixSettings {
        refinements: 8,
        ..HealpixSettings::default()
    };
    let expected = solve(
        &m,
        &phase(0.),
        &s,
        Nanometers::new(375.),
        view(),
        angular,
        None,
    )
    .unwrap()
    .1;
    for phase in [&table, &double] {
        let result = starlight_healpix_single_scattering(
            &m,
            phase,
            &s,
            view(),
            angular,
            WavelengthSettings::default(),
        )
        .unwrap();
        close(result.single.value() / 30., expected, 1e-12);
        assert!(result.solver_settings.is_none() && result.healpix_settings.is_some());
        assert!(result
            .samples
            .iter()
            .all(|v| v.convergence.is_none() && v.healpix_convergence.is_some()));
    }
    for edges in [
        vec![360, 570],
        vec![360, 370, 400, 570],
        (360..=570).step_by(30).collect(),
    ] {
        let map = load(&payload(&edges, 1, "ring", false));
        for (j, e) in edges.windows(2).enumerate() {
            let s = sky(&map, [f64::from(e[0]), f64::from(e[1])]);
            for absorption in [0., 0.2] {
                let result = starlight_healpix_single_scattering(
                    &medium(0., 0., 0., absorption, 1),
                    &p,
                    &s,
                    view(),
                    HealpixSettings::default(),
                    WavelengthSettings::default(),
                )
                .unwrap();
                close(
                    result.direct.value(),
                    (j + 1) as f64 * f64::from(e[1] - e[0]) * (-absorption / 0.8).exp(),
                    1e-13,
                );
                assert_eq!(result.single.value(), 0.);
                assert!(result.uncertainty.is_none() && result.higher_orders.is_none());
                assert_eq!(
                    result.samples[0]
                        .healpix_convergence
                        .as_ref()
                        .unwrap()
                        .phase_normalization,
                    [None, None]
                );
            }
        }
    }
    let large = load(&payload(&[360, 390], 16, "ring", false));
    assert!(starlight_healpix_single_scattering(
        &m,
        &p,
        &sky(&large, [360., 390.]),
        view(),
        HealpixSettings {
            refinements: 4,
            ..HealpixSettings::default()
        },
        WavelengthSettings {
            nodes: 16,
            refinements: 2,
            ..WavelengthSettings::default()
        }
    )
    .unwrap_err()
    .to_string()
    .contains("wavelength plan"));
}
