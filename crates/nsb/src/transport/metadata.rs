// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Model-only transport metadata.
//!
//! Atmosphere/profile identity is intentionally absent: it belongs with the
//! evaluation context that owns the actual [`crate::site::AtmosphericConditions`],
//! not an independent caller-supplied string.

use super::extinction::{ExtinctionIngredients, MolecularAbsorption};
use super::model::AirmassModel;

/// Immutable description of a transport **model configuration**.
///
/// This does not record which atmosphere was evaluated. Evaluation provenance
/// must come from the site/evaluator context that supplied
/// [`crate::site::AtmosphericConditions`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TransportModelMetadata {
    /// Transport model identity (`identity`, `direct-transmission`).
    pub model_id: &'static str,
    /// Whether Rayleigh extinction is configured (direct path only).
    pub rayleigh: bool,
    /// Whether Mie extinction is configured (direct path only).
    pub mie: bool,
    /// Molecular absorption treatment.
    pub absorption: MolecularAbsorption,
    /// Airmass model identity, or `none` for identity transport.
    pub airmass_model_id: &'static str,
    /// Approximation class identity.
    pub approximation: &'static str,
    /// Human-readable validated-domain statement.
    pub validated_domain: &'static str,
    /// Implementation provenance.
    pub provenance: &'static str,
    /// Uncertainty reporting policy identity (`absent` when none is claimed).
    pub uncertainty: &'static str,
}

impl TransportModelMetadata {
    pub(crate) fn identity() -> Self {
        Self {
            model_id: "identity",
            rayleigh: false,
            mie: false,
            absorption: MolecularAbsorption::None,
            airmass_model_id: "none",
            approximation: "exact-within-representation",
            validated_domain: "any finite typed spectral radiance; atmosphere ignored",
            provenance: concat!(
                "NSB identity atmospheric transport; no extinction or scattering; ",
                "output equals input within the quantity representation"
            ),
            uncertainty: "absent",
        }
    }

    pub(crate) fn direct(ingredients: ExtinctionIngredients, airmass: AirmassModel) -> Self {
        Self {
            model_id: "direct-transmission",
            rayleigh: ingredients.rayleigh,
            mie: ingredients.mie,
            absorption: ingredients.absorption,
            airmass_model_id: airmass.as_str(),
            approximation: "clear-sky-single-column",
            validated_domain: concat!(
                "clear-sky celestial direct path; zenith in [0, 90] degrees; ",
                "individual airmass formulas may be weakly validated near the horizon; ",
                "not a site-calibrated extinction law"
            ),
            provenance: concat!(
                "NSB direct atmospheric transport composing Siderust Bodhaine Rayleigh, ",
                "Patat Mie, optional Siderust ozone transmittance table, and Siderust ",
                "Beer–Lambert transmission; NSB local-pressure Rayleigh helper avoids ",
                "double-counting site-profile surface pressure with altitude"
            ),
            uncertainty: "absent",
        }
    }
}
