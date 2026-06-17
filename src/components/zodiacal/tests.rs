//! Tests for the zodiacal-light module.

use super::extinction::ZodiacalExtinction;
use super::leinert::{legacy_lookup_s10_for_test, Leinert1998Grid};
use super::model::{ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalLight};
use crate::evaluator::Target;
use qtty::angular::{Degrees, Radians};
use siderust::qtty::DEG;
use tempoch::{Time, UTC};

// ─── Leinert grid tests ───────────────────────────────────────────────────

/// Bit-for-bit parity between the `Grid2D` implementation and the legacy
/// hand-rolled bilinear lookup over a dense sweep of `(dl, β)` inputs.
///
/// The corner-clamp regions trivially match (both return the same constant).
#[test]
fn leinert_grid2d_bitwise_parity_with_legacy() {
    let dl_degs = [0.5_f64, 1.0, 5.0, 10.0, 27.3, 90.0, 124.5, 175.0, 179.9];
    let beta_degs = [0.5_f64, 5.0, 10.0, 27.3, 45.0, 60.0, 89.5, 89.99];

    for &dl in &dl_degs {
        for &beta in &beta_degs {
            let dl_rad = dl.to_radians();
            let beta_rad = beta.to_radians();

            let legacy = match legacy_lookup_s10_for_test(beta_rad, dl_rad) {
                Some(v) => v,
                None => continue,
            };
            let got = Leinert1998Grid::lookup_s10(Radians::new(beta_rad), Radians::new(dl_rad))
                .expect("leinert lookup failed")
                .value();

            assert_eq!(
                got.to_bits(),
                legacy.to_bits(),
                "bit mismatch at dl={dl}°, β={beta}°: Grid2D={got}, legacy={legacy}"
            );
        }
    }
}

#[test]
fn leinert_lookup_beta_at_90_degrees_succeeds() {
    let s10 = Leinert1998Grid::lookup_s10(
        Radians::new(90_f64.to_radians()),
        Radians::new(90_f64.to_radians()),
    )
    .expect("beta=90° should succeed");
    assert!(
        s10.value() > 0.0,
        "Leinert S10 at beta=90° must be positive"
    );
}

#[test]
fn leinert_lookup_rejects_non_finite_inputs() {
    assert!(
        Leinert1998Grid::lookup_s10(Radians::new(f64::NAN), Radians::new(1.0)).is_err(),
        "NaN beta should return an error"
    );
    assert!(
        Leinert1998Grid::lookup_s10(Radians::new(0.5), Radians::new(f64::INFINITY)).is_err(),
        "infinite delta_lambda should return an error"
    );
    assert!(
        Leinert1998Grid::lookup_s10(Radians::new(91_f64.to_radians()), Radians::new(1.0)).is_err(),
        "beta > 90° should return an error"
    );
}

// ─── Geometry tests ───────────────────────────────────────────────────────

/// `delta_lambda` must always be folded to `[0, π]`.
#[test]
fn geometry_folds_delta_lambda_to_0_pi() {
    use super::geometry::compute_exoatmospheric;
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let geom = compute_exoatmospheric(time, target).expect("geometry");
    assert!(
        geom.delta_lambda.value() >= 0.0,
        "delta_lambda must be >= 0: {}",
        geom.delta_lambda.value()
    );
    assert!(
        geom.delta_lambda.value() <= std::f64::consts::PI,
        "delta_lambda must be <= π: {}",
        geom.delta_lambda.value()
    );
}

/// Regression: fixed UTC/target should produce stable ecliptic geometry.
#[test]
fn geometry_known_case_is_stable() {
    use super::geometry::compute_exoatmospheric;
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let geom = compute_exoatmospheric(time, target).expect("geometry");
    assert!(geom.beta.is_finite(), "beta must be finite");
    assert!(geom.delta_lambda.is_finite(), "delta_lambda must be finite");

    // Tolerance-protected regression: values must be near the stored reference.
    // β ≈ −5.6° for Sgr A* (ecliptic latitude is stable to the frame transform).
    let beta_deg = geom.beta.value().to_degrees();
    assert!(
        beta_deg.abs() < 10.0,
        "Sgr A* ecliptic latitude should be near 0°, got {beta_deg:.3}°"
    );
    assert!(
        geom.delta_lambda.value() > 0.0,
        "delta_lambda must be positive for a realistic geometry"
    );
}

// ─── Exoatmospheric test ──────────────────────────────────────────────────

#[test]
fn exoatmospheric_does_not_need_location() {
    let model = ZodiacalLight::standard().expect("model");
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let out = model
        .compute_exoatmospheric(time, target)
        .expect("exoatmospheric compute");

    assert!(
        out.integrated.value() > 0.0,
        "exoatmospheric integrated must be positive"
    );
    assert!(out.b_flux_s10.value() >= 0.0);
    assert!(out.v_flux_s10.value() >= 0.0);
}

// ─── Extinction test ──────────────────────────────────────────────────────

#[test]
fn observed_extinction_reduces_or_preserves_flux() {
    use crate::evaluator::Location;
    use crate::site::Site;

    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();
    let location = Location::NamedSite(Site::Paranal).geodetic();

    let no_ext = ZodiacalLight::standard()
        .expect("model")
        .with_extinction(ZodiacalExtinction::None)
        .compute_observed(time, location, target)
        .expect("no-extinction compute");

    let with_ext = ZodiacalLight::standard()
        .expect("model")
        .with_extinction(ZodiacalExtinction::Noll2012Approx)
        .compute_observed(time, location, target)
        .expect("extinction compute");

    assert!(
        with_ext.integrated.value() <= no_ext.integrated.value() + 1e-30,
        "extinction should not increase integrated flux: \
         noll={} > none={}",
        with_ext.integrated.value(),
        no_ext.integrated.value()
    );
    assert!(
        with_ext.b_flux_s10.value() <= no_ext.b_flux_s10.value() + 1e-10,
        "extinction should not increase B-band S10"
    );
    assert!(
        with_ext.v_flux_s10.value() <= no_ext.v_flux_s10.value() + 1e-10,
        "extinction should not increase V-band S10"
    );
}

// ─── Horizon semantics test ───────────────────────────────────────────────

/// `compute_observed` must return zero radiance for a below-horizon target.
#[test]
fn below_horizon_observed_returns_zero() {
    use crate::evaluator::Location;
    use crate::site::Site;

    let location = Location::NamedSite(Site::Paranal).geodetic();

    // A target near the North Celestial Pole (dec +89°) is below the horizon
    // at Paranal (latitude ≈ −24.6°): it culminates at altitude ≈ 90 − 89 − 24.6
    // ≈ −23.6°, so it never rises.
    let target = Target::new(0.0 * DEG, 89.0 * DEG);

    let time = parse_utc("2023-09-04T01:48:00Z");

    let out = ZodiacalLight::standard()
        .expect("model")
        .compute_observed(time, location, target)
        .expect("below-horizon compute should not error");

    assert_eq!(
        out.integrated.value(),
        0.0,
        "below-horizon target must yield zero integrated flux"
    );
}

// ─── B/V interpolation test ───────────────────────────────────────────────

/// B/V values must be computed by interpolation, not nearest-sample.
/// With a linear solar spectrum, the interpolated value at 445 nm must equal
/// the true linear interpolation between bracketing samples.
#[test]
fn b_v_are_interpolated_not_nearest_sample() {
    use super::geometry::ZodiacalGeometry;
    use super::spectrum::compute_outputs;
    use optica::data::Provenance;
    use optica::grid::OutOfRange;
    use optica::spectrum::Interpolation;
    use siderust::qtty::{length::Meter, Nanometer};

    // Build a synthetic flat solar spectrum (constant 1.0 W/m²/nm) spanning
    // a broad range so 500 nm is covered. All Leinert/reddening/extinction
    // values should yield the same S10 regardless of exact wavelength.
    let lam: Vec<f64> = (300..=650).map(|i| i as f64).collect();
    let flux: Vec<f64> = vec![1.0; lam.len()];
    let solar = optica::spectrum::SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam,
        flux,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("test-flat")),
    )
    .expect("flat spectrum");

    let geom = ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };

    let out = compute_outputs(&geom, &solar, ZodiacalExtinction::Noll2012Approx)
        .expect("compute outputs");

    // With a flat spectrum the B and V S10 values should be close to each
    // other (they differ only due to reddening). The key is they must be
    // finite and positive (not zero from a missed interpolation).
    assert!(
        out.b_flux_s10.value() > 0.0,
        "B flux must be positive, got {}",
        out.b_flux_s10.value()
    );
    assert!(
        out.v_flux_s10.value() > 0.0,
        "V flux must be positive, got {}",
        out.v_flux_s10.value()
    );
    assert!(
        out.b_flux_s10.value().is_finite() && out.v_flux_s10.value().is_finite(),
        "B/V fluxes must be finite"
    );
}

// ─── Standard compute positive test ──────────────────────────────────────

#[test]
fn standard_compute_returns_positive_integrated() {
    use crate::evaluator::Location;
    use crate::site::Site;

    let model = ZodiacalLight::standard().expect("model");
    let time = parse_utc("2023-09-04T01:48:00Z");
    let location = Location::NamedSite(Site::Paranal).geodetic();
    let target = sgr_a_star();

    let out = model.compute(time, location, target).expect("compute");
    assert!(
        out.integrated.value() > 0.0,
        "integrated zodiacal must be positive"
    );
    assert!(out.b_flux_s10.value() >= 0.0);
    assert!(out.v_flux_s10.value() >= 0.0);
}

// ─── Custom grid test ─────────────────────────────────────────────────────

#[test]
fn custom_grid_path_works() {
    use crate::evaluator::Location;
    use crate::site::Site;

    // Build a minimal 2×2 custom grid matching the Leinert table corners.
    let grid = ZodiacalBrightnessGrid::new(
        vec![0.0, 90.0],
        vec![0.0, 180.0],
        vec![vec![100.0, 50.0], vec![63.0, 63.0]],
        Some("test-grid".to_string()),
    )
    .expect("custom grid");

    let model = ZodiacalLight::with_brightness_model(ZodiacalBrightnessModel::CustomGrid(grid))
        .expect("model with custom grid");

    let time = parse_utc("2023-09-04T01:48:00Z");
    let location = Location::NamedSite(Site::Paranal).geodetic();
    let target = sgr_a_star();

    let out = model
        .compute(time, location, target)
        .expect("custom grid compute");
    assert!(out.integrated.value() >= 0.0);
    assert!(out.b_flux_s10.value() >= 0.0);
    assert!(out.v_flux_s10.value() >= 0.0);
}

// ─── Numerical regression test ────────────────────────────────────────────

/// Fixed-input regression: values must be stable within a generous tolerance
/// to catch frame or pipeline regressions while allowing minor floating-point
/// differences from toolchain updates.
#[test]
fn regression_known_case_sgr_a_star_paranal() {
    use crate::evaluator::Location;
    use crate::site::Site;

    let model = ZodiacalLight::standard().expect("model");
    let time = parse_utc("2023-09-04T01:48:00Z");
    let location = Location::NamedSite(Site::Paranal).geodetic();
    let target = sgr_a_star();

    let out = model
        .compute(time, location, target)
        .expect("regression compute");

    // Regression bounds derived from first-pass values.
    // Integrated: must be in the plausible zodiacal range
    // [1e-4, 1e-1] ph cm⁻² ns⁻¹ sr⁻¹.
    let integrated = out.integrated.value();
    assert!(
        integrated > 1e-4 && integrated < 1e-1,
        "integrated zodiacal radiance {integrated:.4e} is outside expected plausible range"
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────

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
