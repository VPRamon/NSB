//! Tests for the zodiacal-light module.

use super::extinction::ZodiacalExtinction;
use super::leinert::{reference_lookup_s10_for_test, Leinert1998Grid};
use super::model::{ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalLight};
use crate::evaluator::Target;
use qtty::angular::{Degrees, Radians};
use qtty::radiometry::S10s;
use siderust::catalogs::observatories;
use siderust::qtty::Nanometers;
use siderust::qtty::DEG;
use tempoch::{Time, UTC};

#[test]
fn leinert_grid2d_matches_historical_reference() {
    let dl_degs = [0.5_f64, 1.0, 5.0, 10.0, 27.3, 90.0, 124.5, 175.0, 179.9];
    let beta_degs = [0.5_f64, 5.0, 10.0, 27.3, 45.0, 60.0, 89.5, 89.99];

    for &dl in &dl_degs {
        for &beta in &beta_degs {
            let dl_rad = dl.to_radians();
            let beta_rad = beta.to_radians();

            let reference = match reference_lookup_s10_for_test(beta_rad, dl_rad) {
                Some(v) => v,
                None => continue,
            };
            let got = Leinert1998Grid::lookup_s10(Radians::new(beta_rad), Radians::new(dl_rad))
                .expect("leinert lookup failed")
                .value();

            assert_eq!(
                got.to_bits(),
                reference.to_bits(),
                "bit mismatch at dl={dl}°, β={beta}°: Grid2D={got}, reference={reference}"
            );
        }
    }
}

#[test]
fn leinert_grid_matches_published_anchor_values() {
    let cases: [(f64, f64, f64); 4] = [
        // beta_deg, delta_lambda_deg, S10 at 500 nm.
        (0.0, 180.0, 180.0),
        (90.0, 180.0, 63.0),
        (0.0, 90.0, 202.0),
        (10.0, 30.0, 3700.0),
    ];

    for (beta_deg, delta_lambda_deg, expected) in cases {
        let got = Leinert1998Grid::lookup_s10(
            Radians::new(beta_deg.to_radians()),
            Radians::new(delta_lambda_deg.to_radians()),
        )
        .expect("Leinert anchor lookup")
        .value();
        assert_eq!(
            got, expected,
            "Leinert anchor mismatch at beta={beta_deg}°, delta_lambda={delta_lambda_deg}°"
        );
    }
}

#[test]
fn leinert_lookup_beta_at_90_degrees_succeeds() {
    let s10 = Leinert1998Grid::lookup_s10(
        Radians::new(90_f64.to_radians()),
        Radians::new(90_f64.to_radians()),
    )
    .expect("beta=90° should succeed");
    assert!(s10.value() > 0.0);
}

#[test]
fn leinert_lookup_rejects_non_finite_inputs() {
    assert!(Leinert1998Grid::lookup_s10(Radians::new(f64::NAN), Radians::new(1.0)).is_err());
    assert!(Leinert1998Grid::lookup_s10(Radians::new(0.5), Radians::new(f64::INFINITY)).is_err());
    assert!(
        Leinert1998Grid::lookup_s10(Radians::new(91_f64.to_radians()), Radians::new(1.0)).is_err()
    );
}

#[test]
fn noll2012_extinction_matches_numeric_reference_value() {
    let transmission = ZodiacalExtinction::Noll2012Approx
        .transmission(
            crate::units::WattsPerSquareMeterSteradianMicrometer::new(1.0),
            Nanometers::new(500.0),
            Degrees::new(0.0),
        )
        .value();
    let expected = 0.848_018_546_292_333;
    assert!(
        (transmission - expected).abs() <= 1.0e-12,
        "Noll-style extinction reference changed: got {transmission}, expected {expected}"
    );
    assert_eq!(
        ZodiacalExtinction::None
            .transmission(
                crate::units::WattsPerSquareMeterSteradianMicrometer::new(1.0),
                Nanometers::new(500.0),
                Degrees::new(60.0),
            )
            .value(),
        1.0
    );
}

#[test]
fn geometry_folds_delta_lambda_to_0_pi() {
    use super::geometry::compute_exoatmospheric;
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let geom = compute_exoatmospheric(time, target).expect("geometry");
    assert!(geom.delta_lambda.value() >= 0.0);
    assert!(geom.delta_lambda.value() <= std::f64::consts::PI);
}

#[test]
fn geometry_known_case_is_stable() {
    use super::geometry::compute_exoatmospheric;
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let geom = compute_exoatmospheric(time, target).expect("geometry");
    assert!(geom.beta.is_finite());
    assert!(geom.delta_lambda.is_finite());

    let beta_deg = geom.beta.value().to_degrees();
    assert!(beta_deg.abs() < 10.0);
    assert!(geom.delta_lambda.value() > 0.0);
}

#[test]
fn exoatmospheric_does_not_need_location() {
    let model = ZodiacalLight::leinert1998().expect("model");
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let out = model
        .compute_exoatmospheric(time, target)
        .expect("exoatmospheric compute");

    assert!(out.integrated.value() > 0.0);
    assert!(out.b_flux_s10.value() >= 0.0);
    assert!(out.v_flux_s10.value() >= 0.0);
}

#[test]
fn observed_extinction_reduces_or_preserves_flux() {
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();
    let observer = observatories::EL_PARANAL.geodetic();

    let no_ext = ZodiacalLight::leinert1998()
        .expect("model")
        .with_extinction(ZodiacalExtinction::None)
        .compute_observed(time, observer, target)
        .expect("no-extinction compute");

    let with_ext = ZodiacalLight::leinert1998()
        .expect("model")
        .with_extinction(ZodiacalExtinction::Noll2012Approx)
        .compute_observed(time, observer, target)
        .expect("extinction compute");

    assert!(with_ext.integrated.value() <= no_ext.integrated.value() + 1e-30);
    assert!(with_ext.b_flux_s10.value() <= no_ext.b_flux_s10.value() + 1e-10);
    assert!(with_ext.v_flux_s10.value() <= no_ext.v_flux_s10.value() + 1e-10);
}

#[test]
fn below_horizon_observed_returns_zero() {
    let observer = observatories::EL_PARANAL.geodetic();
    let target = Target::new(0.0 * DEG, 89.0 * DEG);
    let time = parse_utc("2023-09-04T01:48:00Z");

    let out = ZodiacalLight::leinert1998()
        .expect("model")
        .compute_observed(time, observer, target)
        .expect("below-horizon compute should not error");

    assert_eq!(out.integrated.value(), 0.0);
}

#[test]
fn b_and_v_diagnostics_follow_spectrally_resolved_solar_shape() {
    use super::geometry::ZodiacalGeometry;
    use super::spectrum::compute_outputs;
    use optica::data::Provenance;
    use optica::grid::OutOfRange;
    use optica::spectrum::Interpolation;
    use siderust::qtty::{length::Meter, Nanometer};

    // Non-flat spectrum: B (~440 nm) is bright, V (~550 nm) is faint so the
    // band diagnostics cannot collapse to one nearest-sample value.
    let lam: Vec<f64> = (300..=650).map(|i| i as f64).collect();
    let flux: Vec<f64> = lam
        .iter()
        .map(|wavelength| if *wavelength < 500.0 { 2.0 } else { 0.25 })
        .collect();
    let solar = optica::spectrum::SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam,
        flux,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("test-step")),
    )
    .expect("step spectrum");

    let geom = ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };

    let out = compute_outputs(&geom, &solar, ZodiacalExtinction::Noll2012Approx)
        .expect("compute outputs");

    assert!(out.b_flux_s10.value() > 0.0);
    assert!(out.v_flux_s10.value() > 0.0);
    assert!(out.b_flux_s10.value().is_finite() && out.v_flux_s10.value().is_finite());
    assert!(
        out.b_flux_s10.value() > 2.0 * out.v_flux_s10.value(),
        "B diagnostic must track the brighter blue continuum relative to V"
    );
}

#[test]
fn custom_brightness_grid_evaluates_finite_positive_radiance() {
    let grid = ZodiacalBrightnessGrid::new(
        vec![Degrees::new(0.0), Degrees::new(90.0)],
        vec![Degrees::new(0.0), Degrees::new(180.0)],
        vec![
            vec![S10s::new(100.0), S10s::new(50.0)],
            vec![S10s::new(63.0), S10s::new(63.0)],
        ],
        Some("test-grid".to_string()),
    )
    .expect("custom grid");

    let model = ZodiacalLight::with_brightness_model(ZodiacalBrightnessModel::CustomGrid(grid))
        .expect("model with custom grid");

    let time = parse_utc("2023-09-04T01:48:00Z");
    let observer = observatories::EL_PARANAL.geodetic();
    let target = sgr_a_star();

    let custom = model
        .compute(time, observer, target)
        .expect("custom grid compute");
    let leinert = ZodiacalLight::leinert1998()
        .expect("leinert")
        .compute(time, observer, target)
        .expect("leinert compute");

    assert!(custom.integrated.value() > 0.0);
    assert!(custom.b_flux_s10.value() > 0.0);
    assert!(custom.v_flux_s10.value() > 0.0);
    assert_ne!(
        custom.integrated.value(),
        leinert.integrated.value(),
        "custom brightness grid must change the observable radiance"
    );
}

#[test]
fn regression_known_case_sgr_a_star_paranal() {
    let model = ZodiacalLight::leinert1998().expect("model");
    let time = parse_utc("2023-09-04T01:48:00Z");
    let observer = observatories::EL_PARANAL.geodetic();
    let target = sgr_a_star();

    let out = model
        .compute(time, observer, target)
        .expect("regression compute");

    let integrated = out.integrated.value();
    assert!(
        integrated > 1e-4 && integrated < 1e-1,
        "integrated zodiacal radiance {integrated:.4e} is outside expected plausible range"
    );
}

fn sgr_a_star() -> Target {
    Target::new(266.41683 * DEG, -29.00781 * DEG)
}

fn parse_utc(s: &str) -> Time<UTC> {
    use chrono::{DateTime, Utc};
    let dt = DateTime::parse_from_rfc3339(s)
        .expect("parse UTC")
        .with_timezone(&Utc);
    Time::<UTC>::from_chrono(dt)
}
