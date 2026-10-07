// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Physical origin of radiance presented to atmospheric transport.

/// Classification of radiance relative to the atmosphere.
///
/// Direct Beer–Lambert celestial transport is valid only for
/// [`RadianceOrigin::TopOfAtmosphere`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RadianceOrigin {
    /// Celestial / exo-atmospheric source (zodiacal, starlight, future DGL).
    TopOfAtmosphere,
    /// Emission generated inside the atmosphere (airglow).
    AtmosphericEmission,
    /// Radiance that already includes atmospheric scattering (legacy moonlight).
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

    pub(crate) const fn allows_celestial_direct_transmission(self) -> bool {
        matches!(self, Self::TopOfAtmosphere)
    }
}
