//! Scattered moonlight component: Krisciunas & Schaefer (1991) and Jones et al. (2013).
//!
//! This module exposes two site-bound models:
//!
//! * [`KrisciunasSchaefer1991`] is the published analytic V-band reference model. It stores
//!   the observing location and `k_ext`, then computes lunar phase, Moon
//!   zenith, Moon-target separation, source zenith, and Moon distance
//!   internally from `(time, target)`.
//! * [`Jones2013Spectral`] is the wavelength-resolved scattered moonlight
//!   model. It stores the observing location and [`AtmosphericConditions`],
//!   builds a Siderust [`siderust::atmosphere::AtmosphereProfile`] internally,
//!   and derives observer altitude only from the model location.
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
use crate::units::MagnitudesPerAirmass;
use crate::NSB_S10_ZP;
use qtty::angular::{Degree, Degrees, Radian, Radians};
use qtty::radiometry::{
    self, spectral_radiance_to_photon_radiance_ns_nm,
    PhotonsPerSquareCentimeterNanosecondSteradian, WattsPerSquareMeterSteradianNanometer,
};
use scattering::ScatterGrid;
#[cfg(test)]
use siderust::astro::apparent::CorrectionPolicy;
use siderust::atmosphere::{
    airmass, mie_optical_depth, rayleigh_optical_depth_bodhaine99, rayleigh_phase,
    AtmosphereProfile, KrisciunasSchaefer1991 as KrisciunasSchaeferAirmass,
};
use siderust::coordinates::cartesian;
use siderust::coordinates::centers::{Geocentric, Geodetic};
use siderust::coordinates::frames::{EclipticMeanJ2000, EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::TransformFrame;
use siderust::ephemeris::{Ephemeris, Vsop87Ephemeris};
use siderust::event::horizontal;
use siderust::event::horizontal::star_horizontal;
#[cfg(test)]
use siderust::event::horizontal::star_horizontal_with_policy;
#[cfg(test)]
use siderust::event::lunar::meeus_ch47::moon_position_meeus_ch47;
use siderust::qtty::{AstronomicalUnit, IlluminationFractions, Kilometer, Kilometers, Nanometers};
use siderust::{reflected_lunar_spectral_radiance_jones2013, MoonPhaseGeometry};
use std::sync::OnceLock;
use tempoch::{Period, Time, JD, TT, UTC};

mod jones_2013_spectral;
mod krisciunas_schaefer1991;
mod scattering;

pub use crate::site::AtmosphericConditions;
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
pub const DEFAULT_K_EXT: MagnitudesPerAirmass = MagnitudesPerAirmass::new(0.172);

const S10_V_TO_INTEGRATED_PH: PhotonsPerSquareCentimeterNanosecondSteradian =
    PhotonsPerSquareCentimeterNanosecondSteradian::new(1.242e-3);
const WL_LOW: Nanometers = Nanometers::new(300.0);
const WL_HIGH: Nanometers = Nanometers::new(650.0);
const B_FILTER: Nanometers = Nanometers::new(445.0);
const V_FILTER: Nanometers = Nanometers::new(551.0);

/// Empirical aerosol-scattering weight applied to the Jones 2013 Mie phase term.
///
/// This is not a physical constant. It is a calibration knob that compensates
/// for the bundled Mie phase grid and the simplified single-scattering path used
/// by this implementation. Site-calibrated profiles should be validated against
/// reference spectra before changing this factor.
const JONES_MIE_WEIGHT: f64 = 0.05;

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
/// Integrated scattered-moonlight radiance and diagnostic B/V values.
pub struct MoonOutputs {
    /// Photon radiance integrated over 300–650 nm.
    pub integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian,
    /// Monochromatic B-reference S10 diagnostic.
    pub b_flux_s10: radiometry::S10s,
    /// Monochromatic V-reference S10 diagnostic.
    pub v_flux_s10: radiometry::S10s,
}

fn lunar_geometry(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> MoonlightGeometry {
    let jd = time.to::<TT>().to::<JD>();
    let moon_geo_ecliptic = Vsop87Ephemeris::moon_geocentric(jd);
    lunar_geometry_from_position(jd, location, target, moon_geo_ecliptic)
}

/// Test-only reduced lunar geometry retained to quantify approximation error.
#[cfg(test)]
fn approximate_lunar_geometry(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> MoonlightGeometry {
    let jd = time.to::<TT>().to::<JD>();
    let moon = moon_position_meeus_ch47(jd);
    let source = star_horizontal(target.ra(), target.dec(), &location, jd);
    let moon_geocentric = star_horizontal_with_policy(
        moon.ra.to::<Degree>(),
        moon.dec.to::<Degree>(),
        &location,
        jd,
        CorrectionPolicy::GEOMETRIC,
    );
    let horizontal_parallax = (6_378.137 / moon.dist.value()).clamp(-1.0, 1.0).asin();
    let moon_altitude = Radians::new(
        moon_geocentric.alt().to::<Radian>().value()
            - horizontal_parallax * moon_geocentric.alt().cos(),
    );
    let source_altitude = source.alt().to::<Radian>();
    let delta_azimuth = (source.az().to::<Radian>() - moon_geocentric.az().to::<Radian>()).value();
    let separation = Degrees::new(
        (source_altitude.sin() * moon_altitude.sin()
            + source_altitude.cos() * moon_altitude.cos() * delta_azimuth.cos())
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees(),
    );

    let days = jd.raw().value() - 2_451_545.0;
    let mean_longitude = (280.466_46 + 0.985_647_36 * days).to_radians();
    let mean_anomaly = (357.529_11 + 0.985_600_28 * days).to_radians();
    let sun_longitude = mean_longitude
        + 1.914_602_f64.to_radians() * mean_anomaly.sin()
        + 0.019_993_f64.to_radians() * (2.0 * mean_anomaly).sin()
        + 0.000_289_f64.to_radians() * (3.0 * mean_anomaly).sin();
    let delta_longitude = (moon.ecl_lon.value() - sun_longitude).rem_euclid(std::f64::consts::TAU);
    let elongation = (moon.ecl_lat.cos() * delta_longitude.cos())
        .clamp(-1.0, 1.0)
        .acos();
    let moon_distance_au = moon.dist.value() / 149_597_870.7;
    let sun_moon_distance = (1.0 + moon_distance_au * moon_distance_au
        - 2.0 * moon_distance_au * elongation.cos())
    .sqrt();
    let phase_angle =
        ((sun_moon_distance * sun_moon_distance + moon_distance_au * moon_distance_au - 1.0)
            / (2.0 * sun_moon_distance * moon_distance_au))
            .clamp(-1.0, 1.0)
            .acos();
    MoonlightGeometry {
        separation,
        moon_zenith: Degrees::new(90.0 - moon_altitude.to::<Degree>().value()),
        phase: MoonPhaseGeometry {
            phase_angle: Radians::new(phase_angle),
            illuminated_fraction: IlluminationFractions::new(0.5 * (1.0 + phase_angle.cos())),
            elongation: Radians::new(delta_longitude),
            waxing: delta_longitude < std::f64::consts::PI,
        },
        source_zenith: Degrees::new(90.0) - source.alt(),
        moon_distance: moon.dist,
    }
}

fn lunar_geometry_from_position(
    jd: siderust::time::JulianDate,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
    moon_geo_ecliptic: cartesian::Position<Geocentric, EclipticMeanJ2000, Kilometer>,
) -> MoonlightGeometry {
    let source = star_horizontal(target.ra(), target.dec(), &location, jd);
    let source_zenith = Degrees::new(90.0) - source.alt();
    let moon_geo_equatorial: cartesian::Position<Geocentric, EquatorialMeanJ2000, Kilometer> =
        moon_geo_ecliptic.to_frame();
    let moon_topocentric =
        horizontal::geocentric_j2000_to_apparent_topocentric(&moon_geo_equatorial, location, jd);
    let moon_pos = horizontal::equatorial_to_horizontal(&moon_topocentric, location, jd);
    let moon_dir = moon_pos.direction();
    let moon_zenith = Degrees::new(90.0) - moon_dir.alt();
    let separation = source.angular_separation(&moon_dir);
    let phase = phase_from_position(jd, &moon_geo_ecliptic);
    MoonlightGeometry {
        source_zenith,
        moon_zenith,
        separation,
        phase,
        moon_distance: moon_pos.distance.to::<Kilometer>(),
    }
}

fn phase_from_position(
    jd: siderust::time::JulianDate,
    moon: &cartesian::Position<Geocentric, EclipticMeanJ2000, Kilometer>,
) -> MoonPhaseGeometry {
    const AU_KM: f64 = 149_597_870.7;
    let moon_spherical = moon.to_spherical();
    let moon_lon = moon_spherical.azimuth.to::<Radian>();
    let moon_lat = moon_spherical.polar.to::<Radian>();
    let moon_distance_km = moon_spherical.distance.value();

    let earth: cartesian::Position<
        siderust::coordinates::centers::Heliocentric,
        EclipticMeanJ2000,
        AstronomicalUnit,
    > = Vsop87Ephemeris::earth_heliocentric(jd);
    let earth_spherical = earth.to_spherical();
    let sun_lon = Radians::new(
        (earth_spherical.azimuth.to::<Radian>().value() + std::f64::consts::PI)
            .rem_euclid(std::f64::consts::TAU),
    );
    let sun_distance_au = earth_spherical.distance.value();

    let delta_lon = (moon_lon - sun_lon).value();
    let (sin_moon_lat, cos_moon_lat) = moon_lat.sin_cos();
    let (sin_delta_lon, cos_delta_lon) = delta_lon.sin_cos();
    let cross = (sin_moon_lat * sin_moon_lat
        + cos_moon_lat * cos_moon_lat * sin_delta_lon * sin_delta_lon)
        .sqrt();
    let dot = cos_moon_lat * cos_delta_lon;
    let elongation_angle = cross.atan2(dot);
    let signed_elongation = delta_lon.rem_euclid(std::f64::consts::TAU);

    let moon_distance_au = moon_distance_km / AU_KM;
    let sun_moon_distance = (sun_distance_au * sun_distance_au
        + moon_distance_au * moon_distance_au
        - 2.0 * sun_distance_au * moon_distance_au * elongation_angle.cos())
    .max(0.0)
    .sqrt();
    let phase_angle = if sun_moon_distance < 1.0e-15 {
        0.0
    } else {
        ((sun_moon_distance * sun_moon_distance + moon_distance_au * moon_distance_au
            - sun_distance_au * sun_distance_au)
            / (2.0 * sun_moon_distance * moon_distance_au))
            .clamp(-1.0, 1.0)
            .acos()
    };

    MoonPhaseGeometry {
        phase_angle: Radians::new(phase_angle),
        illuminated_fraction: IlluminationFractions::new(0.5 * (1.0 + phase_angle.cos())),
        elongation: Radians::new(signed_elongation),
        waxing: signed_elongation > 0.0 && signed_elongation < std::f64::consts::PI,
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
    AtmosphericConditions::generic_clear_sky(location)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use siderust::bodies::Moon;
    use siderust::event::lunar::phase::moon_phase_geocentric;
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

    #[test]
    fn reduced_lunar_geometry_is_bounded_and_exact_path_matches_siderust() {
        let locations = [
            cta_n(),
            Geodetic::new_raw(
                SiderustDegrees::new(-70.3147),
                SiderustDegrees::new(-24.6834),
                Meters::new(2_147.0),
            ),
        ];
        let targets = [
            SphericalDirection::<EquatorialMeanJ2000>::new(
                SiderustDegrees::new(83.6331),
                SiderustDegrees::new(22.0145),
            ),
            SphericalDirection::<EquatorialMeanJ2000>::new(
                SiderustDegrees::new(266.41683),
                SiderustDegrees::new(-29.00781),
            ),
        ];
        let epoch = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
        let mut max_moon_zenith_error: f64 = 0.0;
        let mut max_separation_error: f64 = 0.0;
        let mut max_phase_error: f64 = 0.0;
        let mut max_distance_error: f64 = 0.0;

        for day in (0..365).step_by(7) {
            let time = Time::<UTC>::from_chrono(epoch + Duration::days(day));
            let jd = time.to::<TT>().to::<JD>();
            for location in locations {
                for target in targets {
                    let exact = lunar_geometry(time, location, target);
                    let approximate = approximate_lunar_geometry(time, location, target);
                    let reference_moon = Moon::get_horizontal::<Kilometer>(jd, location);
                    let reference_phase = moon_phase_geocentric::<Vsop87Ephemeris>(jd);

                    assert!(
                        (exact.moon_zenith.value()
                            - (90.0 - reference_moon.direction().alt().value()))
                        .abs()
                            < 1.0e-10
                    );
                    assert!(
                        (exact.phase.phase_angle.value() - reference_phase.phase_angle.value())
                            .abs()
                            < 1.0e-12
                    );
                    assert!(
                        (exact.moon_distance.value()
                            - reference_moon.distance.to::<Kilometer>().value())
                        .abs()
                            < 1.0e-8
                    );

                    max_moon_zenith_error = max_moon_zenith_error
                        .max((approximate.moon_zenith - exact.moon_zenith).abs().value());
                    max_separation_error = max_separation_error
                        .max((approximate.separation - exact.separation).abs().value());
                    max_phase_error = max_phase_error.max(
                        (approximate.phase.phase_angle - exact.phase.phase_angle)
                            .abs()
                            .to::<Degree>()
                            .value(),
                    );
                    max_distance_error = max_distance_error.max(
                        (approximate.moon_distance - exact.moon_distance)
                            .abs()
                            .value(),
                    );
                }
            }
        }

        eprintln!(
            "reduced lunar max errors: zenith={max_moon_zenith_error:.6}deg separation={max_separation_error:.6}deg phase={max_phase_error:.6}deg distance={max_distance_error:.3}km"
        );
        // These are discovery-model envelopes, not accepted output tolerances.
        // Exact samples classify every smooth interval and refine every crossing.
        assert!(max_moon_zenith_error < 2.1);
        assert!(max_separation_error < 2.2);
        assert!(max_phase_error < 0.3);
        assert!(max_distance_error < 7_000.0);
    }
}
