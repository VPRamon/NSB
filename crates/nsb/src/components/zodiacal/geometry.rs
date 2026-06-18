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
