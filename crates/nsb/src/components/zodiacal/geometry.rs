//! Zodiacal-light geometry: derive ecliptic coordinates and zenith distance
//! from observational inputs.
//!
//! # Ecliptic convention
//!
//! The target direction is converted to `EclipticMeanJ2000` (mean ecliptic
//! and equinox of J2000.0) using siderust's `TransformFrame`. The solar
//! longitude `λ_sun` is obtained from `Sun::ecliptic_longitude_geocentric(jd)`,
//! which also returns the value in the mean ecliptic of J2000.0. Both
//! quantities therefore share the same reference frame and can be directly
//! subtracted.
//!
//! # Output angles
//!
//! - `beta`: ecliptic latitude of the target, `β ∈ [−π/2, π/2]`.
//! - `delta_lambda`: `|λ_target − λ_sun|` folded into `[0, π]`. The Leinert
//!   table is symmetric about the anti-Sun point, so the full `[0, 2π)` range
//!   reduces to `[0, π]`.
//!
//! # Zenith distance
//!
//! The zenith distance is computed from the target altitude using the
//! standard siderust `star_horizontal` function. It is needed by the Noll
//! atmospheric extinction model. If no location is supplied (exoatmospheric
//! path), the zenith field is absent from the returned [`ZodiacalGeometry`].

use crate::evaluator::Target;
use qtty::angular::{Degrees, Radians};
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EclipticMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::TransformFrame;
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::Radian;
use siderust::JulianDate;
use tempoch::{Time, UTC};

use crate::error::{NsbError, Result};

/// Zodiacal-light geometry derived from observational inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct ZodiacalGeometry {
    /// Ecliptic latitude of the target, in radians.
    pub beta: Radians,
    /// `|λ_target − λ_sun|` folded to `[0, π]`, in radians.
    pub delta_lambda: Radians,
    /// Target zenith distance, in degrees. Present only when a location is
    /// provided (observed path); `None` for the exoatmospheric path.
    pub zenith: Option<Degrees>,
}

/// Compute zodiacal geometry from a UTC time and an equatorial target
/// direction, without an observer location (exoatmospheric).
pub(super) fn compute_exoatmospheric(time: Time<UTC>, target: Target) -> Result<ZodiacalGeometry> {
    let jd = to_jd(time);
    let (beta, delta_lambda) = ecliptic_geometry(target, jd)?;
    Ok(ZodiacalGeometry {
        beta,
        delta_lambda,
        zenith: None,
    })
}

/// Compute zodiacal geometry from a UTC time, observer location, and
/// equatorial target direction. Also computes zenith distance for the target.
pub(super) fn compute_observed(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: Target,
) -> Result<ZodiacalGeometry> {
    let jd = to_jd(time);
    let (beta, delta_lambda) = ecliptic_geometry(target, jd)?;
    let hz = star_horizontal(target.ra(), target.dec(), &location, jd);
    let alt = hz.alt();
    let zenith = Degrees::new(90.0) - alt;
    Ok(ZodiacalGeometry {
        beta,
        delta_lambda,
        zenith: Some(zenith),
    })
}

/// Reduced solar ephemeris used only for threshold-crossing discovery.
pub(super) fn compute_observed_for_discovery(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: Target,
) -> Result<ZodiacalGeometry> {
    let jd = to_jd(time);
    let ecl: SphericalDirection<EclipticMeanJ2000> = target.to_frame();
    let beta = ecl.lat().to::<Radian>();
    let ecliptic_lon = ecl.lon().to::<Radian>();
    let delta_lambda = ecliptic_lon.abs_separation(approximate_solar_longitude_j2000(jd));
    let hz = star_horizontal(target.ra(), target.dec(), &location, jd);
    Ok(ZodiacalGeometry {
        beta,
        delta_lambda,
        zenith: Some(Degrees::new(90.0) - hz.alt()),
    })
}

fn approximate_solar_longitude_j2000(jd: JulianDate) -> Radians {
    let days = jd.raw().value() - 2_451_545.0;
    let centuries = days / 36_525.0;
    let mean_longitude = (280.466_46 + 0.985_647_36 * days).to_radians();
    let mean_anomaly = (357.529_11 + 0.985_600_28 * days).to_radians();
    let longitude_of_date = mean_longitude
        + 1.914_602_f64.to_radians() * mean_anomaly.sin()
        + 0.019_993_f64.to_radians() * (2.0 * mean_anomaly).sin()
        + 0.000_289_f64.to_radians() * (3.0 * mean_anomaly).sin();
    let ecliptic_precession = (1.397 * centuries + 0.000_31 * centuries * centuries).to_radians();
    Radians::new((longitude_of_date - ecliptic_precession).rem_euclid(std::f64::consts::TAU))
}

fn ecliptic_geometry(target: Target, jd: JulianDate) -> Result<(Radians, Radians)> {
    let ecl: SphericalDirection<EclipticMeanJ2000> = target.to_frame();
    let beta = ecl.lat().to::<Radian>();
    let ecliptic_lon = ecl.lon().to::<Radian>();
    let lambda_sun = SunBody::ecliptic_longitude_geocentric(jd);
    let delta_lambda = ecliptic_lon.abs_separation(lambda_sun);
    if !beta.is_finite() {
        return Err(NsbError::OutOfRange(format!(
            "computed ecliptic latitude β={} rad is not finite",
            beta.value()
        )));
    }
    if !delta_lambda.is_finite() {
        return Err(NsbError::OutOfRange(format!(
            "computed Δλ={} rad is not finite",
            delta_lambda.value()
        )));
    }
    Ok((beta, delta_lambda))
}

fn to_jd(time: Time<UTC>) -> JulianDate {
    use tempoch::{JD, TT};
    time.to::<TT>().to::<JD>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn reduced_solar_longitude_tracks_vsop87_over_a_year() {
        let start =
            Time::<UTC>::from_chrono(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap());
        let mut max_error_deg = 0.0_f64;
        for day in 0..=365 {
            let time =
                Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(day));
            let jd = to_jd(time);
            let exact = SunBody::ecliptic_longitude_geocentric(jd);
            let approximate = approximate_solar_longitude_j2000(jd);
            max_error_deg =
                max_error_deg.max(exact.abs_separation(approximate).value().to_degrees());
        }
        assert!(
            max_error_deg < 0.02,
            "maximum longitude error {max_error_deg} deg"
        );
    }
}
