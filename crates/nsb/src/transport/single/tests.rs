// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::units::qtty::{Albedos, Quantity};
use siderust::atmosphere::AtmosphericOptics;
use siderust::optica::phase::ScatteringFactor;

#[derive(Clone)]
struct Sky {
    value: f64,
    dipole: f64,
    coverage: SkyCoverage,
}
impl Sky {
    fn new(value: f64) -> Self {
        Self {
            value,
            dipole: 0.0,
            coverage: SkyCoverage {
                identity: "synthetic-linear-spectrum-v1; local Horizontal; epoch independent"
                    .into(),
                wavelengths: [Nanometers::new(360.0), Nanometers::new(570.0)],
                full_hemisphere: true,
                origin: RadianceOrigin::TopOfAtmosphere,
            },
        }
    }
}
impl IncidentSky for Sky {
    fn coverage(&self) -> SkyCoverage {
        self.coverage.clone()
    }
    fn radiance(&self, w: Nanometers, direction: Horizontal) -> Result<Radiance> {
        // A true wavelength-resolved synthetic spectrum, distinct from band data.
        Ok(Radiance::new(
            self.value
                * (w.value() / 450.0)
                * (1.0 + self.dipole * direction.alt().to::<Radian>().sin()),
        ))
    }
}
fn phase(g: f64) -> AerosolPhase {
    AerosolPhase::HenyeyGreenstein(HenyeyGreensteinPhaseFunction::try_new(g).unwrap())
}
fn optics(r: f64, a: f64, ssa: f64, m: f64) -> AtmosphericOptics {
    AtmosphericOptics::from_extinction(
        Quantity::new(r),
        Quantity::new(a),
        Albedos::new(ssa),
        Quantity::new(m),
    )
    .unwrap()
}
fn medium(r: f64, a: f64, ssa: f64, m: f64) -> LayeredAtmosphere {
    LayeredAtmosphere::try_new(
        &[Meters::new(2100.0), Meters::new(3100.0)],
        [Nanometers::new(360.0), Nanometers::new(570.0)],
        vec![optics(r / 1000.0, a / 1000.0, ssa, m / 1000.0)],
    )
    .unwrap()
}
fn view(mu: f64, az: f64) -> Horizontal {
    Horizontal::new(Degrees::new(mu.asin().to_degrees()), Degrees::new(az))
}
fn solve(m: &LayeredAtmosphere, p: &AerosolPhase, s: &Sky, mu: f64) -> SingleScattering {
    single_scattering(
        m,
        p,
        s,
        Nanometers::new(450.0),
        view(mu, 23.0),
        SolverSettings::default(),
    )
    .unwrap()
}
fn close(a: f64, b: f64, tol: f64) {
    assert!(
        (a - b).abs() <= tol * a.abs().max(b.abs()).max(1e-20),
        "{a:.17e} != {b:.17e}"
    );
}

// Independent analytic homogeneous oracle: factor the smaller exponent and use
// expm1 on a nonpositive argument, avoiding cancellation and intermediate overflow.
fn homogeneous(tau: f64, mu_i: f64, mu_o: f64, scattering_depth: f64) -> f64 {
    let a = tau / mu_i;
    let b = tau / mu_o;
    let d = (a - b).abs();
    let divided = if d == 0.0 { 1.0 } else { -(-d).exp_m1() / d };
    scattering_depth / mu_o * (-a.min(b)).exp() * divided
}
fn oracle(m: &LayeredAtmosphere, p: &AerosolPhase, s: &Sky, mu_o: f64, orders: [usize; 3]) -> f64 {
    let w = Nanometers::new(450.0);
    let edges = m.edges();
    let depth = m
        .vertical_optical_depth(w, edges[0], edges[edges.len() - 1])
        .unwrap();
    let mut sum = 0.0;
    for (mu, weight) in quadrature::gauss(orders[0]).unwrap() {
        for j in 0..orders[1] {
            let az = 2.0 * PI * j as f64 / orders[1] as f64;
            let cosine = mu * mu_o
                + (1.0 - mu * mu).sqrt()
                    * (1.0 - mu_o * mu_o).sqrt()
                    * (az - 23_f64.to_radians()).cos();
            let angle = Radians::new(cosine.clamp(-1.0, 1.0).acos());
            let source = s.radiance(w, view(mu, az.to_degrees())).unwrap().value();
            sum += source * weight * 2.0 * PI / orders[1] as f64
                * (RayleighPhaseFunction.phase(w, angle).value()
                    * homogeneous(depth.extinction().value(), mu, mu_o, depth.rayleigh.value())
                    + p.density(w, angle).unwrap()
                        * homogeneous(
                            depth.extinction().value(),
                            mu,
                            mu_o,
                            depth.aerosol_scattering.value(),
                        ));
        }
    }
    sum
}

#[test]
fn vacuum_absorption_exact_direct_and_spectral_units() {
    for m in [medium(0., 0., 0., 0.), medium(0., 0.2, 0., 0.3)] {
        for mu in [1.0, 0.8, 10_f64.to_radians().sin()] {
            let result = solve(&m, &phase(0.7), &Sky::new(2.0), mu);
            assert_eq!(result.single.value(), 0.0);
            let d = m
                .vertical_optical_depth(Nanometers::new(450.), m.edges()[0], m.edges()[1])
                .unwrap();
            close(
                result.direct.value(),
                2.0 * (-d.extinction().value() / mu).exp(),
                1e-14,
            );
            assert!(result.higher_orders.is_none());
            assert_eq!(result.convergence.angular_change.value(), 0.0);
        }
    }
    let m = medium(0., 0., 1., 0.);
    let r = single_scattering(
        &m,
        &phase(0.),
        &Sky::new(2.),
        Nanometers::new(570.),
        view(1., 0.),
        SolverSettings::default(),
    )
    .unwrap();
    close(r.direct.value(), 2. * 570. / 450., 1e-14);
}

#[test]
fn homogeneous_reference_and_thin_equal_mu_limits() {
    close(
        homogeneous(0.2, 0.7, 0.7, 0.12),
        0.12 / 0.7 * (-0.2_f64 / 0.7).exp(),
        1e-15,
    );
    close(
        homogeneous(0.2, 0.7 + 1e-14, 0.7, 0.12),
        homogeneous(0.2, 0.7, 0.7, 0.12),
        1e-13,
    );
    assert_eq!(homogeneous(0., 0.2, 0.3, 0.), 0.);
    assert!(homogeneous(1e6, 1e-6, 0.7, 0.2).is_finite());
    for tau in [1e-8, 0.03, 0.4] {
        for mu in [1.0, 0.71, 0.18] {
            let m = medium(tau * 0.3, tau * 0.5, 0.6, tau * 0.2);
            let mut sky = Sky::new(0.03);
            sky.dipole = 0.4;
            let p = phase(0.65);
            let r = solve(&m, &p, &sky, mu);
            close(
                r.single.value(),
                oracle(&m, &p, &sky, mu, r.convergence.orders),
                3e-5,
            );
            assert!(r.single.value() >= 0.0 && r.single.value().is_finite());
        }
    }
}

#[test]
fn quadrature_positive_full_hemisphere_and_phase_normalization() {
    for n in [2, 3, 16, 128, 512] {
        let nodes = quadrature::gauss(n).unwrap();
        close(nodes.iter().map(|v| v.1).sum(), 1., 2e-14);
        close(nodes.iter().map(|(x, w)| x * x * w).sum(), 1. / 3., 2e-14);
        assert!(nodes.iter().all(|&(x, w)| x > 0. && x < 1. && w > 0.));
        assert!(nodes.windows(2).all(|p| p[0].0 < p[1].0));
    }
    let w = Nanometers::new(450.);
    for g in [0., 0.65, 0.9, -0.65] {
        let p = phase(g);
        let mut norm = 0.;
        let mut moment = 0.;
        for (x, weight) in quadrature::gauss(256).unwrap() {
            let cosine = 2. * x - 1.;
            let density = p.density(w, Radians::new(cosine.acos())).unwrap();
            norm += 4. * PI * weight * density;
            moment += 4. * PI * weight * density * cosine;
        }
        close(norm, 1., 2e-12);
        assert!((moment - g).abs() < 2e-12);
    }
    let norm: f64 = quadrature::gauss(16)
        .unwrap()
        .iter()
        .map(|(x, w)| {
            4. * PI
                * w
                * RayleighPhaseFunction
                    .phase(wavelength(), Radians::new((2. * x - 1.).acos()))
                    .value()
        })
        .sum();
    close(norm, 1., 1e-14);
}
fn wavelength() -> Nanometers {
    Nanometers::new(450.)
}

#[test]
fn layers_split_merge_constituents_ssa_and_repeatability() {
    let p = phase(0.65);
    let s = Sky::new(0.01);
    let m = medium(0.07, 0.12, 0.7, 0.03);
    let split = LayeredAtmosphere::try_new(
        &[
            Meters::new(2100.),
            Meters::new(2300.),
            Meters::new(2700.),
            Meters::new(3100.),
        ],
        m.wavelength_domain(),
        vec![optics(0.00007, 0.00012, 0.7, 0.00003); 3],
    )
    .unwrap();
    let a = solve(&m, &p, &s, 0.8);
    let b = solve(&split, &p, &s, 0.8);
    close(a.single.value(), b.single.value(), 5e-5);
    close(a.direct.value(), b.direct.value(), 1e-14);
    assert_eq!(a.single, solve(&m, &p, &s, 0.8).single);
    for ssa in [0., 0.3, 1.] {
        let r = solve(&medium(0., 0.2, ssa, 0.1), &p, &s, 0.8);
        let full = solve(&medium(0., 0.2, 1., 0.1), &p, &s, 0.8);
        if ssa == 0. {
            assert_eq!(r.single.value(), 0.);
        } else {
            close(r.single.value(), ssa * full.single.value(), 1e-12);
        }
    }
    let absorbed = solve(&medium(0.07, 0.12, 0.7, 0.3), &p, &s, 0.8);
    assert!(absorbed.single.value() < a.single.value());
    assert!(absorbed.direct.value() < a.direct.value());
    // Different layers do not commute: incoming/outgoing extinction matters.
    let two = LayeredAtmosphere::try_new(
        &[Meters::new(2100.), Meters::new(2500.), Meters::new(3100.)],
        m.wavelength_domain(),
        vec![optics(0.0001, 0., 0., 0.0002), optics(0., 0.0002, 0.9, 0.)],
    )
    .unwrap();
    let r = solve(&two, &p, &s, 0.8);
    assert!(r.single.value() > 0. && r.single.value().is_finite());
}

#[test]
fn aerosol_forward_sign_exact_direct_query_and_no_double_extinction() {
    // Highly directional smooth illumination centered at the view. A wrong
    // propagation sign makes positive g dimmer than negative g.
    struct Lobe;
    impl IncidentSky for Lobe {
        fn coverage(&self) -> SkyCoverage {
            Sky::new(1.).coverage()
        }
        fn radiance(&self, _: Nanometers, d: Horizontal) -> Result<Radiance> {
            let cosine = d.alt().to::<Radian>().sin();
            Ok(Radiance::new(cosine.powi(20)))
        }
    }
    let m = medium(0., 0.05, 1., 0.);
    let f = single_scattering(
        &m,
        &phase(0.8),
        &Lobe,
        wavelength(),
        view(1., 0.),
        SolverSettings::default(),
    )
    .unwrap();
    let b = single_scattering(
        &m,
        &phase(-0.8),
        &Lobe,
        wavelength(),
        view(1., 0.),
        SolverSettings::default(),
    )
    .unwrap();
    assert!(f.single.value() > 10. * b.single.value());
    close(f.direct.value(), (-0.05_f64).exp(), 1e-14); // exact zenith, never a GL node
    let s = Sky::new(1.);
    let p = phase(0.);
    let r = solve(&m, &p, &s, 1.);
    close(
        r.single.value(),
        oracle(&m, &p, &s, 1., r.convergence.orders),
        3e-5,
    );
}

#[test]
fn strict_tables_and_double_hg() {
    let value = Quantity::<ScatteringFactor>::new(1. / (4. * PI));
    let table = NormalizedPhaseTable::new(
        &[Nanometers::new(400.), Nanometers::new(500.)],
        &[Radians::new(0.), Radians::new(PI)],
        &[value; 4],
    )
    .unwrap();
    let p = AerosolPhase::Tabulated(Box::new(table));
    let m = medium(0.04, 0.1, 0.8, 0.);
    close(
        solve(&m, &p, &Sky::new(1.), 0.7).single.value(),
        solve(&m, &phase(0.), &Sky::new(1.), 0.7).single.value(),
        1e-13,
    );
    assert!(single_scattering(
        &m,
        &p,
        &Sky::new(1.),
        Nanometers::new(570.),
        view(1., 0.),
        SolverSettings::default()
    )
    .is_err());
    let bad = Quantity::<ScatteringFactor>::new(1.);
    assert!(NormalizedPhaseTable::new(
        &[Nanometers::new(400.), Nanometers::new(500.)],
        &[Radians::new(0.), Radians::new(PI)],
        &[bad; 4]
    )
    .is_err());
    let dhg = AerosolPhase::DoubleHenyeyGreenstein(
        DoubleHenyeyGreensteinPhaseFunction::try_new(0.3, -0.2, 0.7).unwrap(),
    );
    assert!(solve(&m, &dhg, &Sky::new(1.), 0.7).single.value() > 0.);
}

#[test]
fn invalid_requests_and_explicit_convergence_failure() {
    let m = medium(0.1, 0.1, 0.8, 0.);
    let p = phase(0.65);
    let s = Sky::new(1.);
    for w in [f64::NAN, f64::INFINITY, 359.9, 570.1] {
        assert!(single_scattering(
            &m,
            &p,
            &s,
            Nanometers::new(w),
            view(1., 0.),
            SolverSettings::default()
        )
        .is_err());
    }
    for alt in [f64::NAN, 9., -10.] {
        assert!(single_scattering(
            &m,
            &p,
            &s,
            wavelength(),
            Horizontal::new_unchecked(Degrees::new(alt), Degrees::new(0.)),
            SolverSettings::default()
        )
        .is_err());
    }
    for value in [-1., f64::NAN, f64::INFINITY] {
        assert!(single_scattering(
            &m,
            &p,
            &Sky::new(value),
            wavelength(),
            view(0.8, 0.),
            SolverSettings::default()
        )
        .is_err());
    }
    for mutate in 0..8 {
        let mut sky = s.clone();
        match mutate {
            0 => sky.coverage.full_hemisphere = false,
            1 => sky.coverage.origin = RadianceOrigin::AtmosphericEmission,
            2 => sky.coverage.origin = RadianceOrigin::PreScatteredAtmosphere,
            3 => sky.coverage.wavelengths[0] = Nanometers::new(460.),
            4 => sky.coverage.wavelengths[1] = Nanometers::new(440.),
            5 => sky.coverage.wavelengths[0] = Nanometers::new(f64::NAN),
            6 => sky.coverage.identity.clear(),
            _ => sky.coverage.wavelengths[0] = Nanometers::new(-1.),
        }
        assert!(single_scattering(
            &m,
            &p,
            &sky,
            wavelength(),
            view(1., 0.),
            SolverSettings::default()
        )
        .is_err());
    }
    for mutate in 0..12 {
        let mut settings = SolverSettings::default();
        match mutate {
            0 => settings.mu_nodes = 0,
            1 => settings.azimuth_nodes = 1,
            2 => settings.path_nodes = 1,
            3 => settings.refinements = 0,
            4 => settings.refinements = usize::MAX,
            5 => settings.relative_tolerance = f64::NAN,
            6 => settings.relative_tolerance = 0.,
            7 => settings.relative_tolerance = 0.01,
            8 => settings.absolute_tolerance = Radiance::new(-1.),
            9 => settings.absolute_tolerance = Radiance::new(f64::INFINITY),
            10 => settings.absolute_tolerance = Radiance::new(1e-8),
            _ => settings.mu_nodes = 512,
        }
        assert!(single_scattering(&m, &p, &s, wavelength(), view(1., 0.), settings).is_err());
    }
    let settings = SolverSettings {
        mu_nodes: 2,
        azimuth_nodes: 4,
        path_nodes: 2,
        refinements: 1,
        ..SolverSettings::default()
    };
    assert!(
        single_scattering(&m, &p, &s, wavelength(), view(0.7, 0.), settings)
            .unwrap_err()
            .to_string()
            .contains("convergence failed")
    );
    assert!(single_scattering(
        &m,
        &p,
        &s,
        Nanometers::new(370.),
        view(1., 0.),
        SolverSettings::default()
    )
    .is_ok());
    assert!(finite_nonnegative(f64::INFINITY, "test").is_err());
}

#[test]
fn independent_scipy_exponential_integral_reference() {
    let data: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/single-scattering/scipy-1.17.1.json"
    ))
    .unwrap();
    for fixture in data["fixtures"].as_array().unwrap() {
        let edges: Vec<_> = fixture["edges_m"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Meters::new(v.as_f64().unwrap()))
            .collect();
        let layers: Vec<_> = fixture["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                optics(
                    v[0].as_f64().unwrap(),
                    v[1].as_f64().unwrap(),
                    v[2].as_f64().unwrap(),
                    v[3].as_f64().unwrap(),
                )
            })
            .collect();
        let m = LayeredAtmosphere::try_new(
            &edges,
            [Nanometers::new(360.), Nanometers::new(570.)],
            layers,
        )
        .unwrap();
        let mut s = Sky::new(0.01);
        s.dipole = 0.4;
        let r = solve(&m, &phase(0.), &s, 0.8);
        close(r.direct.value(), fixture["direct"].as_f64().unwrap(), 1e-13);
        close(r.single.value(), fixture["single"].as_f64().unwrap(), 1e-4);
        println!("{}: single={:.17e}; reference={}; orders={:?}; angular={:.3e}, path={:.3e}, joint={:.3e}",fixture["name"],r.single.value(),fixture["single"],r.convergence.orders,r.convergence.angular_change.value(),r.convergence.path_change.value(),r.convergence.joint_change.value());
    }
}

#[test]
fn missing_angular_data_nonfinite_angular_data_and_overflow() {
    struct Broken {
        value: Option<f64>,
    }
    impl IncidentSky for Broken {
        fn coverage(&self) -> SkyCoverage {
            Sky::new(1.).coverage()
        }
        fn radiance(&self, _: Nanometers, d: Horizontal) -> Result<Radiance> {
            if d.alt().value() == 90. {
                return Ok(Radiance::new(1.));
            }
            self.value
                .map(Radiance::new)
                .ok_or_else(|| NsbError::DataMissing {
                    file: "synthetic",
                    message: "missing angular sample".into(),
                })
        }
    }
    let m = medium(0.1, 0.1, 0.8, 0.);
    for value in [None, Some(-1.), Some(f64::NAN), Some(f64::INFINITY)] {
        assert!(single_scattering(
            &m,
            &phase(0.),
            &Broken { value },
            wavelength(),
            view(1., 0.),
            SolverSettings::default()
        )
        .is_err());
    }
    let mut s = Sky::new(f64::MAX);
    s.dipole = 0.4;
    assert!(single_scattering(
        &m,
        &phase(0.),
        &s,
        wavelength(),
        view(1., 0.),
        SolverSettings::default()
    )
    .is_err());
    // The source is finite but multiplication by the phase density overflows.
    let mu = quadrature::gauss(16).unwrap()[8].0;
    assert!(single_scattering(
        &m,
        &phase(0.999999),
        &Sky::new(f64::MAX),
        wavelength(),
        view(mu, 0.),
        SolverSettings::default()
    )
    .is_err());
    // Medium spectral-domain failure is propagated with context.
    let narrow = LayeredAtmosphere::try_new(
        m.edges(),
        [Nanometers::new(500.), Nanometers::new(570.)],
        vec![optics(0., 0., 0., 0.)],
    )
    .unwrap();
    assert!(single_scattering(
        &narrow,
        &phase(0.),
        &Sky::new(1.),
        wavelength(),
        view(1., 0.),
        SolverSettings::default()
    )
    .is_err());
}

#[test]
fn truncated_downward_flux_is_bounded_by_available_incident_energy() {
    let tau = 0.08;
    let m = medium(tau, 0., 1., 0.);
    let p = phase(0.);
    let s = Sky::new(1.);
    let minimum = 10_f64.to_radians().sin();
    let mut downward_single = 0.;
    let mut removed = 0.;
    for (x, weight) in quadrature::gauss(24).unwrap() {
        removed += 2. * PI * weight * x * (-(-tau / x).exp_m1());
        let mu = minimum + (1. - minimum) * x;
        downward_single +=
            2. * PI * weight * (1. - minimum) * mu * solve(&m, &p, &s, mu).single.value();
    }
    // Only first-scattered photons reaching the supported downward aperture are
    // counted. Upward escape, grazing output and higher orders remain omitted.
    assert!(downward_single > 0.);
    assert!(downward_single < removed);
    assert!(removed < PI);
}

#[test]
fn profile_local_pressure_column_and_existing_direct_are_identical() {
    use crate::site::AtmosphericConditions;
    use crate::transport::{
        AirmassModel, DirectPathGeometry, DirectTransmission, ExtinctionIngredients,
    };
    use crate::units::qtty::Kilometers;
    use siderust::atmosphere::{rayleigh_optical_depth_bodhaine99, AtmosphereProfile};
    let profile = AtmosphereProfile::EL_PARANAL;
    let bottom = profile.observer_altitude.to::<crate::units::qtty::Meter>();
    let m = LayeredAtmosphere::from_profile(
        profile,
        &[
            bottom,
            Meters::new(bottom.value() + 1000.),
            Meters::new(bottom.value() + 10000.),
        ],
        [Nanometers::new(360.), Nanometers::new(570.)],
        &[0.3, 0.7],
        &[0.8, 0.2],
        Albedos::new(0.8),
    )
    .unwrap();
    let depth = m
        .vertical_optical_depth(wavelength(), m.edges()[0], m.edges()[2])
        .unwrap();
    let expected = rayleigh_optical_depth_bodhaine99(
        wavelength(),
        profile.surface_pressure,
        Kilometers::new(0.),
        profile.rayleigh_scale_height,
    );
    close(depth.rayleigh.value(), expected.value(), 1e-14);
    let conditions = AtmosphericConditions::from_profile_without_altitude(profile);
    let direct = DirectTransmission::new(
        ExtinctionIngredients::default(),
        AirmassModel::PlaneParallel,
    );
    let old = direct
        .transmission(
            wavelength(),
            DirectPathGeometry::new(Degrees::new(30.)).unwrap(),
            conditions,
        )
        .unwrap();
    let r = single_scattering(
        &m,
        &phase(0.65),
        &Sky::new(1.),
        wavelength(),
        view(30_f64.to_radians().cos(), 0.),
        SolverSettings::default(),
    )
    .unwrap();
    close(r.direct.value(), old.value(), 1e-14);
    let partial = m
        .vertical_optical_depth(
            wavelength(),
            m.edges()[0],
            Meters::new(bottom.value() + 500.),
        )
        .unwrap();
    close(partial.rayleigh.value(), expected.value() * 0.15, 1e-14);
}

#[test]
fn oblique_azimuthal_illumination_rotation_and_forward_peak_converge() {
    struct Beam {
        az: f64,
    }
    impl IncidentSky for Beam {
        fn coverage(&self) -> SkyCoverage {
            Sky::new(1.).coverage()
        }
        fn radiance(&self, _: Nanometers, d: Horizontal) -> Result<Radiance> {
            let alt = d.alt().to::<Radian>().value();
            let mu = 40_f64.to_radians().sin();
            let dot = alt.sin() * mu
                + alt.cos()
                    * (1. - mu * mu).sqrt()
                    * (d.az().to::<Radian>().value() - self.az.to_radians()).cos();
            Ok(Radiance::new(0.01 * (8. * (dot - 1.)).exp()))
        }
    }
    let m = medium(0.02, 0.15, 0.9, 0.01);
    let settings = SolverSettings::default();
    let evaluate = |offset: f64, p: &AerosolPhase| {
        single_scattering(
            &m,
            p,
            &Beam { az: 110. + offset },
            wavelength(),
            Horizontal::new(Degrees::new(40.), Degrees::new(110. + offset)),
            settings,
        )
        .unwrap()
    };
    let r = evaluate(0., &phase(0.85));
    let rotated = evaluate(53., &phase(0.85));
    close(r.single.value(), rotated.single.value(), 1e-4);
    close(r.direct.value(), rotated.direct.value(), 1e-13);
    assert!(r.convergence.orders[0] > settings.mu_nodes * 2);
    assert!(r.single.value() > evaluate(0., &phase(-0.85)).single.value());
    println!(
        "forward HG g=0.85: orders={:?}, angular={:.3e}, path={:.3e}, joint={:.3e}",
        r.convergence.orders,
        r.convergence.angular_change.value(),
        r.convergence.path_change.value(),
        r.convergence.joint_change.value()
    );
    let rayleigh = medium(0.2, 0., 0., 0.);
    let a = single_scattering(
        &rayleigh,
        &phase(0.),
        &Sky::new(1.),
        wavelength(),
        view(0.7, 0.),
        settings,
    )
    .unwrap();
    let b = single_scattering(
        &rayleigh,
        &phase(0.),
        &Sky::new(1.),
        wavelength(),
        view(0.7, 117.),
        settings,
    )
    .unwrap();
    close(a.single.value(), b.single.value(), 1e-13);
}

#[test]
fn collimated_equal_mu_path_oracle_and_independent_path_refinement() {
    // One explicitly selected angular quadrature contribution isolates the
    // path rule. We do not claim angular convergence for this discrete fixture.
    let mu_i = quadrature::gauss(2).unwrap()[1].0;
    struct Collimated {
        mu: f64,
    }
    impl IncidentSky for Collimated {
        fn coverage(&self) -> SkyCoverage {
            Sky::new(1.).coverage()
        }
        fn radiance(&self, _: Nanometers, d: Horizontal) -> Result<Radiance> {
            let matches =
                (d.alt().to::<Radian>().sin() - self.mu).abs() < 1e-14 && d.az().value() == 0.;
            Ok(Radiance::new(if matches { 1. } else { 0. }))
        }
    }
    let p = phase(0.);
    let sky = Collimated { mu: mu_i };
    for tau in [0., 1e-12, 0.3, 2.] {
        for mu_o in [mu_i, mu_i * (1. + 1e-12)] {
            let m = medium(0., tau, 0.7, 0.);
            let kernel = quadrature::Kernel {
                atmosphere: &m,
                phase: &p,
                sky: &sky,
                wavelength: wavelength(),
                view: view(mu_o, 0.),
            };
            let numerical = kernel.integrate([2, 4, 32]).unwrap();
            // Angular weight = (1/2)*(2pi/4), isotropic phase = 1/(4pi).
            let exact = homogeneous(tau, mu_i, mu_o, 0.7 * tau) / 16.;
            if exact == 0. {
                assert_eq!(numerical, 0.);
            } else {
                close(numerical, exact, 2e-13);
            }
        }
    }
    let m = medium(0.6, 1.4, 0.7, 0.);
    let s = Sky::new(1.);
    let settings = SolverSettings {
        path_nodes: 2,
        ..SolverSettings::default()
    };
    let r = single_scattering(&m, &p, &s, wavelength(), view(0.2, 23.), settings).unwrap();
    assert!(r.convergence.orders[2] > 4);
    close(
        r.single.value(),
        oracle(&m, &p, &s, 0.2, r.convergence.orders),
        1e-4,
    );
    println!("independent coarse-path refinement: orders={:?}, angular={:.3e}, path={:.3e}, joint={:.3e}",r.convergence.orders,r.convergence.angular_change.value(),r.convergence.path_change.value(),r.convergence.joint_change.value());
}
