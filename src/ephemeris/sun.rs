//! Apparent solar position helpers, replacing Python `GetSunposition(mjd)`.
//!
//! Exposes only the geocentric ecliptic longitude of the Sun (radians), which
//! is what `CalculateZL` ultimately needs.  Uses siderust's VSOP87 backend for
//! sub-arcsecond accuracy.

use siderust::coordinates::{
    cartesian, centers::{Geocentric, Heliocentric}, frames::EclipticMeanJ2000, spherical,
};
use siderust::coordinates::transform::Transform;
use siderust::qtty::{AstronomicalUnit, Radian, Radians};
use siderust::time::JulianDate;

/// Geocentric ecliptic longitude of the Sun via VSOP87 (siderust).
pub fn ecliptic_longitude(jd: JulianDate) -> Radians {
    let helio = cartesian::Position::<Heliocentric, EclipticMeanJ2000, AstronomicalUnit>::CENTER;
    let geo_ecl: cartesian::Position<Geocentric, EclipticMeanJ2000, AstronomicalUnit> =
        helio.transform(jd);
    spherical::Position::from_cartesian(&geo_ecl)
        .direction()
        .lon()
        .to::<Radian>()
}
