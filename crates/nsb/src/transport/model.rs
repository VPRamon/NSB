//! Transport model selection: identity and direct transmission.

use super::direct::{validate_wavelength, DirectTransmission};
use super::extinction::OpticalDepthBreakdown;
use super::geometry::DirectPathGeometry;
use super::metadata::TransportModelMetadata;
use super::origin::RadianceOrigin;
use crate::error::{NsbError, Result};
use crate::site::AtmosphericConditions;
use qtty::dimensionless::Transmittances;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as PhotonSpectralRadiance,
    WattsPerSquareMeterSteradianNanometer as EnergySpectralRadiance,
};
use siderust::qtty::{Nanometers, OpticalDepths};

/// Airmass formula used by direct transmission.
///
/// Each variant maps to a Siderust airmass implementation. NSB does not
/// re-derive airmass expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum AirmassModel {
    /// Young (1994) airmass.
    #[default]
    Young1994,
    /// Krisciunas & Schaefer (1991) airmass.
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

/// Atmospheric transport model applied to wavelength-resolved radiance.
///
/// Not `Copy`: future direct/scattered implementations may hold non-`Copy`
/// tables or LUT handles.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum TransportModel {
    /// No atmosphere: `output == input` within the typed representation.
    #[default]
    Identity,
    /// Direct Beer–Lambert extinction of a celestial source.
    Direct(DirectTransmission),
}

impl TransportModel {
    /// Identity / no-atmosphere transport.
    pub const fn identity() -> Self {
        Self::Identity
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
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Direct(_) => "direct-transmission",
        }
    }

    /// Model-only scientific metadata (no atmosphere/profile identity).
    pub fn metadata(&self) -> TransportModelMetadata {
        match self {
            Self::Identity => TransportModelMetadata::identity(),
            Self::Direct(model) => model.metadata(),
        }
    }

    /// Vertical optical depth for the configured extinction column.
    ///
    /// Identity returns a zero breakdown. Wavelength must be finite and `> 0`.
    pub fn optical_depth(
        &self,
        wavelength: Nanometers,
        atmosphere: AtmosphericConditions,
    ) -> Result<OpticalDepthBreakdown> {
        match self {
            Self::Identity => {
                validate_wavelength(wavelength)?;
                Ok(OpticalDepthBreakdown {
                    rayleigh: OpticalDepths::new(0.0),
                    mie: OpticalDepths::new(0.0),
                    absorption: OpticalDepths::new(0.0),
                    total: OpticalDepths::new(0.0),
                })
            }
            Self::Direct(model) => model.optical_depth(wavelength, atmosphere),
        }
    }

    /// Direct-path transmission for the selected model.
    ///
    /// Identity returns `1`. Geometry must come from [`DirectPathGeometry::new`].
    pub fn transmission(
        &self,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<Transmittances> {
        match self {
            Self::Identity => {
                validate_wavelength(wavelength)?;
                Ok(Transmittances::new(1.0))
            }
            Self::Direct(model) => model.transmission(wavelength, geometry, atmosphere),
        }
    }

    /// Propagate energy spectral radiance with mandatory origin checking.
    pub fn apply_energy_spectral(
        &self,
        origin: RadianceOrigin,
        incident: EnergySpectralRadiance,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<EnergySpectralRadiance> {
        let scale = self.propagation_scale(origin, wavelength, geometry, atmosphere)?;
        Ok(EnergySpectralRadiance::new(incident.value() * scale))
    }

    /// Propagate photon spectral radiance with mandatory origin checking.
    pub fn apply_photon_spectral(
        &self,
        origin: RadianceOrigin,
        incident: PhotonSpectralRadiance,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<PhotonSpectralRadiance> {
        let scale = self.propagation_scale(origin, wavelength, geometry, atmosphere)?;
        Ok(PhotonSpectralRadiance::new(incident.value() * scale))
    }

    fn ensure_origin_compatible(&self, origin: RadianceOrigin) -> Result<()> {
        match self {
            Self::Identity => Ok(()),
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

    fn propagation_scale(
        &self,
        origin: RadianceOrigin,
        wavelength: Nanometers,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<f64> {
        self.ensure_origin_compatible(origin)?;
        match self {
            Self::Identity => {
                validate_wavelength(wavelength)?;
                Ok(1.0)
            }
            Self::Direct(model) => model.scale_factor(wavelength, geometry, atmosphere),
        }
    }
}
