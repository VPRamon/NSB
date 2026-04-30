//! Zodiacal light component.
//!
//! Port of `CalculateZL` and helpers from `NSB_Utils.py`.
//!
//! The pipeline:
//! 1. Look up the Leinert (1998) S10 brightness at `(β, λ-λ_sun)`.
//! 2. Scale the solar spectrum so its 500-nm value matches the table.
//! 3. Apply Leinert reddening.
//! 4. Apply atmospheric extinction (Noll et al. 2012 fext for Rayleigh + Mie).
//! 5. Convert from energy to photons via `5.03e7 · λ_Å` and integrate over
//!    the 300–650 nm band.
//!
//! Scientific role:
//! zodiacal light is sunlight scattered by interplanetary dust. It is one of
//! the dominant dark-sky components away from the Galactic plane, especially
//! near the ecliptic and for sightlines not far from the Sun.
//!
//! Contribution to the science:
//! this file provides the most geometry-dependent optical-background component
//! in the crate. It connects sky position relative to the Sun and ecliptic
//! plane to a wavelength-dependent radiance, then propagates that radiance
//! through reddening and extinction before integrating it into the NSB band.

use std::sync::OnceLock;

use crate::data::leinert::{
    CORNER_LL_LT_20_B_LT_25, CORNER_LL_LT_25_B_LT_20, CORNER_LL_LT_30_B_LT_15, LEINERT_S10,
    S10_TO_W_M2_SR_UM,
};
use crate::error::{NsbError, Result};
use crate::spectra::SampledSpectrum;
use qtty::angular::{Degree, Degrees, Radians};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use siderust::atmosphere::{airmass, AirmassFormula};
use siderust::qtty::{length::Meter, Nanometer, Nanometers};
use siderust::spectra::{
    algo, Interpolation, OutOfRange as SpectrumOutOfRange, Provenance as SpectrumProvenance,
};
use siderust::tables::{ConstantRegion, Grid2D, OutOfRange, Provenance};

/// Unit marker for S10 values — the inner unit struct from `qtty::radiometry`.
use qtty::radiometry::S10 as S10Unit;

/// Type alias for the Leinert S10 grid.
/// x-axis = β [deg, ascending], y-axis = λ-λ_sun [deg, descending].
type LeinertGrid = Grid2D<Degree, Degree, S10Unit>;

/// Lazily-initialised Leinert S10 grid (thread-safe via [`OnceLock`]).
///
/// The grid is constructed from `LEINERT_S10` in its natural row-descending
/// order (`row[0]` ↔ `λ−λ_sun = 180°`, `row[36]` ↔ `0°`) using
/// [`Grid2D::from_raw_row_major_y_descending`], which internally normalizes
/// the y-axis to ascending storage and applies a uniform reflection
/// (`y_internal = (180 + 0) − dl_deg = 180 − dl_deg`) at lookup time.
/// This is bit-for-bit equivalent to the legacy hand-rolled
/// `lt = (180 − dl_deg − 5·l0)/5` arithmetic that NSB used before
/// upstreaming the lookup to siderust.
///
/// The three Leinert (1998) corner-extrapolation patches around the solar
/// disk are attached as [`ConstantRegion`]s, so callers no longer need to
/// dispatch them manually before [`Grid2D::interp_at`].
fn s10_grid() -> &'static LeinertGrid {
    static G: OnceLock<LeinertGrid> = OnceLock::new();
    G.get_or_init(|| {
        // β axis: 0°, 5°, …, 90° (19 values, ascending).
        let xs: Vec<f64> = (0..=18).map(|i| i as f64 * 5.0).collect();
        // λ−λ_sun axis in its natural descending order: 180°, 175°, …, 0°.
        let ys_desc: Vec<f64> = (0..37).map(|i| 180.0 - i as f64 * 5.0).collect();
        let mut table: Vec<f64> = Vec::with_capacity(37 * 19);
        for row in LEINERT_S10.iter() {
            table.extend_from_slice(row);
        }
        Grid2D::from_raw_row_major_y_descending(xs, ys_desc, table)
            .expect("Leinert S10 grid invariants")
            // Corner extrapolations from Leinert (1998), equivalent to the
            // legacy `if dl_deg < X && beta_deg < Y` clamp branches.
            .with_constant_region(ConstantRegion::lower_corner(
                25.0,
                20.0,
                CORNER_LL_LT_20_B_LT_25,
            ))
            .with_constant_region(ConstantRegion::lower_corner(
                20.0,
                25.0,
                CORNER_LL_LT_25_B_LT_20,
            ))
            .with_constant_region(ConstantRegion::lower_corner(
                15.0,
                30.0,
                CORNER_LL_LT_30_B_LT_15,
            ))
            .with_provenance(Provenance::cited("Leinert+1998"))
    })
}

const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;
const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;

/// Inputs needed by `compute`. Coordinates are in radians and degrees as
/// indicated; using primitive types here avoids dragging the full ephemeris
/// surface into the test paths.
#[derive(Debug, Clone, Copy)]
pub struct ZlInputs {
    /// Source ecliptic latitude `β` [rad].
    pub beta_rad: f64,
    /// Source ecliptic longitude minus solar longitude `(λ - λ_sun)` [rad],
    /// reduced to `[0, π]`.
    pub delta_lambda_rad: f64,
    /// Source zenith distance [deg].
    pub zenith_deg: f64,
}

#[derive(Debug, Clone)]
pub struct ZlOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
    pub spectrum: SampledSpectrum<Nanometer, Meter, f64>,
}

/// Bilinear interpolation in the Leinert table at `(β [rad], (λ-λ_sun) [rad])`.
/// Returns `S10` brightness at 500 nm. The Leinert (1998) corner clamps
/// around the solar disk are attached to the grid as constant-fill regions
/// at construction time, so this function is now a thin radians→degrees
/// wrapper around [`Grid2D::interp_at`].
pub fn leinert_lookup_s10(beta_rad: f64, delta_lambda_rad: f64) -> Result<S10> {
    let beta_deg = beta_rad.to_degrees().abs();
    let dl_deg = delta_lambda_rad.to_degrees().abs().min(180.0);
    if !(0.0..90.0).contains(&beta_deg) {
        return Err(NsbError::OutOfRange(format!("β={beta_deg}° not in [0,90)")));
    }
    if !(0.0..=180.0).contains(&dl_deg) {
        return Err(NsbError::OutOfRange(format!("Δλ={dl_deg}° not in [0,180]")));
    }
    s10_grid()
        .interp_at(
            Degrees::new(beta_deg),
            Degrees::new(dl_deg),
            OutOfRange::ClampToEndpoints,
            OutOfRange::ClampToEndpoints,
        )
        .map_err(|e| NsbError::Interpolation(e.to_string()))
}

/// Leinert reddening factor at a given wavelength and elongation.
/// Mirrors `GetZodicalReddening` in NSB_Utils.py.
fn reddening_factor(beta_rad: f64, delta_lambda_rad: f64, lambda_nm: f64) -> f64 {
    let elong_deg = (delta_lambda_rad.cos() * beta_rad.cos())
        .acos()
        .to_degrees();
    let log_ratio = (lambda_nm / 500.0).ln();
    if elong_deg <= 30.0 {
        if (220.0..550.0).contains(&lambda_nm) {
            return 1.0 + 1.2 * log_ratio;
        }
        if (550.0..2500.0).contains(&lambda_nm) {
            return 1.0 + 0.8 * log_ratio;
        }
        return 1.0;
    }
    if elong_deg >= 90.0 {
        if (220.0..550.0).contains(&lambda_nm) {
            return 1.0 + 0.9 * log_ratio;
        }
        if (550.0..2500.0).contains(&lambda_nm) {
            return 1.0 + 0.6 * log_ratio;
        }
        return 1.0;
    }
    // Linear interpolation in elongation.
    let (y1, y2) = if (220.0..550.0).contains(&lambda_nm) {
        (1.2, 0.9)
    } else if (550.0..2500.0).contains(&lambda_nm) {
        (0.9, 0.6)
    } else {
        return 1.0;
    };
    let y = (y2 - y1) * (elong_deg - 30.0) / 60.0 + y1;
    1.0 + y * log_ratio
}

/// Atmospheric extinction (Noll et al. 2012) — Rayleigh + Mie combined.
/// Returns the transmission `T(λ)`.
fn extinction_transmission(zl_value_w_m2_sr_um: f64, lambda_nm: f64, zenith_deg: f64) -> f64 {
    // dex = log10(zl_value)
    let dex = zl_value_w_m2_sr_um.log10();
    let fext_m = if dex <= 2.255 {
        1.309 * dex - 2.598
    } else {
        0.468 * dex - 0.702
    };
    let fext_r = if dex <= 2.244 {
        1.407 * dex - 2.692
    } else {
        0.527 * dex - 0.715
    };

    let lam_um = lambda_nm * 1e-3;
    let kaer = if lam_um < 0.4 {
        0.05
    } else {
        0.013 * lam_um.powf(-1.38)
    };
    let tau0 = (10f64).powf(-0.4 * kaer).ln();
    let am = airmass(
        Radians::new(zenith_deg.to_radians()),
        AirmassFormula::Young1994,
    );
    let tau_eff = tau0 * (fext_r + fext_m) * am;
    (-tau_eff).exp()
}

/// Compute the zodiacal-light contribution.
pub fn compute(
    inp: &ZlInputs,
    solar_spectrum: &SampledSpectrum<Nanometer, Meter, f64>,
) -> Result<ZlOutputs> {
    let zl_500_s10 = leinert_lookup_s10(inp.beta_rad, inp.delta_lambda_rad)?;
    let zl_500_wmsrum = zl_500_s10.value() * S10_TO_W_M2_SR_UM;

    // Scale the solar spectrum so its 500 nm value matches zl_500.
    let f_sun_500 = solar_spectrum
        .interp_at(Nanometers::new(500.0))
        .expect("solar interp at 500 nm")
        .value(); // W m⁻² nm⁻¹
                  // Convert solar W/m²/nm → W/m²/sr/nm by dividing by π·sr (Lambertian),
                  // matching the Python `f_sun_sr = f_sun / pi` convention.
    let f_sun_500_sr = f_sun_500 / std::f64::consts::PI;
    if f_sun_500_sr <= 0.0 {
        return Err(NsbError::DataParse {
            file: "solar_spectrum.dat",
            message: "non-positive flux at 500 nm".into(),
        });
    }
    // ZL value at 500 nm: convert W/m²/sr/μm → W/m²/sr/nm by dividing by 1000.
    let target_500 = zl_500_wmsrum / 1000.0; // W m⁻² sr⁻¹ nm⁻¹
    let k = target_500 / f_sun_500_sr;

    // Build per-wavelength ZL spectrum on the solar grid, restricted to [WL_LOW, WL_HIGH].
    let mut lam = Vec::new();
    let mut zl_ph = Vec::new();
    // Track ZL value (W m⁻² sr⁻¹ μm⁻¹) at the B and V filter wavelengths
    // after reddening and extinction, for S10 conversion.
    let (mut b_zl_um, mut v_zl_um) = (0.0_f64, 0.0_f64);
    let (mut b_dist, mut v_dist) = (f64::INFINITY, f64::INFINITY);
    let solar_xs = solar_spectrum.xs_raw();
    let solar_ys = solar_spectrum.ys_raw();
    for i in 0..solar_xs.len() {
        let l = solar_xs[i];
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&l) {
            continue;
        }
        let f_sun_sr = solar_ys[i] / std::f64::consts::PI;
        let zl = f_sun_sr * k * reddening_factor(inp.beta_rad, inp.delta_lambda_rad, l);
        // ZL value in W m⁻² sr⁻¹ nm⁻¹ at this wavelength → as proxy magnitude
        // for the extinction-input we use the value normalized to W/m²/sr/μm:
        let zl_w_m2_sr_um = zl * 1000.0;
        let trans = extinction_transmission(zl_w_m2_sr_um, l, inp.zenith_deg);
        let zl_ext = zl * trans; // W m⁻² sr⁻¹ nm⁻¹
        let zl_ext_um = zl_ext * 1000.0; // W m⁻² sr⁻¹ μm⁻¹

        if (l - B_FILTER_NM).abs() < b_dist {
            b_dist = (l - B_FILTER_NM).abs();
            b_zl_um = zl_ext_um;
        }
        if (l - V_FILTER_NM).abs() < v_dist {
            v_dist = (l - V_FILTER_NM).abs();
            v_zl_um = zl_ext_um;
        }

        // Convert energy → photons.
        // 1 W/m²/sr/nm = 1e7 erg/s · 1e-4/cm² · 0.1/Å  =  100 erg/(s·cm²·sr·Å)
        let lam_a = l * 10.0;
        let zl_erg_cgs = zl_ext * 100.0; // erg s⁻¹ cm⁻² sr⁻¹ Å⁻¹
        let zl_ph_per_a = zl_erg_cgs * 5.03e7 * lam_a; // ph s⁻¹ cm⁻² sr⁻¹ Å⁻¹
        let zl_ph_per_nm = zl_ph_per_a * 10.0; // ph s⁻¹ cm⁻² sr⁻¹ nm⁻¹
        let zl_ph_per_ns_per_nm = zl_ph_per_nm * 1e-9; // ph ns⁻¹ cm⁻² sr⁻¹ nm⁻¹
        lam.push(l);
        zl_ph.push(zl_ph_per_ns_per_nm);
    }

    let spectrum = SampledSpectrum::<Nanometer, Meter, f64>::from_raw(
        lam,
        zl_ph,
        Interpolation::Linear,
        SpectrumOutOfRange::ClampToEndpoints,
        Some(SpectrumProvenance::computed("zodiacal")),
    )
    .expect("zodiacal spectrum invariants");
    let integrated = BandPhotonRadiance::new(algo::trapz_range(
        &spectrum.xs_raw(),
        &spectrum.ys_raw(),
        WL_LOW_NM,
        WL_HIGH_NM,
    ));

    // B/V S10 fluxes — Python takes zl_ext (W/m²/sr/μm) at the B and V
    // filter centres and divides by `S10_TO_W_M2_SR_UM` (1.28e-8).
    let b_flux = S10::new(b_zl_um / S10_TO_W_M2_SR_UM);
    let v_flux = S10::new(v_zl_um / S10_TO_W_M2_SR_UM);

    Ok(ZlOutputs {
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
        spectrum,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy hand-rolled implementation preserved verbatim for parity testing.
    fn legacy_leinert_lookup_s10_for_test(beta_rad: f64, delta_lambda_rad: f64) -> Option<f64> {
        let beta_deg = beta_rad.to_degrees().abs();
        let dl_deg = delta_lambda_rad.to_degrees().abs().min(180.0);
        if !(0.0..90.0).contains(&beta_deg) {
            return None;
        }
        if !(0.0..=180.0).contains(&dl_deg) {
            return None;
        }
        if dl_deg < 20.0 && beta_deg < 25.0 {
            return Some(CORNER_LL_LT_20_B_LT_25);
        }
        if dl_deg < 25.0 && beta_deg < 20.0 {
            return Some(CORNER_LL_LT_25_B_LT_20);
        }
        if dl_deg < 30.0 && beta_deg < 15.0 {
            return Some(CORNER_LL_LT_30_B_LT_15);
        }

        let b0 = (beta_deg / 5.0).floor() as usize;
        let b1 = (b0 + 1).min(18);
        let bt = (beta_deg - 5.0 * b0 as f64) / 5.0;

        let l0_idx = ((180.0 - dl_deg.ceil()) / 5.0).floor() as isize;
        let l0 = l0_idx.max(0).min(35) as usize;
        let l1 = (l0 + 1).min(36);
        let lt = (180.0 - dl_deg - 5.0 * l0 as f64) / 5.0;

        Some(siderust::tables::algo::bilinear_unit(
            LEINERT_S10[l0][b0],
            LEINERT_S10[l0][b1],
            LEINERT_S10[l1][b0],
            LEINERT_S10[l1][b1],
            bt,
            lt,
        ))
    }

    /// Bit-for-bit parity between the new Grid2D implementation and the
    /// legacy hand-rolled one. Tested over a dense sweep of (dl, β) inputs,
    /// skipping corner-region inputs (the clamp branches return constants and
    /// trivially match).
    ///
    /// The ascending y-axis parameterisation (`dl_asc = 180 − dl_deg`) ensures
    /// the internal `ty` arithmetic path is identical to the legacy
    /// `lt = (180 − dl_deg − 5·l0)/5`, giving bit-for-bit equality at all
    /// query points including exact grid points.
    #[test]
    fn leinert_grid2d_bitwise_parity_with_legacy() {
        let dl_degs = [0.5_f64, 1.0, 5.0, 10.0, 27.3, 90.0, 124.5, 175.0, 179.9];
        let beta_degs = [0.5_f64, 5.0, 10.0, 27.3, 45.0, 60.0, 89.5, 89.99];

        for &dl in &dl_degs {
            for &beta in &beta_degs {
                let dl_rad = dl.to_radians();
                let beta_rad = beta.to_radians();

                let legacy = match legacy_leinert_lookup_s10_for_test(beta_rad, dl_rad) {
                    Some(v) => v,
                    None => continue, // out-of-range – skip
                };
                let got = leinert_lookup_s10(beta_rad, dl_rad)
                    .expect("leinert_lookup_s10 failed")
                    .value();

                assert_eq!(
                    got.to_bits(),
                    legacy.to_bits(),
                    "bit mismatch at dl={dl}°, β={beta}°: Grid2D={got}, legacy={legacy}"
                );
            }
        }
    }
}
