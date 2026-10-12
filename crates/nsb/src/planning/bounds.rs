// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Internal feasibility enclosures, deliberately compiled only for tests.

use super::types::PreparedThresholdQuery;
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

/// Only two proofs are admitted in this spike:
/// - cached Starlight is exactly the same scalar added by evaluate_integrated;
/// - the prepared evaluator skips Moonlight outside its closed visibility set.
///
/// No interpolation, geometry, sum rounding, or new physical assumptions are
/// involved. This certifies the prepared planning signal, conditional on the
/// existing Siderust visibility partitions, not independently the true sky.
/// Airglow still validates inputs outside night, so it cannot use a zero shortcut.
pub(super) fn prepared_bounds(
    prepared: &PreparedThresholdQuery,
    window: Interval<ModifiedJulianDate>,
) -> Result<Option<RadianceBounds>> {
    let supported = ComponentMask::STARLIGHT | ComponentMask::MOON;
    if !prepared.components.difference(supported).is_empty() {
        return Ok(None);
    }
    if prepared.components.contains(ComponentMask::MOON) {
        let Some(visible) = &prepared.moon_visible_periods else {
            return Ok(None);
        };
        // Touching a boundary is an overlap: contains_time is inclusive.
        if visible
            .iter()
            .any(|p| p.start <= window.end && window.start <= p.end)
        {
            return Ok(None);
        }
    }
    let value = if prepared.components.contains(ComponentMask::STARLIGHT) {
        prepared.starlight_integrated
    } else {
        Radiance::new(0.0)
    };
    Ok(Some(RadianceBounds::constant(value)?))
}
