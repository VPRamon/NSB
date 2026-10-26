// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Closed-interval certificates for the authoritative prepared planning signal.

use super::types::{PreparedStarlight, PreparedThresholdQuery};
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{ComponentMask, NsbError, Result};
use siderust::time::{Interval, ModifiedJulianDate};

/// Provider obligation: enclose *every authoritative floating-point output*
/// over the closed interval, and establish that evaluation is defined there.
/// This is stronger than enclosing a few samples or the ideal real formula.
/// Providers doing arithmetic must round outward, including model error.
#[derive(Debug, Clone, Copy)]
pub(super) struct RadianceBounds {
    pub lower: Radiance,
    pub upper: Radiance,
}

impl RadianceBounds {
    pub fn new(lower: Radiance, upper: Radiance) -> Result<Self> {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(NsbError::OutOfRange("invalid radiance enclosure".into()));
        }
        Ok(Self { lower, upper })
    }

    pub fn constant(value: Radiance) -> Result<Self> {
        Self::new(value, value)
    }
}

/// Sound whole-interval classification. Errors propagate through `Result`.
#[derive(Debug, Clone, Copy)]
pub(super) enum BoundOutcome {
    /// A finite enclosure; inconclusive production bounds fail explicitly.
    Enclosure { range: RadianceBounds },
    /// Known regime boundaries prevent an enclosure at this scale. Splitting
    /// can isolate a supported regime (for example a Moon-down interior).
    NeedsSubdivision,
    /// No temporal envelope exists here. More point probes or subdivisions
    /// cannot manufacture one; retain the entire region as unresolved.
    Unsupported,
}

impl BoundOutcome {
    pub fn enclosure(range: RadianceBounds) -> Self {
        Self::Enclosure { range }
    }
}

/// Only two proofs are currently admitted:
/// - cached constant Starlight is exactly the same scalar added by
///   evaluate_integrated (equatorial targets only);
/// - the prepared evaluator skips Moonlight outside its closed visibility set.
///
/// Dynamic (horizontal) Starlight has no whole-interval constant certificate:
/// HEALPix nearest-pixel jumps make a smooth enclosure invalid, and adaptive
/// evidence is never promoted to a certificate. Callers receive
/// [`BoundOutcome::Unsupported`] rather than a false enclosure.
///
/// No interpolation, geometry, sum rounding, or new physical assumptions are
/// involved. This certifies the prepared planning signal, conditional on the
/// existing Siderust visibility partitions, not independently the true sky.
/// Airglow still validates inputs outside night, so it cannot use a zero shortcut.
pub(super) fn prepared_bounds(
    prepared: &PreparedThresholdQuery,
    window: Interval<ModifiedJulianDate>,
) -> Result<BoundOutcome> {
    let supported = ComponentMask::STARLIGHT | ComponentMask::MOON;
    if !prepared.components.difference(supported).is_empty() {
        return Ok(BoundOutcome::Unsupported);
    }
    if matches!(prepared.starlight, PreparedStarlight::Dynamic) {
        // Horizontal Starlight varies with Earth rotation and HEALPix pixels.
        // Do not manufacture a constant-target certificate.
        return Ok(BoundOutcome::Unsupported);
    }
    if prepared.components.contains(ComponentMask::MOON) {
        let Some(visible) = &prepared.moon_visible_periods else {
            return Ok(BoundOutcome::Unsupported);
        };
        // Touching a boundary is an overlap: contains_time is inclusive.
        if visible
            .iter()
            .any(|p| p.start <= window.start && window.end <= p.end)
        {
            return Ok(BoundOutcome::Unsupported);
        }
        if visible
            .iter()
            .any(|p| p.start <= window.end && window.start <= p.end)
        {
            return Ok(BoundOutcome::NeedsSubdivision);
        }
    }
    let value = match prepared.starlight {
        PreparedStarlight::Constant(value) => value,
        PreparedStarlight::Disabled => Radiance::new(0.0),
        PreparedStarlight::Dynamic => {
            return Ok(BoundOutcome::Unsupported);
        }
    };
    Ok(BoundOutcome::enclosure(RadianceBounds::constant(value)?))
}
