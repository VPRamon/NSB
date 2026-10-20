// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Native-bin transport through a separately refined equal-area boundary grid.
//! Parent pixels remain constant radiance cells. Subdivision integrates their
//! transport kernels; it does not infer unmeasured subpixel stellar structure.
use super::*;
use crate::components::starlight::StarlightProvenance;
use crate::transport::single::starlight::{
    HealpixSettings, StarlightGeometry, StarlightSky, WavelengthSettings, WithinBinSpectrum,
};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandRadiance;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};

// A complete bin includes at least six monochromatic evaluations and three
// boundary resolutions. It has a distinct bounded plan from a single wavelength.
mod projection;

const BAND_WORK_LIMIT: usize = 4_000_000_000;
// The opt-in Fourier plan counts complex-transform scalar arithmetic explicitly;
// this is a separate contract, not a relaxation of the streamed pairwise guard.
const PROJECTED_BAND_WORK_LIMIT: usize = 32_000_000_000;

/// Experimental complete native-bin multiple scattering and retained provenance.
#[derive(Debug, Clone)]
pub struct StarlightMultipleScattering {
    /// Complete original native bin, with no extrapolation.
    pub band: [Nanometers; 2],
    /// Exact view's unattenuated containing-parent-pixel integral.
    pub toa: BandRadiance,
    /// Integrated unscattered radiance.
    pub direct: BandRadiance,
    /// Integrated exactly once-scattered radiance.
    pub single: BandRadiance,
    /// Integrated second and subsequent scattering orders.
    pub higher_orders: BandRadiance,
    /// Single plus higher orders.
    pub total_scattered: BandRadiance,
    /// Direct plus all computed scattered orders.
    pub total: BandRadiance,
    /// Physical/source uncertainty is unavailable.
    pub uncertainty: Option<BandRadiance>,
    /// Original checksum-bearing source identity; no admission change.
    pub source: SkyCoverage,
    /// Original dataset provenance.
    pub provenance: StarlightProvenance,
    /// Original Galactic HEALPix grid and ordering.
    pub grid: HealpixGrid,
    /// Actual observer and explicit TT/UT1 epochs.
    pub geometry: StarlightGeometry,
    /// Explicit within-bin density assumption.
    pub assumption: WithinBinSpectrum,
    /// Requested independent wavelength refinement.
    pub wavelength_settings: WavelengthSettings,
    /// Requested independent boundary-cell refinement.
    pub boundary_settings: HealpixSettings,
    /// Accepted wavelength quadrature order.
    pub wavelength_order: usize,
    /// Direct/single/higher changes from the last wavelength doubling.
    pub wavelength_change: [BandRadiance; 3],
    /// Accepted per-wavelength solutions with atmosphere, phase, view and settings.
    pub samples: Vec<MultipleScattering>,
    /// Accepted boundary NSIDE at each wavelength.
    pub boundary_nside: Vec<u32>,
    /// Latest boundary changes in single/higher spectral radiance, per wavelength.
    pub boundary_change: Vec<[Radiance; 2]>,
    /// Entire worst-case spectral, boundary and diffuse plan, charged up front.
    pub planned_work: usize,
}

fn grid(nside: u32) -> Result<HealpixGrid> {
    HealpixGrid::new(
        Nside::new(nside).map_err(|e| invalid(&e.to_string()))?,
        HealpixOrdering::Ring,
    )
    .map_err(|e| invalid(&e.to_string()))
}

struct HealpixBoundary<'a, 'map> {
    sky: &'a StarlightSky<'map>,
    nside: u32,
}
impl IncidentBoundary for HealpixBoundary<'_, '_> {
    fn visit(&self, visitor: &mut dyn FnMut(Direction) -> Result<()>) -> Result<()> {
        let sky = self.sky;
        let fine = grid(self.nside)?;
        let bin = sky
            .map
            .bin_edges_nm()
            .iter()
            .position(|e| f64::from(*e) == sky.band[0].value())
            .ok_or_else(|| invalid("missing native bin"))?;
        let width = (sky.band[1] - sky.band[0]).value();
        for i in 0..fine.npix() {
            let gal = fine
                .pixel_center::<Galactic>(HealpixIndex::new(i))
                .map_err(|e| invalid(&e.to_string()))?;
            let local: [f64; 3] = std::array::from_fn(|j| {
                sky.basis[j]
                    .iter()
                    .zip(gal.as_array())
                    .map(|(a, b)| a * b)
                    .sum()
            });
            if local[2] > 0.0 {
                let parent = sky
                    .map
                    .grid()
                    .direction_to_pixel(gal)
                    .map_err(|e| invalid(&e.to_string()))?;
                let incoming = sky.map.spectral_bin_at_index(parent, bin)?.value() / width;
                visitor(Direction {
                    mu: local[2],
                    vector: local,
                    weight: fine.pixel_area_sr(),
                    incoming,
                })?;
            }
        }
        Ok(())
    }
}

fn initial(sky: &StarlightSky<'_>, s: HealpixSettings) -> Result<u32> {
    if !sky.map.nside().is_power_of_two()
        || !s.subdivision.is_power_of_two()
        || s.subdivision > 64
        || !(2..=4).contains(&s.refinements)
        || !s.relative_tolerance.is_finite()
        || !(0.0..=1e-4).contains(&s.relative_tolerance)
        || s.relative_tolerance == 0.0
        || !s.absolute_tolerance.is_finite()
        || !(0.0..=1e-12).contains(&s.absolute_tolerance.value())
    {
        return Err(invalid(
            "unsupported multiple-scattering HEALPix boundary plan",
        ));
    }
    sky.map
        .nside()
        .checked_mul(s.subdivision)
        .ok_or_else(|| invalid("HEALPix boundary size overflow"))
}

/// Complete-bin FlatPhotonDensity transport, with separately refined native
/// HEALPix boundary kernels, diffuse angles, vertical cells, orders and wavelength.
///
/// Each hierarchical child carries its parent's mean radiance and exact solid angle.
/// Thus subdivision conserves each parent integral on the full sphere. The local
/// horizon clips this cell quadrature by center; its numerical error is included
/// in the independently tested boundary changes. The internal diffuse field uses
/// full-sphere ordinates, not the original HEALPix boundary or point sources.
///
/// # Errors
/// Rejects invalid settings, non-power-of-two native NSIDE, excessive work/memory,
/// incomplete wavelength/phase coverage and any unresolved convergence criterion.
/// Successive boundary changes are diagnostics, not certified error bounds.
pub fn starlight_multiple_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    solver: MultipleScatteringSettings,
    angular: HealpixSettings,
    wavelength: WavelengthSettings,
) -> Result<StarlightMultipleScattering> {
    integrate_native(
        atmosphere,
        phase,
        sky,
        view,
        NativePlan {
            solver,
            angular,
            wavelength,
            projection: None,
        },
    )
    .map(|r| r.0)
}

/// Independently refined positive boundary-kernel projection and Fourier solver.
#[derive(Debug, Clone, Copy)]
pub struct ProjectedSettings {
    /// Existing independent diffuse/vertical/order controls.
    pub solver: MultipleScatteringSettings,
    /// Original HEALPix boundary refinement controls.
    pub boundary: HealpixSettings,
    /// Original complete-bin wavelength controls.
    pub wavelength: WavelengthSettings,
    /// Base positive-cosine intervals; nodes cluster at horizon and zenith.
    pub kernel_mu: usize,
    /// Base periodic azimuth interpolation nodes, power of two.
    pub kernel_azimuth: usize,
}
/// Accepted opt-in transport with separately tested kernel interpolation.
#[derive(Debug, Clone)]
pub struct ProjectedStarlightMultipleScattering {
    /// Existing component-separated native-bin result and provenance.
    pub transport: StarlightMultipleScattering,
    /// Accepted kernel intervals/nodes (mu, azimuth).
    pub kernel_grid: [usize; 2],
    /// Per-wavelength mu-only and azimuth-only changes of single/higher radiance.
    pub kernel_changes: Vec<[[Radiance; 2]; 2]>,
    /// Conservative concurrently live requested numerical-buffer bound in bytes.
    pub workspace_bytes: usize,
}

/// Factorized boundary transport, retaining native source means and all sampled
/// azimuthal phase modes. Kernel interpolation, HEALPix boundary, diffuse angle,
/// height, order and wavelength convergence are separately required.
///
/// # Errors
/// Fails closed on invalid grids, memory/work excess, phase under-resolution,
/// lack of contraction or any unresolved refinement criterion.
pub fn projected_starlight_multiple_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    settings: ProjectedSettings,
) -> Result<ProjectedStarlightMultipleScattering> {
    let (transport, kernel_changes, workspace_bytes) = integrate_native(
        atmosphere,
        phase,
        sky,
        view,
        NativePlan {
            solver: settings.solver,
            angular: settings.boundary,
            wavelength: settings.wavelength,
            projection: Some([settings.kernel_mu, settings.kernel_azimuth]),
        },
    )?;
    Ok(ProjectedStarlightMultipleScattering {
        transport,
        kernel_grid: [2 * settings.kernel_mu, 2 * settings.kernel_azimuth],
        kernel_changes,
        workspace_bytes,
    })
}
struct NativePlan {
    solver: MultipleScatteringSettings,
    angular: HealpixSettings,
    wavelength: WavelengthSettings,
    projection: Option<[usize; 2]>,
}
type NativeResult = (StarlightMultipleScattering, Vec<[[Radiance; 2]; 2]>, usize);
fn integrate_native(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    plan: NativePlan,
) -> Result<NativeResult> {
    let NativePlan {
        solver,
        angular,
        wavelength,
        projection,
    } = plan;
    let n = wavelength.nodes;
    let refinements = wavelength.refinements;
    if !(2..=16).contains(&n)
        || !(1..=4).contains(&refinements)
        || n * (1 << refinements) > 64
        || !wavelength.relative_tolerance.is_finite()
        || !(0.0..=1e-4).contains(&wavelength.relative_tolerance)
        || wavelength.relative_tolerance == 0.0
        || !wavelength.absolute_tolerance.is_finite()
        || !(0.0..=1e-12).contains(&wavelength.absolute_tolerance.value())
    {
        return Err(invalid("invalid multiple-scattering wavelength settings"));
    }
    let first_nside = initial(sky, angular)?;
    let layers = atmosphere.edges().len().saturating_sub(1);
    let diffuse_work = if projection.is_some() {
        projected_preflight(layers, solver)?
    } else {
        preflight(layers, solver)?
    };
    let mut workspace_bound = 0;
    let mut projection_temporary = 0usize;
    let mut retained_projection_bytes = 0usize;
    let mut scalar_work = 0usize;
    let mut geometry_work = 0usize;
    let cache_phase = projection.is_some()
        && matches!(
            phase,
            AerosolPhase::HenyeyGreenstein(_) | AerosolPhase::DoubleHenyeyGreenstein(_)
        );
    let mut phase_build_work = 0usize;
    let mut phase_cache_bytes = 0usize;
    if let Some([u, p]) = projection {
        validate_projection(u, p, solver)?;
        if cache_phase {
            for (grid, q) in [
                ([u, 2 * p], 4 * solver.mu_nodes + 1),
                ([2 * u, p], 4 * solver.mu_nodes + 1),
                ([2 * u, 2 * p], 6 * solver.mu_nodes + 1),
            ] {
                phase_build_work = phase_build_work
                    .checked_add(projection::phase_work(grid[0], grid[1], q)?)
                    .ok_or_else(|| invalid("phase cache work overflow"))?;
                phase_cache_bytes = phase_cache_bytes
                    .checked_add(projection::phase_bytes(grid[0], grid[1], q)?)
                    .ok_or_else(|| invalid("phase cache byte overflow"))?;
            }
        }
    }
    let mut nside = first_nside;
    for level in 0..=angular.refinements {
        let pixels = usize::try_from(grid(nside)?.npix()).map_err(|e| invalid(&e.to_string()))?;
        let mut work = if projection.is_some() {
            geometry_work = geometry_work
                .checked_add(product(&[pixels, 3, 64])?)
                .ok_or_else(|| invalid("projection geometry work overflow"))?;
            0
        } else {
            product(&[pixels, 4, 16])?
        };
        if let Some([u, p]) = projection {
            validate_projection(u, p, solver)?;
            let mass = product(&[
                (u + 1) * (2 * p) + (2 * u + 1) * p + (2 * u + 1) * (2 * p),
                8,
            ])?;
            retained_projection_bytes = retained_projection_bytes
                .checked_add(mass)
                .ok_or_else(|| invalid("projection memory overflow"))?;
        }
        for (a, z) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            let dirs = product(&[2, solver.mu_nodes, solver.azimuth_nodes, a, a])?;
            let cells = product(&[layers, solver.cells_per_layer, z])?;
            let bytes = angular::workspace(dirs, cells, solver.azimuth_nodes * a)?;
            workspace_bound = workspace_bound.max(bytes);
            if let Some([u, p]) = projection {
                work = work
                    .checked_add(projection_work(
                        2 * u,
                        2 * p,
                        2 * solver.mu_nodes * a + 1,
                        cells,
                        cache_phase,
                    )?)
                    .ok_or_else(|| invalid("projection work overflow"))?;
            } else {
                work = work
                    .checked_add(product(&[2, pixels / 2, cells, dirs + 1])?)
                    .ok_or_else(|| invalid("boundary work overflow"))?;
            }
        }
        if let Some([u, p]) = projection {
            let cells = product(&[layers, solver.cells_per_layer, 2])?;
            let temporary = product(&[2 * u + 1, 2 * p, std::mem::size_of::<angular::Complex>()])?
                + product(&[cells, 2 * p + 2 * u + 1, 16])?
                + product(&[8, 2 * p, 16])?;
            projection_temporary = projection_temporary.max(temporary);
        }
        scalar_work = scalar_work
            .checked_add(work)
            .and_then(|v| v.checked_add(diffuse_work))
            .ok_or_else(|| invalid("spectral boundary work overflow"))?;
        if level < angular.refinements {
            nside = nside
                .checked_mul(2)
                .ok_or_else(|| invalid("boundary NSIDE overflow"))?;
        }
    }
    if let Some([u, p]) = projection {
        let cells = product(&[layers, solver.cells_per_layer, 2])?;
        let q = 4 * solver.mu_nodes + 1;
        for (mu, phi) in [(u, 2 * p), (2 * u, p)] {
            scalar_work = scalar_work
                .checked_add(projection_work(mu, phi, q, cells, cache_phase)?)
                .and_then(|x| {
                    x.checked_add(
                        angular::plan_work(
                            (q - 1) * solver.azimuth_nodes * 2,
                            cells,
                            solver.azimuth_nodes * 2,
                            solver.max_orders,
                        )
                        .ok()?,
                    )
                })
                .ok_or_else(|| invalid("projection diagnostic work overflow"))?;
        }
    }
    let calls = n * ((1 << (refinements + 1)) - 1);
    let wavelength_work: usize = (0..=refinements)
        .map(|i| {
            let o = n * (1 << i);
            64 * o * o.div_ceil(2)
        })
        .sum();
    let planned_work = scalar_work
        .checked_mul(calls)
        .and_then(|w| w.checked_add(geometry_work))
        .and_then(|w| w.checked_add(phase_build_work))
        .and_then(|w| w.checked_add(wavelength_work))
        .ok_or_else(|| invalid("complete-bin work overflow"))?;
    let work_limit = if projection.is_some() {
        PROJECTED_BAND_WORK_LIMIT
    } else {
        BAND_WORK_LIMIT
    };
    if planned_work > work_limit {
        return Err(invalid(
            &format!("complete-bin multiple-scattering plan requires {planned_work} work units, exceeds limit {work_limit}"),
        ));
    }
    workspace_bound = workspace_bound
        .checked_add(projection_temporary)
        .and_then(|x| x.checked_add(retained_projection_bytes))
        .and_then(|x| x.checked_add(phase_cache_bytes))
        .ok_or_else(|| invalid("projection workspace overflow"))?;
    if workspace_bound > MEMORY_LIMIT {
        return Err(invalid("projected transport workspace exceeds 16 MiB"));
    }
    let domain = atmosphere.wavelength_domain();
    if sky.band[0] < domain[0] || sky.band[1] > domain[1] {
        return Err(invalid("atmosphere must cover the complete native bin"));
    }
    for w in sky.band {
        phase.density(w, Radians::new(0.0))?;
    }
    let mut projected = Vec::new();
    if let Some([u, p]) = projection {
        let mut caches = Vec::new();
        if cache_phase {
            for (grid, all) in [
                ([u, 2 * p], false),
                ([2 * u, p], false),
                ([2 * u, 2 * p], true),
            ] {
                caches.push(std::sync::Arc::new(projection::PhaseKernels::new(
                    sky, phase, view, solver, grid, all,
                )?));
            }
        }
        let mut nside = first_nside;
        for level in 0..=angular.refinements {
            let input = HealpixBoundary { sky, nside };
            let mut bank = [
                projection::ProjectedBoundary::new(&input, u, 2 * p)?,
                projection::ProjectedBoundary::new(&input, 2 * u, p)?,
                projection::ProjectedBoundary::new(&input, 2 * u, 2 * p)?,
            ];
            for (entry, kernel) in bank.iter_mut().zip(&caches) {
                entry.kernels = Some(kernel.clone());
            }
            projected.push(bank);
            if level < angular.refinements {
                nside *= 2;
            }
        }
    }
    let integrate = |order| -> Result<_> {
        let mut sum = [0.0; 3];
        let mut results = Vec::with_capacity(order);
        let mut nsides = Vec::with_capacity(order);
        let mut changes = Vec::with_capacity(order);
        let mut kernel_changes = Vec::with_capacity(order);
        for (node, weight) in single::quadrature::gauss(order)? {
            let w = sky.band[0] + (sky.band[1] - sky.band[0]) * node;
            let mut previous: Option<MultipleScattering> = None;
            let mut previous_passed = false;
            let mut accepted = None;
            let mut last_change = [f64::INFINITY; 2];
            let mut nside = first_nside;
            for level in 0..=angular.refinements {
                let incident = HealpixBoundary { sky, nside };
                let boundary: &dyn IncidentBoundary = if projection.is_some() {
                    &projected
                        .get(level)
                        .ok_or_else(|| invalid("missing projected boundary level"))?[2]
                } else {
                    &incident
                };
                let current = evaluate_with_budget(
                    atmosphere,
                    phase,
                    sky,
                    w,
                    view,
                    (solver, Some(boundary)),
                    projection.is_some(),
                )?;
                if let Some(old) = previous {
                    let change = [
                        (current.single.value() - old.single.value()).abs(),
                        (current.higher_orders.value() - old.higher_orders.value()).abs(),
                    ];
                    last_change = change;
                    let pass = agrees(
                        current.single.value(),
                        old.single.value(),
                        angular.relative_tolerance,
                        angular.absolute_tolerance.value(),
                    ) && agrees(
                        current.higher_orders.value(),
                        old.higher_orders.value(),
                        angular.relative_tolerance,
                        angular.absolute_tolerance.value(),
                    );
                    if pass && previous_passed {
                        accepted = Some((current, nside, change));
                        break;
                    }
                    previous_passed = pass;
                }
                previous = Some(current);
                if level < angular.refinements {
                    nside *= 2;
                }
            }
            let (result, nside, change) = accepted.ok_or_else(|| {
                invalid(&format!("HEALPix boundary kernel/horizon refinement did not converge at NSIDE {nside}: changes {last_change:?}"))
            })?;
            let mut kernel_change = [[Radiance::new(0.0); 2]; 2];
            if projection.is_some() {
                let level = boundary_level(first_nside, nside);
                for (axis, change) in kernel_change.iter_mut().enumerate() {
                    let coarse = solve(
                        atmosphere,
                        phase,
                        sky,
                        w,
                        view,
                        solver,
                        (2, 2, Some(&projected[level][axis])),
                    )?;
                    *change = [
                        Radiance::new((coarse.first - result.single.value()).abs()),
                        Radiance::new((coarse.higher - result.higher_orders.value()).abs()),
                    ];
                    if !agrees(
                        coarse.first,
                        result.single.value(),
                        angular.relative_tolerance,
                        angular.absolute_tolerance.value(),
                    ) || !agrees(
                        coarse.higher,
                        result.higher_orders.value(),
                        angular.relative_tolerance,
                        angular.absolute_tolerance.value(),
                    ) {
                        return Err(invalid(&format!(
                            "boundary kernel projection axis {axis} did not converge: {change:?}; fine first={} higher={} tolerance={}", result.single.value(), result.higher_orders.value(), angular.relative_tolerance
                        )));
                    }
                }
            }
            kernel_changes.push(kernel_change);
            for (s, v) in sum.iter_mut().zip([
                result.direct.value(),
                result.single.value(),
                result.higher_orders.value(),
            ]) {
                *s += v * weight * (sky.band[1] - sky.band[0]).value();
            }
            results.push(result);
            nsides.push(nside);
            changes.push(change.map(Radiance::new));
        }
        Ok((sum, results, nsides, changes, kernel_changes))
    };
    let (mut previous, _, _, _, _) = integrate(n)?;
    for i in 1..=refinements {
        let order = n * (1 << i);
        let (current, samples, boundary_nside, boundary_change, kernel_changes) = integrate(order)?;
        let change = std::array::from_fn::<_, 3, _>(|j| (current[j] - previous[j]).abs());
        if (0..3).all(|j| {
            agrees(
                current[j],
                previous[j],
                wavelength.relative_tolerance,
                wavelength.absolute_tolerance.value(),
            )
        }) {
            let scattered = nonnegative(current[1] + current[2], "integrated scattered radiance")?;
            return Ok((
                StarlightMultipleScattering {
                    band: sky.band,
                    toa: sky.toa(view)?,
                    direct: BandRadiance::new(current[0]),
                    single: BandRadiance::new(current[1]),
                    higher_orders: BandRadiance::new(current[2]),
                    total_scattered: BandRadiance::new(scattered),
                    total: BandRadiance::new(nonnegative(
                        current[0] + scattered,
                        "integrated total",
                    )?),
                    uncertainty: None,
                    source: sky.coverage(),
                    provenance: sky.map.provenance().clone(),
                    grid: sky.map.grid(),
                    geometry: sky.geometry,
                    assumption: sky.assumption,
                    wavelength_settings: wavelength,
                    boundary_settings: angular,
                    wavelength_order: order,
                    wavelength_change: change.map(BandRadiance::new),
                    samples,
                    boundary_nside,
                    boundary_change,
                    planned_work,
                },
                kernel_changes,
                workspace_bound,
            ));
        }
        previous = current;
    }
    Err(invalid(
        "multiple-scattering native-bin wavelength refinement did not converge",
    ))
}

#[cfg(test)]
mod tests;

fn boundary_level(first: u32, final_nside: u32) -> usize {
    (final_nside / first).ilog2() as usize
}
fn validate_projection(u: usize, p: usize, solver: MultipleScatteringSettings) -> Result<()> {
    if !(8..=256).contains(&u)
        || !u.is_power_of_two()
        || !(16..=512).contains(&p)
        || !p.is_power_of_two()
        || !p.is_multiple_of(2 * solver.azimuth_nodes)
    {
        return Err(invalid("unsupported factorized boundary kernel grid"));
    }
    Ok(())
}
fn projection_work(u: usize, p: usize, q: usize, cells: usize, cached: bool) -> Result<usize> {
    let fft = angular::transform_work(p)?;
    let build = product(&[u + 1, fft])?;
    let kernel = product(&[
        q,
        u + 1,
        (if cached { 2 * p } else { 2 * p + 2 * fft }) + 8 * cells * p,
    ])?;
    let sweep = product(&[q, cells, fft + 8 * p])?;
    build
        .checked_add(kernel)
        .and_then(|x| x.checked_add(sweep))
        .ok_or_else(|| invalid("projected boundary work overflow"))
}
