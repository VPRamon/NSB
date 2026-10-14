// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
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
fn noll2012_extinction_matches_published_equations() {
    // Noll 2012 Eqs. 1, 4, 18–20. Reference intensity is always at 500 nm.
    // Independently compute molecular depth from the Bodhaine rational law,
    // rather than calling the production atmospheric helper in this oracle.
    for (i0, fr, fm) in [
        (100.0, 1.407 * 2.0 - 2.692, 1.309 * 2.0 - 2.598),
        (
            10_f64.powf(2.25),
            0.527 * 2.25 - 0.715,
            1.309 * 2.25 - 2.598,
        ),
        (10_f64.powf(2.5), 0.527 * 2.5 - 0.715, 0.468 * 2.5 - 0.702),
        (
            60.0,
            1.407 * 60_f64.log10() - 2.692,
            1.309 * 60_f64.log10() - 2.598,
        ),
    ] {
        for wave in [350.0, 500.0, 650.0] {
            let lam: f64 = wave / 1000.0;
            let l2 = lam * lam;
            let tr = 744.0 / 1013.25 * 0.0021520 * (1.0455996 - 341.29061 / l2 - 0.90230850 * l2)
                / (1.0 + 0.0027059889 / l2 - 85.968563 * l2);
            let k = if wave < 400.0 {
                0.05
            } else {
                0.013 * lam.powf(-1.38)
            };
            let tm = (10_f64.powf(0.4 * k)).ln();
            let x0 = (1.002432 + 0.148386 + 0.0096467) / (1.0 + 0.149864 + 0.0102963 + 0.000303978);
            let expected = (-(fr * tr + fm * tm) * x0).exp();
            let input =
                crate::units::Quantity::<crate::units::WattPerSquareMeterSteradianMicrometer>::new(
                    i0 * 1e-8,
                )
                .to::<crate::units::unit::WattPerSquareMeterSteradianNanometer>();
            let actual = ZodiacalExtinction::Noll2012Approx
                .transmission_for_spectral_radiance(input, Nanometers::new(wave), Degrees::new(0.0))
                .value();
            assert!(
                (actual - expected).abs() < 1e-13,
                "I0={i0}, wavelength={wave}"
            );
            if i0 == 60.0 {
                assert!(actual > 1.0, "faint-field in-scattering");
            }
            if i0 == 100.0 {
                assert!(actual < 1.0, "positive extinction");
            }
            let inclined = ZodiacalExtinction::Noll2012Approx
                .transmission_for_spectral_radiance(
                    input,
                    Nanometers::new(wave),
                    Degrees::new(50.0),
                )
                .value();
            // Airmass must amplify either extinction or net in-scattering.
            assert!((inclined - 1.0).abs() > (actual - 1.0).abs());
            assert_eq!(
                ZodiacalExtinction::None
                    .transmission_for_spectral_radiance(
                        input,
                        Nanometers::new(wave),
                        Degrees::new(50.0),
                    )
                    .value(),
                1.0
            );
        }
    }
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

    // Regenerated after correcting Noll units, sign, species depths and the
    // fixed 500 nm fit argument. See noll-equation-correction.md for old values.
    let integrated = out.integrated.value();
    assert!(
        (integrated - 0.139_361_113_489_048).abs() <= 1.0e-12,
        "integrated={integrated:.17}"
    );
    assert!(
        (out.b_flux_s10.value() - 132.444_535_079_010_7).abs() <= 1.0e-10,
        "b={:.17}",
        out.b_flux_s10.value()
    );
    assert!(
        (out.v_flux_s10.value() - 153.843_362_422_185_9).abs() <= 1.0e-10,
        "v={:.17}",
        out.v_flux_s10.value()
    );
    // Preserve the historical solar-only impact assertions under their original
    // propagation; comparing that TSIS baseline to corrected transport would
    // conflate a physical-equation fix with a change of solar spectrum.
    let geom = super::geometry::compute_observed(time, observer, target).unwrap();
    let legacy = legacy_solar_impact_outputs(&geom);
    assert!(((legacy.0 / 0.062_772_202_918_743_08) - 1.0).abs() < 0.18);
    assert!(((legacy.1 / 65.028_954_088_309_32) - 1.0).abs() < 0.12);
    assert!(((legacy.2 / 75.428_008_018_252_03) - 1.0).abs() < 0.18);
}

#[test]
fn analytic_solar_reference_validation_geometry_stays_within_tsis_impact_bounds() {
    use super::geometry::ZodiacalGeometry;
    // Offline baseline from the former TSIS-derived runtime
    // solar_spectrum.dat SHA-256
    // 71da8c3c5e2204dea0fde06329ef89bcec63a22980ed640bad54402cac5fee02.
    // The old bytes are intentionally not required by this test.
    const TSIS_BASELINE: (f64, f64, f64) =
        (0.056_968_993_257_1, 58.708_789_926_3, 68.565_285_636_9);
    let geometry = ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };
    let output = legacy_solar_impact_outputs(&geometry);

    assert!(
        ((output.0 / TSIS_BASELINE.0) - 1.0).abs() < 0.18,
        "integrated impact exceeded 18%"
    );
    assert!(
        ((output.1 / TSIS_BASELINE.1) - 1.0).abs() < 0.12,
        "B diagnostic impact exceeded 12%"
    );
    assert!(
        ((output.2 / TSIS_BASELINE.2) - 1.0).abs() < 0.18,
        "V diagnostic impact exceeded 18%"
    );
}

#[test]
fn zero_custom_zodiacal_source_remains_finite_with_effective_extinction() {
    let geom = super::geometry::ZodiacalGeometry {
        beta: Radians::new(0.3),
        delta_lambda: Radians::new(1.5),
        zenith: Some(Degrees::new(30.0)),
    };
    let solar = crate::spectra::solar::load().unwrap();
    let out = super::spectrum::compute_outputs_with_s10(
        &geom,
        &solar,
        ZodiacalExtinction::Noll2012Approx,
        crate::units::radiometry::S10s::new(0.0),
    )
    .unwrap();
    assert_eq!(out.integrated.value(), 0.0);
    assert_eq!(out.b_flux_s10.value(), 0.0);
    assert_eq!(out.v_flux_s10.value(), 0.0);
}

// Test-only counterfactual: retain the solar-reference impact checks with the
// historical propagation held fixed. This must never be used in production.
// The legacy equation is intentionally wrong physics; its historical fixtures
// measure only the Planck-versus-TSIS substitution under that fixed equation.
fn legacy_solar_impact_outputs(geom: &super::geometry::ZodiacalGeometry) -> (f64, f64, f64) {
    let solar = crate::spectra::solar::load().unwrap();
    let s10 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)
        .unwrap()
        .value();
    let solar500 = solar.interp_at(Nanometers::new(500.0)).value();
    let z = geom
        .zenith
        .unwrap_or(Degrees::new(0.0))
        .value()
        .to_radians();
    let c = z.cos();
    let x = (1.002432 * c * c + 0.148386 * c + 0.0096467)
        / (c * c * c + 0.149864 * c * c + 0.0102963 * c + 0.000303978);
    let mut nodes = Vec::new();
    let mut bv = [0.0; 2];
    for (&w, &sun) in solar.xs_raw().iter().zip(solar.ys_raw()) {
        if !(300.0..=650.0).contains(&w) {
            continue;
        }
        let source = s10 * 1.28e-11 * sun / solar500
            * super::reddening::reddening_factor(geom.beta, geom.delta_lambda, Nanometers::new(w))
                .value();
        let dex = (source * 1000.0).log10();
        let fr = if dex <= 2.244 {
            1.407 * dex - 2.692
        } else {
            0.527 * dex - 0.715
        };
        let fm = if dex <= 2.255 {
            1.309 * dex - 2.598
        } else {
            0.468 * dex - 0.702
        };
        let k = if w < 400.0 {
            0.05
        } else {
            0.013 * (w / 1000.0).powf(-1.38)
        };
        let value = source * (0.4 * std::f64::consts::LN_10 * k * (fr + fm) * x).exp();
        if w == 445.0 {
            bv[0] = value / 1.28e-11;
        }
        if w == 551.0 {
            bv[1] = value / 1.28e-11;
        }
        nodes.push((w, value * w * 1e-9 / 1.986445857148968e-25 * 1e-13));
    }
    let integrated = nodes
        .windows(2)
        .map(|w| (w[1].0 - w[0].0) * (w[0].1 + w[1].1) / 2.0)
        .sum();
    (integrated, bv[0], bv[1])
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
