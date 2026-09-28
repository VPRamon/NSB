//! Transport model selection: identity and direct transmission.
//!
//! Source-model identity remains outside this enum. Selecting a transport
//! path must never rewrite a component's declared scientific source model.

use qtty::dimensionless::Transmittances;
use qtty::radiometry::WattsPerSquareMeterSteradianNanometer;
use siderust::qtty::Nanometers;

use super::direct::DirectTransmission;
use super::extinction::OpticalDepthBreakdown;
use super::geometry::{DirectPathGeometry, ScatteringGeometry};
use super::identity::IdentityTransport;
use super::metadata::TransportMetadata;
use super::origin::RadianceOrigin;
use super::scattering::{ScatteredPath, ScatteringPathStatus, SingleScatteringNotImplemented};
use crate::error::{NsbError, Result};
use crate::site::AtmosphericConditions;

/// Airmass formula used by direct transmission.
///
/// Each variant maps to a Siderust [`siderust::atmosphere::AirmassFormula`]
/// implementation. NSB does not re-derive airmass expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum AirmassModel {
    /// Young (1994) airmass (zodiacal Noll path heritage).
    #[default]
    Young1994,
    /// Krisciunas & Schaefer (1991) airmass (moonlight heritage).
    KrisciunasSchaefer1991,
    /// Plane-parallel `sec(z)` airmass.
    PlaneParallel,
    /// Rozenberg (1966) airmass.
    Rozenberg1966,
}

impl AirmassModel {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Young1994 => "young-1994",
            Self::KrisciunasSchaefer1991 => "krisciunas-schaefer-1991",
            Self::PlaneParallel => "plane-parallel",
            Self::Rozenberg1966 => "rozenberg-1966",
        }
    }
}

/// Atmospheric transport model applied to spectral radiance.
///
/// Direct extinction and (future) single in-scattering are separate
/// operations. Identity exists for TOA / test isolation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportModel {
    /// No atmosphere: `output == input`.
    Identity(IdentityTransport),
    /// Direct Beer–Lambert extinction of a celestial source.
    Direct(DirectTransmission),
}

impl TransportModel {
    /// Identity / no-atmosphere transport.
    pub const fn identity() -> Self {
        Self::Identity(IdentityTransport::new())
    }

    /// Default direct path: Rayleigh + Mie with Young (1994) airmass.
    pub const fn direct_rayleigh_mie() -> Self {
        Self::Direct(DirectTransmission::rayleigh_mie_young1994())
    }

    /// Direct path including the bundled ozone absorption table.
    pub const fn direct_rayleigh_mie_ozone() -> Self {
        Self::Direct(DirectTransmission::rayleigh_mie_ozone_young1994())
    }

    /// Stable machine-readable model identity.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity(_) => "identity",
            Self::Direct(_) => "direct-transmission",
        }
    }

    /// Scientific metadata for this transport choice.
    ///
    /// `atmosphere_profile_id` records which atmospheric assumptions were
    /// supplied (for example a site-profile name). It does **not** upgrade
    /// [`crate::CalibrationStatus`].
    pub fn metadata(&self, atmosphere_profile_id: &'static str) -> TransportMetadata {
        match self {
            Self::Identity(model) => model.metadata(),
            Self::Direct(model) => model.metadata(atmosphere_profile_id),
        }
    }

    /// Reject physically inappropriate origins for the celestial direct path.
    pub fn ensure_origin_compatible(&self, origin: RadianceOrigin) -> Result<()> {
        match self {
            Self::Identity(_) => Ok(()),
            Self::Direct(_) => {
                if origin.allows_celestial_direct_transmission() {
                    Ok(())
                } else {
                    Err(NsbError::Unsupported(format!(
                        "transport model `{}` is a celestial top-of-atmosphere direct path; \
                         radiance origin `{}` requires a different propagation model \
                         (airglow in-atmosphere emission or legacy pre-scattered moonlight)",
                        self.as_str(),
                        origin.as_str()
                    )))
                }
            }
        }
    }

    /// Vertical optical depth when the model has an extinction column.
    pub fn optical_depth(
        &self,
        wavelength: Nanometers,
        atmosphere: AtmosphericConditions,
    ) -> OpticalDepthBreakdown {
        match self {
            Self::Identity(_) => OpticalDepthBreakdown {
                rayleigh: siderust::qtty::OpticalDepths::new(0.0),
                mie: siderust::qtty::OpticalDepths::new(0.0),
                absorption: siderust::qtty::OpticalDepths::new(0.0),
                total: siderust::qtty::OpticalDepths::new(0.0),
            },
            Self::Direct(model) => model.optical_depth(wavelength, atmosphere),
        }
    }

    /// Direct-path transmission for the selected model.
    pub fn transmission(
        &self,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Transmittances {
        match self {
            Self::Identity(model) => model.transmission(wavelength, geometry, atmosphere),
            Self::Direct(model) => model.transmission(wavelength, geometry, atmosphere),
        }
    }

    /// Propagate spectral radiance along the direct path.
    pub fn apply_spectral(
        &self,
        incident: WattsPerSquareMeterSteradianNanometer,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> WattsPerSquareMeterSteradianNanometer {
        match self {
            Self::Identity(model) => {
                model.apply_spectral(incident, wavelength, geometry, atmosphere)
            }
            Self::Direct(model) => model.apply_spectral(incident, wavelength, geometry, atmosphere),
        }
    }

    /// Propagate spectral radiance after checking radiance origin compatibility.
    pub fn apply_spectral_for_origin(
        &self,
        origin: RadianceOrigin,
        incident: WattsPerSquareMeterSteradianNanometer,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<WattsPerSquareMeterSteradianNanometer> {
        self.ensure_origin_compatible(origin)?;
        Ok(self.apply_spectral(incident, wavelength, geometry, atmosphere))
    }

    /// Single in-scattering path (not yet implemented).
    ///
    /// Direct and scattered operations remain separate. This returns
    /// [`ScatteringPathStatus::NotImplemented`] rather than silently returning
    /// zero scattered radiance.
    pub fn scattered_path(
        &self,
        _geometry: ScatteringGeometry,
        _atmosphere: AtmosphericConditions,
    ) -> ScatteredPath {
        ScatteredPath {
            status: ScatteringPathStatus::NotImplemented,
            detail: SingleScatteringNotImplemented {
                message: concat!(
                    "single in-scattering is scaffolded for API extension ",
                    "(Rayleigh/Mie phase functions, HEALPix sampling, LUTs) ",
                    "but is not implemented in this foundation release"
                ),
            },
        }
    }
}

impl Default for TransportModel {
    fn default() -> Self {
        Self::identity()
    }
}
