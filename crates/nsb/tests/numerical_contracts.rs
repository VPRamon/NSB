// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use nsb::transport::{AirmassModel, DirectPathGeometry, DirectTransmission, ExtinctionIngredients};
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian;
use nsb::units::unit;
use serde_json::{json, Value};
use siderust::qtty::{Degrees, Nanometers};

fn check(family: &str, input: Value, actual: f64, reference: f64, atol: f64, units: &str) {
    assert!(
        actual.is_finite() && (actual - reference).abs() <= atol,
        "{family} {input}: {actual} vs {reference} {units}; absolute tolerance {atol}"
    );
}

#[test]
fn clipped_linear_spectral_integrals() {
    // This is exactly the public upstream kernel called by nsb::band::integrate_linear.
    // Analytic constant/linear spectra on irregular nodes and clipped endpoints.
    let x = [300.0, 351.0, 469.0, 650.0];
    for (slope, intercept) in [(0.0, 2.0), (0.002, 0.4)] {
        let y = x.map(|w| slope * w + intercept);
        for (a, b) in [(300.0, 650.0), (350.25, 570.75), (469.0, 650.0)] {
            let reference = (slope * (a + b) / 2.0 + intercept) * (b - a);
            let actual = siderust::optica::spectrum::integrate::trapz_range(&x, &y, a, b);
            check(
                "clipped_linear",
                json!({"slope":slope,"intercept":intercept,"lower_nm":a,"upper_nm":b}),
                actual,
                reference,
                1e-10,
                "photons cm^-2 ns^-1 sr^-1 (synthetic density per nm)",
            );
        }
    }
}

#[test]
fn quadratic_trapezoid_convergence() {
    // Quadratic composite trapezoid has exact error L*h^2/6. This tests
    // convergence of the kernel, not a nonexistent sky-model resolution knob.
    for n in [8, 16, 32, 64] {
        let x: Vec<_> = (0..=n)
            .map(|i| 300.0 + 350.0 * i as f64 / n as f64)
            .collect();
        let y: Vec<_> = x.iter().map(|w| ((w - 300.0) / 350.0).powi(2)).collect();
        let actual = siderust::optica::spectrum::integrate::trapz_range(&x, &y, 300.0, 650.0);
        let reference = 350.0 / 3.0;
        let error = 350.0 / (6.0 * (n as f64).powi(2));
        check(
            "quadratic_convergence",
            json!({"intervals":n,"analytic_integral":reference,"analytic_discretization_error":error,"computed_discretization_error":actual-reference}),
            actual,
            reference + error,
            1e-10,
            "photons cm^-2 ns^-1 sr^-1 (synthetic)",
        );
    }
}

#[test]
fn radiance_unit_conversion() {
    // 1 ph cm^-2 ns^-1 sr^-1 = 10^4 * 10^9 ph m^-2 s^-1 sr^-1.
    let si = PhotonsPerSquareCentimeterNanosecondSteradian::new(1.0)
        .to::<unit::PhotonPerSquareMeterSecondSteradian>()
        .value();
    check(
        "radiance_units",
        json!({"input":1.0,"input_units":"ph cm^-2 ns^-1 sr^-1"}),
        si,
        1e13,
        0.01,
        "ph m^-2 s^-1 sr^-1",
    );
}

#[test]
fn beer_lambert_composition() {
    let atmosphere = nsb::site::AtmosphericConditions::paranal_average();
    let direct = DirectTransmission::new(
        ExtinctionIngredients::RAYLEIGH_MIE,
        AirmassModel::PlaneParallel,
    );
    for wavelength in [360.0, 500.0, 650.0] {
        let wavelength = Nanometers::new(wavelength);
        // Tau is deliberately treated as the input to the algebraic contract.
        // This is not an independent oracle for the empirical optical depths.
        let tau = direct
            .optical_depth(wavelength, atmosphere)
            .unwrap()
            .total
            .value();
        for zenith in [0.0_f64, 30.0, 60.0, 80.0] {
            let reference = (-tau / zenith.to_radians().cos()).exp();
            let actual = direct
                .transmission(
                    wavelength,
                    DirectPathGeometry::new(Degrees::new(zenith)).unwrap(),
                    atmosphere,
                )
                .unwrap()
                .value();
            check(
                "beer_lambert_composition",
                json!({"wavelength_nm":wavelength.value(),"zenith_deg":zenith,"vertical_tau":tau}),
                actual,
                reference,
                2e-14,
                "dimensionless transmission",
            );
        }
    }
}
