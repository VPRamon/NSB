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
            self.scale * (1.0 + self.dipole * d.alt().to::<Radian>().sin()),
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
