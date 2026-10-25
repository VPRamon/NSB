// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Explicit native-bin capability, separate from broadband evaluation/planning.
use super::NsbEvaluator;
use crate::components::starlight::StarlightMap;
use crate::transport::multiple::starlight::{
    projected_starlight_multiple_scattering, ProjectedSettings,
    ProjectedStarlightMultipleScattering,
};
use crate::transport::single::starlight::{
    direct_bin, starlight_healpix_single_scattering, HealpixSettings, StarlightGeometry,
    StarlightSky, StarlightTransport, WavelengthSettings, WithinBinSpectrum,
};
use crate::transport::single::AerosolPhase;
use crate::units::qtty::Nanometers;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandRadiance;
use crate::{NsbError, Result};
use siderust::atmosphere::LayeredAtmosphere;
use siderust::coordinates::spherical::direction::Horizontal;
use std::sync::Arc;

/// Deliberate opt-in physical/numerical selection. No default transport mode.
/// All atmospheric modes use the same plane-parallel column extinction law.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum SpectralStarlightMode<'a> {
    /// Exact native TOA integral; no within-bin spectrum or atmosphere needed.
    Identity,
    /// Unscattered ground contribution only; no angular work or phase required.
    Direct {
        /// Actual optical column; its first edge is the observer's column origin.
        atmosphere: &'a LayeredAtmosphere,
        /// Explicit reconstruction of information absent from stored integrals.
        assumption: WithinBinSpectrum,
        /// Complete-bin quadrature and convergence controls.
        wavelength: WavelengthSettings,
    },
    /// Direct plus exactly once-scattered light, with layer-exact path integration.
    Single {
        /// Actual optical column, independent of WGS84 ellipsoidal height.
        atmosphere: &'a LayeredAtmosphere,
        /// Existing validated aerosol phase.
        phase: &'a AerosolPhase,
        /// Explicit within-bin assumption.
        assumption: WithinBinSpectrum,
        /// Independent HEALPix kernel/horizon refinement.
        angular: HealpixSettings,
        /// Independent wavelength refinement.
        wavelength: WavelengthSettings,
    },
    /// Experimental projected/Fourier physical orders; every guard remains active.
    Multiple {
        /// Actual optical column, independent of WGS84 ellipsoidal height.
        atmosphere: &'a LayeredAtmosphere,
        /// Existing validated aerosol phase.
        phase: &'a AerosolPhase,
        /// Explicit within-bin assumption.
        assumption: WithinBinSpectrum,
        /// Independent boundary, diffuse, height, order, kernel and spectral plan.
        settings: ProjectedSettings,
    },
}

/// Exact incident pixel mean and original context. The source is shared, never
/// copied; its checksum, full coverage, grid, provenance and statistical channels
/// remain available through the existing map API. Missing uncertainties are not zero.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StarlightIncident {
    /// Verified original experimental product, with its population limitations.
    pub source: Arc<StarlightMap>,
    /// Exact complete native bin, vacuum nm.
    pub band: [Nanometers; 2],
    /// Incident bin-integrated photons/(cm² ns sr), before any transport.
    pub toa: BandRadiance,
    /// WGS84 observer and caller-supplied TT/UT1 for the same physical instant.
    pub geometry: StarlightGeometry,
    /// Geometric horizontal direction; north-clockwise azimuth.
    pub view: Horizontal,
}

/// Complete-bin direct contribution. Scattered light and propagated physical
/// uncertainty are uncomputed; this is not a complete ground-radiance prediction.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StarlightDirect {
    /// Exact source/TOA and geometry context.
    pub incident: StarlightIncident,
    /// Unscattered photons/(cm² ns sr), attenuated once along the output path.
    pub direct: BandRadiance,
    /// Actual upstream optical column and declared height datum.
    pub atmosphere: LayeredAtmosphere,
    /// Approximation, distinct from quadrature diagnostics or measured spectra.
    pub assumption: WithinBinSpectrum,
    /// Original complete-bin numerical controls.
    pub wavelength_settings: WavelengthSettings,
    /// Accepted Gaussian order.
    pub wavelength_order: usize,
    /// Last absolute successive-order change; not a physical confidence interval.
    pub wavelength_change: BandRadiance,
    /// Conservative entire wavelength plan, charged before integration.
    pub planned_work: usize,
}

/// Mode-specific output, retaining existing scattering results without copying
/// their radiance fields into a second representation. All modes are experimental;
/// no observational admission, calibration or population completeness is implied.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SpectralStarlightResult {
    /// No atmospheric calculation.
    Identity(StarlightIncident),
    /// Only the surviving unscattered contribution.
    Direct(Box<StarlightDirect>),
    /// Existing first-order result; higher orders are unavailable.
    Single(Box<StarlightTransport>),
    /// Existing accepted projected result and all convergence/resource diagnostics.
    Multiple(Box<ProjectedStarlightMultipleScattering>),
}

impl SpectralStarlightResult {
    /// Propagated physical uncertainty is unavailable. Source statistical errors,
    /// quadrature changes and physical/model unknowns are separate channels.
    pub fn uncertainty(&self) -> Option<BandRadiance> {
        match self {
            Self::Single(r) => r.uncertainty,
            Self::Multiple(r) => r.transport.uncertainty,
            _ => None,
        }
    }

    /// Exact incident native-bin radiance, photons/(cm² ns sr).
    pub fn toa(&self) -> BandRadiance {
        match self {
            Self::Identity(r) => r.toa,
            Self::Direct(r) => r.incident.toa,
            Self::Single(r) => r.toa,
            Self::Multiple(r) => r.transport.toa,
        }
    }

    /// Surviving unscattered light; unavailable in identity mode.
    pub fn direct(&self) -> Option<BandRadiance> {
        match self {
            Self::Identity(_) => None,
            Self::Direct(r) => Some(r.direct),
            Self::Single(r) => Some(r.direct),
            Self::Multiple(r) => Some(r.transport.direct),
        }
    }

    /// Exactly once-scattered light; unavailable unless computed.
    pub fn single(&self) -> Option<BandRadiance> {
        match self {
            Self::Single(r) => Some(r.single),
            Self::Multiple(r) => Some(r.transport.single),
            _ => None,
        }
    }

    /// Second and subsequent orders; unavailable in all partial calculations.
    pub fn higher_orders(&self) -> Option<BandRadiance> {
        match self {
            Self::Multiple(r) => Some(r.transport.higher_orders),
            _ => None,
        }
    }

    /// All computed scattering orders; unavailable for truncated/direct modes.
    pub fn total_scattered(&self) -> Option<BandRadiance> {
        match self {
            Self::Multiple(r) => Some(r.transport.total_scattered),
            _ => None,
        }
    }

    /// Direct plus accepted physical orders, within the solver's explicit
    /// discretization/tail criteria. Partial calculations never supply this total.
    pub fn ground_total(&self) -> Option<BandRadiance> {
        match self {
            Self::Multiple(r) => Some(r.transport.total),
            _ => None,
        }
    }
}

impl NsbEvaluator {
    /// Evaluate one complete native spectral Starlight bin with an explicit
    /// transport choice. Uses the already configured, checksum-verified source.
    /// No download, broadband composition, site calibration or model selection.
    ///
    /// Supply TT and UT1 for the same physical instant using external supported
    /// time/Earth-orientation data; no UTC substitution is performed. The WGS84
    /// observer height and atmosphere's column datum remain distinct. Atmospheric
    /// modes assume scalar plane-parallel geometric rays, black ground, no
    /// refraction/polarization/emission/reflection, and zenith 0–80°.
    ///
    /// # Errors
    /// Rejects absent/integrated products, incomplete bins or bands outside
    /// 360–570 nm, invalid geometry, inadequate atmosphere/phase coverage,
    /// excessive work/memory and any unaccepted convergence. Errors contain no
    /// partial accepted radiance. Physical/model uncertainty remains unknown.
    pub fn evaluate_spectral_starlight(
        &self,
        geometry: StarlightGeometry,
        view: Horizontal,
        band: [Nanometers; 2],
        mode: SpectralStarlightMode<'_>,
    ) -> Result<SpectralStarlightResult> {
        let model = self.starlight().ok_or_else(|| {
            NsbError::Unsupported(
                "spectral Starlight requires an explicitly configured product".into(),
            )
        })?;
        let assumption = match mode {
            SpectralStarlightMode::Identity => WithinBinSpectrum::FlatPhotonDensity,
            SpectralStarlightMode::Direct { assumption, .. }
            | SpectralStarlightMode::Single { assumption, .. }
            | SpectralStarlightMode::Multiple { assumption, .. } => assumption,
        };
        // Reuse the current verified local→ICRS→Galactic adapter. Identity uses
        // only its exact bin lookup, never a reconstructed spectral density.
        let sky = StarlightSky::new(model.map(), geometry, band, assumption)?;
        let incident = || -> Result<StarlightIncident> {
            Ok(StarlightIncident {
                source: model.shared_map(),
                band,
                toa: sky.toa(view)?,
                geometry,
                view,
            })
        };
        Ok(match mode {
            SpectralStarlightMode::Identity => SpectralStarlightResult::Identity(incident()?),
            SpectralStarlightMode::Direct {
                atmosphere,
                wavelength,
                ..
            } => {
                let r = direct_bin(atmosphere, &sky, view, wavelength)?;
                SpectralStarlightResult::Direct(Box::new(StarlightDirect {
                    incident: incident()?,
                    direct: r.direct,
                    atmosphere: atmosphere.clone(),
                    assumption,
                    wavelength_settings: wavelength,
                    wavelength_order: r.order,
                    wavelength_change: r.change,
                    planned_work: r.planned_work,
                }))
            }
            SpectralStarlightMode::Single {
                atmosphere,
                phase,
                angular,
                wavelength,
                ..
            } => SpectralStarlightResult::Single(Box::new(starlight_healpix_single_scattering(
                atmosphere, phase, &sky, view, angular, wavelength,
            )?)),
            SpectralStarlightMode::Multiple {
                atmosphere,
                phase,
                settings,
                ..
            } => SpectralStarlightResult::Multiple(Box::new(
                projected_starlight_multiple_scattering(atmosphere, phase, &sky, view, settings)?,
            )),
        })
    }
}
