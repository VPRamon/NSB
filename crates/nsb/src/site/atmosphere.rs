//! Shared atmospheric assumptions for site-aware NSB components.
//!
//! These conditions are intentionally component-neutral. Moonlight and airglow
//! both consume the same pressure, Rayleigh, and aerosol assumptions selected by
//! a [`super::SiteProfile`].

use siderust::atmosphere::{
    rayleigh_optical_depth_bodhaine99, AtmosphereProfile, MieParams, DEFAULT_SCALE_HEIGHT,
};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Hectopascals, Kilometers, Nanometers, OpticalDepths};

/// Atmospheric inputs shared by NSB components that model scattering.
///
/// Observer altitude is deliberately not stored here. Site geometry remains
/// tied to the [`Geodetic`] observer passed to the component or evaluator.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct AtmosphericConditions {
    /// Surface pressure used for Rayleigh scattering.
    pub surface_pressure: Hectopascals,
    /// Rayleigh atmospheric scale height.
    pub rayleigh_scale_height: Kilometers,
    /// Aerosol optical-depth and phase-function parameters.
    pub mie_params: MieParams,
}

/// Bodhaine Rayleigh optical depth for conditions whose pressure is already local.
///
/// `AtmosphericConditions::surface_pressure` represents the local atmospheric
/// column selected by the site profile. Siderust's Bodhaine helper also applies
/// an exponential reduction from observer altitude, so supplying both local
/// pressure and the query altitude would reduce the Rayleigh column twice.
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

impl AtmosphericConditions {
    /// Convert a Siderust profile while intentionally discarding its altitude.
    pub fn from_profile_without_altitude(profile: AtmosphereProfile) -> Self {
        Self {
            surface_pressure: profile.surface_pressure,
            rayleigh_scale_height: profile.rayleigh_scale_height,
            mie_params: profile.mie_params,
        }
    }

    /// Generic clear-sky conditions for an arbitrary location.
    ///
    /// Pressure is estimated from the supplied altitude and the aerosol
    /// parameters use the generic Paranal-like clear-sky values available from
    /// Siderust. This is not a named-site calibration.
    pub fn generic_clear_sky(location: Geodetic<ECEF>) -> Self {
        let altitude_m = location.height.value().max(0.0);
        let pressure = 1013.25 * (-altitude_m / 8_400.0).exp();
        Self::clear_sky_with_pressure(Hectopascals::new(pressure))
    }

    /// Clear-sky Mie/Rayleigh defaults with an explicit surface pressure.
    ///
    /// Use this when constructing caller-defined planning profiles that fix
    /// pressure independently of the query observer altitude.
    pub fn clear_sky_with_pressure(surface_pressure: Hectopascals) -> Self {
        Self {
            surface_pressure,
            rayleigh_scale_height: DEFAULT_SCALE_HEIGHT,
            mie_params: MieParams::PARANAL,
        }
    }

    /// Paranal-like average clear-sky conditions from Siderust's built-in profile.
    pub fn paranal_average() -> Self {
        Self::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
    }
}
