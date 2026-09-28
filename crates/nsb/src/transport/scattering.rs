//! Scaffolding for single in-scattering.
//!
//! The public contract separates direct extinction from scattered radiance:
//!
//! ```text
//! radiance arriving from direction A
//!     ↓
//! atmospheric scattering
//!     ↓
//! radiance observed in direction B
//! ```
//!
//! Future work may add Rayleigh / Mie phase-function mixtures, HEALPix
//! hemisphere sampling, and LUT kernels. This foundation exposes geometry and
//! status types plus a Rayleigh phase-function helper that reuses Siderust,
//! without shipping an incomplete all-sky scattering engine.

use qtty::angular::Radians;
use siderust::atmosphere::{rayleigh_phase, ScatteringFactor};
use siderust::qtty::Quantity;

use super::geometry::ScatteringGeometry;

/// Status of a scattered-path evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScatteringPathStatus {
    /// Single in-scattering is not implemented in this release.
    NotImplemented,
}

impl ScatteringPathStatus {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotImplemented => "not-implemented",
        }
    }
}

/// Detail for an unimplemented scattered path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SingleScatteringNotImplemented {
    /// Human-readable explanation.
    pub message: &'static str,
}

/// Result of requesting a scattered atmospheric path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ScatteredPath {
    /// Implementation status.
    pub status: ScatteringPathStatus,
    /// Detail when [`ScatteringPathStatus::NotImplemented`].
    pub detail: SingleScatteringNotImplemented,
}

/// Rayleigh phase-function value from Siderust.
///
/// Provided so callers and tests can exercise the scattering extension seam
/// without NSB re-implementing the phase function. Normalisation and angular
/// limits follow Siderust / Bodhaine conventions.
pub fn rayleigh_phase_value(scattering_angle: Radians) -> Quantity<ScatteringFactor> {
    rayleigh_phase(scattering_angle)
}

/// Placeholder documentation helper: geometry is accepted so future solvers
/// can share the same request shape.
#[allow(dead_code)] // Public scaffolding; used once scattered solvers land.
pub(crate) fn scattering_geometry_is_finite(geometry: ScatteringGeometry) -> bool {
    geometry.source_zenith.value().is_finite()
        && geometry.observer_zenith.value().is_finite()
        && geometry.scattering_angle.value().is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::angular::Degrees;
    use std::f64::consts::PI;

    #[test]
    fn rayleigh_phase_is_finite_and_symmetric() {
        let forward = rayleigh_phase_value(Radians::new(0.0)).value();
        let backward = rayleigh_phase_value(Radians::new(PI)).value();
        let side = rayleigh_phase_value(Radians::new(PI / 2.0)).value();
        assert!(forward.is_finite() && backward.is_finite() && side.is_finite());
        assert!((forward - backward).abs() < 1e-14);
        assert!(forward > side);
        assert!(forward > 0.0 && side > 0.0);
    }

    #[test]
    fn scattering_geometry_scaffold_accepts_finite_angles() {
        let geometry =
            ScatteringGeometry::new(Degrees::new(30.0), Degrees::new(20.0), Radians::new(0.4));
        assert!(scattering_geometry_is_finite(geometry));
    }
}
