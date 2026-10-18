// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Experimental scalar, elastic single scattering in a finite plane-parallel column.
//!
//! Directions point toward the sky in one local Horizontal frame (north clockwise
//! azimuth); both photon propagation vectors are their negatives, so the phase
//! cosine is their dot product. The lower boundary is black and nonemitting.
//! Observer height is the first atmospheric edge. No refraction, polarization,
//! surface reflection, internal emission or higher scattering orders are modeled.
//! This reference is opt-in and does not participate in `NsbEvaluator` totals.
//!
//! The domain is 360–570 nm and geometric output zenith 0–80 degrees. This is an
//! implementation domain, not empirical scientific admission. Resolution changes
//! are diagnostics, not rigorous error bounds or physical uncertainty estimates.

mod budget;
mod quadrature;
/// Native-bin spectral Starlight illumination and transport (explicit opt-in).
pub mod starlight;
use super::RadianceOrigin;
use crate::units::qtty::{Degrees, Meters, Nanometers, Radian, Radians};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradianNanometer as Radiance;
use crate::{NsbError, Result};
use siderust::atmosphere::{LayeredAtmosphere, NormalizedPhaseTable};
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::{
    DoubleHenyeyGreensteinPhaseFunction, HenyeyGreensteinPhaseFunction, PhaseFunction,
    RayleighPhaseFunction,
};
use std::f64::consts::PI;

/// Source identity and declared spectral/complete visible-hemisphere coverage.
/// The identity should include the synthetic model version or dataset checksum,
/// epoch and site when applicable. No implicit angular mask is supported.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyCoverage {
    /// Source provenance, supplied by the source implementation.
    pub identity: String,
    /// Inclusive wavelength limits.
    pub wavelengths: [Nanometers; 2],
    /// True only when every direction with positive altitude is available.
    pub full_hemisphere: bool,
    /// Physical origin, which must be top of atmosphere.
    pub origin: RadianceOrigin,
}

/// Wavelength-resolved celestial illumination, deterministic and without side effects.
///
/// Implementations must return an error for missing data. The exact direct-view
/// direction is queried separately from the quadrature nodes. Band-integrated
/// maps cannot implement this contract by inventing a spectral density.
pub trait IncidentSky {
    /// Describe the actual source and its coverage.
    fn coverage(&self) -> SkyCoverage;
    /// Spectral photon radiance at a sky direction in the common local frame.
    fn radiance(&self, wavelength: Nanometers, direction: Horizontal) -> Result<Radiance>;
}

/// Validated upstream aerosol phase. All densities integrate to unity over 4π.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AerosolPhase {
    /// Analytic HG, including isotropic g=0.
    HenyeyGreenstein(HenyeyGreensteinPhaseFunction),
    /// Two validated HG lobes.
    DoubleHenyeyGreenstein(DoubleHenyeyGreensteinPhaseFunction),
    /// Strict upstream normalized table; no clamping or renormalization.
    Tabulated(Box<NormalizedPhaseTable>),
}
impl AerosolPhase {
    fn density(&self, w: Nanometers, theta: Radians) -> Result<f64> {
        let p = match self {
            Self::HenyeyGreenstein(p) => p.phase(w, theta),
            Self::DoubleHenyeyGreenstein(p) => p.phase(w, theta),
            Self::Tabulated(p) => p.try_phase(w, theta).map_err(upstream)?,
        };
        // Named marker conversion: Optica ScatteringFactor represents sr^-1.
        // quadrature::phase_solid_angle converts it before multiplying dΩ.
        finite_nonnegative(p.value(), "aerosol phase density")
    }
}

/// Bounded deterministic resolution and successive-resolution tolerances.
///
/// The entire requested refinement plan must also fit the private computational
/// budget (250 million conservative work units; 8 MiB of quadrature buffers).
/// Admission uses layer count before any source query, even if a request might
/// converge early or contain no scattering. See the maintained solver notes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolverSettings {
    /// Initial Gauss–Legendre order on 0 < mu < 1.
    pub mu_nodes: usize,
    /// Initial equally spaced azimuth count.
    pub azimuth_nodes: usize,
    /// Initial Gauss–Legendre order within each atmospheric layer.
    pub path_nodes: usize,
    /// Maximum joint doublings (1–5); final orders are also bounded.
    pub refinements: usize,
    /// Relative resolution-change threshold, strictly positive and at most 1e-4.
    pub relative_tolerance: f64,
    /// Absolute threshold for near-zero radiance, at most 1e-12 in output units.
    pub absolute_tolerance: Radiance,
}
impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            mu_nodes: 16,
            azimuth_nodes: 32,
            path_nodes: 16,
            refinements: 4,
            relative_tolerance: 1e-4,
            absolute_tolerance: Radiance::new(1e-12),
        }
    }
}
impl SolverSettings {
    fn validate(self) -> Result<()> {
        let factor = 1usize.checked_shl(self.refinements as u32);
        if !(1..=5).contains(&self.refinements)
            || !(2..=512).contains(&self.mu_nodes)
            || !(4..=1024).contains(&self.azimuth_nodes)
            || !(2..=512).contains(&self.path_nodes)
            || factor.is_none_or(|f| {
                self.mu_nodes * f > 512
                    || self.azimuth_nodes * f > 1024
                    || self.path_nodes * f > 512
            })
            || !self.relative_tolerance.is_finite()
            || !(0.0..=1e-4).contains(&self.relative_tolerance)
            || self.relative_tolerance == 0.0
            || !self.absolute_tolerance.value().is_finite()
            || !(0.0..=1e-12).contains(&self.absolute_tolerance.value())
        {
            return Err(invalid(
                "single-scattering solver settings exceed supported bounds",
            ));
        }
        Ok(())
    }
    fn agrees(self, a: f64, b: f64) -> bool {
        (a - b).abs()
            <= self.absolute_tolerance.value() + self.relative_tolerance * a.abs().max(b.abs())
    }
}

/// Measured changes at the accepted final resolution, in spectral photon units.
#[derive(Debug, Clone, PartialEq)]
pub struct Convergence {
    /// Absolute change from refining angles alone at the previous path order.
    pub angular_change: Radiance,
    /// Absolute change from refining path alone at the previous angular order.
    pub path_change: Radiance,
    /// Absolute change from jointly refining both.
    pub joint_change: Radiance,
    /// Accepted mu, azimuth and per-layer path orders, respectively.
    pub orders: [usize; 3],
}

/// First-order result, with an owned snapshot of its physical/numerical context.
#[derive(Debug, Clone)]
pub struct SingleScattering {
    /// Direct celestial radiance after total extinction.
    pub direct: Radiance,
    /// Exactly one scattering event, already attenuated along both paths.
    pub single: Radiance,
    /// Always None: uncomputed, not a zero estimate.
    pub higher_orders: Option<Radiance>,
    /// Independent resolution-change diagnostics.
    pub convergence: Convergence,
    /// Actual immutable upstream medium, including constituent coefficients/profile.
    pub atmosphere: LayeredAtmosphere,
    /// Actual upstream aerosol phase configuration.
    pub aerosol_phase: AerosolPhase,
    /// Source provenance and declared coverage.
    pub source: SkyCoverage,
    /// Exact requested wavelength.
    pub wavelength: Nanometers,
    /// Exact geometric local viewing direction.
    pub view: Horizontal,
    /// Requested tolerances and refinement budget.
    pub settings: SolverSettings,
}
impl SingleScattering {
    /// Model identity. Physical/source/atmospheric uncertainties remain unknown.
    pub const MODEL: &'static str = "nsb-plane-parallel-scalar-single-v1-experimental";
}

/// Compute direct and single-scattered spectral photon radiance.
///
/// Integrates `I_TOA exp(-tau_above/mu_in) P beta_sca` over the full incoming
/// hemisphere, then `exp(-tau_below/mu_out) dz/mu_out` along the viewing path.
/// Both optical depths come from the same upstream exact partial-column API.
/// Rayleigh and aerosol sources use their own scattering coefficients and phases.
///
/// # Errors
/// Rejects unsupported domain/origin/coverage, invalid illumination, upstream
/// medium/phase errors, arithmetic overflow, excessive computational work and
/// unresolved convergence. No partial result is returned on failure. The source must be deterministic; quadrature
/// cannot certify arbitrary unresolved angular structure in caller illumination.
pub fn single_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &impl IncidentSky,
    wavelength: Nanometers,
    view: Horizontal,
    settings: SolverSettings,
) -> Result<SingleScattering> {
    let layers = atmosphere.edges().len() - 1;
    budget::estimate(settings, layers)?.enforce(layers, settings)?;
    let coverage = sky.coverage();
    let [lo, hi] = coverage.wavelengths.map(|v| v.value());
    let w = wavelength.value();
    if !w.is_finite() || !(360.0..=570.0).contains(&w) {
        return Err(invalid(
            "single scattering requires wavelength in 360–570 nm",
        ));
    }
    if !view.alt().value().is_finite()
        || !(10.0..=90.0).contains(&view.alt().value())
        || !view.az().value().is_finite()
    {
        return Err(invalid(
            "single scattering requires geometric zenith in 0–80 degrees",
        ));
    }
    if coverage.origin != RadianceOrigin::TopOfAtmosphere {
        return Err(NsbError::Unsupported(
            "single scattering requires TOA celestial illumination".into(),
        ));
    }
    if !coverage.full_hemisphere
        || coverage.identity.trim().is_empty()
        || !lo.is_finite()
        || !hi.is_finite()
        || lo <= 0.0
        || hi < lo
        || w < lo
        || w > hi
    {
        return Err(invalid(
            "source must declare spectral coverage and the complete visible hemisphere",
        ));
    }
    // Validate table spectral coverage even in vacuum or an absorbing medium.
    phase.density(wavelength, Radians::new(0.0))?;
    phase.density(wavelength, Radians::new(PI))?;
    let edges = atmosphere.edges();
    let tau = atmosphere
        .vertical_optical_depth(wavelength, edges[0], edges[edges.len() - 1])
        .map_err(upstream)?;
    let mu_out = view.alt().to::<Radian>().sin();
    let direct = finite_nonnegative(
        sky.radiance(wavelength, view)?.value(),
        "direct-view illumination",
    )? * (-tau.extinction().value() / mu_out).exp();
    let kernel = quadrature::Kernel {
        atmosphere,
        phase,
        sky,
        wavelength,
        view,
    };
    let mut orders = [
        settings.mu_nodes,
        settings.azimuth_nodes,
        settings.path_nodes,
    ];
    let mut coarse = kernel.integrate(orders)?;
    for _ in 0..settings.refinements {
        let angular = kernel.integrate([orders[0] * 2, orders[1] * 2, orders[2]])?;
        let path = kernel.integrate([orders[0], orders[1], orders[2] * 2])?;
        orders = orders.map(|n| n * 2);
        let joint = kernel.integrate(orders)?;
        // Also compare each mixed-resolution result against the joint result,
        // preventing cancellation between independent angular/path changes.
        if [
            (angular, coarse),
            (path, coarse),
            (joint, coarse),
            (joint, angular),
            (joint, path),
        ]
        .iter()
        .all(|&(a, b)| settings.agrees(a, b))
        {
            return Ok(SingleScattering {
                direct: Radiance::new(direct),
                single: Radiance::new(joint),
                higher_orders: None,
                convergence: Convergence {
                    angular_change: Radiance::new((angular - coarse).abs()),
                    path_change: Radiance::new((path - coarse).abs()),
                    joint_change: Radiance::new((joint - coarse).abs()),
                    orders,
                },
                atmosphere: atmosphere.clone(),
                aerosol_phase: phase.clone(),
                source: coverage,
                wavelength,
                view,
                settings,
            });
        }
        coarse = joint;
    }
    Err(NsbError::Unsupported(format!("single-scattering convergence failed at orders {orders:?}; tolerances relative={}, absolute={}", settings.relative_tolerance, settings.absolute_tolerance.value())))
}

fn invalid(message: &str) -> NsbError {
    NsbError::OutOfRange(message.into())
}
fn upstream(e: impl std::fmt::Display) -> NsbError {
    invalid(&format!("single-scattering optics: {e}"))
}
fn finite_nonnegative(v: f64, context: &str) -> Result<f64> {
    if !v.is_finite() || v < 0.0 {
        Err(invalid(&format!(
            "{context} must be finite and nonnegative"
        )))
    } else {
        Ok(v)
    }
}

#[cfg(test)]
mod tests;
