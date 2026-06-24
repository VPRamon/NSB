//! Zodiacal-light component: sunlight scattered by interplanetary dust.
//!
//! # Physical background
//!
//! Zodiacal light is sunlight scattered by micron-sized dust particles
//! distributed throughout the inner Solar System (the "zodiacal cloud"). It
//! is strongest near the ecliptic plane and near the Sun, and it constitutes
//! one of the dominant optical sky-background contributions under dark, clear
//! conditions at observatories away from the Galactic plane.
//!
//! # Default model
//!
//! The default model is based on the Leinert et al. (1998) empirical
//! brightness table, which tabulates the zodiacal surface brightness in S10
//! units (10th-magnitude equivalent stars per square degree) as a function of
//! ecliptic latitude `β` and ecliptic longitude offset `λ − λ_sun`.
//!
//! The pipeline is:
//!
//! 1. Derive target ecliptic geometry from `(time, target)`.
//! 2. Look up the Leinert (1998) S10 brightness at `(β, Δλ)`.
//! 3. Scale the solar spectrum so its 500 nm value matches the S10 brightness.
//! 4. Apply Leinert wavelength reddening.
//! 5. Optionally apply Noll et al. (2012) atmospheric extinction.
//! 6. Convert energy radiance to photon radiance.
//! 7. Integrate over the 300–650 nm band.
//!
//! # Model separation: source vs propagation
//!
//! The implementation distinguishes two conceptual steps:
//!
//! - **Source model** (`leinert`, `geometry`, `spectrum`, `reddening`):
//!   computes what the zodiacal sky looks like above the atmosphere.
//! - **Atmospheric propagation** (`extinction`):
//!   attenuates the signal as it passes through the atmosphere.
//!
//! Use [`ZodiacalLight::compute_exoatmospheric`] for the celestial component
//! alone (no location required, no extinction applied). Use
//! [`ZodiacalLight::compute_observed`] or [`ZodiacalLight::compute`] to also
//! apply atmospheric extinction for a ground-based observer.
//!
//! # Extensibility
//!
//! - The brightness source model can be replaced via [`ZodiacalBrightnessModel`].
//!   Custom grids are supported through [`ZodiacalBrightnessGrid`].
//! - The extinction strategy is explicit via [`ZodiacalExtinction`].
//! - The solar spectrum can be replaced via
//!   [`ZodiacalLight::with_solar_spectrum`].
//!
//! # References
//!
//! - Leinert et al. (1998), *A&AS* 127, 1–99.
//! - Noll et al. (2012), *A&A* 543, A92.

pub(crate) mod extinction;
pub(crate) mod geometry;
pub(crate) mod leinert;
pub(crate) mod model;
pub(crate) mod output;
pub(crate) mod reddening;
pub(crate) mod spectrum;

pub use extinction::ZodiacalExtinction;
pub use model::{ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalLight};
pub use output::{ZodiacalOutputs, ZodiacalSpectrum};

#[cfg(test)]
mod tests;
