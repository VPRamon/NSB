//! Noll et al. (2012) effective Rayleigh/Mie scattering for airglow.
//!
//! This stage models atmospheric scattering of emitted airglow along the
//! observer line of sight. It is distinct from the Van Rhijn emitting-layer
//! geometry correction applied elsewhere in the airglow stack.
//!
//! # Model (Cerro Paranal Advanced Sky Model / Noll+2012 §3.4)
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
//! Wavelength-dependent transmission uses Siderust Rayleigh and Mie vertical
//! optical depths `τ_R(λ)` and `τ_M(λ)` from the selected
//! [`AtmosphericConditions`]:
//!
//! ```text
//! τ_eff(λ, z) = f_R(z) τ_R(λ) + f_M(z) τ_M(λ)
//! T_scatter(λ, z) = exp(-X_ag(z) τ_eff(λ, z))
//! ```
//!
//! `f_R` and `f_M` may be negative near zenith (net scattering into the line of
//! sight); they are not clamped.
//!
//! Molecular atmospheric absorption from the full Cerro Paranal ASM/SkyCalc
//! pipeline is not reproduced here.
//!
//! # Reference
//!
//! Noll, S., et al. (2012). "An atmospheric radiation model for Cerro Paranal".
//! *A&A* 543, A92. Eqs. (23)–(25).

use crate::components::moonlight::AtmosphericConditions;
use qtty::angular::{Degrees, Radian};
use qtty::dimensionless::Transmittances;
use siderust::atmosphere::{mie_optical_depth, rayleigh_optical_depth_bodhaine99};
use siderust::qtty::{Kilometers, Nanometers};

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
pub fn effective_airglow_airmass(zenith: Degrees) -> f64 {
    let sin_z = zenith.to::<Radian>().value().sin();
    let denom = 1.0 - AIRGLOW_AIRMASS_SIN2_COEFF * sin_z * sin_z;
    if denom <= 0.0 {
        return f64::INFINITY;
    }
    denom.powf(-0.5)
}

/// Noll Rayleigh and Mie scattering multipliers for zenith distance `z`.
#[inline]
pub fn noll_scattering_factors(zenith: Degrees) -> (f64, f64) {
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

/// Wavelength-dependent Noll effective airglow scattering transmission.
pub fn spectral_airglow_scattering_transmission(
    wavelength: Nanometers,
    zenith: Degrees,
    observer_altitude: Kilometers,
    atmosphere: AtmosphericConditions,
) -> Transmittances {
    let geometry = noll_airglow_scattering_geometry(zenith);
    spectral_airglow_scattering_transmission_with_geometry(
        wavelength,
        observer_altitude,
        atmosphere,
        &geometry,
    )
}

pub(crate) fn spectral_airglow_scattering_transmission_with_geometry(
    wavelength: Nanometers,
    observer_altitude: Kilometers,
    atmosphere: AtmosphericConditions,
    geometry: &NollAirglowScatteringGeometry,
) -> Transmittances {
    let tau_rayleigh = rayleigh_optical_depth_bodhaine99(
        wavelength,
        atmosphere.surface_pressure,
        observer_altitude,
        atmosphere.rayleigh_scale_height,
    )
    .value();
    let tau_mie = mie_optical_depth(&atmosphere.mie_params, wavelength).value();
    let tau_eff = geometry.f_rayleigh * tau_rayleigh + geometry.f_mie * tau_mie;
    let exponent = -geometry.effective_airmass * tau_eff;
    if !exponent.is_finite() {
        return Transmittances::new(0.0);
    }
    Transmittances::new(exponent.exp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::atmosphere::profile::AtmosphereProfile;
    use siderust::atmosphere::{mie_optical_depth, rayleigh_optical_depth_bodhaine99};
    use siderust::qtty::Hectopascals;

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
    fn transmission_is_wavelength_dependent_at_fixed_zenith() {
        let atmosphere = paranal_atmosphere();
        let altitude = Kilometers::new(2.635);
        let zenith = Degrees::new(45.0);
        let blue = spectral_airglow_scattering_transmission(
            Nanometers::new(350.0),
            zenith,
            altitude,
            atmosphere,
        )
        .value();
        let red = spectral_airglow_scattering_transmission(
            Nanometers::new(600.0),
            zenith,
            altitude,
            atmosphere,
        )
        .value();
        assert!(blue.is_finite() && red.is_finite());
        assert!(
            blue < red,
            "Rayleigh-dominated shorter wavelengths should attenuate more: blue={blue}, red={red}"
        );
    }

    #[test]
    fn transmission_reference_at_paranal_zenith_and_60_degrees() {
        let atmosphere = paranal_atmosphere();
        let altitude = Kilometers::new(2.635);
        let wl = Nanometers::new(550.0);
        let geometry_zenith = noll_airglow_scattering_geometry(Degrees::new(0.0));
        let tau_r = rayleigh_optical_depth_bodhaine99(
            wl,
            Hectopascals::new(744.0),
            altitude,
            atmosphere.rayleigh_scale_height,
        )
        .value();
        let tau_m = mie_optical_depth(&atmosphere.mie_params, wl).value();
        let tau_eff_z = geometry_zenith.f_rayleigh * tau_r + geometry_zenith.f_mie * tau_m;
        let expected_zenith = (-geometry_zenith.effective_airmass * tau_eff_z).exp();
        let got_zenith = spectral_airglow_scattering_transmission_with_geometry(
            wl,
            altitude,
            atmosphere,
            &geometry_zenith,
        )
        .value();
        assert!((got_zenith - expected_zenith).abs() < 1e-12);

        let geometry_60 = noll_airglow_scattering_geometry(Degrees::new(60.0));
        let tau_eff_60 = geometry_60.f_rayleigh * tau_r + geometry_60.f_mie * tau_m;
        let expected_60 = (-geometry_60.effective_airmass * tau_eff_60).exp();
        let got_60 = spectral_airglow_scattering_transmission_with_geometry(
            wl,
            altitude,
            atmosphere,
            &geometry_60,
        )
        .value();
        assert!((got_60 - expected_60).abs() < 1e-12);
        assert!(
            got_60 < got_zenith,
            "larger zenith should reduce transmission"
        );
    }

    #[test]
    fn different_atmospheres_change_transmission() {
        let altitude = Kilometers::new(2.1);
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
        let low =
            spectral_airglow_scattering_transmission(wl, zenith, altitude, low_pressure).value();
        let high =
            spectral_airglow_scattering_transmission(wl, zenith, altitude, high_pressure).value();
        assert_ne!(low, high);
    }
}
