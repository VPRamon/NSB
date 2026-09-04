//! Noll et al. (2012) effective Rayleigh/Mie scattering for airglow.
//!
//! This stage models atmospheric scattering of emitted airglow along the
//! observer line of sight. It is distinct from the selected emitting-volume
//! geometry correction applied elsewhere in the airglow stack.
//!
//! # Model (Cerro Paranal Advanced Sky Model / Noll+2012 §4.1)
//!
//! Effective airglow airmass for zenith distance `z`:
//!
//! ```text
//! X_ag = (1 - 0.972 sin²(z))^(-1/2)
//! ```
//!
//! Scattering multipliers (with `x = log10(X_ag)`):
//!
//! ```text
//! f_R = 1.669 x - 0.146
//! f_M = 1.732 x - 0.318
//! ```
//!
//! Wavelength-dependent transmission uses Rayleigh and Mie vertical optical
//! depths `τ_R(λ)` and `τ_M(λ)` from the selected [`AtmosphericConditions`]:
//!
//! ```text
//! τ_eff(λ, z) = f_R(z) τ_R(λ) + f_M(z) τ_M(λ)
//! T_scatter(λ, z) = exp(-X_ag(z) τ_eff(λ, z))
//! ```
//!
//! `f_R` and `f_M` may be negative near zenith (net scattering into the line of
//! sight); they are not clamped.
//!
//! # Scientific validity domain
//!
//! Noll fitted the effective extinction factors primarily for zenith distances
//! `z ≲ 60°`. NSB evaluates the same parametric form at larger angles for
//! numerical stability, but results beyond that fitted range should be treated
//! as extrapolations with weaker upstream validation.
//!
//! # Rayleigh optical depth and local pressure
//!
//! [`AtmosphericConditions::surface_pressure`] is the observatory-local
//! pressure. Siderust's [`rayleigh_optical_depth_bodhaine99`] scales by
//! both `surface_pressure / 1013.25 hPa` and `exp(-observer_altitude / H)`. Passing
//! local pressure together with a non-zero observer altitude therefore applies
//! the atmospheric-column reduction twice. Airglow therefore evaluates Bodhaine
//! Rayleigh depth with the local pressure only (`observer_altitude = 0` in the
//! Siderust call). See [`rayleigh_optical_depth_local_pressure`].
//!
//! Molecular atmospheric absorption from the full Cerro Paranal ASM/SkyCalc
//! pipeline is not reproduced here.
//!
//! # Reference
//!
//! Noll, S., et al. (2012). "An atmospheric radiation model for Cerro Paranal".
//! *A&A* 543, A92. §4.1; Eqs. (23)–(25).

use crate::components::moonlight::AtmosphericConditions;
use qtty::angular::{Degrees, Radian};
use qtty::dimensionless::Transmittances;
use siderust::atmosphere::{mie_optical_depth, rayleigh_optical_depth_bodhaine99};
use siderust::qtty::{Kilometers, Nanometers, OpticalDepths};

/// Noll effective-extinction fit is calibrated primarily through this zenith angle.
pub(crate) const NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG: f64 = 60.0;

/// Coefficient in the Noll effective airglow airmass (Eq. 23).
const AIRGLOW_AIRMASS_SIN2_COEFF: f64 = 0.972;

/// Rayleigh scattering multiplier: slope and intercept (Eq. 24).
const F_RAYLEIGH_SLOPE: f64 = 1.669;
const F_RAYLEIGH_INTERCEPT: f64 = -0.146;

/// Mie scattering multiplier: slope and intercept (Eq. 25).
const F_MIE_SLOPE: f64 = 1.732;
#[allow(clippy::approx_constant)] // Noll Eq. (25) intercept, not 1/π
const F_MIE_INTERCEPT: f64 = -0.318;

/// Zenith-dependent Noll airglow scattering geometry factors.
///
/// Precomputed from zenith angle so wavelength loops do not repeat airmass work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NollAirglowScatteringGeometry {
    /// Effective airglow airmass `X_ag`.
    pub effective_airmass: f64,
    /// Rayleigh multiplier `f_R`.
    pub f_rayleigh: f64,
    /// Mie multiplier `f_M`.
    pub f_mie: f64,
}

/// Effective airglow airmass `X_ag` for zenith distance `z` (Noll Eq. 23).
#[inline]
pub(crate) fn effective_airglow_airmass(zenith: Degrees) -> f64 {
    let sin_z = zenith.to::<Radian>().value().sin();
    let denom = 1.0 - AIRGLOW_AIRMASS_SIN2_COEFF * sin_z * sin_z;
    if denom <= 0.0 {
        return f64::INFINITY;
    }
    denom.powf(-0.5)
}

/// Noll Rayleigh and Mie scattering multipliers for zenith distance `z`.
#[inline]
pub(crate) fn noll_scattering_factors(zenith: Degrees) -> (f64, f64) {
    let x_ag = effective_airglow_airmass(zenith);
    let log_x = x_ag.log10();
    let f_rayleigh = F_RAYLEIGH_SLOPE * log_x + F_RAYLEIGH_INTERCEPT;
    let f_mie = F_MIE_SLOPE * log_x + F_MIE_INTERCEPT;
    (f_rayleigh, f_mie)
}

/// Build precomputed geometry factors for a zenith angle.
pub(crate) fn noll_airglow_scattering_geometry(zenith: Degrees) -> NollAirglowScatteringGeometry {
    let effective_airmass = effective_airglow_airmass(zenith);
    let (f_rayleigh, f_mie) = noll_scattering_factors(zenith);
    NollAirglowScatteringGeometry {
        effective_airmass,
        f_rayleigh,
        f_mie,
    }
}

/// Bodhaine Rayleigh optical depth using observatory-local pressure only.
///
/// `AtmosphericConditions::surface_pressure` already encodes the reduced column
/// mass at the site altitude. Siderust's [`rayleigh_optical_depth_bodhaine99`]
/// also applies `exp(-observer_altitude / scale_height)`, which would reduce the
/// column a second time if both local pressure and site altitude were supplied.
///
/// Until Siderust exposes an explicit local-pressure Bodhaine entry point, this
/// helper calls the published function with `observer_altitude = 0 km` so the
/// pressure ratio is applied once.
pub(crate) fn rayleigh_optical_depth_local_pressure(
    wavelength: Nanometers,
    atmosphere: AtmosphericConditions,
) -> OpticalDepths {
    rayleigh_optical_depth_bodhaine99(
        wavelength,
        atmosphere.surface_pressure,
        Kilometers::new(0.0),
        atmosphere.rayleigh_scale_height,
    )
}

/// Independent Bodhaine sea-level kernel used only for regression tests.
#[cfg(test)]
pub(crate) fn bodhaine_rayleigh_tau_sea_level(wavelength_um: f64) -> f64 {
    let l2 = wavelength_um * wavelength_um;
    let inv_l2 = 1.0 / l2;
    0.0021520 * (1.0455996 - 341.29061 * inv_l2 - 0.90230850 * l2)
        / (1.0 + 0.0027059889 * inv_l2 - 85.968563 * l2)
}

/// Wavelength-dependent Noll effective airglow scattering transmission.
#[cfg(test)]
pub(crate) fn spectral_airglow_scattering_transmission(
    wavelength: Nanometers,
    zenith: Degrees,
    atmosphere: AtmosphericConditions,
) -> Transmittances {
    let geometry = noll_airglow_scattering_geometry(zenith);
    spectral_airglow_scattering_transmission_with_geometry(wavelength, atmosphere, &geometry)
}

pub(crate) fn spectral_airglow_scattering_transmission_with_geometry(
    wavelength: Nanometers,
    atmosphere: AtmosphericConditions,
    geometry: &NollAirglowScatteringGeometry,
) -> Transmittances {
    let tau_rayleigh = rayleigh_optical_depth_local_pressure(wavelength, atmosphere).value();
    let tau_mie = mie_optical_depth(&atmosphere.mie_params, wavelength).value();
    let tau_eff = geometry.f_rayleigh * tau_rayleigh + geometry.f_mie * tau_mie;
    // `X_ag` is the path-length/airmass multiplier in Noll Eq. (18)/(25); `f_R` and
    // `f_M` are effective optical-depth coefficients, not a substitute for `X_ag`.
    let exponent = -geometry.effective_airmass * tau_eff;
    if !exponent.is_finite() {
        return Transmittances::new(0.0);
    }
    Transmittances::new(exponent.exp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::atmosphere::mie_optical_depth;
    use siderust::atmosphere::profile::AtmosphereProfile;
    use siderust::qtty::Hectopascals;

    const SEA_LEVEL_PRESSURE_HPA: f64 = 1013.25;

    fn paranal_atmosphere() -> AtmosphericConditions {
        AtmosphericConditions::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
    }

    #[test]
    fn effective_airglow_airmass_at_zenith_is_unity() {
        let x = effective_airglow_airmass(Degrees::new(0.0));
        assert!((x - 1.0).abs() < 1e-12, "X_ag at zenith: {x}");
    }

    #[test]
    fn effective_airglow_airmass_matches_noll_reference_angles() {
        let cases = [
            (0.0, 1.0),
            (30.0, (1.0 - 0.972 * 0.25_f64).powf(-0.5)),
            (60.0, (1.0 - 0.972 * 0.75_f64).powf(-0.5)),
            (
                75.0,
                (1.0 - 0.972 * (75.0_f64.to_radians().sin()).powi(2)).powf(-0.5),
            ),
        ];
        for (zenith_deg, expected) in cases {
            let got = effective_airglow_airmass(Degrees::new(zenith_deg));
            assert!(
                (got - expected).abs() < 1e-12,
                "zenith {zenith_deg}°: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn noll_scattering_factors_match_reference_equations() {
        let zeniths = [0.0, 30.0, 60.0, 75.0];
        for z in zeniths {
            let x_ag = effective_airglow_airmass(Degrees::new(z));
            let log_x = x_ag.log10();
            let expected_r = F_RAYLEIGH_SLOPE * log_x + F_RAYLEIGH_INTERCEPT;
            let expected_m = F_MIE_SLOPE * log_x + F_MIE_INTERCEPT;
            let (f_r, f_m) = noll_scattering_factors(Degrees::new(z));
            assert!((f_r - expected_r).abs() < 1e-12, "f_R at z={z}");
            assert!((f_m - expected_m).abs() < 1e-12, "f_M at z={z}");
        }
    }

    #[test]
    fn scattering_factors_are_negative_near_zenith() {
        let (f_r, f_m) = noll_scattering_factors(Degrees::new(0.0));
        assert!(f_r < 0.0, "f_R at zenith should be negative: {f_r}");
        assert!(f_m < 0.0, "f_M at zenith should be negative: {f_m}");
    }

    #[test]
    fn rayleigh_local_pressure_is_not_double_reduced_by_altitude() {
        let atmosphere = paranal_atmosphere();
        let wavelength = Nanometers::new(550.0);
        let local = rayleigh_optical_depth_local_pressure(wavelength, atmosphere).value();
        let tau_sea = bodhaine_rayleigh_tau_sea_level(0.550);
        let expected_local = atmosphere.surface_pressure.value() / SEA_LEVEL_PRESSURE_HPA * tau_sea;
        assert!(
            (local - expected_local).abs() < 1e-12,
            "local-pressure Rayleigh depth: got {local}, expected {expected_local}"
        );

        let double_reduced = rayleigh_optical_depth_bodhaine99(
            wavelength,
            atmosphere.surface_pressure,
            Kilometers::new(2.635),
            atmosphere.rayleigh_scale_height,
        )
        .value();
        assert!(
            (double_reduced - local * (-2.635_f64 / 8.0).exp()).abs() < 1e-12,
            "Siderust with site altitude should apply the extra exp(-h/H) factor"
        );
        assert!(
            local > double_reduced,
            "local pressure must not be reduced twice by altitude"
        );
    }

    #[test]
    fn transmission_is_wavelength_dependent_at_fixed_zenith() {
        let atmosphere = paranal_atmosphere();
        let zenith = Degrees::new(45.0);
        let blue =
            spectral_airglow_scattering_transmission(Nanometers::new(350.0), zenith, atmosphere)
                .value();
        let red =
            spectral_airglow_scattering_transmission(Nanometers::new(600.0), zenith, atmosphere)
                .value();
        assert!(blue.is_finite() && red.is_finite());
        assert!(
            blue < red,
            "Rayleigh-dominated shorter wavelengths should attenuate more: blue={blue}, red={red}"
        );
    }

    #[test]
    fn noll_transmission_matches_independent_paranal_reference_at_45_degrees() {
        let atmosphere = paranal_atmosphere();
        let zenith = Degrees::new(45.0);
        let wavelength = Nanometers::new(550.0);

        // Independently derived from Noll Eqs. (23)–(25) and Bodhaine (1999) with
        // local Paranal pressure 744 hPa (no extra exp(-h/H) reduction).
        let expected_x_ag = 1.394_820_881_629_177;
        let expected_f_r = 0.095_201_277_198_442;
        let expected_f_m = -0.067_694_061_049_909;
        let expected_tau_r = 0.071_272_180_636_833;
        let expected_tau_m = 0.05;
        let expected_tau_eff = expected_f_r * expected_tau_r + expected_f_m * expected_tau_m;
        let expected_transmission = 0.995_268_142_865_769;

        let geometry = noll_airglow_scattering_geometry(zenith);
        assert!((geometry.effective_airmass - expected_x_ag).abs() < 1e-12);
        assert!((geometry.f_rayleigh - expected_f_r).abs() < 1e-12);
        assert!((geometry.f_mie - expected_f_m).abs() < 1e-12);

        let tau_r = rayleigh_optical_depth_local_pressure(wavelength, atmosphere).value();
        let tau_m = mie_optical_depth(&atmosphere.mie_params, wavelength).value();
        assert!((tau_r - expected_tau_r).abs() < 1e-12);
        assert!((tau_m - expected_tau_m).abs() < 1e-12);

        let tau_eff = geometry.f_rayleigh * tau_r + geometry.f_mie * tau_m;
        assert!((tau_eff - expected_tau_eff).abs() < 1e-15);

        let got = spectral_airglow_scattering_transmission_with_geometry(
            wavelength, atmosphere, &geometry,
        )
        .value();
        assert!(
            (got - expected_transmission).abs() < 1e-12,
            "transmission: got {got}, expected {expected_transmission}"
        );

        // Removing `X_ag` from the exponent would brighten the result materially.
        let without_x_ag = (-tau_eff).exp();
        assert!(
            (without_x_ag - got).abs() > 1e-4,
            "X_ag must remain in the Noll transmission exponent"
        );
    }

    #[test]
    fn different_atmospheres_change_transmission() {
        let zenith = Degrees::new(45.0);
        let wl = Nanometers::new(500.0);
        let low_pressure = AtmosphericConditions {
            surface_pressure: Hectopascals::new(600.0),
            ..paranal_atmosphere()
        };
        let high_pressure = AtmosphericConditions {
            surface_pressure: Hectopascals::new(900.0),
            ..paranal_atmosphere()
        };
        let low = spectral_airglow_scattering_transmission(wl, zenith, low_pressure).value();
        let high = spectral_airglow_scattering_transmission(wl, zenith, high_pressure).value();
        assert_ne!(low, high);
    }
}
