// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Experimental native-bin Starlight transport. Pixel averages are sampled by
//! containing HEALPix pixel; no interpolation, PSF or point-source interpretation.
//! Numerical resolution changes are not physical uncertainty bounds. Discontinuous
//! maps can fail smooth quadrature; the pixel-aware path conserves parent areas
//! and refines the attenuation/phase kernel with independent wavelength checks.
mod healpix;
pub use healpix::{HealpixConvergence, HealpixSettings};

use super::*;
use crate::components::starlight::{StarlightMap, StarlightProvenance};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandRadiance;
use crate::Observer;
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::{Galactic, ICRS};
use siderust::coordinates::transform::{DirectionAstroExt, FromHorizontal};
use siderust::healpix::HealpixGrid;
use tempoch::{JulianDate, TT, UT1};

/// Explicit local geometry. TT controls precession/nutation; UT1 Earth rotation.
/// Caller must supply the same physical instant in both scales, with its chosen
/// Earth-orientation data. No UTC=UT1 or TT=UT1 approximation is made here.
#[derive(Debug, Clone, Copy)]
pub struct StarlightGeometry {
    /// WGS84 geodetic site, including its ellipsoidal height. This is independent
    /// of the atmospheric column's caller-declared datum (MSL for profiles).
    /// No geoid conversion or consistency verification is performed.
    pub observer: Observer,
    /// Terrestrial Time Julian date.
    pub tt: JulianDate<TT>,
    /// Universal Time 1 Julian date.
    pub ut1: JulianDate<UT1>,
}

/// Explicit model for information absent from the stored bin integrals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WithinBinSpectrum {
    /// Constant photon spectral density I_bin / width, independently per bin.
    /// This is not a measured continuous Gaia XP spectrum.
    FlatPhotonDensity,
}

/// One complete native bin, adapted to the authoritative spectral sky contract.
#[derive(Debug)]
pub struct StarlightSky<'a> {
    pub(in crate::transport) map: &'a StarlightMap,
    pub(in crate::transport) geometry: StarlightGeometry,
    pub(in crate::transport) band: [Nanometers; 2],
    pub(in crate::transport) assumption: WithinBinSpectrum,
    // Columns are upstream-transformed local Cartesian basis vectors. This is
    // a fixed rotation, not an independent celestial transformation algorithm.
    pub(in crate::transport) basis: [[f64; 3]; 3],
    identity: String,
}
impl<'a> StarlightSky<'a> {
    /// Select exactly one stored bin. Legacy integrated products, partial bins,
    /// missing edges and bins beyond the reference 360–570 nm domain fail.
    pub fn new(
        map: &'a StarlightMap,
        geometry: StarlightGeometry,
        band: [Nanometers; 2],
        assumption: WithinBinSpectrum,
    ) -> Result<Self> {
        let checksum = map.sha256().filter(|_| map.is_spectral()).ok_or_else(|| {
            invalid("Starlight transport requires a checksum-validated spectral TOA map")
        })?;
        if !band.iter().all(|w| w.is_finite())
            || band[0].value() < 360.0
            || band[1].value() > 570.0
            || !map
                .bin_edges_nm()
                .windows(2)
                .any(|e| band[0].value() == f64::from(e[0]) && band[1].value() == f64::from(e[1]))
        {
            return Err(invalid(
                "Starlight transport requires one complete stored bin within 360–570 nm",
            ));
        }
        let site = geometry.observer;
        if !site.lon.is_finite()
            || !site.lat.is_finite()
            || !site.height.is_finite()
            || site.lat.value().abs() > 90.0
            || !geometry.tt.value().is_finite()
            || !geometry.ut1.value().is_finite()
        {
            return Err(invalid(
                "Starlight site and TT/UT1 epochs must be finite with valid latitude",
            ));
        }
        let mut basis = [[0.0; 3]; 3];
        for (i, column) in basis.iter_mut().enumerate() {
            // Canonical unit basis vectors are finite and normalized by construction.
            let local =
                Direction::new_unchecked(f64::from(i == 0), f64::from(i == 1), f64::from(i == 2));
            let equatorial =
                FromHorizontal::to_equatorial(&local, &geometry.ut1, &geometry.tt, &site);
            let icrs: Direction<ICRS> = equatorial.to_frame(&geometry.tt);
            // Use the canonical fixed-frame API. The pinned provider-based
            // catalog implementation reverses ICRS/Galactic; see anchor tests.
            let galactic: Direction<Galactic> =
                siderust::coordinates::transform::TransformFrame::to_frame(&icrs);
            *column = [galactic.x(), galactic.y(), galactic.z()];
            if !column.iter().all(|v| v.is_finite()) {
                return Err(invalid(
                    "Starlight epoch exceeds the finite upstream transformation domain",
                ));
            }
        }
        Ok(Self {
            map, geometry, band, assumption, basis,
            identity: format!("{}; sha256={}; {geometry:?}; flat-photon-density; bin={band:?}; containing-pixel average", map.provenance().dataset_name, checksum),
        })
    }

    fn galactic(&self, direction: Horizontal) -> Result<Direction<Galactic>> {
        if !direction.alt().is_finite()
            || !direction.az().is_finite()
            || direction.alt().value().abs() > 90.0
        {
            return Err(invalid(
                "Starlight lookup requires a finite geometric Horizontal direction",
            ));
        }
        let d = direction.to_cartesian();
        let v = [d.x(), d.y(), d.z()];
        let xyz: [f64; 3] = std::array::from_fn(|j| (0..3).map(|i| self.basis[i][j] * v[i]).sum());
        Direction::try_new(xyz[0], xyz[1], xyz[2])
            .ok_or_else(|| invalid("nonfinite Starlight Galactic transformation"))
    }

    pub(in crate::transport) fn toa(&self, direction: Horizontal) -> Result<BandRadiance> {
        self.map
            .integrate_band(self.galactic(direction)?, self.band[0], self.band[1])
            .map(|v| v.0)
    }
}
impl IncidentSky for StarlightSky<'_> {
    fn coverage(&self) -> SkyCoverage {
        SkyCoverage {
            identity: self.identity.clone(),
            wavelengths: self.band,
            full_hemisphere: true,
            origin: RadianceOrigin::TopOfAtmosphere,
        }
    }
    fn radiance(&self, wavelength: Nanometers, direction: Horizontal) -> Result<Radiance> {
        if !wavelength.is_finite() || wavelength < self.band[0] || wavelength > self.band[1] {
            return Err(invalid(
                "wavelength outside selected Starlight bin; no extrapolation",
            ));
        }
        let integrated = self.toa(direction)?;
        // Typed integrated photon radiance / wavelength gives per-nm radiance.
        // QTTY composite Per<BandUnit, Nanometer> and the named spectral marker
        // have identical physical scale; bridge explicitly without unit rescaling.
        Ok(Radiance::new(
            (integrated / (self.band[1] - self.band[0])).value(),
        ))
    }
}

/// Independent deterministic wavelength refinement; always integrates a full bin.
#[derive(Debug, Clone, Copy)]
pub struct WavelengthSettings {
    /// Initial Gauss–Legendre order (2–16).
    pub nodes: usize,
    /// Maximum doublings (1–4), final order at most 64.
    pub refinements: usize,
    /// Positive relative threshold, at most 1e-4, applied to direct and single.
    pub relative_tolerance: f64,
    /// Absolute threshold in integrated photon units, at most 1e-12.
    pub absolute_tolerance: BandRadiance,
}
impl Default for WavelengthSettings {
    fn default() -> Self {
        Self {
            nodes: 2,
            refinements: 2,
            relative_tolerance: 1e-5,
            absolute_tolerance: BandRadiance::new(1e-12),
        }
    }
}

/// Angular/path diagnostics for one wavelength at the accepted spectral order.
#[derive(Debug, Clone)]
pub struct WavelengthSample {
    /// Interior wavelength node.
    pub wavelength: Nanometers,
    /// Independent reference angular/path convergence at this wavelength.
    pub convergence: Option<Convergence>,
    /// Pixel-aware angular refinement; None for the smooth-sky path. Vertical
    /// integration uses the exact homogeneous-layer antiderivative in this mode.
    pub healpix_convergence: Option<HealpixConvergence>,
}

/// Component-separated integrated photon radiance and immutable evaluation context.
#[derive(Debug, Clone)]
pub struct StarlightTransport {
    /// Native input-bin identity (vacuum wavelengths).
    pub band: [Nanometers; 2],
    /// Exact requested direction's containing-pixel TOA bin integral.
    pub toa: BandRadiance,
    /// Ground direct radiance, integrated across the entire stored bin.
    pub direct: BandRadiance,
    /// Ground once-scattered radiance, integrated across the entire stored bin.
    pub single: BandRadiance,
    /// Always None: higher orders were not computed.
    pub higher_orders: Option<BandRadiance>,
    /// Always None: source/atmosphere/model uncertainty is not propagated.
    pub uncertainty: Option<BandRadiance>,
    /// Explicit spectral reconstruction model, distinct from numerical error.
    pub assumption: WithinBinSpectrum,
    /// Requested wavelength quadrature settings.
    pub wavelength_settings: WavelengthSettings,
    /// Accepted GL wavelength order.
    pub wavelength_order: usize,
    /// Successive wavelength-order changes: direct and single, respectively.
    pub wavelength_change: [BandRadiance; 2],
    /// Accepted spectral nodes and angular/path diagnostics.
    pub samples: Vec<WavelengthSample>,
    /// Original product identity and scientific provenance; no admission upgrade.
    pub provenance: StarlightProvenance,
    /// Original HEALPix resolution and ordering; frame is Galactic.
    pub grid: HealpixGrid,
    /// Original local site and explicit TT/UT1 epoch context.
    pub geometry: StarlightGeometry,
    /// Exact local view (north-clockwise azimuth, geometric altitude).
    pub view: Horizontal,
    /// Actual layered optical model used by the reference.
    pub atmosphere: LayeredAtmosphere,
    /// Actual scattering phase configuration.
    pub aerosol_phase: AerosolPhase,
    /// Requested angular/path settings.
    pub solver_settings: Option<SolverSettings>,
    /// Pixel-aware settings; None for the original smooth-sky reference path.
    pub healpix_settings: Option<HealpixSettings>,
    /// Resolution diagnostics cannot certify arbitrary unresolved pixel structure.
    pub numerical_limitations: &'static str,
    /// Conservative total work for the entire configured spectral refinement plan.
    pub planned_work: usize,
}
impl StarlightTransport {
    /// Experimental model identity. No polarization, refraction, surface reflection,
    /// internal emission, multiple scattering, PSF or subpixel reconstruction.
    /// Missing populations, calibration, covariance and model uncertainty remain unknown.
    pub const MODEL: &'static str = "nsb-starlight-flat-bin-single-v1-experimental";
}

/// Transport one complete stored spectral bin, with independent wavelength refinement.
///
/// Uses the existing single-scattering operator at every GL wavelength. Rejects
/// unresolved angular/path or wavelength convergence without returning a partial
/// result. The 250M reference work-unit budget applies to the *entire configured
/// wavelength plan*, including refinements not reached after early convergence.
/// Solver buffers run sequentially and retain the existing 8 MiB limit. No map
/// or angular buffers are duplicated; result diagnostics contain at most 64 nodes.
/// Summing successful complete-bin results gives their union, without assuming
/// cross-bin covariance. This operation never enters `NsbEvaluator` defaults.
pub fn starlight_single_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    solver: SolverSettings,
    wavelength: WavelengthSettings,
) -> Result<StarlightTransport> {
    evaluate_starlight(
        atmosphere,
        phase,
        sky,
        view,
        Integration::Smooth(solver),
        wavelength,
    )
}

/// Transport a native bin with conservative HEALPix subpixel refinement and
/// exact homogeneous-layer path integration in the shared reference kernel.
/// The source retains piecewise-constant parent-pixel means; attenuation/phase
/// are evaluated within those pixels. No positive horizon cutoff is imposed.
pub fn starlight_healpix_single_scattering(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    angular: HealpixSettings,
    wavelength: WavelengthSettings,
) -> Result<StarlightTransport> {
    evaluate_starlight(
        atmosphere,
        phase,
        sky,
        view,
        Integration::Healpix(angular),
        wavelength,
    )
}

#[derive(Clone, Copy)]
enum Integration {
    Smooth(SolverSettings),
    Healpix(HealpixSettings),
}

fn evaluate_starlight(
    atmosphere: &LayeredAtmosphere,
    phase: &AerosolPhase,
    sky: &StarlightSky<'_>,
    view: Horizontal,
    integration: Integration,
    wavelength: WavelengthSettings,
) -> Result<StarlightTransport> {
    let n = wavelength.nodes;
    let r = wavelength.refinements;
    if !(2..=16).contains(&n)
        || !(1..=4).contains(&r)
        || n * (1 << r) > 64
        || !wavelength.relative_tolerance.is_finite()
        || !(0.0..=1e-4).contains(&wavelength.relative_tolerance)
        || wavelength.relative_tolerance == 0.0
        || !wavelength.absolute_tolerance.is_finite()
        || !(0.0..=1e-12).contains(&wavelength.absolute_tolerance.value())
    {
        return Err(invalid("invalid Starlight wavelength quadrature settings"));
    }
    let layers = atmosphere.edges().len() - 1;
    let (scalar_work, shared_work) = match integration {
        Integration::Smooth(solver) => {
            let work = budget::estimate(solver, layers)?;
            work.enforce(layers, solver)?;
            (work.total()?, 0)
        }
        Integration::Healpix(angular) => {
            let (w, a) = healpix::estimate(sky, layers, angular, phase)?;
            (
                w,
                a.checked_add(layers)
                    .and_then(|v| v.checked_add(8))
                    .ok_or_else(|| invalid("HEALPix shared work overflow"))?,
            )
        }
    };
    let calls = n * ((1 << (r + 1)) - 1);
    let wavelength_work: usize = (0..=r)
        .map(|i| {
            let order = n * (1 << i);
            64 * order * order.div_ceil(2)
        })
        .sum();
    let planned_work = scalar_work
        .checked_mul(calls)
        .and_then(|value| value.checked_add(shared_work))
        .and_then(|value| value.checked_add(wavelength_work))
        .ok_or_else(|| invalid("Starlight spectral work estimate overflow"))?;
    if planned_work > budget::MAX_WORK {
        return Err(invalid("Starlight complete wavelength plan exceeds 250 million work units; reduce orders/refinements"));
    }
    // The column's first edge defines the plane-parallel observer origin.
    // WGS84 ellipsoidal site height is not an atmospheric MSL height. Callers
    // supply physically consistent inputs; no geoid offset is inferred here.
    let domain = atmosphere.wavelength_domain();
    if sky.band[0] < domain[0] || sky.band[1] > domain[1] {
        return Err(invalid("atmosphere must cover the complete Starlight bin"));
    }
    // Validate phase at endpoints too; interior quadrature must not hide gaps.
    for w in sky.band {
        phase.density(w, Radians::new(0.0))?;
    }
    let shared_audits = if let Integration::Healpix(angular) = integration {
        let midpoint = sky.band[0] + (sky.band[1] - sky.band[0]) * 0.5;
        direct_illumination(atmosphere, phase, sky, midpoint, view)?;
        if healpix::wavelength_independent(phase) {
            let kernel = quadrature::Kernel {
                atmosphere,
                phase,
                sky,
                wavelength: midpoint,
                view,
            };
            Some(healpix::phase_audits(sky, &kernel, angular)?)
        } else {
            None
        }
    } else {
        None
    };
    let integrate = |order| -> Result<([f64; 2], Vec<WavelengthSample>)> {
        let mut sum = [0.0; 2];
        let mut samples = Vec::with_capacity(order);
        for (node, weight) in quadrature::gauss(order)? {
            let w = sky.band[0] + (sky.band[1] - sky.band[0]) * node;
            let (direct, single, sample) = match integration {
                Integration::Smooth(solver) => {
                    let result = single_scattering(atmosphere, phase, sky, w, view, solver)?;
                    (
                        result.direct.value(),
                        result.single.value(),
                        WavelengthSample {
                            wavelength: w,
                            convergence: Some(result.convergence),
                            healpix_convergence: None,
                        },
                    )
                }
                Integration::Healpix(angular) => {
                    let (direct, single, convergence) = healpix::solve(
                        atmosphere,
                        phase,
                        sky,
                        w,
                        view,
                        angular,
                        shared_audits.as_deref(),
                    )?;
                    (
                        direct,
                        single,
                        WavelengthSample {
                            wavelength: w,
                            convergence: None,
                            healpix_convergence: Some(convergence),
                        },
                    )
                }
            };
            let width_weight = (sky.band[1] - sky.band[0]).value() * weight;
            sum[0] += direct * width_weight;
            sum[1] += single * width_weight;
            samples.push(sample);
        }
        for v in sum {
            finite_nonnegative(v, "integrated Starlight radiance")?;
        }
        Ok((sum, samples))
    };
    let (mut previous, _) = integrate(n)?;
    for i in 1..=r {
        let order = n * (1 << i);
        let (current, samples) = integrate(order)?;
        let change = std::array::from_fn::<_, 2, _>(|j| (current[j] - previous[j]).abs());
        if (0..2).all(|j| {
            change[j]
                <= wavelength.absolute_tolerance.value()
                    + wavelength.relative_tolerance * current[j].abs().max(previous[j].abs())
        }) {
            return Ok(StarlightTransport {
                band: sky.band,
                toa: sky.toa(view)?,
                direct: BandRadiance::new(current[0]),
                single: BandRadiance::new(current[1]),
                higher_orders: None,
                uncertainty: None,
                assumption: sky.assumption,
                wavelength_settings: wavelength,
                wavelength_order: order,
                wavelength_change: change.map(BandRadiance::new),
                samples,
                provenance: sky.map.provenance().clone(),
                grid: sky.map.grid(),
                geometry: sky.geometry,
                view,
                atmosphere: atmosphere.clone(),
                aerosol_phase: phase.clone(),
                solver_settings:match integration {Integration::Smooth(v)=>Some(v),_=>None},
                healpix_settings:match integration {Integration::Healpix(v)=>Some(v),_=>None},
                numerical_limitations: "Successive quadrature changes are diagnostics rather than rigorous error bounds. Pixel-aware mode refines kernel and horizon sampling within constant parent pixels; high phase anisotropy may exceed its work budget. Smooth mode uses containing-pixel sampling; successive angular/path/wavelength resolution changes are diagnostics, not certified HEALPix integration error bounds. Sparse pixels and boundaries may remain unresolved; see #192.",
                planned_work,
            });
        }
        previous = current;
    }
    Err(invalid(
        "Starlight wavelength convergence failed; angular/path convergence is separate",
    ))
}

#[cfg(test)]
mod tests;
