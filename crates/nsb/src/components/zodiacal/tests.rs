//! Tests for the zodiacal-light module.

use super::extinction::ZodiacalExtinction;
use super::leinert::{test_support::reference_lookup_s10, Leinert1998Grid};
use super::model::ZodiacalLight;
use crate::error::{NsbError, Result};
use crate::evaluator::Target;
use crate::units::angular::{Degrees, Radians};
use crate::units::radiometry::{S10s, S10s as S10};
use siderust::catalogs::observatories;
use siderust::qtty::Nanometers;
use siderust::qtty::DEG;
use tempoch::{Time, UTC};

#[derive(Debug, Clone)]
struct ZodiacalBrightnessGrid {
    beta_axis: Vec<Degrees>,
    delta_lambda_axis: Vec<Degrees>,
    s10_values: Vec<Vec<S10>>,
}

impl ZodiacalBrightnessGrid {
    fn new(
        beta_axis: Vec<Degrees>,
        delta_lambda_axis: Vec<Degrees>,
        s10_values: Vec<Vec<S10>>,
    ) -> Result<Self> {
        if beta_axis.len() < 2 || delta_lambda_axis.len() < 2 {
            return Err(NsbError::OutOfRange(
                "custom ZodiacalBrightnessGrid axes must have at least 2 points each".to_string(),
            ));
        }
        if !is_strictly_increasing(&beta_axis) {
            return Err(NsbError::OutOfRange(
                "beta_axis must be strictly increasing".to_string(),
            ));
        }
        if !is_strictly_increasing(&delta_lambda_axis) {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis must be strictly increasing".to_string(),
            ));
        }
        if *beta_axis.first().unwrap() < Degrees::new(0.0)
            || *beta_axis.last().unwrap() > Degrees::new(90.0)
        {
            return Err(NsbError::OutOfRange(
                "beta_axis values must be in [0, 90] degrees".to_string(),
            ));
        }
        if *delta_lambda_axis.first().unwrap() < Degrees::new(0.0)
            || *delta_lambda_axis.last().unwrap() > Degrees::new(180.0)
        {
            return Err(NsbError::OutOfRange(
                "delta_lambda_axis values must be in [0, 180] degrees".to_string(),
            ));
        }
        if s10_values.len() != beta_axis.len() {
            return Err(NsbError::OutOfRange(format!(
                "s10_values row count {} != beta_axis length {}",
                s10_values.len(),
                beta_axis.len()
            )));
        }
        for (i, row) in s10_values.iter().enumerate() {
            if row.len() != delta_lambda_axis.len() {
                return Err(NsbError::OutOfRange(format!(
                    "s10_values row {} has length {} != delta_lambda_axis length {}",
                    i,
                    row.len(),
                    delta_lambda_axis.len()
                )));
            }
            for &value in row {
                if !value.is_finite() || value < S10::new(0.0) {
                    return Err(NsbError::OutOfRange(format!(
                        "s10_values[{i}] contains non-finite or negative value: {}",
                        value.value()
                    )));
                }
            }
        }
        Ok(Self {
            beta_axis,
            delta_lambda_axis,
            s10_values,
        })
    }

    fn lookup_s10(&self, beta: Degrees, delta_lambda: Degrees) -> Result<S10> {
        let beta = beta.abs().min(Degrees::new(90.0));
        let delta_lambda = delta_lambda.abs().min(Degrees::new(180.0));
        let (ib0, ib1, tb) = bracket(&self.beta_axis, beta);
        let (il0, il1, tl) = bracket(&self.delta_lambda_axis, delta_lambda);
        Ok(bilinear(
            self.s10_values[ib0][il0],
            self.s10_values[ib0][il1],
            self.s10_values[ib1][il0],
            self.s10_values[ib1][il1],
            tb,
            tl,
        ))
    }
}

fn is_strictly_increasing(values: &[Degrees]) -> bool {
    values.windows(2).all(|window| window[1] > window[0])
}

fn bracket(axis: &[Degrees], value: Degrees) -> (usize, usize, f64) {
    let pos = axis.partition_point(|&x| x <= value);
    let i1 = pos.min(axis.len() - 1);
    let i0 = if i1 == 0 { 0 } else { i1 - 1 };
    let t = if axis[i1] > axis[i0] {
        (value - axis[i0]).value() / (axis[i1] - axis[i0]).value()
    } else {
        0.0
    };
    (i0, i1, t.clamp(0.0, 1.0))
}

fn bilinear(v00: S10, v01: S10, v10: S10, v11: S10, tx: f64, ty: f64) -> S10 {
    let r0 = v00 + (v10 - v00) * tx;
    let r1 = v01 + (v11 - v01) * tx;
    r0 + (r1 - r0) * ty
}

#[test]
fn leinert_grid2d_matches_historical_reference() {
    let dl_degs = [0.5_f64, 1.0, 5.0, 10.0, 27.3, 90.0, 124.5, 175.0, 179.9];
    let beta_degs = [0.5_f64, 5.0, 10.0, 27.3, 45.0, 60.0, 89.5, 89.99];

    for &dl in &dl_degs {
        for &beta in &beta_degs {
            let dl_rad = dl.to_radians();
            let beta_rad = beta.to_radians();

            let reference = match reference_lookup_s10(beta_rad, dl_rad) {
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
        .transmission_for_spectral_radiance(
            crate::units::Quantity::<crate::units::WattPerSquareMeterSteradianMicrometer>::new(1.0)
                .to::<crate::units::unit::WattPerSquareMeterSteradianNanometer>(),
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
            .transmission_for_spectral_radiance(
                crate::units::Quantity::<crate::units::WattPerSquareMeterSteradianMicrometer>::new(
                    1.0
                )
                .to::<crate::units::unit::WattPerSquareMeterSteradianNanometer>(),
                Nanometers::new(500.0),
                Degrees::new(60.0),
            )
            .value(),
        1.0
    );
}

#[test]
fn geometry_folds_delta_lambda_to_0_pi() {
    use super::geometry::test_support::compute_exoatmospheric;
    let time = parse_utc("2023-09-04T01:48:00Z");
    let target = sgr_a_star();

    let geom = compute_exoatmospheric(time, target).expect("geometry");
    assert!(geom.delta_lambda.value() >= 0.0);
    assert!(geom.delta_lambda.value() <= std::f64::consts::PI);
}

#[test]
fn geometry_known_case_is_stable() {
    use super::geometry::test_support::compute_exoatmospheric;
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

    let out = super::model::test_support::compute_exoatmospheric(&model, time, target)
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
    use crate::spectra::solar::SolarSpectrum;
    use siderust::optica::data::Provenance;
    use siderust::optica::grid::OutOfRange;
    use siderust::optica::spectrum::Interpolation;

    // Non-flat spectrum: B (~440 nm) is bright, V (~550 nm) is faint so the
    // band diagnostics cannot collapse to one nearest-sample value.
    let lam: Vec<f64> = (300..=650).map(|i| i as f64).collect();
    let flux: Vec<f64> = lam
        .iter()
        .map(|wavelength| if *wavelength < 500.0 { 2.0 } else { 0.25 })
        .collect();
    let solar = SolarSpectrum::from_raw(
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
    )
    .expect("custom grid");

    let time = parse_utc("2023-09-04T01:48:00Z");
    let observer = observatories::EL_PARANAL.geodetic();
    let target = sgr_a_star();
    let geom = super::geometry::compute_observed(time, observer, target).expect("geometry");
    let s10_500 = grid
        .lookup_s10(
            geom.beta.abs().to::<crate::units::angular::Degree>(),
            geom.delta_lambda.to::<crate::units::angular::Degree>(),
        )
        .expect("custom brightness");
    let solar = crate::spectra::solar::load().expect("solar spectrum");
    let custom = super::spectrum::compute_outputs_with_s10(
        &geom,
        &solar,
        ZodiacalExtinction::Noll2012Approx,
        s10_500,
    )
    .expect("custom grid compute");
    let leinert = ZodiacalLight::leinert1998()
        .expect("leinert")
        .compute(time, observer, target)
        .expect("leinert compute");

    assert!(custom.integrated.value() > 0.0);
    assert!(custom.b_flux_s10.value() > 0.0);
    assert!(custom.v_flux_s10.value() > 0.0);
    assert_ne!(custom.integrated.value(), leinert.integrated.value());
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
        (integrated - 0.062_373_849_830_161_79).abs() <= 1.0e-12,
        "integrated={integrated:.17}"
    );
    assert!(
        (out.b_flux_s10.value() - 69.560_151_467_088_9).abs() <= 1.0e-10,
        "b={:.17}",
        out.b_flux_s10.value()
    );
    assert!(
        (out.v_flux_s10.value() - 72.993_011_975_782_07).abs() <= 1.0e-10,
        "v={:.17}",
        out.v_flux_s10.value()
    );
}

fn hsrs_from_env(variable: &str) -> crate::spectra::solar::SolarSpectrum {
    use crate::spectra::solar::SolarSpectrum;
    use siderust::optica::grid::OutOfRange;
    use siderust::optica::spectrum::Interpolation;

    let path = std::env::var(variable).unwrap_or_else(|_| panic!("{variable} path"));
    let raw = std::fs::read_to_string(path).expect("HSRS CSV");
    let mut wavelengths = Vec::new();
    let mut irradiances = Vec::new();
    for line in raw.lines().skip(1) {
        let (wavelength, irradiance) = line.split_once(',').expect("two columns");
        wavelengths.push(wavelength.parse().expect("wavelength"));
        irradiances.push(irradiance.parse().expect("irradiance"));
    }
    SolarSpectrum::from_raw(
        wavelengths,
        irradiances,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .expect("HSRS spectrum")
}

/// Reproducible upstream resolution study used by the validation report.
#[test]
#[ignore = "requires NSB_TSIS_P025 and NSB_TSIS_NATIVE from the solar-spectrum update workspace"]
fn native_hsrs_resolution_comparison() {
    use super::geometry::ZodiacalGeometry;
    use super::spectrum::compute_outputs;

    let candidate = hsrs_from_env("NSB_TSIS_P025");
    let native = hsrs_from_env("NSB_TSIS_NATIVE");
    let geometry = ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };
    let candidate = compute_outputs(&geometry, &candidate, ZodiacalExtinction::Noll2012Approx)
        .expect("candidate-resolution evaluation");
    let native = compute_outputs(&geometry, &native, ZodiacalExtinction::Noll2012Approx)
        .expect("native-resolution evaluation");
    eprintln!(
        "candidate=({:.17},{:.17},{:.17}) native=({:.17},{:.17},{:.17})",
        candidate.integrated.value(),
        candidate.b_flux_s10.value(),
        candidate.v_flux_s10.value(),
        native.integrated.value(),
        native.b_flux_s10.value(),
        native.v_flux_s10.value()
    );
    // Source-selection gates (p025nm ↔ native). Measured: ~0.265% / 0.94% / 0.027%.
    assert!(((candidate.integrated.value() / native.integrated.value()) - 1.0).abs() < 3.0e-3);
    assert!(((candidate.b_flux_s10.value() / native.b_flux_s10.value()) - 1.0).abs() < 0.01);
    assert!(((candidate.v_flux_s10.value() / native.v_flux_s10.value()) - 1.0).abs() < 5.0e-4);
}

/// Scientific error budget for the compact runtime representation.
#[test]
#[ignore = "requires NSB_TSIS_P025 from the solar-spectrum update workspace"]
fn compact_runtime_hsrs_comparison() {
    use super::geometry::ZodiacalGeometry;
    use super::spectrum::compute_outputs;
    use crate::spectra::solar;

    let runtime = solar::load().expect("bundled compact runtime spectrum");
    let selected = hsrs_from_env("NSB_TSIS_P025");
    let geometry = ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };
    let runtime = compute_outputs(&geometry, &runtime, ZodiacalExtinction::Noll2012Approx)
        .expect("compact runtime evaluation");
    let selected = compute_outputs(&geometry, &selected, ZodiacalExtinction::Noll2012Approx)
        .expect("p025nm evaluation");
    eprintln!(
        "runtime=({:.17},{:.17},{:.17}) p025nm=({:.17},{:.17},{:.17})",
        runtime.integrated.value(),
        runtime.b_flux_s10.value(),
        runtime.v_flux_s10.value(),
        selected.integrated.value(),
        selected.b_flux_s10.value(),
        selected.v_flux_s10.value()
    );
    // 0.05% is one sixth of the HSRS's best quoted radiometric uncertainty
    // (0.3%) and distinguishes computational reduction error from source error.
    assert!(((runtime.integrated.value() / selected.integrated.value()) - 1.0).abs() < 5.0e-4);
    assert!(((runtime.b_flux_s10.value() / selected.b_flux_s10.value()) - 1.0).abs() < 1.0e-12);
    assert!(((runtime.v_flux_s10.value() / selected.v_flux_s10.value()) - 1.0).abs() < 1.0e-12);
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
