//! Line-of-sight geometry for direct and scattered transport paths.

use qtty::angular::{Degrees, Radians};

/// Geometry for a celestial direct-transmission path.
///
/// For the first foundation release the observer line of sight is the only
/// geometric input: Beer–Lambert extinction depends on airmass at the source
/// zenith distance. Source/observer direction pairs for in-scattering live in
/// [`ScatteringGeometry`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DirectPathGeometry {
    /// Zenith distance of the source / line of sight (degrees).
    pub zenith: Degrees,
}

impl DirectPathGeometry {
    /// Construct a direct-path geometry from zenith distance.
    pub const fn new(zenith: Degrees) -> Self {
        Self { zenith }
    }
}

/// Geometry for a future single in-scattering path.
///
/// ```text
/// radiance from source direction
///     ↓
/// atmospheric scattering
///     ↓
/// radiance observed along the line of sight
/// ```
///
/// This type is part of the public extension contract. Evaluating a scattered
/// path still returns [`super::ScatteringPathStatus::NotImplemented`] until a
/// validated single-scattering solver lands.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ScatteringGeometry {
    /// Zenith distance of the illuminating source (degrees).
    pub source_zenith: Degrees,
    /// Zenith distance of the observer line of sight (degrees).
    pub observer_zenith: Degrees,
    /// Scattering angle between source and observer directions (radians).
    pub scattering_angle: Radians,
}

impl ScatteringGeometry {
    /// Construct a scattering geometry from typed angles.
    pub const fn new(
        source_zenith: Degrees,
        observer_zenith: Degrees,
        scattering_angle: Radians,
    ) -> Self {
        Self {
            source_zenith,
            observer_zenith,
            scattering_angle,
        }
    }
}
