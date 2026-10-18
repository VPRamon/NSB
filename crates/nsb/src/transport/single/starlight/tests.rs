// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::units::qtty::{Albedos, Quantity};
use sha2::{Digest, Sha256};
use siderust::atmosphere::{AtmosphereProfile, AtmosphericOptics};
use siderust::coordinates::frames::EquatorialTrueOfDate;
use siderust::coordinates::spherical;
use siderust::coordinates::transform::ToHorizontal;
use siderust::healpix::{HealpixIndex, HealpixOrdering, Nside};

fn geometry() -> StarlightGeometry {
    StarlightGeometry {
        observer: Observer::new(
            Degrees::new(-111.59989),
            Degrees::new(31.96403),
            Meters::new(2097.0),
        ),
        tt: JulianDate::new(2459680.8),
        ut1: JulianDate::new(2459680.7992),
    }
}
fn payload(edges: &[u16], nside: u32, ordering: &str, anisotropic: bool) -> serde_json::Value {
    let grid = HealpixGrid::new(
        Nside::new(nside).unwrap(),
        if ordering == "ring" {
            HealpixOrdering::Ring
        } else {
            HealpixOrdering::Nested
        },
    )
    .unwrap();
    serde_json::json!({
        "schema":"nsb-starlight-spectral-map-v2", "product_id":"synthetic-transport-v1",
        "band_nm":[edges[0], edges[edges.len()-1]], "bin_edges_nm":edges,
        "nside":nside, "ordering":ordering, "coordinate_frame":"galactic",
        "domain":"top-of-atmosphere", "units":"photon/(cm2 ns sr)",
        "wavelength_convention":"vacuum", "maturity":"experimental",
        "provenance":{"source_catalogue":"synthetic", "source_license":"AGPL-3.0-only", "source_selection":"all", "generation_method":"offline deterministic fixture", "limitations":["synthetic pixel averages"]},
        "catalogue_rows":grid.npix(), "admitted_sources":grid.npix(), "excluded_sources":{},
        "pixels":(0..grid.npix()).map(|i| {
            let d = grid.pixel_center::<Galactic>(HealpixIndex::new(i)).unwrap();
            let scale = if anisotropic { 1.0 + 0.8*d.z() } else { 1.0 };
            serde_json::json!({"pixel":i,"sources":1,"radiance":edges.windows(2).enumerate().map(|(j,e)| scale * (j+1) as f64 * f64::from(e[1]-e[0])).collect::<Vec<_>>()})
        }).collect::<Vec<_>>()
    })
}
fn load(p: &serde_json::Value) -> StarlightMap {
    let bytes = serde_json::to_vec(p).unwrap();
    StarlightMap::from_json_bytes(
        &bytes,
        &Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}
fn sky(map: &StarlightMap, band: [f64; 2]) -> StarlightSky<'_> {
    StarlightSky::new(
        map,
        geometry(),
        band.map(Nanometers::new),
        WithinBinSpectrum::FlatPhotonDensity,
    )
    .unwrap()
}
fn phase(g: f64) -> AerosolPhase {
    AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(g).unwrap())
}
fn medium(r: f64, a: f64, ssa: f64, absorption: f64, layers: usize) -> LayeredAtmosphere {
    let c = AtmosphericOptics::from_extinction(
        Quantity::new(r / 1000.0),
        Quantity::new(a / 1000.0),
        Albedos::new(ssa),
        Quantity::new(absorption / 1000.0),
    )
    .unwrap();
    LayeredAtmosphere::try_new(
        &(0..=layers)
            .map(|i| Meters::new(2097.0 + 1000.0 * i as f64 / layers as f64))
            .collect::<Vec<_>>(),
        [Nanometers::new(360.), Nanometers::new(570.)],
        vec![c; layers],
    )
    .unwrap()
}
fn solver() -> SolverSettings {
    SolverSettings {
        refinements: 2,
        ..SolverSettings::default()
    }
}
fn view() -> Horizontal {
    Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.0))
}
fn solve(map: &StarlightMap, m: &LayeredAtmosphere, g: f64) -> Result<StarlightTransport> {
    starlight_single_scattering(
        m,
        &phase(g),
        &sky(map, [360., 390.]),
        view(),
        solver(),
        WavelengthSettings::default(),
    )
}
fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance * a.abs().max(b.abs()).max(1e-15),
        "{a} != {b}"
    );
}

#[test]
fn arbitrary_bins_typed_density_vacuum_native_integral_and_provenance() {
    for edges in [
        vec![360, 570],
        vec![360, 370, 400, 570],
        (360..=570).step_by(30).collect(),
        (360..=570).step_by(15).collect(),
    ] {
        let map = load(&payload(&edges, 1, "ring", false));
        for (j, e) in edges.windows(2).enumerate() {
            let s = sky(&map, [e[0], e[1]].map(f64::from));
            let mid = Nanometers::new(f64::from(e[0] + e[1]) / 2.0);
            close(
                s.radiance(mid, view()).unwrap().value(),
                (j + 1) as f64,
                1e-14,
            );
            let result = starlight_single_scattering(
                &medium(0., 0., 0., 0., 1),
                &phase(0.),
                &s,
                view(),
                solver(),
                WavelengthSettings::default(),
            )
            .unwrap();
            close(
                result.direct.value(),
                (j + 1) as f64 * f64::from(e[1] - e[0]),
                1e-14,
            );
            close(result.direct.value(), result.toa.value(), 1e-14);
            assert_eq!(result.single.value(), 0.);
            assert!(result.higher_orders.is_none() && result.uncertainty.is_none());
            assert_eq!(result.provenance, *map.provenance());
            assert_eq!(result.grid, map.grid());
            assert_eq!(
                result.band,
                [e[0], e[1]].map(|v| Nanometers::new(f64::from(v)))
            );
            assert_eq!(result.assumption, WithinBinSpectrum::FlatPhotonDensity);
            assert_eq!(result.wavelength_order, 4);
            assert_eq!(result.samples.len(), 4);
            assert!(result.planned_work <= budget::MAX_WORK);
        }
    }
}

#[test]
fn geometry_roundtrip_exact_direct_pixel_seam_poles_horizon_and_ordering() {
    for nside in [1, 2, 4] {
        let ring = load(&payload(&[360, 390], nside, "ring", true));
        let mut nested = payload(&[360, 390], nside, "ring", true);
        nested["ordering"] = serde_json::json!("nested");
        let bytes = serde_json::to_vec(&nested).unwrap();
        let checksum = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert!(StarlightMap::from_json_bytes(&bytes, &checksum)
            .unwrap_err()
            .to_string()
            .contains("NESTED"));
        for (lat, lon) in [(31.96, -111.6), (-24.6, -70.4), (89.9, 0.)] {
            for jd in [2451545.0, 2460000.5] {
                let mut context = geometry();
                context.observer =
                    Observer::new(Degrees::new(lon), Degrees::new(lat), Meters::new(2097.));
                context.tt = JulianDate::new(jd);
                context.ut1 = JulianDate::new(jd - 0.0008);
                let a = StarlightSky::new(
                    &ring,
                    context,
                    [Nanometers::new(360.), Nanometers::new(390.)],
                    WithinBinSpectrum::FlatPhotonDensity,
                )
                .unwrap();
                for (l, gb) in [
                    (0.0001, 12.3),
                    (359.9999, 12.3),
                    (123., 89.999),
                    (42., -89.999),
                    (90., 30.),
                ] {
                    let gal =
                        spherical::Direction::<Galactic>::new(Degrees::new(l), Degrees::new(gb))
                            .to_cartesian();
                    let icrs: Direction<ICRS> = gal.to_frame(&context.tt);
                    let eq: Direction<EquatorialTrueOfDate> = icrs.to_frame(&context.tt);
                    let local = ToHorizontal::to_horizontal(
                        &eq,
                        &context.ut1,
                        &context.tt,
                        &context.observer,
                    )
                    .to_spherical();
                    let back = a.galactic(local).unwrap();
                    for (x, y) in [
                        (gal.x(), back.x()),
                        (gal.y(), back.y()),
                        (gal.z(), back.z()),
                    ] {
                        assert!(
                            (x - y).abs() < 1e-9,
                            "lat={lat} jd={jd} gal=({l},{gb}): {x} != {y}; residual={}",
                            (x - y).abs()
                        );
                    }
                    let expected = ring.integrate_band(gal, a.band[0], a.band[1]).unwrap().0;
                    assert_eq!(a.toa(local).unwrap(), expected);
                }
                for alt in [-0.00001, 0., 0.00001, 90.] {
                    let d = Horizontal::new(Degrees::new(alt), Degrees::new(359.99999));
                    assert!(a.toa(d).unwrap().value() > 0.);
                }
            }
        }
    }
}

#[test]
fn absorption_rayleigh_aerosol_layers_and_authoritative_operator_agree() {
    let map = load(&payload(&[360, 390], 1, "ring", false));
    for (r, a, ssa, m, g) in [
        (0., 0.2, 0., 0.3, 0.65),
        (0.1, 0., 0., 0., 0.),
        (0., 0.2, 1., 0., 0.65),
        (0., 0.2, 1., 0., 0.85),
        (0., 0.2, 0., 0., 0.85),
    ] {
        let atmosphere = medium(r, a, ssa, m, 1);
        let result = solve(&map, &atmosphere, g).unwrap();
        let expected = single_scattering(
            &atmosphere,
            &phase(g),
            &sky(&map, [360., 390.]),
            Nanometers::new(375.),
            view(),
            solver(),
        )
        .unwrap();
        close(result.direct.value(), expected.direct.value() * 30., 1e-13);
        close(result.single.value(), expected.single.value() * 30., 1e-13);
        close(
            result.direct.value(),
            30. * (-(r + a + m) / 0.8).exp(),
            1e-13,
        );
        if r == 0. && ssa == 0. {
            assert_eq!(result.single.value(), 0.);
        }
        let layered = solve(&map, &medium(r, a, ssa, m, 8), g).unwrap();
        close(result.single.value(), layered.single.value(), 1e-4);
        assert!(result
            .samples
            .iter()
            .all(|s| s.convergence.angular_change.value() <= 1e-4));
    }
}

#[test]
fn anisotropic_pixel_boundary_diagnostic_fails_without_relaxing_tolerances() {
    for nside in [1, 2, 4] {
        let map = load(&payload(&[360, 390], nside, "ring", true));
        let result = solve(&map, &medium(0.1, 0.03, 1., 0., 1), 0.85);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("convergence failed"));
    }
}

#[test]
fn wavelength_profile_refinement_and_repeatability() {
    let map = load(&payload(&[360, 570], 1, "ring", false));
    let p = AtmosphereProfile {
        observer_altitude: Meters::new(2097.).to(),
        ..AtmosphereProfile::EL_PARANAL
    };
    let m = LayeredAtmosphere::from_profile(
        p,
        &[Meters::new(2097.), Meters::new(12000.)],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[1.],
        &[1.],
        Albedos::new(0.9),
    )
    .unwrap();
    let s = sky(&map, [360., 570.]);
    let run = |w| starlight_single_scattering(&m, &phase(0.65), &s, view(), solver(), w);
    let w = WavelengthSettings {
        refinements: 3,
        relative_tolerance: 1e-7,
        ..WavelengthSettings::default()
    };
    let result = run(w).unwrap();
    assert!(result.wavelength_order >= 8);
    assert!(result
        .wavelength_change
        .iter()
        .all(|v| v.value().is_finite()));
    let repeat = run(w).unwrap();
    assert_eq!(result.direct, repeat.direct);
    assert_eq!(result.single, repeat.single);
    let refined = run(WavelengthSettings {
        nodes: 8,
        refinements: 1,
        ..w
    })
    .unwrap();
    close(result.direct.value(), refined.direct.value(), 1e-7);
    close(result.single.value(), refined.single.value(), 1e-7);
    let err = run(WavelengthSettings {
        refinements: 1,
        relative_tolerance: 1e-14,
        absolute_tolerance: BandRadiance::new(0.),
        ..w
    })
    .unwrap_err();
    assert!(err.to_string().contains("wavelength convergence failed"));
}

#[test]
fn invalid_source_geometry_spectral_requests_and_resource_plans_fail_closed() {
    let map = load(&payload(&[360, 390, 420], 1, "ring", false));
    for band in [
        [360., 380.],
        [350., 390.],
        [360., 420.],
        [390., 391.],
        [420., 450.],
        [f64::NAN, 390.],
        [390., f64::INFINITY],
    ] {
        assert!(StarlightSky::new(
            &map,
            geometry(),
            band.map(Nanometers::new),
            WithinBinSpectrum::FlatPhotonDensity
        )
        .is_err());
    }
    let outside = load(&payload(&[300, 360], 1, "ring", false));
    assert!(StarlightSky::new(
        &outside,
        geometry(),
        [Nanometers::new(300.), Nanometers::new(360.)],
        WithinBinSpectrum::FlatPhotonDensity
    )
    .is_err());
    for i in 0..6 {
        let mut c = geometry();
        match i {
            0 => c.observer.lon = Degrees::new(f64::NAN),
            1 => c.observer.lat = Degrees::new(91.),
            2 => c.observer.height = Meters::new(f64::INFINITY),
            3 => c.tt = JulianDate::new(f64::INFINITY),
            4 => c.ut1 = JulianDate::new(f64::INFINITY),
            _ => c.tt = JulianDate::new(1e308),
        }
        assert!(StarlightSky::new(
            &map,
            c,
            [Nanometers::new(360.), Nanometers::new(390.)],
            WithinBinSpectrum::FlatPhotonDensity
        )
        .is_err());
    }
    let s = sky(&map, [360., 390.]);
    for w in [350., 400., f64::NAN] {
        assert!(s.radiance(Nanometers::new(w), view()).is_err());
    }
    assert!(s
        .toa(Horizontal::new_unchecked(
            Degrees::new(f64::NAN),
            Degrees::new(0.)
        ))
        .is_err());
    let m = medium(0., 0., 0., 0., 1);
    let run = |solver, w| starlight_single_scattering(&m, &phase(0.), &s, view(), solver, w);
    for w in [
        WavelengthSettings {
            nodes: 0,
            ..WavelengthSettings::default()
        },
        WavelengthSettings {
            refinements: usize::MAX,
            ..WavelengthSettings::default()
        },
        WavelengthSettings {
            nodes: 16,
            refinements: 4,
            ..WavelengthSettings::default()
        },
        WavelengthSettings {
            relative_tolerance: 0.,
            ..WavelengthSettings::default()
        },
        WavelengthSettings {
            relative_tolerance: f64::NAN,
            ..WavelengthSettings::default()
        },
        WavelengthSettings {
            absolute_tolerance: BandRadiance::new(-1.),
            ..WavelengthSettings::default()
        },
    ] {
        assert!(run(solver(), w).is_err());
    }
    assert!(
        run(SolverSettings::default(), WavelengthSettings::default())
            .unwrap_err()
            .to_string()
            .contains("complete wavelength plan")
    );
    assert!(run(
        SolverSettings {
            mu_nodes: 512,
            ..solver()
        },
        WavelengthSettings::default()
    )
    .is_err());
    let mut c = geometry();
    c.observer.height = Meters::new(0.);
    let wrong = StarlightSky::new(&map, c, s.band, s.assumption).unwrap();
    assert!(starlight_single_scattering(
        &m,
        &phase(0.),
        &wrong,
        view(),
        solver(),
        WavelengthSettings::default()
    )
    .unwrap_err()
    .to_string()
    .contains("height"));
    let narrow = LayeredAtmosphere::try_new(
        m.edges(),
        [Nanometers::new(370.), Nanometers::new(380.)],
        vec![m.coefficients(Nanometers::new(375.), m.edges()[0]).unwrap()],
    )
    .unwrap();
    assert!(starlight_single_scattering(
        &narrow,
        &phase(0.),
        &s,
        view(),
        solver(),
        WavelengthSettings::default()
    )
    .unwrap_err()
    .to_string()
    .contains("complete Starlight bin"));
}

#[test]
fn loader_integrity_and_legacy_contract_remain_authoritative() {
    use crate::components::starlight::StarlightPixel;
    use crate::units::radiometry::S10s;
    let p = payload(&[360, 390], 1, "ring", false);
    let bytes = serde_json::to_vec(&p).unwrap();
    assert!(StarlightMap::from_json_bytes(&bytes, &"0".repeat(64)).is_err());
    for (field, value) in [
        ("schema", serde_json::json!("unknown")),
        ("ordering", serde_json::json!("unknown")),
        ("domain", serde_json::json!("ground")),
        ("bin_edges_nm", serde_json::json!([360, 380, 390])),
        ("pixels", serde_json::json!([])),
    ] {
        let mut bad = p.clone();
        bad[field] = value;
        let b = serde_json::to_vec(&bad).unwrap();
        assert!(StarlightMap::from_json_bytes(
            &b,
            &Sha256::digest(&b)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
        .is_err());
    }
    for value in [serde_json::json!(-1.), serde_json::Value::Null] {
        let mut bad = p.clone();
        bad["pixels"][0]["radiance"][0] = value;
        let b = serde_json::to_vec(&bad).unwrap();
        assert!(StarlightMap::from_json_bytes(
            &b,
            &Sha256::digest(&b)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
        .is_err());
    }
    let map = load(&p);
    let legacy = StarlightMap::from_healpix(
        map.grid(),
        vec![StarlightPixel::new(BandRadiance::new(1.), S10s::new(0.), S10s::new(0.)); 12],
        map.provenance().clone(),
    )
    .unwrap();
    assert!(StarlightSky::new(
        &legacy,
        geometry(),
        [Nanometers::new(360.), Nanometers::new(390.)],
        WithinBinSpectrum::FlatPhotonDensity
    )
    .is_err());
    assert_eq!(legacy.band_nm(), [300, 650]);
}

#[test]
fn representative_layered_cost_diagnostic() {
    for anisotropic in [false, true] {
        let map = load(&payload(&[360, 390, 420, 450], 4, "ring", anisotropic));
        let m = medium(0.1, 0.03, 1., 0., 8);
        let start = std::time::Instant::now();
        for edges in map.bin_edges_nm().windows(2) {
            let s = sky(&map, [f64::from(edges[0]), f64::from(edges[1])]);
            let result = starlight_single_scattering(
                &m,
                &phase(0.65),
                &s,
                view(),
                solver(),
                WavelengthSettings::default(),
            );
            match result {
                Ok(r) => {
                    assert!(!anisotropic);
                    println!("uniform NSIDE4 layers8 band={:?} work={} order={} changes={:?} scalar_orders={:?}",r.band,r.planned_work,r.wavelength_order,r.wavelength_change,r.samples[0].convergence.orders);
                }
                Err(e) => {
                    assert!(anisotropic);
                    assert!(e.to_string().contains("convergence failed"));
                }
            }
        }
        println!(
            "anisotropic={anisotropic} NSIDE4 bins3 layers8 elapsed={:?}",
            start.elapsed()
        );
    }
}
