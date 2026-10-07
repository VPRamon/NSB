//! Zodiacal-light wavelength reddening model (Leinert et al. 1998).
//!
//! Zodiacal light is slightly redder than the solar spectrum because the
//! interplanetary-dust scattering function has a mild wavelength dependence.
//! Leinert et al. (1998), Eq. (22), parameterise this reddening as a function
//! of *elongation* (angular distance from the Sun) and wavelength.
//!
//! # Validity
//!
//! The published colour correction is normalized at 500 nm and uses two
//! wavelength regimes:
//! - 220–500 nm
//! - 500–2500 nm
//!
//! The relation is linear in `log10(lambda / 500 nm)`. At exactly 500 nm both
//! branches evaluate to 1.0. Outside the published 220–2500 nm domain this
//! implementation returns 1.0 rather than extrapolating the empirical fit.
//!
//! The elongation interpolation is linear between 30° (inner zodiacal cloud,
//! steeper reddening) and 90° (outer/ecliptic-pole direction, shallower).
//!
//! # Reference
//! Leinert et al. (1998), *A&AS* 127, 1–99, §8.4.2, Eq. (22).

use crate::units::angular::Radians;

/// Wavelength-dependent reddening factor `f(lambda, epsilon)` where `epsilon` is the
/// elongation angle (angular distance from the Sun).
///
/// The elongation is derived from `beta` (ecliptic latitude) and
/// `delta_lambda` (target ecliptic longitude minus solar longitude, folded
/// to `[0, pi]`).
///
/// # Arguments
///
/// - `beta`: ecliptic latitude in radians.
/// - `delta_lambda`: `|lambda_target - lambda_sun|` folded to `[0, pi]` in radians.
/// - `lambda_nm`: wavelength in nanometres.
///
/// # Returns
///
/// A multiplicative colour-correction factor to be applied to the solar-spectrum
/// scaled zodiacal radiance at wavelength `lambda_nm`.
pub(super) fn reddening_factor(beta: Radians, delta_lambda: Radians, lambda_nm: f64) -> f64 {
    let cos_elong = (delta_lambda.cos() * beta.cos()).clamp(-1.0, 1.0);
    let elong_deg = cos_elong.acos().to_degrees();

    let (slope_30, slope_90) = if (220.0..=500.0).contains(&lambda_nm) {
        (1.2, 0.9)
    } else if (500.0..=2500.0).contains(&lambda_nm) {
        (0.8, 0.6)
    } else {
        return 1.0;
    };

    let elong_fraction = ((elong_deg - 30.0) / 60.0).clamp(0.0, 1.0);
    let slope = slope_30 + (slope_90 - slope_30) * elong_fraction;
    let log_ratio = (lambda_nm / 500.0).log10();

    1.0 + slope * log_ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABS_TOL: f64 = 1.0e-12;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= ABS_TOL,
            "expected {expected:.15}, got {actual:.15}"
        );
    }

    #[test]
    fn reddening_factor_acos_clamp_does_not_panic() {
        // When |cos(delta_lambda) * cos(beta)| slightly exceeds 1.0 due to
        // floating-point rounding, the clamped path must not return NaN.
        let f = reddening_factor(Radians::new(0.0), Radians::new(0.0), 450.0);
        assert!(
            f.is_finite(),
            "reddening factor must be finite at elong=0: {f}"
        );
    }

    #[test]
    fn reddening_matches_leinert_eq22_at_30_and_90_degrees() {
        let beta = Radians::new(0.0);

        // Leinert et al. (1998), Eq. (22):
        // epsilon=30 deg: slope 1.2 below 500 nm, 0.8 above.
        let elong_30 = Radians::new(30.0_f64.to_radians());
        assert_close(
            reddening_factor(beta, elong_30, 400.0),
            0.883_707_984_390_332_4,
        );
        assert_close(reddening_factor(beta, elong_30, 600.0), 1.063_344_996_838_1);

        // epsilon=90 deg: slope 0.9 below 500 nm, 0.6 above.
        let elong_90 = Radians::new(90.0_f64.to_radians());
        assert_close(
            reddening_factor(beta, elong_90, 400.0),
            0.912_780_988_292_749_2,
        );
        assert_close(
            reddening_factor(beta, elong_90, 600.0),
            1.047_508_747_628_575,
        );
    }

    #[test]
    fn reddening_uses_published_500_nm_branch_boundary() {
        let beta = Radians::new(0.0);
        let elong_30 = Radians::new(30.0_f64.to_radians());

        assert_close(
            reddening_factor(beta, elong_30, 499.0),
            0.998_956_649_544_845_3,
        );
        assert_close(reddening_factor(beta, elong_30, 500.0), 1.0);
        assert_close(
            reddening_factor(beta, elong_30, 501.0),
            1.000_694_177_224_981_6,
        );

        // The historical implementation incorrectly changed branches at
        // 550 nm. All three wavelengths must use the long-wavelength slope.
        assert_close(
            reddening_factor(beta, elong_30, 549.0),
            1.032_481_872_091_258_5,
        );
        assert_close(
            reddening_factor(beta, elong_30, 550.0),
            1.033_114_148_126_58,
        );
        assert_close(
            reddening_factor(beta, elong_30, 551.0),
            1.033_745_275_612_613,
        );
    }

    #[test]
    fn reddening_interpolates_linearly_in_elongation() {
        let beta = Radians::new(0.0);
        let elong_60 = Radians::new(60.0_f64.to_radians());

        assert_close(
            reddening_factor(beta, elong_60, 400.0),
            0.898_244_486_341_540_7,
        );
        assert_close(
            reddening_factor(beta, elong_60, 600.0),
            1.055_426_872_233_337_3,
        );
    }

    #[test]
    fn reddening_factor_is_finite_and_positive_in_release_band() {
        let beta = Radians::new(0.3);
        let dl = Radians::new(1.5);
        for &wl in &[300.0_f64, 445.0, 499.0, 500.0, 501.0, 551.0, 650.0] {
            let f = reddening_factor(beta, dl, wl);
            assert!(
                f.is_finite() && f > 0.0,
                "reddening factor must be finite and positive at {wl} nm: {f}"
            );
        }
    }

    #[test]
    fn reddening_factor_outside_published_range_is_one() {
        let beta = Radians::new(0.3);
        let dl = Radians::new(1.5);
        assert_eq!(reddening_factor(beta, dl, 100.0), 1.0);
        assert_eq!(reddening_factor(beta, dl, 3000.0), 1.0);
    }
}
