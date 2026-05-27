//! Scattered moonlight component — Krisciunas & Schaefer (1991) model.
//!
//! Implements the analytic single-scattering V-band brightness model from
//! Krisciunas, K. & Schaefer, B. E. 1991, PASP, 103, 1033,
//! "A model of the brightness of moonlight":
//!
//! ```text
//! I*(α) = 10^(-0.4 (3.84 + 0.026 |α| + 4e-9 α^4))                (eq. 8 / 9)
//! f(ρ)  = 10^5.36 · (1.06 + cos²ρ) + 10^(6.15 − ρ/40)             (eq. 16/17)
//! X(Z)  = (1 − 0.96 sin²Z)^(-1/2)                                 (eq. 3)
//! B_moon = f(ρ) · I*(α) · 10^(-0.4 k X(Z_m)) · (1 − 10^(-0.4 k X(Z)))   (nL)
//! V_sky = (20.7233 − ln(B_moon / 34.08)) / 0.92104                (eq. 1)
//! ```
//!
//! The model is V-band only; the B-band scattered-moonlight S10 is
//! approximated by the same surface brightness in S10 units (the Moon is
//! roughly solar in colour, and the ±0.5 mag intrinsic scatter of the K&S
//! model dominates any B−V refinement we could add at this stage).
//!
//! `integrated` (band photon radiance) is derived from the V S10 by
//! treating the moonlight spectrum as flat across the NSB integration
//! band (300–650 nm) at the V-band spectral density. This is a coarse but
//! self-consistent proxy that lets the orchestrator's threshold search and
//! totals pick up the moonlight contribution without a wavelength-resolved
//! port.
//!
//! The module also exposes a Jones et al. (2013)-style spectral path using the
//! bundled Mie phase and multiple-scattering correction grids. The K&S path is
//! retained for explicit Python-parity configuration.
//!
//! Scientific role:
//! moonlight can dominate the optical sky background when the Moon is above
//! the horizon. The effect depends on lunar phase, Moon-target separation, and
//! how much atmosphere the Moon and target rays pass through.
//!
//! Contribution to the science:
//! this file adds a physically motivated scattered-moonlight term to the NSB
//! model. Although it is still simplified relative to a full wavelength-
//! resolved scattering pipeline, it captures the main observing-system impact
//! of moon phase and geometry on sky brightness.

use std::sync::OnceLock;

use crate::error::Result;
use crate::single_scatter::ScatterGrid;
use crate::spectra::SampledSpectrum;
use crate::NSB_S10_ZP;
use qtty::angular::{Degree, Degrees, Radian, Radians};
use qtty::radiometry::{
    self, spectral_radiance_to_photon_radiance_ns_nm, WattsPerSquareMeterSteradianNanometer,
};
use siderust::atmosphere::{
    airmass, mie_optical_depth, rayleigh_optical_depth_bodhaine99, rayleigh_phase,
    AtmosphereProfile, KrisciunasSchaefer1991, MieParams, DEFAULT_SCALE_HEIGHT,
};
use siderust::qtty::{Kilometers, Nanometer, Nanometers};
use siderust::{reflected_lunar_spectral_radiance_jones2013, MoonPhaseGeometry};

/// Default V-band atmospheric extinction coefficient (mag/airmass) used by
/// K&S 1991 in their published curves.
pub const DEFAULT_K_EXT: f64 = 0.172;

/// Conversion factor from a V-band S10 surface brightness to an estimated
/// band-integrated photon radiance over [300, 650] nm. Derived assuming a
/// flat-in-wavelength spectral density at the V filter (see module docs):
///
///   1 S10 ≈ 1.28e-8 W m⁻² sr⁻¹ μm⁻¹ → 1.28e-11 W m⁻² sr⁻¹ nm⁻¹
///   ÷ E_ph(551 nm) (= 3.6066e-19 J)  → 3.549e7 ph s⁻¹ m⁻² sr⁻¹ nm⁻¹
///   × 1e-4 (m² → cm²) × 1e-9 (s → ns) → 3.549e-6 ph cm⁻² ns⁻¹ sr⁻¹ nm⁻¹
///   × 350 nm bandwidth                → 1.242e-3 ph cm⁻² ns⁻¹ sr⁻¹
const S10_V_TO_INTEGRATED_PH: f64 = 1.242e-3;
const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;
const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;
const S10_TO_W_M2_SR_UM: f64 = 1.28e-8;
const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;
const JONES_MIE_WEIGHT: f64 = 0.05;

#[derive(Debug, Clone, Copy)]
pub struct MoonInputs {
    /// Moon-source angular separation.
    pub separation: Degrees,
    /// Moon zenith distance.
    pub moon_zenith: Degrees,
    /// Geocentric (or topocentric) lunar phase geometry from siderust.
    pub phase: MoonPhaseGeometry,
    /// Source zenith distance.
    pub source_zenith: Degrees,
}

#[derive(Debug, Clone)]
pub struct MoonOutputs {
    pub integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian,
    pub b_flux_s10: radiometry::S10s,
    pub v_flux_s10: radiometry::S10s,
}

/// Compute the K&S 1991 scattered moonlight contribution.
///
/// Returns zero contribution when the Moon is below the horizon
/// (`moon_zenith ≥ 90°`), when the source is below the horizon
/// (`source_zenith ≥ 90°`), or when the Moon-source angular separation
/// is non-positive.
pub fn compute(inp: &MoonInputs) -> Result<MoonOutputs> {
    compute_with_extinction(inp, DEFAULT_K_EXT)
}

/// Variant of [`compute`] that lets the caller override the V-band
/// extinction coefficient `k` (mag/airmass).
pub fn compute_with_extinction(inp: &MoonInputs, k_ext: f64) -> Result<MoonOutputs> {
    if !inp.moon_zenith.is_finite()
        || !inp.source_zenith.is_finite()
        || !inp.separation.is_finite()
        || !k_ext.is_finite()
    {
        return Ok(zero_outputs());
    }
    if inp.moon_zenith >= Degrees::new(90.0)
        || inp.source_zenith >= Degrees::new(90.0)
        || inp.separation <= Degrees::new(0.0)
    {
        return Ok(zero_outputs());
    }

    let b_nl = scattered_brightness_nanolamberts(
        inp.phase.phase_angle,
        inp.separation,
        inp.moon_zenith,
        inp.source_zenith,
        k_ext,
    );

    if !b_nl.is_finite() || b_nl <= 0.0 {
        return Ok(zero_outputs());
    }

    let v_mag_arcsec2 = v_mag_per_arcsec2_from_nl(b_nl);
    let v_s10 = 10f64.powf(0.4 * (NSB_S10_ZP - v_mag_arcsec2));
    let integrated = v_s10 * S10_V_TO_INTEGRATED_PH;

    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(integrated),
        b_flux_s10: radiometry::S10s::new(v_s10),
        v_flux_s10: radiometry::S10s::new(v_s10),
    })
}

/// Scattered-moon surface brightness at the source location, in nanolamberts
/// (eq. 15 of K&S 1991).
fn scattered_brightness_nanolamberts(
    alpha: Radians,
    rho: Degrees,
    z_moon: Degrees,
    z_src: Degrees,
    k_ext: f64,
) -> f64 {
    let i_star = lunar_illuminance_outside_atmosphere(alpha);
    let f_rho = scattering_function(rho);
    let am_moon = airmass::<KrisciunasSchaefer1991>(z_moon.to::<Radian>());
    let am_src = airmass::<KrisciunasSchaefer1991>(z_src.to::<Radian>());
    let trans_moon = 10f64.powf(-0.4 * k_ext * am_moon.value());
    let absorb_path = 1.0 - 10f64.powf(-0.4 * k_ext * am_src.value());
    f_rho * i_star * trans_moon * absorb_path
}

/// `I*(α)` — lunar illuminance above the atmosphere (relative units, eq. 8).
fn lunar_illuminance_outside_atmosphere(alpha: Radians) -> f64 {
    let a = alpha.abs().to::<Degree>().value();
    let exponent = -0.4 * (3.84 + 0.026 * a + 4.0e-9 * a.powi(4));
    10f64.powf(exponent)
}

/// `f(ρ)` — angular scattering function of K&S 1991 (eq. 16/17), summing
/// the Rayleigh + aerosol forward-scattering term and the Mie aureole term.
fn scattering_function(rho: Degrees) -> f64 {
    let cos_rho = rho.cos();
    let rayleigh = 10f64.powf(5.36) * (1.06 + cos_rho * cos_rho);
    let aureole = 10f64.powf(6.15 - rho.value() / 40.0);
    rayleigh + aureole
}

/// Convert moonlight brightness `B` (nanolamberts) into V-band surface
/// brightness (mag/arcsec²) via the inverse of K&S eq. 1.
fn v_mag_per_arcsec2_from_nl(b_nl: f64) -> f64 {
    (20.7233 - (b_nl / 34.08).ln()) / 0.92104
}

fn zero_outputs() -> MoonOutputs {
    MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::zero(),
        b_flux_s10: radiometry::S10s::zero(),
        v_flux_s10: radiometry::S10s::zero(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::angular::Radians;
    use qtty::photometry::s10_to_surface_brightness;
    use qtty::radiometry::S10s;
    use siderust::qtty::IlluminationFractions;

    fn make_phase(alpha_deg: f64) -> MoonPhaseGeometry {
        MoonPhaseGeometry {
            phase_angle: Radians::new(alpha_deg.to_radians()),
            illuminated_fraction: IlluminationFractions::new(
                0.5 * (1.0 + alpha_deg.to_radians().cos()),
            ),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn inputs(alpha_deg: f64, rho_deg: f64, z_moon: f64, z_src: f64) -> MoonInputs {
        MoonInputs {
            separation: Degrees::new(rho_deg),
            moon_zenith: Degrees::new(z_moon),
            phase: make_phase(alpha_deg),
            source_zenith: Degrees::new(z_src),
        }
    }

    fn v_mag_arcsec2(out: &MoonOutputs) -> f64 {
        s10_to_surface_brightness(out.v_flux_s10, NSB_S10_ZP).value()
    }

    #[test]
    fn full_moon_reference_geometry_matches_published_brightness() {
        // K&S 1991 Table 1 / Fig. 4: with k=0.172, α=0°, ρ=90°, Z=Z_m=45°
        // the analytic model lands near V ≈ 18 mag/arcsec² (the model itself
        // has comparable scatter, so we allow ±0.7 mag).
        let out = compute(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out.v_flux_s10 > S10s::zero());
        let v_mag = v_mag_arcsec2(&out);
        assert!(
            (v_mag - 18.0).abs() < 0.7,
            "V_sky = {v_mag:.2} mag/arcsec² not within 0.7 of published ~18"
        );
    }

    #[test]
    fn new_moon_contribution_is_negligible_relative_to_full() {
        // α = 180° → I*(α) is suppressed by both the linear and α^4 terms
        // and the scattered brightness is many orders of magnitude smaller
        // than at full moon.
        let out_new = compute(&inputs(180.0, 90.0, 45.0, 45.0)).unwrap();
        let out_full = compute(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out_full.v_flux_s10 > S10s::zero());
        assert!(
            out_new.v_flux_s10 < out_full.v_flux_s10 * 1e-3,
            "new-moon V S10 ({}) should be << full-moon ({})",
            out_new.v_flux_s10.value(),
            out_full.v_flux_s10.value()
        );
    }

    #[test]
    fn scattering_function_exhibits_expected_behavior() {
        // The K&S 1991 scattering function f(ρ) = 10^5.36 · (1.06 + cos²ρ)
        // + 10^(6.15 − ρ/40) is NOT monotonic. The Rayleigh term (1.06 + cos²ρ)
        // is symmetric around ρ=90° (both ρ=0° and ρ=180° have cos²ρ=1),
        // so brightness peaks near forward scattering, has a minimum near 90°,
        // and increases again in backscattering directions.
        //
        // This test verifies the model computes positive brightness at various
        // separations and that extremal points match expectations.
        let inputs_at_rho = |rho| compute(&inputs(45.0, rho, 30.0, 60.0)).unwrap();

        let b5 = inputs_at_rho(5.0).v_flux_s10;
        let b90 = inputs_at_rho(90.0).v_flux_s10;
        let b120 = inputs_at_rho(120.0).v_flux_s10;
        let b175 = inputs_at_rho(175.0).v_flux_s10;

        // Forward scattering (small ρ) is strong
        assert!(b5 > S10s::zero(), "brightness at ρ=5° must be positive");

        // Brightness at 90° (perpendicular) is the minimum
        assert!(b90 > S10s::zero(), "brightness at ρ=90° must be positive");
        assert!(
            b90 < b5,
            "brightness at ρ=90° should be less than forward scattering"
        );

        // Backscattering increases again due to Rayleigh symmetry
        assert!(
            b120 > b90,
            "brightness should increase from ρ=90° to ρ=120°"
        );
        assert!(
            b175 > b120,
            "brightness should increase toward ρ=180° (backscattering)"
        );

        // But even backscattering should be less than forward scattering
        assert!(
            b175 < b5,
            "backscattering should be weaker than forward scattering"
        );
    }

    #[test]
    fn moon_below_horizon_returns_zero() {
        let out = compute(&inputs(0.0, 30.0, 95.0, 30.0)).unwrap();
        assert_eq!(out.v_flux_s10, S10s::zero());
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn airmass_at_zenith_is_unity() {
        let am = airmass::<KrisciunasSchaefer1991>(Degrees::new(0.0).to::<Radian>());
        assert!((am.value() - 1.0).abs() < 1e-12, "X(0) = {:?}", am);
    }
}

/// Compute the Jones et al. (2013) scattered moonlight contribution.
///
/// # Reference
///
/// Jones, A., Staig, A., Noll, S., & Kausch, W. 2013, "An advanced scattered
/// moonlight model for Cerro Paranal", Astronomy & Astrophysics, 560, A91.
/// DOI: 10.1051/0004-6361/201322433 arXiv:1307.1407v1
///
/// # Overview
///
/// The Jones et al. (2013) model improves upon Krisciunas & Schaefer (1991) by:
///
/// 1. **Better handling of solar contamination**: Separates direct/diffuse
///    solar irradiance affecting the scattered moonlight more cleanly.
/// 2. **Improved phase function**: Uses a more physics-based scattering function
///    that better accounts for Rayleigh/Mie transitions in different scattering
///    angles, particularly at large zenith distances and deep twilight.
/// 3. **Aerosol single-scattering albedo**: Incorporates wavelength-dependent
///    aerosol properties rather than a fixed model.
/// 4. **Enhanced edge cases**: Better behavior when the Moon is near the
///    horizon, during twilight, or at large separation angles.
///
/// This implementation performs wavelength-resolved integration over the NSB
/// band, using the bundled Mie phase and multiple-scattering correction grids.
/// The one-argument variant uses a smooth reference solar spectrum and mean
/// lunar distance; [`compute_jones2013_spectral`] lets the evaluator provide
/// the bundled solar spectrum and topocentric Moon distance.
///
/// # Arguments
///
/// - `inp`: Moon geometry, phase angle, zenith distances, and separation angle.
/// - `k_ext`: V-band extinction coefficient (mag/airmass). Use `DEFAULT_K_EXT`
///   for consistency with published K&S curves.
///
/// # Returns
///
/// V and B surface brightnesses (S10 units) and band-integrated photon
/// radiance, or zero values on invalid input.
///
/// # Physical Parameters Used
///
/// - **Rayleigh optical depth**: Modeled as wavelength/altitude dependent;
///   at sea level ≈ 0.13–0.15 optically thin.
/// - **Aerosol component**: Single-scattering albedo nominally ≈ 0.95–0.98
///   depending on wavelength and aerosol type.
/// - **Moon illuminance**: Phase-angle-dependent solar flux reflected by
///   the lunar surface (albedo ≈ 0.12).
/// - **Mie aureole**: Dominates near the Moon; more pronounced than in K&S.
///
/// # Why Prefer Jones et al. over K&S
///
/// Jones et al. (2013) is **recommended** when:
/// - Observing during deep twilight (Sun 6–18° below horizon).
/// - Moon altitude is very high (Zm < 20°) or very low (Zm > 70°).
/// - Studying faint sources where the ±0.5 mag K&S scatter matters.
///
/// K&S (1991) remains adequate when:
/// - Quick estimates are needed and the ±0.5 mag model uncertainty is acceptable.
/// - Observing in full darkness (Sun > 18° below horizon) with bright moonlight.
///
pub fn compute_jones2013(inp: &MoonInputs) -> Result<MoonOutputs> {
    compute_jones2013_with_extinction(inp, DEFAULT_K_EXT)
}

/// Variant of `compute_jones2013` allowing override of extinction coefficient.
///
/// See [`compute_jones2013`] for full documentation.
pub fn compute_jones2013_with_extinction(inp: &MoonInputs, k_ext: f64) -> Result<MoonOutputs> {
    let samples = reference_solar_samples();
    compute_jones2013_from_samples(
        inp,
        &samples,
        siderust::event::lunar::photometry::MEAN_MOON_DISTANCE,
        k_ext,
    )
}

/// Spectral Jones et al. moonlight model using the caller's solar spectrum and
/// actual topocentric/geocentric Moon distance.
pub fn compute_jones2013_spectral(
    inp: &MoonInputs,
    solar_spectrum: &SampledSpectrum<Nanometer, siderust::qtty::length::Meter>,
    moon_distance: Kilometers,
) -> Result<MoonOutputs> {
    let samples: Vec<(f64, f64)> = solar_spectrum
        .xs_raw()
        .iter()
        .copied()
        .zip(solar_spectrum.ys_raw().iter().copied())
        .collect();
    compute_jones2013_from_samples(inp, &samples, moon_distance, DEFAULT_K_EXT)
}

fn compute_jones2013_from_samples(
    inp: &MoonInputs,
    solar_samples: &[(f64, f64)],
    moon_distance: Kilometers,
    k_ext: f64,
) -> Result<MoonOutputs> {
    if !inp.moon_zenith.is_finite()
        || !inp.source_zenith.is_finite()
        || !inp.separation.is_finite()
        || !k_ext.is_finite()
        || !moon_distance.is_finite()
    {
        return Ok(zero_outputs());
    }
    if inp.moon_zenith >= Degrees::new(90.0)
        || inp.source_zenith >= Degrees::new(90.0)
        || inp.separation <= Degrees::new(0.0)
        || moon_distance <= Kilometers::new(0.0)
    {
        return Ok(zero_outputs());
    }

    let mie = mie_grid();
    let correction = correction_grid();
    let am_moon = airmass::<KrisciunasSchaefer1991>(inp.moon_zenith.to::<Radian>());
    let am_src = airmass::<KrisciunasSchaefer1991>(inp.source_zenith.to::<Radian>());
    let tau_scale = k_ext / DEFAULT_K_EXT;

    let mut lam = Vec::new();
    let mut density = Vec::new();
    for &(lambda_nm, solar_irradiance) in solar_samples {
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&lambda_nm) {
            continue;
        }
        let wavelength = Nanometers::new(lambda_nm);
        let lunar_radiance = reflected_lunar_spectral_radiance_jones2013(
            solar_irradiance,
            wavelength,
            inp.phase.phase_angle,
            moon_distance,
        );
        if !lunar_radiance.value().is_finite() || lunar_radiance.value() <= 0.0 {
            continue;
        }
        let lunar_ph = spectral_radiance_to_photon_radiance_ns_nm(
            WattsPerSquareMeterSteradianNanometer::new(lunar_radiance.value()),
            wavelength,
        )
        .value();
        let tau_r = rayleigh_optical_depth_bodhaine99(
            wavelength,
            AtmosphereProfile::EL_PARANAL.surface_pressure,
            AtmosphereProfile::EL_PARANAL.observer_altitude,
            DEFAULT_SCALE_HEIGHT,
        )
        .value()
            * tau_scale;
        let tau_m = mie_optical_depth(&MieParams::PARANAL, wavelength).value() * tau_scale;
        let phase_r = rayleigh_phase(inp.separation.to::<Radian>()).value();
        let phase_m = mie.lookup(inp.separation, wavelength);
        let multi = correction.lookup(inp.separation, wavelength);
        let am_moon_v = am_moon.value();
        let am_src_v = am_src.value();
        let scatter = (tau_r * phase_r + tau_m * JONES_MIE_WEIGHT * phase_m).max(0.0);
        let transmission = (-(tau_r + tau_m) * 0.5 * (am_moon_v + am_src_v)).exp();
        let source_path = 1.0 - (-(tau_r + tau_m) * am_src_v).exp();
        let value = lunar_ph * scatter * transmission * source_path.max(0.0) * multi;
        if value.is_finite() && value > 0.0 {
            lam.push(lambda_nm);
            density.push(value);
        }
    }

    if lam.len() < 2 {
        return Ok(zero_outputs());
    }

    let integrated = integrate_trapz(&lam, &density);
    let b_density = interp_linear(&lam, &density, B_FILTER_NM);
    let v_density = interp_linear(&lam, &density, V_FILTER_NM);

    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(integrated),
        b_flux_s10: spectral_photon_density_to_s10(b_density, Nanometers::new(B_FILTER_NM)),
        v_flux_s10: spectral_photon_density_to_s10(v_density, Nanometers::new(V_FILTER_NM)),
    })
}

fn mie_grid() -> &'static ScatterGrid {
    static GRID: OnceLock<ScatterGrid> = OnceLock::new();
    GRID.get_or_init(|| ScatterGrid::mie_phase().expect("bundled Mie phase grid"))
}

fn correction_grid() -> &'static ScatterGrid {
    static GRID: OnceLock<ScatterGrid> = OnceLock::new();
    GRID.get_or_init(|| {
        ScatterGrid::multiple_scattering_correction().expect("bundled scattering correction grid")
    })
}

fn reference_solar_samples() -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut lambda = WL_LOW_NM;
    while lambda <= WL_HIGH_NM {
        let x = (lambda - 550.0) / 180.0;
        let irradiance = 1.88 * (-0.5 * x * x).exp() + 0.25;
        out.push((lambda, irradiance));
        lambda += 10.0;
    }
    out
}

fn integrate_trapz(xs: &[f64], ys: &[f64]) -> f64 {
    xs.windows(2)
        .zip(ys.windows(2))
        .map(|(xw, yw)| 0.5 * (yw[0] + yw[1]) * (xw[1] - xw[0]))
        .sum()
}

fn interp_linear(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let i = xs.partition_point(|&xi| xi <= x);
    let x0 = xs[i - 1];
    let x1 = xs[i];
    let t = (x - x0) / (x1 - x0);
    ys[i - 1] + t * (ys[i] - ys[i - 1])
}

fn spectral_photon_density_to_s10(density: f64, wavelength: Nanometers) -> radiometry::S10s {
    let lambda_m = wavelength.value() * 1.0e-9;
    let photon_energy = HC_JOULE_METER / lambda_m;
    let w_m2_sr_nm = density * 1.0e13 * photon_energy;
    let w_m2_sr_um = w_m2_sr_nm * 1.0e3;
    radiometry::S10s::new(w_m2_sr_um / S10_TO_W_M2_SR_UM)
}

#[cfg(test)]
mod jones_tests {
    use super::*;
    use qtty::angular::Radians;
    use qtty::photometry::s10_to_surface_brightness;
    use qtty::radiometry::S10s;
    use siderust::qtty::IlluminationFractions;

    const LUT_MOON_PHASE_0454: &str =
        include_str!("../../data/lut_moon/Phase_0.454_waxing_moon_LUT.csv");

    fn make_phase(alpha_deg: f64) -> MoonPhaseGeometry {
        MoonPhaseGeometry {
            phase_angle: Radians::new(alpha_deg.to_radians()),
            illuminated_fraction: IlluminationFractions::new(
                0.5 * (1.0 + alpha_deg.to_radians().cos()),
            ),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn phase_from_illumination_fraction(fraction: f64) -> MoonPhaseGeometry {
        let phase_angle = (2.0 * fraction - 1.0).clamp(-1.0, 1.0).acos();
        MoonPhaseGeometry {
            phase_angle: Radians::new(phase_angle),
            illuminated_fraction: IlluminationFractions::new(fraction),
            elongation: Radians::new(0.0),
            waxing: true,
        }
    }

    fn horizontal_separation_deg(
        moon_alt_deg: f64,
        moon_az_deg: f64,
        source_alt_deg: f64,
        source_az_deg: f64,
    ) -> f64 {
        let moon_alt = moon_alt_deg.to_radians();
        let source_alt = source_alt_deg.to_radians();
        let delta_az = (source_az_deg - moon_az_deg).to_radians();
        let cos_sep =
            moon_alt.sin() * source_alt.sin() + moon_alt.cos() * source_alt.cos() * delta_az.cos();
        cos_sep.clamp(-1.0, 1.0).acos().to_degrees()
    }

    fn lut_inputs(line: &str) -> (MoonInputs, f64) {
        let values: Vec<f64> = line
            .split(',')
            .map(|field| field.trim().parse::<f64>().expect("numeric LUT field"))
            .collect();
        assert_eq!(values.len(), 6);
        let moon_az = values[0];
        let moon_alt = values[1];
        let moon_phase = values[2];
        let source_alt = values[3];
        let source_az = values[4];
        let expected = values[5];
        let separation = horizontal_separation_deg(moon_alt, moon_az, source_alt, source_az);
        (
            MoonInputs {
                separation: Degrees::new(separation),
                moon_zenith: Degrees::new(90.0 - moon_alt),
                phase: phase_from_illumination_fraction(moon_phase),
                source_zenith: Degrees::new(90.0 - source_alt),
            },
            expected,
        )
    }

    fn inputs(alpha_deg: f64, rho_deg: f64, z_moon: f64, z_src: f64) -> MoonInputs {
        MoonInputs {
            separation: Degrees::new(rho_deg),
            moon_zenith: Degrees::new(z_moon),
            phase: make_phase(alpha_deg),
            source_zenith: Degrees::new(z_src),
        }
    }

    fn v_mag_arcsec2(out: &MoonOutputs) -> f64 {
        s10_to_surface_brightness(out.v_flux_s10, NSB_S10_ZP).value()
    }

    #[test]
    fn jones2013_full_moon_high_altitude() {
        // Full moon (α=0°) at high Moon altitude (Zm=20°), separation 90°.
        let inp = inputs(0.0, 90.0, 20.0, 45.0);

        let out_jones = compute_jones2013(&inp).unwrap();

        assert!(
            out_jones.v_flux_s10 > S10s::zero(),
            "Jones full moon should have positive brightness"
        );
        assert!(out_jones.integrated.value().is_finite());

        let v_jones = v_mag_arcsec2(&out_jones);
        assert!(
            v_jones.is_finite(),
            "Jones full moon V magnitude should be finite"
        );
    }

    #[test]
    fn jones2013_twilight_conditions() {
        // Twilight scenario: Moon at moderate altitude, source at zenith,
        // phase angle 45° (half-moon scenario).
        let inp = inputs(45.0, 60.0, 50.0, 30.0);

        let out = compute_jones2013(&inp).unwrap();
        assert!(
            out.v_flux_s10 > S10s::zero(),
            "Jones twilight should have positive brightness"
        );

        // Ensure the output is well-formed
        assert!(
            out.integrated.value() > 0.0,
            "Jones integrated radiance should be positive"
        );
        assert!(
            out.b_flux_s10 > S10s::zero(),
            "Jones B-band brightness should be positive"
        );
    }

    #[test]
    fn jones2013_new_moon_negligible() {
        // New moon (α=180°) should have negligible scattered brightness
        // compared to full moon.
        let inp_new = inputs(180.0, 90.0, 45.0, 45.0);
        let inp_full = inputs(0.0, 90.0, 45.0, 45.0);

        let out_new = compute_jones2013(&inp_new).unwrap();
        let out_full = compute_jones2013(&inp_full).unwrap();

        assert!(out_full.v_flux_s10 > S10s::zero());
        assert!(
            out_new.v_flux_s10 < out_full.v_flux_s10 * 1e-3,
            "Jones new moon should be << full moon; new={}, full={}",
            out_new.v_flux_s10.value(),
            out_full.v_flux_s10.value()
        );
    }

    #[test]
    fn jones2013_moon_below_horizon_returns_zero() {
        let out = compute_jones2013(&inputs(0.0, 30.0, 95.0, 30.0)).unwrap();
        assert_eq!(
            out.v_flux_s10,
            S10s::zero(),
            "Jones should return zero when Moon below horizon"
        );
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn jones2013_vs_ks_comparison() {
        // The spectral Jones path is intentionally no longer a K&S alias.
        let inp = inputs(60.0, 75.0, 40.0, 50.0);

        let out_ks = compute(&inp).unwrap();
        let out_jones = compute_jones2013(&inp).unwrap();

        let ratio = out_jones.v_flux_s10.value() / out_ks.v_flux_s10.value();
        assert!(ratio.is_finite() && ratio > 0.0);
        assert!(
            (ratio - 1.0).abs() > 1.0e-6,
            "Spectral Jones should not collapse to K&S parity; ratio={ratio:.6}"
        );
    }

    #[test]
    fn jones2013_lut_moon_fixture_same_scale() {
        // The bundled LUTs are inherited operational fixtures rather than
        // independent calibration data. Until the generator metadata is
        // recovered, keep this as a broad scale guard against unit or geometry
        // regressions rather than a tight scientific calibration test.
        let fixture_line = LUT_MOON_PHASE_0454
            .lines()
            .nth(1)
            .expect("first LUT data row");
        let (inp, expected) = lut_inputs(fixture_line);
        let out = compute_jones2013(&inp).unwrap();
        let ratio = out.integrated.value() / expected;
        assert!(ratio.is_finite() && ratio > 0.0);
        assert!(
            (0.05..=20.0).contains(&ratio),
            "Jones LUT scale ratio {ratio:.3} outside broad validation tolerance"
        );
    }
}
