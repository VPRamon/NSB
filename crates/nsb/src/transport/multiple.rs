// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Experimental plane-parallel scalar multiple scattering, opt-in only.
//!
//! Horizontal vectors look toward sky sources; photon propagation is opposite.
//! The top boundary has only supplied celestial illumination. The black ground
//! supplies no upward light. Both hemispheres participate in internal transport.
//! Siderust owns all extinction/scattering coefficients; Optica owns the phases.
//! No surface reflection, emission, polarization, refraction or spherical Earth.
//!
//! This monochromatic product-ordinate solver is for smoothly varying skies.
//! Point samples of discontinuous HEALPix pixels do not establish conservative
//! HEALPix convergence or scientific admission of a Starlight product.

use super::single::{self, AerosolPhase, IncidentSky, SkyCoverage};
use crate::units::qtty::{Degrees, Nanometers, Radian, Radians};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradianNanometer as Radiance;
use crate::{NsbError, Result};
use siderust::atmosphere::LayeredAtmosphere;
use siderust::coordinates::spherical::direction::Horizontal;
use siderust::optica::phase::{PhaseFunction, RayleighPhaseFunction};
use std::f64::consts::TAU;

const WORK_LIMIT: usize = 250_000_000;
const MEMORY_LIMIT: usize = 16 * 1024 * 1024;

/// Bounded four-resolution configuration for smooth-sky spectral transport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultipleScatteringSettings {
    /// Gauss nodes per positive zenith-cosine hemisphere.
    pub mu_nodes: usize,
    /// Equally spaced azimuth nodes.
    pub azimuth_nodes: usize,
    /// Equal-height cells within each upstream homogeneous layer.
    pub cells_per_layer: usize,
    /// Maximum number of physical scattering events accumulated.
    pub max_orders: usize,
    /// Relative angular-grid refinement threshold.
    pub angular_tolerance: f64,
    /// Relative vertical-grid refinement threshold.
    pub vertical_tolerance: f64,
    /// Relative rigorously bounded scattering-order tail threshold.
    pub order_tolerance: f64,
    /// Absolute convergence threshold in spectral photon radiance.
    pub absolute_tolerance: Radiance,
}
impl Default for MultipleScatteringSettings {
    fn default() -> Self {
        Self {
            mu_nodes: 4,
            azimuth_nodes: 8,
            cells_per_layer: 8,
            max_orders: 48,
            angular_tolerance: 2e-3,
            vertical_tolerance: 2e-3,
            order_tolerance: 1e-5,
            absolute_tolerance: Radiance::new(1e-12),
        }
    }
}

/// Resolution change and scattering-order tail diagnostics are not physical
/// accuracy or observational uncertainty claims.
#[derive(Debug, Clone, PartialEq)]
pub struct MultipleScatteringConvergence {
    /// Final positive-cosine and azimuth orders, respectively.
    pub angular_nodes: [usize; 2],
    /// Final numerical subcells per Siderust optical layer.
    pub cells_per_layer: usize,
    /// Scattering orders included in the accepted result.
    pub orders: usize,
    /// Largest single/higher-order angular-only change.
    pub angular_change: Radiance,
    /// Largest single/higher-order vertical-only change.
    pub vertical_change: Radiance,
    /// Discrete angular operator Neumann-tail bound in radiance units.
    pub order_tail_bound: Radiance,
    /// Upper bound on the discrete scattering operator norm.
    pub contraction_bound: f64,
    /// Full accepted plan in conservative computational work units.
    pub planned_work: usize,
}
/// Immutable source-separated monochromatic ground-observer transport result.
#[derive(Debug, Clone)]
pub struct MultipleScattering {
    /// Exactly unscattered transmitted top-boundary radiance.
    pub direct: Radiance,
    /// Ground contribution from exactly one scattering event.
    pub single: Radiance,
    /// Sum of second and subsequent scattering-order contributions.
    pub higher_orders: Radiance,
    /// Sum of first and higher scattering orders.
    pub total_scattered: Radiance,
    /// Direct radiance plus all computed scattered radiance.
    pub total: Radiance,
    /// Physical/model/source uncertainty is not available.
    pub uncertainty: Option<Radiance>,
    /// Original top-of-atmosphere source identity and coverage.
    pub source: SkyCoverage,
    /// Original layered optical medium, as supplied by Siderust.
    pub atmosphere: LayeredAtmosphere,
    /// Original Optica aerosol scattering phase law.
    pub aerosol_phase: AerosolPhase,
    /// Requested vacuum wavelength.
    pub wavelength: Nanometers,
    /// Original observer viewing direction in local Horizontal coordinates.
    pub view: Horizontal,
    /// Validated numerical settings.
    pub settings: MultipleScatteringSettings,
    /// Numerical discretization and scattering-order diagnostics.
    pub convergence: MultipleScatteringConvergence,
}
impl MultipleScattering {
    /// Experimental model identity, not an empirical accuracy or admission claim.
    pub const MODEL: &'static str = "nsb-plane-parallel-scalar-multiple-sos-v0-experimental";
}

#[derive(Clone, Copy)]
struct Cell {
    depth: f64,
    extinction: f64,
    rayleigh: f64,
    aerosol: f64,
}
#[derive(Clone, Copy)]
struct Direction {
    mu: f64,
    vector: [f64; 3],
    weight: f64,
    incoming: f64,
}
#[derive(Clone, Copy)]
struct Solution {
    first: f64,
    higher: f64,
    orders: usize,
    bound: f64,
    contraction: f64,
}

fn invalid(s: &str) -> NsbError {
    NsbError::OutOfRange(s.into())
}
fn nonnegative(x: f64, what: &str) -> Result<f64> {
    if x.is_finite() && x >= 0.0 {
        Ok(x)
    } else {
        Err(invalid(&format!("{what} is negative or nonfinite")))
    }
}
fn product(xs: &[usize]) -> Result<usize> {
    xs.iter().try_fold(1usize, |x, y| {
        x.checked_mul(*y)
            .ok_or_else(|| invalid("work size overflow"))
    })
}
fn preflight(layers: usize, s: MultipleScatteringSettings) -> Result<usize> {
    if !(1..=16).contains(&layers)
        || !(2..=8).contains(&s.mu_nodes)
        || !(4..=16).contains(&s.azimuth_nodes)
        || !(2..=16).contains(&s.cells_per_layer)
        || !(2..=64).contains(&s.max_orders)
        || !s.angular_tolerance.is_finite()
        || !(0.0..=0.05).contains(&s.angular_tolerance)
        || s.angular_tolerance == 0.0
        || !s.vertical_tolerance.is_finite()
        || !(0.0..=0.05).contains(&s.vertical_tolerance)
        || s.vertical_tolerance == 0.0
        || !s.order_tolerance.is_finite()
        || !(0.0..=0.01).contains(&s.order_tolerance)
        || s.order_tolerance == 0.0
        || !s.absolute_tolerance.value().is_finite()
        || !(0.0..=1e-10).contains(&s.absolute_tolerance.value())
    {
        return Err(invalid("unsupported multiple-scattering configuration"));
    }
    let mut work = 0usize;
    for (a, z) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
        let n = product(&[2, s.mu_nodes, s.azimuth_nodes, a, a])?;
        let cells = product(&[layers, s.cells_per_layer, z])?;
        let square = product(&[n, n])?;
        work = work
            .checked_add(product(&[2, square, cells, s.max_orders + 1])?)
            .ok_or_else(|| invalid("full refinement work overflow"))?;
        let phase_bytes = product(&[2, square, std::mem::size_of::<f64>()])?;
        let field_bytes = product(&[5, n, cells, std::mem::size_of::<f64>()])?;
        if phase_bytes
            .checked_add(field_bytes)
            .is_none_or(|bytes| bytes > MEMORY_LIMIT)
        {
            return Err(invalid(
                "temporary multiple-scattering memory exceeds 16 MiB",
            ));
        }
    }
    if work > WORK_LIMIT {
        return Err(invalid(
            "multiple-scattering work exceeds 250 million units",
        ));
    }
    Ok(work)
}
fn column(atm: &LayeredAtmosphere, w: Nanometers, split: usize) -> Result<Vec<Cell>> {
    let edges = atm.edges();
    let mut cells = Vec::with_capacity((edges.len() - 1) * split);
    for edge in edges.windows(2) {
        let length = (edge[1] - edge[0]).value();
        if !length.is_finite() || length <= 0.0 {
            return Err(invalid("nonpositive upstream layer height"));
        }
        let o = atm
            .coefficients(w, edge[0])
            .map_err(|e| invalid(&format!("Siderust coefficients: {e}")))?;
        let tau = atm
            .vertical_optical_depth(w, edge[0], edge[1])
            .map_err(|e| invalid(&format!("Siderust optical depth: {e}")))?
            .extinction()
            .value();
        let ext = nonnegative(tau / length, "total extinction")?;
        let ray = nonnegative(o.rayleigh().sigma_s.value(), "Rayleigh coefficient")?;
        let aer = nonnegative(o.aerosol().sigma_s.value(), "aerosol coefficient")?;
        if ray + aer > ext * (1.0 + 1e-10) {
            return Err(invalid("upstream scattering exceeds total extinction"));
        }
        cells.extend(std::iter::repeat_n(
            Cell {
                depth: tau / split as f64,
                extinction: ext,
                rayleigh: ray,
                aerosol: aer,
            },
            split,
        ));
    }
    Ok(cells)
}
fn ordinates(
    sky: &impl IncidentSky,
    w: Nanometers,
    nmu: usize,
    nphi: usize,
) -> Result<Vec<Direction>> {
    let mut result = Vec::with_capacity(2 * nmu * nphi);
    for sign in [1.0, -1.0] {
        for (mu, weight) in single::quadrature::gauss(nmu)? {
            for p in 0..nphi {
                let az = TAU * p as f64 / nphi as f64;
                let incident = if sign > 0.0 {
                    nonnegative(
                        sky.radiance(
                            w,
                            Horizontal::new(
                                Degrees::new(mu.asin().to_degrees()),
                                Degrees::new(az.to_degrees()),
                            ),
                        )?
                        .value(),
                        "incident angular radiance",
                    )?
                } else {
                    0.0
                };
                let r = (1.0 - mu * mu).sqrt();
                result.push(Direction {
                    mu: sign * mu,
                    vector: [r * az.cos(), r * az.sin(), sign * mu],
                    weight: weight * TAU / nphi as f64,
                    incoming: incident,
                });
            }
        }
    }
    Ok(result)
}
fn phase_pair(w: Nanometers, p: &AerosolPhase, a: [f64; 3], b: [f64; 3]) -> Result<[f64; 2]> {
    let cos = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
    let angle = Radians::new(cos.acos());
    Ok([
        nonnegative(
            RayleighPhaseFunction.phase(w, angle).value(),
            "Rayleigh phase",
        )?,
        nonnegative(p.density(w, angle)?, "aerosol phase")?,
    ])
}
fn step(incoming: f64, source: f64, cell: Cell, distance: f64) -> f64 {
    let optical = cell.extinction * distance;
    let integrated = if cell.extinction > 0.0 {
        source / cell.extinction * -(-optical).exp_m1()
    } else {
        source * distance
    };
    incoming * (-optical).exp() + integrated
}
fn solution(
    atm: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &impl IncidentSky,
    w: Nanometers,
    view: Horizontal,
    s: MultipleScatteringSettings,
    resolution: (usize, usize),
) -> Result<Solution> {
    let (a, z) = resolution;
    let cells = column(atm, w, s.cells_per_layer * z)?;
    let dirs = ordinates(sky, w, s.mu_nodes * a, s.azimuth_nodes * a)?;
    let (n, m) = (dirs.len(), cells.len());
    let mut rp = vec![0.0; n * n];
    let mut ap = vec![0.0; n * n];
    let mut max_norm = 0.0f64;
    let mut min_norm = f64::INFINITY;
    let has_ray = cells.iter().any(|c| c.rayleigh > 0.0);
    let has_aer = cells.iter().any(|c| c.aerosol > 0.0);
    for (i, outgoing) in dirs.iter().enumerate() {
        let mut sums = [0.0; 2];
        for (j, incoming) in dirs.iter().enumerate() {
            let p = phase_pair(w, phase, outgoing.vector, incoming.vector)?;
            rp[i * n + j] = p[0] * incoming.weight;
            ap[i * n + j] = p[1] * incoming.weight;
            sums[0] += rp[i * n + j];
            sums[1] += ap[i * n + j];
        }
        for (enabled, norm) in [(has_ray, sums[0]), (has_aer, sums[1])] {
            if enabled {
                max_norm = max_norm.max(norm);
                min_norm = min_norm.min(norm);
            }
        }
    }
    if (has_ray || has_aer) && (min_norm < 0.9 || max_norm > 1.1) {
        return Err(invalid(
            "full-sphere scattering phase quadrature is under-resolved",
        ));
    }
    let omega = cells.iter().fold(0.0f64, |acc, c| {
        if c.extinction > 0.0 {
            acc.max((c.rayleigh + c.aerosol) / c.extinction)
        } else {
            acc
        }
    });
    let tau: f64 = cells.iter().map(|c| c.depth).sum();
    let min_mu = dirs.iter().fold(1.0f64, |v, d| v.min(d.mu.abs()));
    let contraction = omega * max_norm * -(-tau / min_mu).exp_m1();
    if !contraction.is_finite() || contraction >= 1.0 {
        return Err(invalid(
            "discrete scattering contraction is not established",
        ));
    }
    let mu = view.alt().to::<Radian>().sin();
    let az = view.az().to::<Radian>().value();
    let transverse = (1.0 - mu * mu).sqrt();
    let v = [transverse * az.cos(), transverse * az.sin(), mu];
    let mut vr = Vec::with_capacity(n);
    let mut va = Vec::with_capacity(n);
    for d in &dirs {
        let p = phase_pair(w, phase, v, d.vector)?;
        vr.push(p[0] * d.weight);
        va.push(p[1] * d.weight);
    }
    let view_norm = vr.iter().sum::<f64>().max(va.iter().sum::<f64>());
    let view_gain = omega * view_norm * -(-tau / mu).exp_m1();
    // Unscattered boundary field at finite-cell midpoints; upward order 0 = 0.
    let mut current = vec![0.0; m * n];
    for (i, d) in dirs.iter().enumerate() {
        if d.mu > 0.0 {
            let mut above = 0.0;
            for j in (0..m).rev() {
                current[j * n + i] = d.incoming * (-(above + cells[j].depth * 0.5) / d.mu).exp();
                above += cells[j].depth;
            }
        }
    }
    let mut next = vec![0.0; m * n];
    let mut sources = vec![0.0; m * n];
    let mut views = vec![0.0; m];
    let mut first = 0.0;
    let mut higher = 0.0;
    let mut amplitude = 0.0f64;
    for order in 1..=s.max_orders {
        for (j, cell) in cells.iter().enumerate() {
            for i in 0..n {
                let mut ray = 0.0;
                let mut aer = 0.0;
                for k in 0..n {
                    ray += rp[i * n + k] * current[j * n + k];
                    aer += ap[i * n + k] * current[j * n + k];
                }
                sources[j * n + i] =
                    nonnegative(cell.rayleigh * ray + cell.aerosol * aer, "diffuse source")?;
            }
            let mut ray = 0.0;
            let mut aer = 0.0;
            for k in 0..n {
                ray += vr[k] * current[j * n + k];
                aer += va[k] * current[j * n + k];
            }
            views[j] = nonnegative(
                cell.rayleigh * ray + cell.aerosol * aer,
                "ground-view source",
            )?;
        }
        for (i, dir) in dirs.iter().enumerate() {
            let mut incoming = 0.0;
            if dir.mu > 0.0 {
                for j in (0..m).rev() {
                    // Geometric cell length recovered from upstream tau/ext.
                    let distance = if cells[j].extinction > 0.0 {
                        cells[j].depth / cells[j].extinction / dir.mu
                    } else {
                        0.0
                    };
                    next[j * n + i] = nonnegative(
                        step(incoming, sources[j * n + i], cells[j], distance * 0.5),
                        "downward radiance",
                    )?;
                    incoming = step(incoming, sources[j * n + i], cells[j], distance);
                }
            } else {
                for j in 0..m {
                    let distance = if cells[j].extinction > 0.0 {
                        cells[j].depth / cells[j].extinction / -dir.mu
                    } else {
                        0.0
                    };
                    next[j * n + i] = nonnegative(
                        step(incoming, sources[j * n + i], cells[j], distance * 0.5),
                        "upward radiance",
                    )?;
                    incoming = step(incoming, sources[j * n + i], cells[j], distance);
                }
            }
        }
        let mut ground = 0.0;
        for j in (0..m).rev() {
            let distance = if cells[j].extinction > 0.0 {
                cells[j].depth / cells[j].extinction / mu
            } else {
                0.0
            };
            ground = nonnegative(
                step(ground, views[j], cells[j], distance),
                "ground diffuse radiance",
            )?;
        }
        if order == 1 {
            first = ground;
        } else {
            higher = nonnegative(higher + ground, "higher-order radiance")?;
        }
        let sup = next.iter().copied().fold(0.0f64, f64::max);
        amplitude = amplitude.max(sup);
        // The positive discrete operator has norm at most contraction.
        // Every omitted order is bounded by the geometric Neumann tail.
        let tail = (contraction.max(view_gain) / (1.0 - contraction)) * sup;
        if !tail.is_finite() {
            return Err(invalid("multiple-scattering residual overflow"));
        }
        if tail <= s.absolute_tolerance.value() + s.order_tolerance * amplitude.max(first + higher)
        {
            return Ok(Solution {
                first,
                higher,
                orders: order,
                bound: tail,
                contraction,
            });
        }
        std::mem::swap(&mut current, &mut next);
    }
    Err(NsbError::Unsupported(
        "multiple scattering did not converge within the order budget".into(),
    ))
}
fn agrees(a: f64, b: f64, rel: f64, absolute: f64) -> bool {
    (a - b).abs() <= absolute + rel * a.abs().max(b.abs())
}

/// Genuine successive-orders full-sphere scattering for a smooth spectral sky.
///
/// Computes four independent grid resolutions and fails unless angular,
/// vertical and scattering-order criteria all hold. No correction factor and
/// no double atmospheric extinction are introduced.
///
/// # Errors
///
/// Invalid medium/source/domain, numerical nonconvergence, quadrature aliasing,
/// arithmetic overflow, or resource exhaustion return explicit errors.
pub fn multiple_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &impl IncidentSky,
    wavelength: Nanometers,
    view: Horizontal,
    settings: MultipleScatteringSettings,
) -> Result<MultipleScattering> {
    let layers = atmosphere.edges().len().saturating_sub(1);
    let work = preflight(layers, settings)?;
    let (direct, source) = single::direct_illumination(atmosphere, phase, sky, wavelength, view)?;
    let base = solution(atmosphere, phase, sky, wavelength, view, settings, (1, 1))?;
    let angular = solution(atmosphere, phase, sky, wavelength, view, settings, (2, 1))?;
    let vertical = solution(atmosphere, phase, sky, wavelength, view, settings, (1, 2))?;
    let joint = solution(atmosphere, phase, sky, wavelength, view, settings, (2, 2))?;
    let absolute = settings.absolute_tolerance.value();
    for (a, b, rel) in [
        (base.first, angular.first, settings.angular_tolerance),
        (base.higher, angular.higher, settings.angular_tolerance),
        (vertical.first, joint.first, settings.angular_tolerance),
        (vertical.higher, joint.higher, settings.angular_tolerance),
        (base.first, vertical.first, settings.vertical_tolerance),
        (base.higher, vertical.higher, settings.vertical_tolerance),
        (angular.first, joint.first, settings.vertical_tolerance),
        (angular.higher, joint.higher, settings.vertical_tolerance),
    ] {
        if !agrees(a, b, rel, absolute) {
            return Err(NsbError::Unsupported(
                "multiple scattering angular or vertical grid did not converge".into(),
            ));
        }
    }
    let scattered = nonnegative(joint.first + joint.higher, "total scattering")?;
    let total = nonnegative(direct + scattered, "total ground radiance")?;
    let angular_change = (joint.first - vertical.first)
        .abs()
        .max((joint.higher - vertical.higher).abs());
    let vertical_change = (joint.first - angular.first)
        .abs()
        .max((joint.higher - angular.higher).abs());
    Ok(MultipleScattering {
        direct: Radiance::new(direct),
        single: Radiance::new(joint.first),
        higher_orders: Radiance::new(joint.higher),
        total_scattered: Radiance::new(scattered),
        total: Radiance::new(total),
        uncertainty: None,
        source,
        atmosphere: atmosphere.clone(),
        aerosol_phase: phase.clone(),
        wavelength,
        view,
        settings,
        convergence: MultipleScatteringConvergence {
            angular_nodes: [settings.mu_nodes * 2, settings.azimuth_nodes * 2],
            cells_per_layer: settings.cells_per_layer * 2,
            orders: joint.orders,
            angular_change: Radiance::new(angular_change),
            vertical_change: Radiance::new(vertical_change),
            order_tail_bound: Radiance::new(joint.bound),
            contraction_bound: joint.contraction,
            planned_work: work,
        },
    })
}

#[cfg(test)]
mod tests;
