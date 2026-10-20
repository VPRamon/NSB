// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::transport::RadianceOrigin;
use crate::units::qtty::{Albedos, Meters, Quantity};
use siderust::atmosphere::AtmosphericOptics;
use siderust::optica::phase::HenyeyGreensteinPhaseFunction;

struct Sky {
    scale: f64,
    dipole: f64,
    azimuthal: f64,
}
impl IncidentSky for Sky {
    fn coverage(&self) -> SkyCoverage {
        SkyCoverage {
            identity: "synthetic-smooth-sky-multiple-v1".into(),
            wavelengths: [Nanometers::new(360.0), Nanometers::new(570.0)],
            full_hemisphere: true,
            origin: RadianceOrigin::TopOfAtmosphere,
        }
    }
    fn radiance(&self, _w: Nanometers, d: Horizontal) -> Result<Radiance> {
        Ok(Radiance::new(
            self.scale
                * (1.0
                    + self.dipole * d.alt().to::<Radian>().sin()
                    + self.azimuthal
                        * d.alt().to::<Radian>().cos()
                        * (d.az().to::<Radian>().value() - 23_f64.to_radians()).cos()),
        ))
    }
}
fn atmosphere(rayleigh: f64, aerosol: f64, ssa: f64, absorption: f64) -> LayeredAtmosphere {
    let optics = AtmosphericOptics::from_extinction(
        Quantity::new(rayleigh / 1000.0),
        Quantity::new(aerosol / 1000.0),
        Albedos::new(ssa),
        Quantity::new(absorption / 1000.0),
    )
    .unwrap();
    LayeredAtmosphere::try_new(
        &[Meters::new(2100.0), Meters::new(3100.0)],
        [Nanometers::new(360.0), Nanometers::new(570.0)],
        vec![optics],
    )
    .unwrap()
}
fn aerosol() -> AerosolPhase {
    AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.0).unwrap())
}
fn view() -> Horizontal {
    Horizontal::new(Degrees::new(53.13010235415598), Degrees::new(23.0))
}
fn sky() -> Sky {
    Sky {
        scale: 0.025,
        dipole: 0.2,
        azimuthal: 0.0,
    }
}
fn relaxed() -> MultipleScatteringSettings {
    MultipleScatteringSettings {
        angular_tolerance: 0.05,
        vertical_tolerance: 0.05,
        ..MultipleScatteringSettings::default()
    }
}
fn compute(atm: &LayeredAtmosphere) -> MultipleScattering {
    multiple_scattering(
        atm,
        &aerosol(),
        &sky(),
        Nanometers::new(450.0),
        view(),
        relaxed(),
    )
    .unwrap()
}

#[test]
fn vacuum_is_exact_and_scattered_orders_are_zero() {
    let r = compute(&atmosphere(0.0, 0.0, 0.0, 0.0));
    let toa = sky().radiance(Nanometers::new(450.0), view()).unwrap();
    assert_eq!(r.direct, toa);
    assert_eq!(r.single.value(), 0.0);
    assert_eq!(r.higher_orders.value(), 0.0);
    assert_eq!(r.total.value(), toa.value());
    assert!(r.convergence.orders > 0);
}
#[test]
fn absorption_and_zero_ssa_cannot_create_scattered_light() {
    let r = compute(&atmosphere(0.0, 0.2, 0.0, 0.1));
    assert_eq!(r.single.value(), 0.0);
    assert_eq!(r.higher_orders.value(), 0.0);
    assert!(r.direct.value() > 0.0);
    assert!(r.direct.value() < 0.025 * 1.2);
}
#[test]
fn rayleigh_has_nonzero_second_and_higher_orders() {
    let r = compute(&atmosphere(0.08, 0.0, 0.0, 0.0));
    assert!(r.single.value() > 0.0);
    assert!(r.higher_orders.value() > 0.0);
    assert!(r.total.value() > r.direct.value() + r.single.value());
    assert_eq!(
        r.total_scattered.value(),
        r.single.value() + r.higher_orders.value()
    );
    assert!(r.convergence.contraction_bound < 1.0);
}
#[test]
fn first_order_agrees_with_existing_reference_in_thin_sky() {
    let atm = atmosphere(0.015, 0.01, 0.7, 0.005);
    let p = aerosol();
    let sky = sky();
    let first = single::single_scattering(
        &atm,
        &p,
        &sky,
        Nanometers::new(450.0),
        view(),
        single::SolverSettings::default(),
    )
    .unwrap();
    // Isolate order one; higher-order angular aliasing must not hide
    // first-order equivalence on the specified joint numerical grid.
    let multiple = solution(
        &atm,
        &p,
        &sky,
        Nanometers::new(450.0),
        view(),
        relaxed(),
        (2, 2),
    )
    .unwrap();
    let relative = (first.single.value() - multiple.first).abs() / first.single.value();
    assert!(relative < 0.05, "first-order difference: {relative}");
    assert!(multiple.higher > 0.0);
}
#[test]
fn overflow_and_nonconvergence_are_errors_not_partial_results() {
    let atm = atmosphere(0.02, 0.0, 0.0, 0.0);
    let mut s = relaxed();
    s.max_orders = usize::MAX;
    assert!(
        multiple_scattering(&atm, &aerosol(), &sky(), Nanometers::new(450.0), view(), s).is_err()
    );
    s = relaxed();
    s.mu_nodes = 8;
    s.azimuth_nodes = 16;
    s.cells_per_layer = 16;
    s.max_orders = 64;
    assert!(
        multiple_scattering(&atm, &aerosol(), &sky(), Nanometers::new(450.0), view(), s).is_err()
    );
    s = relaxed();
    s.max_orders = 2;
    s.order_tolerance = 1e-10;
    s.absolute_tolerance = Radiance::new(0.0);
    assert!(
        multiple_scattering(&atm, &aerosol(), &sky(), Nanometers::new(450.0), view(), s).is_err()
    );
}

/// Offline dense-matrix oracle: invert (I - T*S) rather than iterate orders.
/// Same finite-cell/ordinate discretization, so this verifies the recurrence
/// but is not an independent continuum-transfer or Gaia accuracy claim.
#[test]
fn higher_orders_match_offline_dense_linear_system_reference() {
    let atm = atmosphere(0.0, 0.12, 0.7, 0.02);
    let sky = Sky {
        scale: 0.025,
        dipole: 0.0,
        azimuthal: 0.0,
    };
    let s = MultipleScatteringSettings {
        mu_nodes: 4,
        azimuth_nodes: 8,
        cells_per_layer: 4,
        max_orders: 48,
        order_tolerance: 1e-8,
        absolute_tolerance: Radiance::new(1e-14),
        ..relaxed()
    };
    let p = aerosol();
    let v = Horizontal::new(
        Degrees::new(0.8_f64.asin().to_degrees()),
        Degrees::new(23.0),
    );
    let result = solution(&atm, &p, &sky, Nanometers::new(450.0), v, s, (1, 1)).unwrap();
    // numpy.linalg.solve generator: nsb-validation scripts/multiple-scattering/multiple-scattering-matrix.py
    let first = 0.0009520451506273049;
    let higher = 0.00013390123542604275;
    assert!((result.first - first).abs() < 1e-10);
    assert!((result.higher - higher).abs() < 1e-10);
    assert!(result.orders >= 3);
    assert!(result.bound < 1e-10);
}

/// Independent continuum oracle: integrate internal angles analytically, then
/// solve a 1D E1/Fredholm integral equation (SciPy expn + dense inverse).
/// This does not share Rust's full-sphere ordinate or source-sweep algorithm.
#[test]
fn homogeneous_isotropic_matches_independent_fredholm_reference() {
    let atm = atmosphere(0.0, 0.12, 0.7, 0.02);
    let sky = Sky {
        scale: 0.025,
        dipole: 0.0,
        azimuthal: 0.0,
    };
    let s = MultipleScatteringSettings {
        mu_nodes: 4,
        azimuth_nodes: 8,
        cells_per_layer: 4,
        max_orders: 48,
        ..relaxed()
    };
    let p = aerosol();
    let v = Horizontal::new(
        Degrees::new(0.8_f64.asin().to_degrees()),
        Degrees::new(23.0),
    );
    let r = solution(&atm, &p, &sky, Nanometers::new(450.0), v, s, (2, 2)).unwrap();
    // Offline 256-cell SciPy fixture generated by the E1/Fredholm solver.
    // Refinement changes are < 2e-6 (first) and < 1e-6 (higher) absolute.
    let first = 0.0009511033150319675;
    let higher = 0.0001342609721455303;
    let relative_first = (r.first - first).abs() / first;
    let relative_higher = (r.higher - higher).abs() / higher;
    assert!(
        relative_first < 0.02,
        "first-order E1 oracle delta={relative_first}"
    );
    assert!(
        relative_higher < 0.02,
        "higher-order E1 oracle delta={relative_higher}"
    );
}

/// Uniform medium divided into two equivalent Siderust layers must preserve
/// the cell optical depths and all scattering orders when cell edges match.
#[test]
fn split_layer_invariance_for_bidirectional_orders() {
    let whole = atmosphere(0.0, 0.12, 0.7, 0.02);
    let coeff = AtmosphericOptics::from_extinction(
        Quantity::new(0.0),
        Quantity::new(0.12 / 1000.0),
        Albedos::new(0.7),
        Quantity::new(0.02 / 1000.0),
    )
    .unwrap();
    let split = LayeredAtmosphere::try_new(
        &[
            Meters::new(2100.0),
            Meters::new(2600.0),
            Meters::new(3100.0),
        ],
        [Nanometers::new(360.0), Nanometers::new(570.0)],
        vec![coeff, coeff],
    )
    .unwrap();
    let src = Sky {
        scale: 0.025,
        dipole: 0.0,
        azimuthal: 0.0,
    };
    let view = Horizontal::new(
        Degrees::new(0.8_f64.asin().to_degrees()),
        Degrees::new(23.0),
    );
    let base = MultipleScatteringSettings {
        cells_per_layer: 4,
        ..relaxed()
    };
    let divided = MultipleScatteringSettings {
        cells_per_layer: 2,
        ..relaxed()
    };
    let a = solution(
        &whole,
        &aerosol(),
        &src,
        Nanometers::new(450.0),
        view,
        base,
        (1, 1),
    )
    .unwrap();
    let b = solution(
        &split,
        &aerosol(),
        &src,
        Nanometers::new(450.0),
        view,
        divided,
        (1, 1),
    )
    .unwrap();
    assert!((a.first - b.first).abs() < 1e-13);
    assert!((a.higher - b.higher).abs() < 1e-13);
}

/// Eigenmode/Fourier DISORT, independently integrated continuous vertical
/// transport, 256 streams. Generator and dependency receipt are in nsb-validation.
#[test]
fn independent_disort_rayleigh_hg_mixed_stratified_and_thicker() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/multiple-scattering/disort-v1.json"
    ))
    .unwrap();
    for (name, case) in fixture["cases"].as_object().unwrap() {
        let layers = case["layers_bottom_to_top"].as_array().unwrap();
        let optics = layers
            .iter()
            .map(|layer| {
                let c: Vec<f64> = layer
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect();
                AtmosphericOptics::from_extinction(
                    Quantity::new(c[0] / 1000.),
                    Quantity::new(c[1] / 1000.),
                    Albedos::new(c[2]),
                    Quantity::new(c[3] / 1000.),
                )
                .unwrap()
            })
            .collect();
        let edges: Vec<_> = (0..=layers.len())
            .map(|j| Meters::new(2100. + 1000. * j as f64))
            .collect();
        let atm = LayeredAtmosphere::try_new(
            &edges,
            [Nanometers::new(360.), Nanometers::new(570.)],
            optics,
        )
        .unwrap();
        let phase = AerosolPhase::HenyeyGreenstein(
            HenyeyGreensteinPhaseFunction::try_new(case["g"].as_f64().unwrap()).unwrap(),
        );
        let expected = case["results"].as_array().unwrap().last().unwrap();
        let settings = MultipleScatteringSettings {
            order_tolerance: 1e-8,
            ..relaxed()
        };
        let source = Sky {
            scale: 0.025,
            dipole: case["dipole"].as_f64().unwrap(),
            azimuthal: case["azimuthal"].as_f64().unwrap_or(0.),
        };
        let result = solution(
            &atm,
            &phase,
            &source,
            Nanometers::new(450.),
            view(),
            settings,
            (2, 2),
        )
        .unwrap();
        let authoritative = single::single_scattering(
            &atm,
            &phase,
            &source,
            Nanometers::new(450.),
            view(),
            single::SolverSettings::default(),
        )
        .unwrap();
        assert!(
            (result.first / authoritative.single.value() - 1.).abs() < 0.001,
            "{name}: authoritative order one"
        );
        let start = std::time::Instant::now();
        let accepted = multiple_scattering(
            &atm,
            &phase,
            &source,
            Nanometers::new(450.),
            view(),
            MultipleScatteringSettings {
                azimuth_nodes: 16,
                cells_per_layer: 4,
                max_orders: if layers.len() == 1 { 24 } else { 16 },
                order_tolerance: 1e-8,
                ..relaxed()
            },
        )
        .unwrap();
        eprintln!(
            "public {name}: elapsed={:?} first={} higher={} diagnostics={:?}",
            start.elapsed(),
            accepted.single.value(),
            accepted.higher_orders.value(),
            accepted.convergence
        );
        let first = expected["single"].as_f64().unwrap();
        let higher = expected["higher"].as_f64().unwrap();
        assert!((accepted.single.value() / first - 1.).abs() < 0.01);
        assert!((accepted.higher_orders.value() / higher - 1.).abs() < 0.01);
        let e1 = (result.first / first - 1.).abs();
        let eh = (result.higher / higher - 1.).abs();
        eprintln!(
            "{name}: first={} higher={} first_relative={e1} higher_relative={eh}",
            result.first, result.higher
        );
        assert!(e1 < 0.01, "{name}: {e1}");
        assert!(eh < 0.01, "{name}: {eh}");
    }
}

#[test]
fn ground_tail_bound_contains_actual_remaining_orders() {
    let atm = atmosphere(0.04, 0.06, 0.8, 0.02);
    let mut s = relaxed();
    s.order_tolerance = 0.01;
    s.absolute_tolerance = Radiance::new(0.);
    let short = solution(
        &atm,
        &aerosol(),
        &sky(),
        Nanometers::new(450.),
        view(),
        s,
        (1, 1),
    )
    .unwrap();
    s.order_tolerance = 1e-12;
    let long = solution(
        &atm,
        &aerosol(),
        &sky(),
        Nanometers::new(450.),
        view(),
        s,
        (1, 1),
    )
    .unwrap();
    let omitted = long.higher - short.higher;
    assert!(omitted >= 0. && omitted <= short.bound);
    assert!(short.bound <= 0.01 * (short.first + short.higher));
    assert!(short.orders < long.orders);
}

#[test]
fn zero_sky_has_black_boundaries_and_no_internal_source() {
    let dark = Sky {
        scale: 0.,
        dipole: 0.,
        azimuthal: 0.0,
    };
    let r = multiple_scattering(
        &atmosphere(0.04, 0.04, 0.8, 0.01),
        &aerosol(),
        &dark,
        Nanometers::new(450.),
        view(),
        relaxed(),
    )
    .unwrap();
    assert_eq!(r.total.value(), 0.);
    assert_eq!(r.convergence.order_tail_bound.value(), 0.);
}

#[test]
fn validation_and_aliasing_fail_closed() {
    assert!(product(&[usize::MAX, 2]).is_err());
    for x in [f64::NAN, f64::INFINITY, -1.] {
        assert!(nonnegative(x, "test").is_err());
    }
    assert_eq!(
        step(
            2.,
            3.,
            Cell {
                depth: 0.,
                extinction: 0.,
                rayleigh: 0.,
                aerosol: 0.
            },
            4.
        ),
        14.
    );
    let atm = atmosphere(0.01, 0.01, 0.8, 0.01);
    let p = AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.95).unwrap());
    assert!(
        multiple_scattering(&atm, &p, &sky(), Nanometers::new(450.), view(), relaxed())
            .unwrap_err()
            .to_string()
            .contains("under-resolved")
    );
    let bad = Sky {
        scale: -1.,
        dipole: 0.,
        azimuthal: 0.0,
    };
    assert!(multiple_scattering(
        &atm,
        &aerosol(),
        &bad,
        Nanometers::new(450.),
        view(),
        relaxed()
    )
    .is_err());
    let strict = MultipleScatteringSettings {
        angular_tolerance: 1e-12,
        vertical_tolerance: 1e-12,
        ..relaxed()
    };
    assert!(multiple_scattering(
        &atm,
        &aerosol(),
        &sky(),
        Nanometers::new(450.),
        view(),
        strict
    )
    .is_err());
    for i in 0..12 {
        let mut s = relaxed();
        match i {
            0 => s.mu_nodes = 0,
            1 => s.azimuth_nodes = usize::MAX,
            2 => s.cells_per_layer = usize::MAX,
            3 => s.max_orders = 1,
            4 => s.angular_tolerance = f64::NAN,
            5 => s.vertical_tolerance = 0.,
            6 => s.order_tolerance = -1.,
            7 => s.absolute_tolerance = Radiance::new(f64::NAN),
            8 => s.absolute_tolerance = Radiance::new(1.),
            9 => s.angular_tolerance = 0.1,
            10 => s.order_tolerance = 0.1,
            _ => s.vertical_tolerance = 0.1,
        }
        assert!(preflight(1, s).is_err());
    }
    assert!(preflight(0, relaxed()).is_err());
    assert!(preflight(17, relaxed()).is_err());
}

#[test]
fn tabulated_isotropic_and_double_hg_use_upstream_phases() {
    use siderust::atmosphere::NormalizedPhaseTable;
    use siderust::optica::phase::{DoubleHenyeyGreensteinPhaseFunction, ScatteringFactor};
    let p = AerosolPhase::Tabulated(Box::new(
        NormalizedPhaseTable::new(
            &[Nanometers::new(360.), Nanometers::new(570.)],
            &[Radians::new(0.), Radians::new(std::f64::consts::PI)],
            &[Quantity::<ScatteringFactor>::new(1. / (2. * TAU)); 4],
        )
        .unwrap(),
    ));
    let atm = atmosphere(0., 0.1, 0.8, 0.03);
    let a =
        multiple_scattering(&atm, &p, &sky(), Nanometers::new(450.), view(), relaxed()).unwrap();
    let b = compute(&atm);
    assert!((a.total.value() - b.total.value()).abs() < 1e-14);
    let dhg = AerosolPhase::DoubleHenyeyGreenstein(
        DoubleHenyeyGreensteinPhaseFunction::try_new(0.2, -0.1, 0.7).unwrap(),
    );
    assert!(
        multiple_scattering(&atm, &dhg, &sky(), Nanometers::new(450.), view(), relaxed())
            .unwrap()
            .higher_orders
            .value()
            > 0.
    );
}

#[test]
fn operation_budget_and_noncontractive_column_are_explicit_errors() {
    let s = MultipleScatteringSettings {
        mu_nodes: 8,
        azimuth_nodes: 8,
        cells_per_layer: 8,
        max_orders: 64,
        ..relaxed()
    };
    assert!(preflight(1, s)
        .unwrap_err()
        .to_string()
        .contains("250 million"));
    let r = multiple_scattering(
        &atmosphere(100., 0., 0., 0.),
        &aerosol(),
        &sky(),
        Nanometers::new(450.),
        view(),
        relaxed(),
    );
    assert!(r.is_err());
}

/// Independent continuous-height DOP853 orders, analytic azimuth moments and
/// 512 full-interval GL streams. Scattering-strength Richardson extraction in
/// Rust is test-only and keeps total extinction fixed through upstream optics.
#[test]
fn second_and_third_orders_match_continuous_height_ivp() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/multiple-scattering/orders-v1.json"
    ))
    .unwrap();
    for (name, c) in fixture["cases"].as_object().unwrap() {
        let layer = c["layers_bottom_to_top"][0].as_array().unwrap();
        let v: Vec<_> = layer.iter().map(|x| x.as_f64().unwrap()).collect();
        let phase = AerosolPhase::HenyeyGreenstein(
            HenyeyGreensteinPhaseFunction::try_new(c["g"].as_f64().unwrap()).unwrap(),
        );
        let source = Sky {
            scale: 0.025,
            dipole: c["dipole"].as_f64().unwrap(),
            azimuthal: 0.,
        };
        let mut y = [0.; 3];
        for (i, scale) in [0.01, 0.005, 0.0025].into_iter().enumerate() {
            let atm = atmosphere(v[0] * scale, v[1], v[2] * scale, v[3] + v[0] * (1. - scale));
            let settings = MultipleScatteringSettings {
                order_tolerance: 1e-14,
                absolute_tolerance: Radiance::new(0.),
                ..relaxed()
            };
            let r = solution(
                &atm,
                &phase,
                &source,
                Nanometers::new(450.),
                view(),
                settings,
                (2, 2),
            )
            .unwrap();
            y[i] = r.higher / (scale * scale);
        }
        let second = (y[0] - 6. * y[1] + 8. * y[2]) / 3.;
        let third = (-2. * y[0] + 10. * y[1] - 8. * y[2]) / 0.01;
        let reference = c["results"].as_array().unwrap().last().unwrap();
        let e2 = (second / reference["second"].as_f64().unwrap() - 1.).abs();
        let e3 = (third / reference["third"].as_f64().unwrap() - 1.).abs();
        eprintln!("{name}: second={second} third={third} relative_second={e2} relative_third={e3}");
        assert!(e2 < 0.01, "{name}: second {e2}");
        assert!(e3 < 0.01, "{name}: third {e3}");
    }
}

#[test]
fn angular_vertical_and_order_refinement_study() {
    let atm = atmosphere(0., 0.12, 0.7, 0.02);
    let source = Sky {
        scale: 0.025,
        dipole: 0.,
        azimuthal: 0.,
    };
    let mut angular = Vec::new();
    for n in [2, 4, 8] {
        let s = MultipleScatteringSettings {
            mu_nodes: n,
            azimuth_nodes: 8,
            cells_per_layer: 16,
            order_tolerance: 1e-10,
            ..relaxed()
        };
        let r = solution(
            &atm,
            &aerosol(),
            &source,
            Nanometers::new(450.),
            view(),
            s,
            (1, 1),
        )
        .unwrap();
        eprintln!(
            "angular mu={n} cells=16 first={} higher={}",
            r.first, r.higher
        );
        angular.push(r);
    }
    let reference_higher = 0.00013426097214553828;
    assert!(
        (angular[2].higher - reference_higher).abs() < (angular[0].higher - reference_higher).abs()
    );
    let mut vertical = Vec::new();
    for cells in [2, 4, 8, 16] {
        let s = MultipleScatteringSettings {
            mu_nodes: 8,
            azimuth_nodes: 8,
            cells_per_layer: cells,
            order_tolerance: 1e-10,
            ..relaxed()
        };
        let r = solution(
            &atm,
            &aerosol(),
            &source,
            Nanometers::new(450.),
            view(),
            s,
            (1, 1),
        )
        .unwrap();
        eprintln!(
            "vertical mu=8 cells={cells} first={} higher={} tail={}",
            r.first, r.higher, r.bound
        );
        vertical.push(r);
    }
    assert!(
        (vertical[3].higher - vertical[2].higher).abs()
            < (vertical[1].higher - vertical[0].higher).abs()
    );
}

#[test]
fn hemispheric_photon_balance_is_exact_for_absorption_and_converges_with_cells() {
    for medium in [atmosphere(0., 0., 0., 0.), atmosphere(0., 0.1, 0., 0.02)] {
        let r = compute(&medium);
        assert!(r.convergence.flux_balance_relative.abs() < 1e-13);
    }
    let medium = atmosphere(0.08, 0., 0., 0.);
    let mut changes = Vec::new();
    for cells in [2, 4, 8, 16] {
        let s = MultipleScatteringSettings {
            cells_per_layer: cells,
            mu_nodes: 8,
            azimuth_nodes: 8,
            order_tolerance: 1e-9,
            ..relaxed()
        };
        let r = solution(
            &medium,
            &aerosol(),
            &sky(),
            Nanometers::new(450.),
            view(),
            s,
            (1, 1),
        )
        .unwrap();
        eprintln!(
            "conservative Rayleigh cells={cells} photon_balance={}",
            r.flux_balance
        );
        changes.push(r.flux_balance.abs());
    }
    assert!(changes[3] < 0.001);
    assert!(changes[3] < changes[0]);
    let absorbing = compute(&atmosphere(0.04, 0.06, 0.8, 0.02));
    assert!(absorbing.convergence.flux_balance_relative.abs() < 0.001);
}

#[test]
fn formal_cell_absorption_closes_incoming_and_volume_source_flux() {
    for delta in [1e-12, 1e-5, 0.02, 2., 200.] {
        let mu = 0.7;
        let cell = Cell {
            depth: delta * mu,
            extinction: 0.4,
            rayleigh: 0.,
            aerosol: 0.,
        };
        let height = cell.depth / cell.extinction;
        let escaped = step(2., 3., cell, height / mu);
        let absorbed = absorbed_in_cell(2., 3., cell, mu);
        assert!((mu * (escaped - 2.) + absorbed - 3. * height).abs() < 1e-12 * (1. + 3. * height));
    }
}

#[test]
fn optically_thin_isotropic_limit_has_linear_first_order_and_vanishing_enhancement() {
    let source = Sky {
        scale: 0.025,
        dipole: 0.,
        azimuthal: 0.,
    };
    let mut relative = Vec::new();
    let mut enhancement = Vec::new();
    for tau in [1e-3, 1e-4, 1e-5] {
        let atm = atmosphere(0., tau, 1., 0.);
        let r = solution(
            &atm,
            &aerosol(),
            &source,
            Nanometers::new(450.),
            view(),
            MultipleScatteringSettings {
                order_tolerance: 1e-10,
                absolute_tolerance: Radiance::new(0.),
                ..relaxed()
            },
            (2, 2),
        )
        .unwrap();
        relative.push((r.first / (0.025 * tau / (2. * 0.8)) - 1.).abs());
        enhancement.push(r.higher / r.first);
    }
    assert!(relative[2] < 1e-3 && relative[2] < relative[0]);
    assert!(enhancement[2] < enhancement[1] && enhancement[1] < enhancement[0]);
}

#[test]
fn historical_hg_phase_resolution_is_an_explicit_remaining_limit() {
    let phase =
        AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.65).unwrap());
    // Examine the finest currently supported *base* grid. Every public result
    // must also pass its base-grid phase test before joint refinement.
    let w = Nanometers::new(375.);
    let dirs = ordinates(&sky(), w, 8, 16, false).unwrap();
    let mut min = f64::INFINITY;
    let mut max = 0.0_f64;
    for out in &dirs {
        let sum: f64 = dirs
            .iter()
            .map(|incoming| {
                phase_pair(w, &phase, out.vector, incoming.vector).unwrap()[1] * incoming.weight
            })
            .sum();
        min = min.min(sum);
        max = max.max(sum);
    }
    eprintln!("HG g=0.65 base mu=8 az=16 phase rows: min={min:.16} max={max:.16}");
    assert!(max > 1.001);
    let error = solve(
        &atmosphere(0.08, 0.03, 1., 0.),
        &phase,
        &sky(),
        w,
        view(),
        MultipleScatteringSettings {
            mu_nodes: 8,
            azimuth_nodes: 16,
            ..relaxed()
        },
        (1, 1, None),
    )
    .err()
    .unwrap();
    assert!(error
        .to_string()
        .contains("phase quadrature is under-resolved"));
}

#[test]
fn refined_hg65_diagnostic_matches_independent_disort() {
    // A single bounded diagnostic solve can resolve g=0.65, although the
    // current public four-grid plan cannot. This is not a supported Gaia plan.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/multiple-scattering/hg65-v1.json"
    ))
    .unwrap();
    let reference = fixture["results"].as_array().unwrap().last().unwrap();
    let s = MultipleScatteringSettings {
        mu_nodes: 8,
        azimuth_nodes: 32,
        cells_per_layer: 16,
        max_orders: 24,
        order_tolerance: 1e-8,
        ..relaxed()
    };
    let directions = 2 * s.mu_nodes * s.azimuth_nodes;
    assert!(workspace_bytes(directions, s.cells_per_layer).unwrap() < MEMORY_LIMIT);
    let work =
        (2 * directions * directions + 8 * directions) * s.cells_per_layer * (s.max_orders + 1);
    assert!(work < WORK_LIMIT);
    let phase =
        AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(0.65).unwrap());
    let atm = atmosphere(0., 0.12, 0.7, 0.02);
    let result = solve(
        &atm,
        &phase,
        &sky(),
        Nanometers::new(375.),
        view(),
        s,
        (1, 1, None),
    )
    .unwrap();
    // Same pre-existing 1% finite-cell continuum allowance as the independently
    // validated synthetic domain; not fitted to this additional case.
    assert!((result.first / reference["single"].as_f64().unwrap() - 1.).abs() < 0.01);
    assert!((result.higher / reference["higher"].as_f64().unwrap() - 1.).abs() < 0.01);
    assert!(multiple_scattering(&atm, &phase, &sky(), Nanometers::new(375.), view(), s).is_err());
    eprintln!(
        "HG65 refined diagnostic: first={} higher={} orders={} tail={} work={work}",
        result.first, result.higher, result.orders, result.bound
    );
}
