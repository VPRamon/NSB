//! Validated direct-path geometry.

use crate::error::{NsbError, Result};
use crate::units::angular::Degrees;

/// Maximum zenith distance accepted by the transport geometry contract (degrees).
///
/// Individual [`super::AirmassModel`] formulas may become singular or weakly
/// validated near the horizon; callers should treat results near 90° cautiously.
pub(crate) const MAX_ZENITH_DEG: f64 = 90.0;

/// Geometry for a celestial direct-transmission path.
///
/// Construct only through [`DirectPathGeometry::new`], which rejects non-finite
/// and out-of-range zenith distances. Values are never silently clamped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectPathGeometry {
    zenith: Degrees,
}

impl DirectPathGeometry {
    /// Validated zenith distance in degrees, inclusive range `[0, 90]`.
    ///
    /// # Errors
    ///
    /// Returns [`NsbError::OutOfRange`] when `zenith` is non-finite or outside
    /// `[0, 90]` degrees.
    pub fn new(zenith: Degrees) -> Result<Self> {
        let value = zenith.value();
        if !value.is_finite() {
            return Err(NsbError::OutOfRange(format!(
                "direct-path zenith must be finite, got {value}"
            )));
        }
        if !(0.0..=MAX_ZENITH_DEG).contains(&value) {
            return Err(NsbError::OutOfRange(format!(
                "direct-path zenith must be in [0, {MAX_ZENITH_DEG}] degrees, got {value}"
            )));
        }
        Ok(Self { zenith })
    }

    /// Zenith distance in degrees.
    pub const fn zenith(self) -> Degrees {
        self.zenith
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_horizon_and_zenith() {
        assert!(DirectPathGeometry::new(Degrees::new(0.0)).is_ok());
        assert!(DirectPathGeometry::new(Degrees::new(90.0)).is_ok());
    }

    #[test]
    fn rejects_nan_negative_and_above_horizon() {
        assert!(DirectPathGeometry::new(Degrees::new(f64::NAN)).is_err());
        assert!(DirectPathGeometry::new(Degrees::new(-0.1)).is_err());
        assert!(DirectPathGeometry::new(Degrees::new(90.1)).is_err());
        assert!(DirectPathGeometry::new(Degrees::new(f64::INFINITY)).is_err());
    }
}
