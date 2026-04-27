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

use crate::data::leinert::{
    LEINERT_S10, S10_TO_W_M2_SR_UM,
    CORNER_LL_LT_20_B_LT_25, CORNER_LL_LT_25_B_LT_20, CORNER_LL_LT_30_B_LT_15,
};
use crate::error::{NsbError, Result};
use qtty::angular::Radians;
use siderust::atmosphere::{airmass, AirmassFormula};
use crate::spectra::{integrate, Spectrum};
use crate::units::{BandPhotonRadiance, S10};

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
    pub spectrum: Spectrum,
}

/// Bilinear interpolation in the Leinert table at `(β [rad], (λ-λ_sun) [rad])`.
/// Returns `S10` brightness at 500 nm. Mirrors the corner-clamp logic from
/// `NSB_Utils.py:GetZodiacalLight`.
pub fn leinert_lookup_s10(beta_rad: f64, delta_lambda_rad: f64) -> Result<S10> {
    let beta_deg = beta_rad.to_degrees().abs();
    let dl_deg = delta_lambda_rad.to_degrees().abs().min(180.0);
    if !(0.0..90.0).contains(&beta_deg) {
        return Err(NsbError::OutOfRange(format!("β={beta_deg}° not in [0,90)")));
    }
    if !(0.0..=180.0).contains(&dl_deg) {
        return Err(NsbError::OutOfRange(format!("Δλ={dl_deg}° not in [0,180]")));
    }
    if dl_deg < 20.0 && beta_deg < 25.0 { return Ok(S10::new(CORNER_LL_LT_20_B_LT_25)); }
    if dl_deg < 25.0 && beta_deg < 20.0 { return Ok(S10::new(CORNER_LL_LT_25_B_LT_20)); }
    if dl_deg < 30.0 && beta_deg < 15.0 { return Ok(S10::new(CORNER_LL_LT_30_B_LT_15)); }

    // β index along columns (0..18, step 5°).
    let b0 = (beta_deg / 5.0).floor() as usize;
    let b1 = (b0 + 1).min(18);
    let bt = (beta_deg - 5.0 * b0 as f64) / 5.0;

    // λ index along rows: row 0 = 180°, row 36 = 0°, step 5°.
    let l0_idx = ((180.0 - dl_deg.ceil()) / 5.0).floor() as isize;
    let l0 = l0_idx.max(0).min(35) as usize;
    let l1 = (l0 + 1).min(36);
    // Python expression: (180 - LambdaMinLambdaSun - 5*l0) / 5
    let lt = (180.0 - dl_deg - 5.0 * l0 as f64) / 5.0;

    let v = siderust::tables::algo::bilinear_unit(
        LEINERT_S10[l0][b0],
        LEINERT_S10[l0][b1],
        LEINERT_S10[l1][b0],
        LEINERT_S10[l1][b1],
        bt,
        lt,
    );
    Ok(S10::new(v))
}

/// Leinert reddening factor at a given wavelength and elongation.
/// Mirrors `GetZodicalReddening` in NSB_Utils.py.
fn reddening_factor(beta_rad: f64, delta_lambda_rad: f64, lambda_nm: f64) -> f64 {
    let elong_deg = (delta_lambda_rad.cos() * beta_rad.cos()).acos().to_degrees();
    let log_ratio = (lambda_nm / 500.0).ln();
    if elong_deg <= 30.0 {
        if (220.0..550.0).contains(&lambda_nm) { return 1.0 + 1.2 * log_ratio; }
        if (550.0..2500.0).contains(&lambda_nm) { return 1.0 + 0.8 * log_ratio; }
        return 1.0;
    }
    if elong_deg >= 90.0 {
        if (220.0..550.0).contains(&lambda_nm) { return 1.0 + 0.9 * log_ratio; }
        if (550.0..2500.0).contains(&lambda_nm) { return 1.0 + 0.6 * log_ratio; }
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
    let fext_m = if dex <= 2.255 { 1.309 * dex - 2.598 } else { 0.468 * dex - 0.702 };
    let fext_r = if dex <= 2.244 { 1.407 * dex - 2.692 } else { 0.527 * dex - 0.715 };

    let lam_um = lambda_nm * 1e-3;
    let kaer = if lam_um < 0.4 { 0.05 } else { 0.013 * lam_um.powf(-1.38) };
    let tau0 = (10f64).powf(-0.4 * kaer).ln();
    let am = airmass(Radians::new(zenith_deg.to_radians()), AirmassFormula::Young1994);
    let tau_eff = tau0 * (fext_r + fext_m) * am;
    (-tau_eff).exp()
}

/// Compute the zodiacal-light contribution.
pub fn compute(inp: &ZlInputs, solar_spectrum: &Spectrum) -> Result<ZlOutputs> {
    let zl_500_s10 = leinert_lookup_s10(inp.beta_rad, inp.delta_lambda_rad)?;
    let zl_500_wmsrum = zl_500_s10.value() * S10_TO_W_M2_SR_UM;

    // Scale the solar spectrum so its 500 nm value matches zl_500.
    let f_sun_500 = solar_spectrum.interp(500.0); // W m⁻² nm⁻¹
    // Convert solar W/m²/nm → W/m²/sr/nm by dividing by π·sr (Lambertian),
    // matching the Python `f_sun_sr = f_sun / pi` convention.
    let f_sun_500_sr = f_sun_500 / std::f64::consts::PI;
    if f_sun_500_sr <= 0.0 {
        return Err(NsbError::DataParse {
            file: "solar_spectrum.dat", message: "non-positive flux at 500 nm".into() });
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
    for i in 0..solar_spectrum.lambda_nm.len() {
        let l = solar_spectrum.lambda_nm[i];
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&l) { continue; }
        let f_sun_sr = solar_spectrum.flux[i] / std::f64::consts::PI;
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
        let zl_ph_per_nm = zl_ph_per_a * 10.0;          // ph s⁻¹ cm⁻² sr⁻¹ nm⁻¹
        let zl_ph_per_ns_per_nm = zl_ph_per_nm * 1e-9;  // ph ns⁻¹ cm⁻² sr⁻¹ nm⁻¹
        lam.push(l);
        zl_ph.push(zl_ph_per_ns_per_nm);
    }

    let spectrum = Spectrum::new(lam, zl_ph).with_tag("zodiacal");
    let integrated = BandPhotonRadiance::new(integrate::band_integral(&spectrum, WL_LOW_NM, WL_HIGH_NM));

    // B/V S10 fluxes — Python takes zl_ext (W/m²/sr/μm) at the B and V
    // filter centres and divides by `S10_TO_W_M2_SR_UM` (1.28e-8).
    let b_flux = S10::new(b_zl_um / S10_TO_W_M2_SR_UM);
    let v_flux = S10::new(v_zl_um / S10_TO_W_M2_SR_UM);

    Ok(ZlOutputs { integrated, b_flux_s10: b_flux, v_flux_s10: v_flux, spectrum })
}
