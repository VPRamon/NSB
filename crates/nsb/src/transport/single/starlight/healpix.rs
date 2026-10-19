// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Equal-area RING subdivision of the declared piecewise-constant sky. Sampling
//! the kernel inside parent pixels does not invent subpixel stellar photometry.
use super::*;
use crate::units::qtty::Steradians;
use siderust::healpix::{HealpixIndex, Nside};
use std::mem::size_of;

/// Bounded angular refinement for HEALPix illumination. Path integration is
/// layer-exact under Siderust's homogeneous-layer model, independent of angles.
#[derive(Debug, Clone, Copy)]
pub struct HealpixSettings {
    /// Initial linear subdivision of native pixels (power of two, 1–64).
    pub subdivision: u32,
    /// Maximum angular doublings (2–10). The entire plan is charged up front.
    pub refinements: usize,
    /// Strictly positive relative successive-change tolerance, at most 1e-4.
    pub relative_tolerance: f64,
    /// Absolute spectral-radiance change tolerance, at most 1e-12.
    pub absolute_tolerance: Radiance,
}
impl Default for HealpixSettings {
    fn default() -> Self {
        Self {
            subdivision: 1,
            refinements: 2,
            relative_tolerance: 1e-4,
            absolute_tolerance: Radiance::new(1e-12),
        }
    }
}

/// Accepted pixel-kernel resolution diagnostics, not a rigorous error bound.
#[derive(Debug, Clone)]
pub struct HealpixConvergence {
    /// Accepted RING integration-grid NSIDE, preserving original parent means.
    pub nside: u32,
    /// Latest absolute change in spectral radiance.
    pub angular_change: Radiance,
    /// Previous independent angular doubling's absolute change. Both must pass.
    pub previous_change: Radiance,
    /// Equal-area antipodal-pair hemisphere measure; no horizon altitude cutoff.
    pub visible_solid_angle: Steradians,
    /// Whole-sphere phase integrals (Rayleigh, aerosol). None denotes an absent
    /// scattering constituent; active constituents must resolve unit integral.
    pub phase_normalization: [Option<f64>; 2],
}

fn checked(v: Option<usize>) -> Result<usize> {
    v.ok_or_else(|| invalid("HEALPix quadrature work estimate overflow"))
}
fn initial(sky: &StarlightSky<'_>, s: HealpixSettings) -> Result<u32> {
    if !s.subdivision.is_power_of_two()
        || s.subdivision > 64
        || !(2..=10).contains(&s.refinements)
        || !s.relative_tolerance.is_finite()
        || !(0.0..=1e-4).contains(&s.relative_tolerance)
        || s.relative_tolerance == 0.0
        || !s.absolute_tolerance.is_finite()
        || !(0.0..=1e-12).contains(&s.absolute_tolerance.value())
    {
        return Err(invalid("invalid HEALPix angular settings"));
    }
    sky.map
        .nside()
        .checked_mul(s.subdivision)
        .ok_or_else(|| invalid("HEALPix subdivision overflow"))
}

/// Each level visits one center/rotation/parent lookup per antipodal pair, one
/// bin read, two phase densities and L exact layer responses. These are seven
/// named angular operations plus L layer operations (conservative, not CPU
/// instructions). Three endpoint depth queries and coefficient lookup per
/// layer each visit at most L layers. Buffers hold only the layer responses;
/// no whole-sky or subpixel arrays are allocated.
pub(super) fn estimate(
    sky: &StarlightSky<'_>,
    layers: usize,
    s: HealpixSettings,
    phase: &AerosolPhase,
) -> Result<(usize, usize)> {
    let mut n = initial(sky, s)?;
    let bytes = checked(
        layers
            .checked_mul(size_of::<quadrature::LayerResponse>())
            .and_then(|n| {
                n.checked_add(64 * size_of::<(f64, f64)>() + 11 * size_of::<PhaseAudit>())
            }),
    )?;
    if bytes > budget::MAX_TEMPORARY_BYTES {
        return Err(invalid("HEALPix layer buffers exceed 8 MiB"));
    }
    let prep = checked(
        layers
            .checked_mul(layers)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| v.checked_add(layers)),
    )?;
    let mut work = prep;
    let mut pairs_sum = 0_usize;
    for i in 0..=s.refinements {
        let grid = HealpixGrid::ring(Nside::new(n).map_err(upstream)?).map_err(upstream)?;
        let pairs = usize::try_from(grid.npix() / 2).map_err(upstream)?;
        pairs_sum = checked(pairs_sum.checked_add(pairs))?;
        let visits = checked(pairs.checked_mul(checked(layers.checked_add(7))?))?;
        work = checked(work.checked_add(visits))?;
        if work > budget::MAX_WORK {
            return Err(invalid(
                "HEALPix configured angular plan exceeds 250 million work units",
            ));
        }
        if i < s.refinements {
            n = n
                .checked_mul(2)
                .ok_or_else(|| invalid("HEALPix refinement overflow"))?;
        }
    }
    let audit_work = checked(pairs_sum.checked_mul(6))?;
    if wavelength_independent(phase) {
        Ok((work, audit_work))
    } else {
        let total = checked(work.checked_add(audit_work))?;
        if total > budget::MAX_WORK {
            return Err(invalid(
                "HEALPix angular and phase audit plan exceeds 250 million work units",
            ));
        }
        Ok((total, 0))
    }
}

pub(super) fn wavelength_independent(phase: &AerosolPhase) -> bool {
    // Optica's analytic HG and double-HG phase implementations ignore wavelength.
    // Strict tables may vary spectrally and are audited independently per node.
    matches!(
        phase,
        AerosolPhase::HenyeyGreenstein(_) | AerosolPhase::DoubleHenyeyGreenstein(_)
    )
}

pub(super) struct PhaseAudit {
    nside: u32,
    normalization: [f64; 2],
}

pub(super) fn phase_audits(
    sky: &StarlightSky<'_>,
    kernel: &quadrature::Kernel<'_, StarlightSky<'_>>,
    s: HealpixSettings,
) -> Result<Vec<PhaseAudit>> {
    let out = kernel.view.to_cartesian();
    let mut n = initial(sky, s)?;
    let mut values = Vec::with_capacity(s.refinements + 1);
    for i in 0..=s.refinements {
        let mut normalization = [0.; 2];
        visit_hemisphere(sky, n, |_, local, area| {
            let cosine = local.x() * out.x() + local.y() * out.y() + local.z() * out.z();
            let pair = kernel.phase_pair(local.z(), cosine, area)?;
            for j in 0..2 {
                normalization[j] += pair[j];
            }
            Ok(())
        })?;
        values.push(PhaseAudit {
            nside: n,
            normalization,
        });
        if i < s.refinements {
            n = n
                .checked_mul(2)
                .ok_or_else(|| invalid("HEALPix refinement overflow"))?;
        }
    }
    Ok(values)
}

pub(super) fn solve(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    w: Nanometers,
    view: Horizontal,
    s: HealpixSettings,
    shared_audits: Option<&[PhaseAudit]>,
) -> Result<(f64, f64, HealpixConvergence)> {
    let (direct, _) = direct_illumination(atmosphere, phase, sky, w, view)?;
    let kernel = quadrature::Kernel {
        atmosphere,
        phase,
        sky,
        wavelength: w,
        view,
    };
    let layers = kernel.layer_responses()?;
    let active =
        std::array::from_fn::<_, 2, _>(|j| layers.iter().any(|l| l.scattering_channels()[j]));
    let owned;
    let audits = if let Some(a) = shared_audits {
        a
    } else {
        owned = phase_audits(sky, &kernel, s)?;
        &owned
    };
    let first = audits
        .first()
        .ok_or_else(|| invalid("missing HEALPix phase audit"))?;
    let mut previous = integrate(sky, &kernel, &layers, first.nside)?;
    let mut previous_change: Option<(f64, f64)> = None;
    let agrees = |a: f64, b: f64| {
        (a - b).abs() <= s.absolute_tolerance.value() + s.relative_tolerance * a.abs().max(b.abs())
    };
    for audit in audits.iter().skip(1) {
        let current = integrate(sky, &kernel, &layers, audit.nside)?;
        let change = (current - previous).abs();
        let normalized = (0..2)
            .all(|j| !active[j] || (audit.normalization[j] - 1.0).abs() <= s.relative_tolerance);
        if normalized
            && agrees(current, previous)
            && previous_change.is_some_and(|(a, b)| agrees(a, b))
        {
            if let Some((a, b)) = previous_change {
                return Ok((
                    direct,
                    current,
                    HealpixConvergence {
                        nside: audit.nside,
                        angular_change: Radiance::new(change),
                        previous_change: Radiance::new((a - b).abs()),
                        visible_solid_angle: Steradians::new(2.0 * PI),
                        phase_normalization: std::array::from_fn(|j| {
                            active[j].then_some(audit.normalization[j])
                        }),
                    },
                ));
            }
        }
        previous_change = Some((current, previous));
        previous = current;
    }
    Err(invalid(&format!(
        "HEALPix angular/phase convergence failed at NSIDE {}; relative={}, absolute={}",
        audits.last().map_or(first.nside, |v| v.nside),
        s.relative_tolerance,
        s.absolute_tolerance.value()
    )))
}

fn integrate(
    sky: &StarlightSky<'_>,
    kernel: &quadrature::Kernel<'_, StarlightSky<'_>>,
    layers: &[quadrature::LayerResponse],
    n: u32,
) -> Result<f64> {
    let bin = sky
        .map
        .bin_edges_nm()
        .iter()
        .position(|&e| f64::from(e) == sky.band[0].value())
        .ok_or_else(|| invalid("missing HEALPix source bin"))?;
    let width = (sky.band[1] - sky.band[0]).value();
    let out = kernel.view.to_cartesian();
    let mut total = 0.;
    visit_hemisphere(sky, n, |parent_gal, local, area| {
        let parent = sky
            .map
            .grid()
            .direction_to_pixel(parent_gal)
            .map_err(upstream)?;
        let incident = sky.map.spectral_bin_at_index(parent, bin)?.value() / width;
        if incident != 0.0 {
            let cosine = local.x() * out.x() + local.y() * out.y() + local.z() * out.z();
            let sample = kernel.weighted_sample(local.z(), cosine, incident, area)?;
            for layer in layers {
                total += layer.integrate(&sample);
            }
        }
        Ok(())
    })?;
    finite_nonnegative(total, "HEALPix single-scattered radiance")
}

/// RING's northern half plus half its equatorial ring is an antipodal
/// transversal. Select the positive local-mu member of each pair. Boundary
/// nodes have mu=0 and zero finite-column path response, not an altitude cutoff.
fn visit_hemisphere(
    sky: &StarlightSky<'_>,
    n: u32,
    mut visit: impl FnMut(
        Direction<Galactic>,
        Direction<siderust::coordinates::frames::Horizontal>,
        Steradians,
    ) -> Result<()>,
) -> Result<()> {
    let grid = HealpixGrid::ring(Nside::new(n).map_err(upstream)?).map_err(upstream)?;
    let area = Steradians::new(grid.pixel_area_sr());
    for i in 0..grid.npix() / 2 {
        let gal = grid
            .pixel_center::<Galactic>(HealpixIndex::new(i))
            .map_err(upstream)?;
        let v: [f64; 3] = std::array::from_fn(|j| {
            sky.basis[j]
                .iter()
                .zip(gal.as_array())
                .map(|(a, b)| a * b)
                .sum()
        });
        let local: Direction<siderust::coordinates::frames::Horizontal> =
            Direction::try_new(v[0], v[1], v[2])
                .ok_or_else(|| invalid("invalid inverse Starlight rotation"))?;
        let sign = if local.z() < 0.0 { -1.0 } else { 1.0 };
        visit(
            Direction::new_unchecked(sign * gal.x(), sign * gal.y(), sign * gal.z()),
            Direction::new_unchecked(sign * local.x(), sign * local.y(), sign * local.z()),
            area,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
