// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Frame-preserving observing targets and time-dependent resolution.
//!
//! NSB accepts fixed equatorial (mean J2000) and fixed geometric horizontal
//! targets through one query API. Physical components always evaluate an
//! instantaneous [`Target`] (`Direction<EquatorialMeanJ2000>`). Equatorial
//! targets resolve identically at every instant; horizontal targets transform
//! through true-of-date equatorial coordinates using Siderust's IAU 2000/2006
//! path (TT for precession/nutation, UT1 via EOP for Earth rotation).
//!
//! Horizontal coordinates are geometric: altitude above the local astronomical
//! horizon and azimuth from North increasing clockwise through East. NSB does
//! not apply atmospheric refraction to query targets.

use crate::error::{NsbError, Result};
use crate::evaluator::{Observer, Target};
use siderust::coordinates::frames::{EquatorialMeanJ2000, Horizontal};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::coordinates::transform::SphericalDirectionAstroExt;
use siderust::qtty::Degrees;
use tempoch::{Time, JD, TT, UTC};

/// Fixed geometric altitude/azimuth direction for a ground observer.
///
/// Altitude is above the local astronomical horizon (`[-90°, +90°]`). Azimuth
/// is measured from North, increasing clockwise through East, and is wrapped
/// into `[0°, 360°)`` on construction where Siderust canonicalizes the
/// spherical direction. Values are geometric; atmospheric refraction is not
/// applied.
pub type HorizontalTarget = SphericalDirection<Horizontal>;

/// Observing target that preserves the caller-selected reference frame.
///
/// Use [`TargetDirection::Equatorial`] for a fixed celestial direction and
/// [`TargetDirection::Horizontal`] for a fixed local Alt/Az pointing. Query
/// constructors accept either variant (or the underlying Siderust directions)
/// through [`Into<TargetDirection>`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum TargetDirection {
    /// Fixed celestial direction in mean equator/equinox J2000.
    Equatorial(Target),
    /// Fixed geometric horizontal pointing for the query observer.
    Horizontal(HorizontalTarget),
}

impl TargetDirection {
    /// Construct a fixed equatorial (mean J2000) target.
    pub fn equatorial(target: Target) -> Self {
        Self::Equatorial(target)
    }

    /// Construct a fixed geometric horizontal target.
    pub fn horizontal(target: HorizontalTarget) -> Self {
        Self::Horizontal(target)
    }

    /// Return true when the stored direction is fixed in the horizontal frame.
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Horizontal(_))
    }

    /// Return true when the stored direction is fixed in mean J2000.
    pub const fn is_equatorial(self) -> bool {
        matches!(self, Self::Equatorial(_))
    }

    /// Altitude of a horizontal target, if that frame is stored.
    pub fn altitude(self) -> Option<Degrees> {
        match self {
            Self::Horizontal(direction) => Some(direction.alt()),
            Self::Equatorial(_) => None,
        }
    }

    /// Azimuth of a horizontal target, if that frame is stored.
    pub fn azimuth(self) -> Option<Degrees> {
        match self {
            Self::Horizontal(direction) => Some(direction.az()),
            Self::Equatorial(_) => None,
        }
    }

    /// Resolve this target to the internal scientific frame at `time`.
    ///
    /// Equatorial targets return the stored direction. Horizontal targets use
    /// Siderust `Horizontal → EquatorialTrueOfDate → EquatorialMeanJ2000` with
    /// TT/UT1 handled by Siderust's default EOP-aware transform.
    pub fn resolve(self, observer: Observer, time: Time<UTC>) -> Result<Target> {
        resolve_target_direction(self, observer, time)
    }
}

impl From<Target> for TargetDirection {
    fn from(value: Target) -> Self {
        Self::Equatorial(value)
    }
}

impl From<HorizontalTarget> for TargetDirection {
    fn from(value: HorizontalTarget) -> Self {
        Self::Horizontal(value)
    }
}

/// Validate observer and target coordinates without resolving frames.
pub(crate) fn validate_target_direction(observer: Observer, target: TargetDirection) -> Result<()> {
    if !observer.lon.is_finite() || !observer.lat.is_finite() || !observer.height.is_finite() {
        return Err(NsbError::OutOfRange(
            "observer and target coordinates must be finite".into(),
        ));
    }
    match target {
        TargetDirection::Equatorial(direction) => {
            if !direction.ra().is_finite() || !direction.dec().is_finite() {
                return Err(NsbError::OutOfRange(
                    "observer and target coordinates must be finite".into(),
                ));
            }
        }
        TargetDirection::Horizontal(direction) => {
            let alt = direction.alt();
            let az = direction.az();
            if !alt.is_finite() || !az.is_finite() {
                return Err(NsbError::OutOfRange(
                    "observer and target coordinates must be finite".into(),
                ));
            }
            if alt.value() < -90.0 || alt.value() > 90.0 {
                return Err(NsbError::OutOfRange(
                    "horizontal target altitude must be within [-90, 90] degrees".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Resolve a frame-preserving target to mean equator/equinox J2000 at `time`.
pub(crate) fn resolve_target_direction(
    target: TargetDirection,
    observer: Observer,
    time: Time<UTC>,
) -> Result<Target> {
    validate_target_direction(observer, target)?;
    match target {
        TargetDirection::Equatorial(direction) => Ok(direction),
        TargetDirection::Horizontal(direction) => {
            let jd_tt = time.to::<TT>().to::<JD>();
            // Siderust's TT-only path derives UT1 from the active EOP table.
            // Do not substitute UTC for UT1 or TT.
            let true_of_date = direction.to_equatorial(&jd_tt, &observer);
            let mean_j2000: SphericalDirection<EquatorialMeanJ2000> = true_of_date.to_frame(&jd_tt);
            if !mean_j2000.ra().is_finite() || !mean_j2000.dec().is_finite() {
                return Err(NsbError::OutOfRange(
                    "horizontal target resolved to a non-finite equatorial direction".into(),
                ));
            }
            Ok(mean_j2000)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEG;
    use siderust::catalogs::observatories::{EL_PARANAL, ROQUE_DE_LOS_MUCHACHOS};
    use tempoch::Time;

    fn utc(s: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            chrono::DateTime::parse_from_rfc3339(s)
                .expect("valid RFC3339")
                .with_timezone(&chrono::Utc),
        )
    }

    #[test]
    fn equatorial_resolution_is_identity() {
        let target = Target::new(83.633 * DEG, 22.014 * DEG);
        let resolved = TargetDirection::from(target)
            .resolve(
                ROQUE_DE_LOS_MUCHACHOS.geodetic(),
                utc("2024-01-15T02:00:00Z"),
            )
            .unwrap();
        assert!((resolved.ra().value() - target.ra().value()).abs() < 1.0e-15);
        assert!((resolved.dec().value() - target.dec().value()).abs() < 1.0e-15);
    }

    #[test]
    fn horizontal_resolution_is_time_dependent() {
        let horizontal = HorizontalTarget::new(60.0 * DEG, 130.0 * DEG);
        let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
        let early = TargetDirection::from(horizontal)
            .resolve(observer, utc("2024-01-15T00:00:00Z"))
            .unwrap();
        let late = TargetDirection::from(horizontal)
            .resolve(observer, utc("2024-01-15T06:00:00Z"))
            .unwrap();
        let separation = early.angular_separation(&late).value();
        assert!(
            separation > 1.0,
            "fixed Alt/Az must sweep celestial coordinates over six hours; got {separation} deg"
        );
    }

    #[test]
    fn same_horizontal_differs_across_observers() {
        let horizontal = HorizontalTarget::new(45.0 * DEG, 90.0 * DEG);
        let time = utc("2024-06-21T00:00:00Z");
        let orm = TargetDirection::from(horizontal)
            .resolve(ROQUE_DE_LOS_MUCHACHOS.geodetic(), time)
            .unwrap();
        let paranal = TargetDirection::from(horizontal)
            .resolve(EL_PARANAL.geodetic(), time)
            .unwrap();
        assert!(orm.angular_separation(&paranal).value() > 0.1);
    }

    #[test]
    fn horizontal_round_trip_near_identity() {
        let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
        let time = utc("2024-03-20T03:30:00Z");
        let jd_tt = time.to::<TT>().to::<JD>();
        let original = HorizontalTarget::new(35.0 * DEG, 210.0 * DEG);
        let equatorial = TargetDirection::from(original)
            .resolve(observer, time)
            .unwrap();
        // Mean J2000 → true of date → horizontal.
        use siderust::coordinates::frames::EquatorialTrueOfDate;
        let true_of_date: SphericalDirection<EquatorialTrueOfDate> = equatorial.to_frame(&jd_tt);
        let recovered = true_of_date.to_horizontal(&jd_tt, &observer);
        assert!((recovered.alt().value() - original.alt().value()).abs() < 1.0e-6);
        let az_err =
            (recovered.az().value() - original.az().value() + 180.0).rem_euclid(360.0) - 180.0;
        assert!(az_err.abs() < 1.0e-6);
    }

    #[test]
    fn rejects_nonfinite_and_out_of_range_altitude() {
        let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
        // Siderust's Direction::new canonicalizes |alt|>90; exercise the NSB
        // range guard with an unchecked out-of-range payload.
        let bad_alt = HorizontalTarget::new_unchecked(Degrees::new(95.0), 10.0 * DEG);
        let err = validate_target_direction(observer, TargetDirection::from(bad_alt)).unwrap_err();
        assert!(matches!(err, NsbError::OutOfRange(_)));

        let nan = HorizontalTarget::new_unchecked(Degrees::new(f64::NAN), 10.0 * DEG);
        let err = validate_target_direction(observer, TargetDirection::from(nan)).unwrap_err();
        assert!(matches!(err, NsbError::OutOfRange(_)));
    }
}
