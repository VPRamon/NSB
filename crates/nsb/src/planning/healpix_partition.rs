// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! HEALPix pixel-transition cuts for fixed horizontal Starlight planning.
//!
//! Integrated Starlight uses nearest-pixel Galactic HEALPix lookup. A fixed
//! Alt/Az pointing sweeps across that map as Earth rotates, so radiance is
//! piecewise constant in celestial direction with discontinuous jumps at pixel
//! boundaries. Adaptive degree-4/8 fits across those jumps are not a valid
//! discovery contract.
//!
//! This module samples the resolved Galactic pixel index over a TT window and
//! binary-searches each transition. Partitioning at those instants restores
//! smooth (in fact constant-Starlight) sub-intervals for the adaptive engine.
//!
//! Guarantees:
//! - every transition found by the coarse sample grid is refined to
//!   [`TRANSITION_TOLERANCE_DAYS`];
//! - transitions shorter than the coarse step can still be missed if a pixel
//!   is entered and left between samples — the coarse step is sized from the
//!   map resolution and Earth rotation so that nominal crossings are sampled
//!   at least twice per pixel transit near the equator.
//!
//! Limitations: this is an empirical partition of the authoritative probe
//! signal, not a certified enclosure of continuous HEALPix geometry.

use super::intervals::tt_mjd_to_utc_time;
use crate::error::{NsbError, Result};
use crate::evaluator::{resolve_target_direction, NsbEvaluator, TargetDirection};
use crate::Observer;
use siderust::coordinates::transform::TransformFrame;
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};

/// Refine transitions to about 0.1 s so they sit with other adaptive brackets.
const TRANSITION_TOLERANCE_DAYS: f64 = 0.1 / 86_400.0;
/// Cap on reported transitions per window (resource guard).
const MAX_TRANSITIONS: usize = 25_000;
/// Sidereal rate used to size the coarse sample (degrees per day).
const SIDEREAL_DEG_PER_DAY: f64 = 360.985_612;

pub(crate) fn starlight_healpix_transitions(
    evaluator: &NsbEvaluator,
    observer: Observer,
    target: TargetDirection,
    window: TimePeriod<ModifiedJulianDate>,
) -> Result<Vec<ModifiedJulianDate>> {
    let TargetDirection::Horizontal(_) = target else {
        return Ok(Vec::new());
    };
    let Some(starlight) = evaluator.starlight() else {
        return Ok(Vec::new());
    };
    let grid = starlight.map().grid();
    let nside = grid.nside().get() as f64;
    // Mean pixel width ~ sqrt(4π / N_pix) rad ≈ 58.6/nside degrees.
    let pixel_deg = 58.6 / nside;
    // Sample at least ~4 times per typical equatorial pixel transit.
    let step_days =
        (pixel_deg / (4.0 * SIDEREAL_DEG_PER_DAY)).clamp(TRANSITION_TOLERANCE_DAYS, 1.0 / 24.0);

    let pixel_at = |mjd: ModifiedJulianDate| -> Result<u64> {
        let time = tt_mjd_to_utc_time(mjd);
        let equatorial = resolve_target_direction(target, observer, time)?;
        let galactic: siderust::coordinates::spherical::direction::Galactic = equatorial.to_frame();
        let index = grid
            .direction_to_pixel(galactic.to_cartesian())
            .map_err(|err| NsbError::OutOfRange(err.to_string()))?;
        Ok(index.get())
    };

    let duration = window.length().value();
    if !duration.is_finite() || duration <= 0.0 {
        return Ok(Vec::new());
    }

    let mut samples = Vec::new();
    let mut offset = 0.0;
    while offset < duration {
        samples.push(window.start + Days::new(offset));
        offset += step_days;
        if samples.len() > MAX_TRANSITIONS {
            return Err(NsbError::Unsupported(
                "horizontal Starlight HEALPix partition budget exceeded".into(),
            ));
        }
    }
    samples.push(window.end);

    let mut transitions = Vec::new();
    for pair in samples.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        let left_pix = pixel_at(left)?;
        let right_pix = pixel_at(right)?;
        if left_pix == right_pix {
            continue;
        }
        let mut lo = left;
        let mut hi = right;
        let lo_pix = left_pix;
        while (hi - lo).value() > TRANSITION_TOLERANCE_DAYS {
            let mid = lo + Days::new((hi - lo).value() * 0.5);
            if mid == lo || mid == hi {
                break;
            }
            let mid_pix = pixel_at(mid)?;
            if mid_pix == lo_pix {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        // Record the first instant where the pixel differs from the left sample.
        let cut = if pixel_at(hi)? != left_pix { hi } else { lo };
        if window.start < cut && cut < window.end {
            transitions.push(cut);
        }
        if transitions.len() > MAX_TRANSITIONS {
            return Err(NsbError::Unsupported(
                "horizontal Starlight HEALPix partition budget exceeded".into(),
            ));
        }
    }
    transitions.sort_by(|a, b| a.partial_cmp(b).expect("finite TT instants"));
    transitions.dedup();
    Ok(transitions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HorizontalTarget, NsbEvaluator, NsbModelConfig, Target, TargetDirection, DEG};
    use siderust::catalogs::observatories::ROQUE_DE_LOS_MUCHACHOS;
    use siderust::qtty::Days;
    use tempoch::{Time, UTC};

    fn utc(s: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            chrono::DateTime::parse_from_rfc3339(s)
                .expect("valid RFC3339")
                .with_timezone(&chrono::Utc),
        )
    }

    fn tt_window(start: &str, end: &str) -> TimePeriod<ModifiedJulianDate> {
        use super::super::intervals::utc_time_to_tt_mjd;
        TimePeriod::new(utc_time_to_tt_mjd(utc(start)), utc_time_to_tt_mjd(utc(end)))
    }

    #[test]
    fn equatorial_targets_yield_no_healpix_cuts() {
        let evaluator = NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky()).unwrap();
        let cuts = starlight_healpix_transitions(
            &evaluator,
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            TargetDirection::equatorial(Target::new(83.633 * DEG, 22.014 * DEG)),
            tt_window("2024-06-18T22:00:00Z", "2024-06-19T04:00:00Z"),
        )
        .unwrap();
        assert!(cuts.is_empty());
    }

    #[test]
    fn empty_or_inverted_windows_yield_no_cuts() {
        let evaluator = NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky()).unwrap();
        let horizontal =
            TargetDirection::horizontal(HorizontalTarget::new(60.0 * DEG, 130.0 * DEG));
        let start = {
            use super::super::intervals::utc_time_to_tt_mjd;
            utc_time_to_tt_mjd(utc("2024-06-18T22:00:00Z"))
        };
        let zero = TimePeriod::new(start, start);
        assert!(starlight_healpix_transitions(
            &evaluator,
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            horizontal,
            zero,
        )
        .unwrap()
        .is_empty());
        let inverted = TimePeriod::new(start + Days::new(1.0), start);
        assert!(starlight_healpix_transitions(
            &evaluator,
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            horizontal,
            inverted,
        )
        .unwrap()
        .is_empty());
    }

    #[test]
    fn horizontal_starlight_finds_pixel_transitions_over_hours() {
        let evaluator = NsbEvaluator::with_config(NsbModelConfig::generic_clear_sky()).unwrap();
        if evaluator.starlight().is_none() {
            return;
        }
        let cuts = starlight_healpix_transitions(
            &evaluator,
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            TargetDirection::horizontal(HorizontalTarget::new(55.0 * DEG, 120.0 * DEG)),
            tt_window("2024-06-18T22:00:00Z", "2024-06-19T06:00:00Z"),
        )
        .unwrap();
        assert!(
            !cuts.is_empty(),
            "fixed Alt/Az over eight hours should cross HEALPix pixels"
        );
    }
}
