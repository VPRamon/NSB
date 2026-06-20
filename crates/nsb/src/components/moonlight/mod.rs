//! Scattered moonlight component: Krisciunas & Schaefer (1991) and Jones et al. (2013).
//!
//! This module exposes two site-bound models:
//!
//! * [`KrisciunasSchaefer1991`] is the analytic V-band legacy model. It stores
//!   the observing location and `k_ext`, then computes lunar phase, Moon
//!   zenith, Moon-target separation, source zenith, and Moon distance
//!   internally from `(time, target)`.
//! * [`Jones2013Spectral`] is the wavelength-resolved scattered moonlight
//!   model. It stores the observing location and [`AtmosphericConditions`],
//!   builds a Siderust [`AtmosphereProfile`] internally, and derives observer
//!   altitude only from the model location.
//!
//! [`Jones2013Spectral::standard_clear_sky`] is a generic approximate
//! clear-sky fallback: it estimates surface pressure from altitude, uses
//! Siderust's default Rayleigh scale height, and uses a generic clear-sky Mie
//! parameter set. It is not a site-calibrated atmosphere.
//!
//! For CTAO use, prefer [`Jones2013Spectral::for_site_profile`] with an explicit
//! [`crate::SiteProfileId`]. The built-in CTAO profiles document their current
//! planning assumptions and calibration maturity instead of silently relying on
//! `standard_clear_sky`.

use crate::error::Result;
use crate::reference::solar;
use crate::site::SiteProfileId;
use crate::NSB_S10_ZP;
use optica::grid::OutOfRange;
use optica::spectrum::algo;
use qtty::angular::{Degree, Degrees, Radian, Radians};
use qtty::radiometry::{
    self, spectral_radiance_to_photon_radiance_ns_nm,
    PhotonsPerSquareCentimeterNanosecondSteradian, WattsPerSquareMeterSteradianNanometer,
};
use scattering::ScatterGrid;
use siderust::atmosphere::{
    airmass, mie_optical_depth, rayleigh_optical_depth_bodhaine99, rayleigh_phase,
    AtmosphereProfile, KrisciunasSchaefer1991 as KrisciunasSchaeferAirmass, MieParams,
    DEFAULT_SCALE_HEIGHT,
};
use siderust::bodies::Moon;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::{Hectopascals, Kilometer, Kilometers, Nanometers};
use siderust::{reflected_lunar_spectral_radiance_jones2013, MoonPhaseGeometry};
use std::sync::OnceLock;
use tempoch::{Period, Time, JD, TT, UTC};

mod jones_2013_spectral;
mod krisciunas_schaefer1991;
mod scattering;

pub use jones_2013_spectral::Jones2013Spectral;
pub use krisciunas_schaefer1991::KrisciunasSchaefer1991;

impl Jones2013Spectral {
    /// Build the Jones et al. (2013) moonlight model from a named NSB site profile.
    ///
    /// This keeps the query geometry tied to `location` while selecting the
    /// profile's explicit pressure, Rayleigh, aerosol/Mie, and provenance-backed
    /// assumptions. CTAO profiles are planning presets until dedicated CTAO
    /// aerosol validation data are bundled.
    pub fn for_site_profile(location: Geodetic<ECEF>, site_profile: SiteProfileId) -> Self {
        let profile = site_profile.profile(location);
        Self::new(location, profile.atmosphere)
    }
}

/// Default V-band atmospheric extinction coefficient (mag/airmass) used by
/// K&S 1991 in their published curves.
pub const DEFAULT_K_EXT: f64 = 0.172;

const S10_V_TO_INTEGRATED_PH: f64 = 1.242e-3;
const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;
const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;
const S10_TO_W_M2_SR_UM: f64 = 1.28e-8;
const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;

/// Empirical aerosol-scattering weight applied to the Jones 2013 Mie phase term.
///
/// This is not a physical constant. It is a calibration knob that compensates
/// for the bundled Mie phase grid and the simplified single-scattering path used
/// by this implementation. Site-calibrated profiles should be validated against
/// reference spectra before changing this factor.
const JONES_MIE_WEIGHT: f64 = 0.05;

/// Atmospheric inputs used by the Jones 2013 spectral scattered-moonlight model.
///
/// The observer altitude is deliberately not stored here. It is always taken
/// from the [`Geodetic`] location passed to [`Jones2013Spectral`], avoiding the
/// ambiguity of mixing a site profile with an unrelated observer altitude.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtmosphericConditions {
    pub surface_pressure: Hectopascals,
    pub rayleigh_scale_height: Kilometers,
    pub mie_params: MieParams,
}

impl AtmosphericConditions {
    pub fn from_profile_without_altitude(profile: AtmosphereProfile) -> Self {
        Self {
            surface_pressure: profile.surface_pressure,
            rayleigh_scale_height: profile.rayleigh_scale_height,
            mie_params: profile.mie_params,
        }
    }

    /// Generic clear-sky conditions for an arbitrary location.
    ///
    /// This is the atmosphere used by [`Jones2013Spectral::standard_clear_sky`].
    /// Pressure is estimated from the supplied altitude and the Mie parameters
    /// are the generic Paranal-like clear-sky values available from Siderust.
    /// This is not a CTA site-calibrated atmosphere.
    pub fn generic_clear_sky(location: Geodetic<ECEF>) -> Self {
        standard_clear_sky_conditions(location)
    }

    /// Paranal-like average clear-sky conditions from Siderust's built-in profile.
    pub fn paranal_average() -> Self {
        Self::from_profile_without_altitude(AtmosphereProfile::EL_PARANAL)
    }

    /// CTA-S clear-sky planning preset.
    ///
    /// The current NSB preset intentionally aliases the Paranal-like profile
    /// because no dedicated CTA-S aerosol calibration has been bundled yet. It
    /// is nevertheless explicit at call sites, so science users can distinguish
    /// it from the generic altitude-derived fallback and replace it with a
    /// calibrated profile when available.
    pub fn cta_s_clear_sky() -> Self {
        Self::paranal_average()
    }

    /// CTA-N clear-sky planning preset.
    ///
    /// This uses a pressure representative of the La Palma/ORM altitude range
    /// and the same bundled clear-sky Mie parameterization used elsewhere in
    /// NSB. It should be treated as a planning preset until CTA-N aerosol phase
    /// functions are bundled and validated.
    pub fn cta_n_clear_sky() -> Self {
        Self {
            surface_pressure: Hectopascals::new(770.0),
            rayleigh_scale_height: DEFAULT_SCALE_HEIGHT,
            mie_params: MieParams::PARANAL,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MoonlightGeometry {
    /// Moon-source angular separation.
    separation: Degrees,
    /// Moon zenith distance.
    moon_zenith: Degrees,
    /// Geocentric lunar phase geometry from siderust.
    phase: MoonPhaseGeometry,
    /// Source zenith distance.
    source_zenith: Degrees,
    /// Topocentric Moon distance.
    moon_distance: Kilometers,
}

#[derive(Debug, Clone)]
pub struct MoonOutputs {
    pub integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian,
    pub b_flux_s10: radiometry::S10s,
    pub v_flux_s10: radiometry::S10s,
}

fn lunar_geometry(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> MoonlightGeometry {
    let jd = time.to::<TT>().to::<JD>();
    let source = star_horizontal(target.ra(), target.dec(), &location, jd);
    let source_zenith = Degrees::new(90.0) - source.alt();
    let moon_pos = Moon::get_horizontal::<Kilometer>(jd, location);
    let moon_dir = moon_pos.direction();
    let moon_zenith = Degrees::new(90.0) - moon_dir.alt();
    let separation = source.angular_separation(&moon_dir);
    let phase = Moon::phase_geocentric(jd);
    MoonlightGeometry {
        source_zenith,
        moon_zenith,
        separation,
        phase,
        moon_distance: moon_pos.distance.to::<Kilometer>(),
    }
}

fn zero_outputs() -> MoonOutputs {
    MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(0.0),
        b_flux_s10: radiometry::S10s::new(0.0),
        v_flux_s10: radiometry::S10s::new(0.0),
    }
}

fn standard_clear_sky_conditions(location: Geodetic<ECEF>) -> AtmosphericConditions {
    let altitude_m = location.height.value().max(0.0);
    let pressure = 1013.25 * (-altitude_m / 8_400.0).exp();
    AtmosphericConditions {
        surface_pressure: Hectopascals::new(pressure),
        rayleigh_scale_height: DEFAULT_SCALE_HEIGHT,
        mie_params: MieParams::PARANAL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::{Degrees as SiderustDegrees, Meters};

    fn cta_n() -> Geodetic<ECEF> {
        Geodetic::new_raw(
            SiderustDegrees::new(-17.892),
            SiderustDegrees::new(28.762),
            Meters::new(2_200.0),
        )
    }

    #[test]
    fn cta_n_moonlight_profile_changes_atmospheric_conditions() {
        let location = cta_n();
        let generic = standard_clear_sky_conditions(location);
        let profile = SiteProfileId::CtaNorth.profile(location);

        assert_ne!(
            generic.surface_pressure,
            profile.atmosphere.surface_pressure
        );
        assert_eq!(profile.atmosphere.surface_pressure.value(), 770.0);
    }

    #[test]
    fn jones_site_profile_constructor_is_explicit_api() {
        let location = cta_n();
        let model = Jones2013Spectral::for_site_profile(location, SiteProfileId::CtaNorth);
        let target = SphericalDirection::<EquatorialMeanJ2000>::new(
            SiderustDegrees::new(270.0),
            SiderustDegrees::new(-30.0),
        );
        let time = Time::<UTC>::from_chrono(
            chrono::DateTime::parse_from_rfc3339("2023-09-04T02:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        let out = model.compute(time, target).unwrap();
        assert!(out.integrated.value() >= 0.0);
    }
}
