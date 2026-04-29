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
//! Future work (see `docs/NSB_STAGED_IMPLEMENTATION_PLAN.md` stages 9–11):
//! a full Mie / single-scatter port of the Python `CalculateMoon` pipeline
//! using the `mie_m15s1.dat` / `sscatcor_m15s1.dat` grids and the
//! `LUT_moon` lookup tables.

use crate::error::Result;
use qtty::angular::Degrees;
use qtty::radiometry;
use siderust::MoonPhaseGeometry;

/// Default V-band atmospheric extinction coefficient (mag/airmass) used by
/// K&S 1991 in their published curves.
pub const DEFAULT_K_EXT: f64 = 0.172;

/// V-band S10 zero-point used throughout NSB (matches
/// `evaluator::NsbResult::v_mag` and `band_flux_to_surface_brightness`).
const NSB_S10_ZP: f64 = 27.78;

/// Conversion factor from a V-band S10 surface brightness to an estimated
/// band-integrated photon radiance over [300, 650] nm. Derived assuming a
/// flat-in-wavelength spectral density at the V filter (see module docs):
///
///   1 S10 ≈ 1.28e-8 W m⁻² sr⁻¹ μm⁻¹ → 1.28e-11 W m⁻² sr⁻¹ nm⁻¹
///   ÷ E_ph(551 nm) (= 3.6066e-19 J)  → 3.549e7 ph s⁻¹ m⁻² sr⁻¹ nm⁻¹
///   × 1e-4 (m² → cm²) × 1e-9 (s → ns) → 3.549e-6 ph cm⁻² ns⁻¹ sr⁻¹ nm⁻¹
///   × 350 nm bandwidth                → 1.242e-3 ph cm⁻² ns⁻¹ sr⁻¹
const S10_V_TO_INTEGRATED_PH: f64 = 1.242e-3;

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
    let z_moon = inp.moon_zenith.value();
    let z_src = inp.source_zenith.value();
    let rho = inp.separation.value();

    if !z_moon.is_finite() || !z_src.is_finite() || !rho.is_finite() || !k_ext.is_finite() {
        return Ok(zero_outputs());
    }
    if z_moon >= 90.0 || z_src >= 90.0 || rho <= 0.0 {
        return Ok(zero_outputs());
    }

    let alpha_deg = inp.phase.phase_angle.to::<qtty::angular::Degree>().value();
    let b_nl = scattered_brightness_nanolamberts(alpha_deg, rho, z_moon, z_src, k_ext);

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
    alpha_deg: f64,
    rho_deg: f64,
    z_moon_deg: f64,
    z_src_deg: f64,
    k_ext: f64,
) -> f64 {
    let i_star = lunar_illuminance_outside_atmosphere(alpha_deg);
    let f_rho = scattering_function(rho_deg);
    let am_moon = airmass_ks(z_moon_deg);
    let am_src = airmass_ks(z_src_deg);
    let trans_moon = 10f64.powf(-0.4 * k_ext * am_moon);
    let absorb_path = 1.0 - 10f64.powf(-0.4 * k_ext * am_src);
    f_rho * i_star * trans_moon * absorb_path
}

/// `I*(α)` — lunar illuminance above the atmosphere (relative units, eq. 8).
fn lunar_illuminance_outside_atmosphere(alpha_deg: f64) -> f64 {
    let a = alpha_deg.abs();
    let exponent = -0.4 * (3.84 + 0.026 * a + 4.0e-9 * a.powi(4));
    10f64.powf(exponent)
}

/// `f(ρ)` — angular scattering function of K&S 1991 (eq. 16/17), summing
/// the Rayleigh + aerosol forward-scattering term and the Mie aureole term.
fn scattering_function(rho_deg: f64) -> f64 {
    let cos_rho = rho_deg.to_radians().cos();
    let rayleigh = 10f64.powf(5.36) * (1.06 + cos_rho * cos_rho);
    let aureole = 10f64.powf(6.15 - rho_deg / 40.0);
    rayleigh + aureole
}

/// K&S airmass approximation `X(Z) = (1 − 0.96 sin²Z)^(-1/2)`.
fn airmass_ks(z_deg: f64) -> f64 {
    let s = z_deg.to_radians().sin();
    (1.0 - 0.96 * s * s).max(f64::MIN_POSITIVE).powf(-0.5)
}

/// Convert moonlight brightness `B` (nanolamberts) into V-band surface
/// brightness (mag/arcsec²) via the inverse of K&S eq. 1.
fn v_mag_per_arcsec2_from_nl(b_nl: f64) -> f64 {
    (20.7233 - (b_nl / 34.08).ln()) / 0.92104
}

fn zero_outputs() -> MoonOutputs {
    MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(0.0),
        b_flux_s10: radiometry::S10s::new(0.0),
        v_flux_s10: radiometry::S10s::new(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::angular::Radians;

    fn make_phase(alpha_deg: f64) -> MoonPhaseGeometry {
        MoonPhaseGeometry {
            phase_angle: Radians::new(alpha_deg.to_radians()),
            illuminated_fraction: 0.5 * (1.0 + alpha_deg.to_radians().cos()),
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
        NSB_S10_ZP - 2.5 * out.v_flux_s10.value().log10()
    }

    #[test]
    fn full_moon_reference_geometry_matches_published_brightness() {
        // K&S 1991 Table 1 / Fig. 4: with k=0.172, α=0°, ρ=90°, Z=Z_m=45°
        // the analytic model lands near V ≈ 18 mag/arcsec² (the model itself
        // has comparable scatter, so we allow ±0.7 mag).
        let out = compute(&inputs(0.0, 90.0, 45.0, 45.0)).unwrap();
        assert!(out.v_flux_s10.value() > 0.0);
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
        assert!(out_full.v_flux_s10.value() > 0.0);
        assert!(
            out_new.v_flux_s10.value() < out_full.v_flux_s10.value() * 1e-3,
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
        
        let b5 = inputs_at_rho(5.0).v_flux_s10.value();
        let b90 = inputs_at_rho(90.0).v_flux_s10.value();
        let b120 = inputs_at_rho(120.0).v_flux_s10.value();
        let b175 = inputs_at_rho(175.0).v_flux_s10.value();
        
        // Forward scattering (small ρ) is strong
        assert!(b5 > 0.0, "brightness at ρ=5° must be positive");
        
        // Brightness at 90° (perpendicular) is the minimum
        assert!(b90 > 0.0, "brightness at ρ=90° must be positive");
        assert!(b90 < b5, "brightness at ρ=90° should be less than forward scattering");
        
        // Backscattering increases again due to Rayleigh symmetry
        assert!(b120 > b90, "brightness should increase from ρ=90° to ρ=120°");
        assert!(b175 > b120, "brightness should increase toward ρ=180° (backscattering)");
        
        // But even backscattering should be less than forward scattering
        assert!(b175 < b5, "backscattering should be weaker than forward scattering");
    }

    #[test]
    fn moon_below_horizon_returns_zero() {
        let out = compute(&inputs(0.0, 30.0, 95.0, 30.0)).unwrap();
        assert_eq!(out.v_flux_s10.value(), 0.0);
        assert_eq!(out.integrated.value(), 0.0);
    }

    #[test]
    fn airmass_at_zenith_is_unity() {
        let am = airmass_ks(0.0);
        assert!((am - 1.0).abs() < 1e-12, "X(0) = {am}");
    }
}
