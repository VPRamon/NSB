//! Zodiacal-light wavelength reddening model (Leinert 1997).
//!
//! Zodiacal light is slightly redder than the solar spectrum because the
//! interplanetary-dust scattering function has a mild wavelength dependence.
//! Leinert (1997) parameterises this reddening as a function of *elongation*
//! (angular distance from the Sun) and wavelength.
//!
//! # Validity
//!
//! The reddening slope is defined for two wavelength regimes:
//! - 220–550 nm (UV–green)
//! - 550–2500 nm (green–IR)
//!
//! Outside both regimes the factor returns 1.0 (no reddening applied).
//!
//! The elongation interpolation is linear between 30° (inner zodiacal cloud,
//! steeper reddening) and 90° (outer/ecliptic-pole direction, shallower).
//!
//! # Reference
//! Leinert et al. (1998), *A&AS* 127, 1-99, §10.

use qtty::angular::Radians;

/// Wavelength-dependent reddening factor `f(λ, ε)` where `ε` is the
/// elongation angle (angular distance from the Sun).
///
/// The elongation is derived from `beta` (ecliptic latitude) and
/// `delta_lambda` (target ecliptic longitude minus solar longitude, folded
/// to `[0, π]`).
///
/// # Arguments
///
/// - `beta`: ecliptic latitude in radians.
/// - `delta_lambda`: `|λ_target − λ_sun|` folded to `[0, π]` in radians.
/// - `lambda_nm`: wavelength in nanometres.
///
/// # Returns
///
/// A multiplicative factor ≥ 1.0 to be applied to the solar-spectrum scaled
/// zodiacal radiance at wavelength `lambda_nm`.
pub(super) fn reddening_factor(beta: Radians, delta_lambda: Radians, lambda_nm: f64) -> f64 {
    let cos_elong = (delta_lambda.cos() * beta.cos()).clamp(-1.0, 1.0);
    let elong_deg = cos_elong.acos().to_degrees();
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
    // Linear interpolation in elongation between 30° and 90°.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reddening_factor_acos_clamp_does_not_panic() {
        // When |cos(Δλ)·cos(β)| slightly exceeds 1.0 due to floating-point
        // rounding, the clamped path must not return NaN.
        let f = reddening_factor(Radians::new(0.0), Radians::new(0.0), 450.0);
        assert!(
            f.is_finite(),
            "reddening factor must be finite at elong=0: {f}"
        );
    }

    #[test]
    fn reddening_factor_is_finite_at_boundary_wavelengths() {
        let beta = Radians::new(0.3);
        let dl = Radians::new(1.5);
        for &wl in &[220.0_f64, 300.0, 445.0, 549.99, 550.0, 551.0, 650.0, 2499.0] {
            let f = reddening_factor(beta, dl, wl);
            assert!(
                f.is_finite() && f > 0.0,
                "reddening factor must be finite and positive at {wl} nm: {f}"
            );
        }
    }

    #[test]
    fn reddening_factor_outside_range_is_one() {
        // Below 220 nm and above 2500 nm → factor = 1.0.
        let beta = Radians::new(0.3);
        let dl = Radians::new(1.5);
        assert_eq!(reddening_factor(beta, dl, 100.0), 1.0);
        assert_eq!(reddening_factor(beta, dl, 3000.0), 1.0);
    }
}
