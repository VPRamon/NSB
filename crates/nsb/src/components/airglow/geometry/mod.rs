//! Airglow target and emitting-volume line-of-sight geometry.
//!
//! Two deliberately separate geometry models are supported:
//!
//! - [`VanRhijnConfig`] preserves the historical NSB/Siderust thin-shell
//!   correction exactly. It is the fast default and uses the 6,371 km mean
//!   Earth radius built into Siderust.
//! - [`VerticalEmissionProfile`] integrates a piecewise-linear emissivity
//!   profile through a spherical atmosphere and normalizes the result by the
//!   same profile viewed at zenith.
//!
//! Atmospheric extinction is not part of either model. The Noll Rayleigh/Mie
//! scattering stage is applied independently in `extinction.rs`.
//!
//! Module layout:
//! - [`van_rhijn`]: thin-shell configuration and evaluation
//! - [`vertical_profile`]: validated profile domain model
//! - [`vertical_profile_io`]: TOML/schema/checksum persistence
//! - [`integration`]: spherical line-of-sight numerical integration

mod integration;
mod van_rhijn;
mod vertical_profile;
mod vertical_profile_io;

pub use van_rhijn::VanRhijnConfig;
pub use vertical_profile::{
    AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalEmissionProfileError, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};

use crate::error::Result;
use crate::units::ScaleFactors;
use integration::VERTICAL_PROFILE_INTEGRATOR_VERSION;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use tempoch::{Time, JD, TT, UTC};
use van_rhijn::VAN_RHIJN_IMPLEMENTATION_VERSION;

/// Configurable Airglow emitting-volume line-of-sight geometry.
///
/// Additional validated geometries may be added; match with a wildcard.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AirglowGeometryModel {
    /// Fast, geometrically thin emitting shell; the historical NSB default.
    VanRhijn(VanRhijnConfig),
    /// Spherical LOS integration of a validated vertical emissivity profile.
    VerticalProfile(VerticalEmissionProfile),
}

impl AirglowGeometryModel {
    /// Evaluate the selected dimensionless line-of-sight correction.
    pub(crate) fn geometry_factor(
        &self,
        observer: Geodetic<ECEF>,
        zenith: Degrees,
    ) -> Result<ScaleFactors> {
        match self {
            Self::VanRhijn(config) => config.geometry_factor(zenith),
            Self::VerticalProfile(profile) => profile.geometry_factor(observer, zenith),
        }
    }

    /// Stable model identifier used in scientific metadata.
    pub const fn model_id(&self) -> &'static str {
        match self {
            Self::VanRhijn(_) => "van_rhijn",
            Self::VerticalProfile(_) => "vertical_profile",
        }
    }

    /// Structured provenance for saved scientific results.
    pub(crate) fn metadata(&self) -> AirglowGeometryMetadata {
        match self {
            Self::VanRhijn(config) => AirglowGeometryMetadata {
                model: self.model_id(),
                implementation_version: VAN_RHIJN_IMPLEMENTATION_VERSION,
                emission_height_km: Some(config.emission_height_km()),
                profile_id: None,
                profile_schema_version: None,
                checksum_sha256: None,
                normalization: None,
                altitude_min_km: None,
                altitude_max_km: None,
                wavelength_min_nm: None,
                wavelength_max_nm: None,
                wavelength_band: None,
                assumptions: "geometrically thin, horizontally uniform emitting shell; historical NSB baseline".into(),
                provenance: "Van Rhijn (1921) analytic shell factor implemented by Siderust 0.11.0".into(),
                license: None,
                validated_zenith: config.validated_zenith_domain(),
            },
            Self::VerticalProfile(profile) => {
                let wavelength = profile.wavelength_applicability();
                AirglowGeometryMetadata {
                    model: self.model_id(),
                    implementation_version: VERTICAL_PROFILE_INTEGRATOR_VERSION,
                    emission_height_km: None,
                    profile_id: Some(profile.profile_id().to_string()),
                    profile_schema_version: Some(profile.schema_version()),
                    checksum_sha256: Some(profile.checksum_sha256().to_string()),
                    normalization: Some(profile.normalization().as_str()),
                    altitude_min_km: profile.altitude_km().first().copied(),
                    altitude_max_km: profile.altitude_km().last().copied(),
                    wavelength_min_nm: Some(wavelength.min),
                    wavelength_max_nm: Some(wavelength.max),
                    wavelength_band: Some(wavelength.band.clone()),
                    assumptions: profile.assumptions().to_string(),
                    provenance: profile.provenance().to_string(),
                    license: Some(profile.license().to_string()),
                    validated_zenith: profile.validated_zenith_domain(),
                }
            }
        }
    }
}

impl Default for AirglowGeometryModel {
    fn default() -> Self {
        Self::VanRhijn(VanRhijnConfig::default())
    }
}

/// Geometry provenance attached to Airglow component metadata.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct AirglowGeometryMetadata {
    /// `van_rhijn` or `vertical_profile`.
    pub model: &'static str,
    /// Versioned implementation identifier.
    pub implementation_version: &'static str,
    /// Thin-shell effective height, when applicable.
    pub emission_height_km: Option<Kilometers>,
    /// Vertical-profile identifier, when applicable.
    pub profile_id: Option<String>,
    /// Vertical-profile schema, when applicable.
    pub profile_schema_version: Option<u32>,
    /// Canonical vertical-profile checksum, when applicable.
    pub checksum_sha256: Option<String>,
    /// Vertical-profile normalization, when applicable.
    pub normalization: Option<&'static str>,
    /// Lowest profile altitude, when applicable.
    pub altitude_min_km: Option<Kilometers>,
    /// Highest profile altitude, when applicable.
    pub altitude_max_km: Option<Kilometers>,
    /// Lower wavelength applicability bound, when applicable.
    pub wavelength_min_nm: Option<Nanometers>,
    /// Upper wavelength applicability bound, when applicable.
    pub wavelength_max_nm: Option<Nanometers>,
    /// Wavelength band/process label, when applicable.
    pub wavelength_band: Option<String>,
    /// Scientific assumptions/reference state.
    pub assumptions: String,
    /// Geometry/profile provenance.
    pub provenance: String,
    /// Profile license, when applicable.
    pub license: Option<String>,
    /// Declared zenith-angle domain.
    pub validated_zenith: ValidatedZenithDomain,
}

pub(crate) fn target_altitude(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> Degrees {
    let jd = time.to::<TT>().to::<JD>();
    star_horizontal(target.ra(), target.dec(), &location, jd).alt()
}
