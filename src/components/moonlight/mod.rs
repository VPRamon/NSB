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

use crate::error::Result;
use crate::reference::solar;
use crate::NSB_S10_ZP;
use scattering::ScatterGrid;
use optica::grid::OutOfRange;
use optica::spectrum::algo;
use qtty::angular::{Degree, Degrees, Radian, Radians};
use qtty::radiometry::{
    self, spectral_radiance_to_photon_radiance_ns_nm,
    PhotonsPerSquareCentimeterNanosecondSteradian, WattsPerSquareMeterSteradianNanometer,
};
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
use siderust::qtty::{Hectopascals, Kilometer, Kilometers, Nanometers, OpticalDepths};
use siderust::{reflected_lunar_spectral_radiance_jones2013, MoonPhaseGeometry};
use std::sync::OnceLock;
use tempoch::{Period, Time, JD, TT, UTC};

mod jones_2013_spectral;
mod krisciunas_schaefer1991;
mod scattering;

pub use jones_2013_spectral::Jones2013Spectral;
pub use krisciunas_schaefer1991::KrisciunasSchaefer1991;

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
const JONES_MIE_WEIGHT: f64 = 0.05;

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
        moon_distance: moon_pos.norm().to::<Kilometer>(),
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
