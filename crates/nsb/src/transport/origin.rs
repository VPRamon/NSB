//! Physical origin of radiance presented to atmospheric transport.
//!
//! Transport models must not silently apply a celestial top-of-atmosphere
//! path to every NSB component. Airglow is emitted inside the atmosphere;
//! Jones moonlight already embeds atmospheric scattering.

/// Classification of radiance relative to the atmosphere.
///
/// Callers use this to select a physically appropriate transport path. The
/// generic celestial direct-transmission model is valid only for
/// [`RadianceOrigin::TopOfAtmosphere`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RadianceOrigin {
    /// Celestial / exo-atmospheric source (zodiacal, starlight, future DGL).
    ///
    /// Direct Beer–Lambert extinction along the line of sight is appropriate.
    TopOfAtmosphere,

    /// Emission generated inside the atmosphere (airglow).
    ///
    /// Emitting-volume geometry and effective attenuation differ from a
    /// celestial top-of-atmosphere path. Do not force this origin through
    /// [`super::TransportModel::Direct`] without an airglow-specific model.
    AtmosphericEmission,

    /// Radiance that already includes atmospheric scattering (legacy Jones /
    /// KS91 moonlight paths).
    ///
    /// Applying generic transport again would double-count scattering.
    PreScatteredAtmosphere,
}

impl RadianceOrigin {
    /// Stable machine-readable identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopOfAtmosphere => "top-of-atmosphere",
            Self::AtmosphericEmission => "atmospheric-emission",
            Self::PreScatteredAtmosphere => "pre-scattered-atmosphere",
        }
    }

    /// Whether [`super::TransportModel::Direct`] is a physically correct
    /// celestial Beer–Lambert path for this origin.
    pub const fn allows_celestial_direct_transmission(self) -> bool {
        matches!(self, Self::TopOfAtmosphere)
    }
}
