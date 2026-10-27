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
//! Discovery probes are spaced at most 1/64 of a nominal equatorial pixel
//! transit (mean width 58.6/nside degrees, sidereal speed 360.986 deg/day).
//! HEALPix cells meet at vertices: arbitrarily short grazing transits exist,
//! so mean width is a resolution scale, never a lower bound on transit time.
//! Midpoint probing and recursive refinement of both halves find return visits
//! and multiple edges when witnessed by a probe. Refinement is to 0.1 ms.
//! Excursions wholly between probes remain an empirical limitation.
//!
//! Only eligible intervals are streamed; memory is proportional to discovered
//! cuts, with separate total probe and transition resource guards.

use super::intervals::tt_mjd_to_utc_time;
use crate::error::{NsbError, Result};
use crate::evaluator::{resolve_target_direction, NsbEvaluator, TargetDirection};
use crate::Observer;
use siderust::coordinates::transform::TransformFrame;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};

/// Sidereal rate used to size the coarse sample (degrees per day).
const SIDEREAL_DEG_PER_DAY: f64 = 360.985_612;

pub(crate) fn starlight_healpix_transitions(
    evaluator: &NsbEvaluator,
    observer: Observer,
    target: TargetDirection,
    windows: &[TimePeriod<ModifiedJulianDate>],
) -> Result<Vec<ModifiedJulianDate>> {
    let TargetDirection::Horizontal(_) = target else {
        return Ok(Vec::new());
    };
    let Some(starlight) = evaluator.starlight() else {
        return Ok(Vec::new());
    };
    let grid = starlight.map().grid();
    healpix_transitions(grid, observer, target, windows)
}

fn healpix_transitions(
    grid: siderust::healpix::HealpixGrid,
    observer: Observer,
    target: TargetDirection,
    windows: &[TimePeriod<ModifiedJulianDate>],
) -> Result<Vec<ModifiedJulianDate>> {
    let nside = grid.nside().get() as f64;
    // Mean pixel width ~ sqrt(4π / N_pix) rad ≈ 58.6/nside degrees.
    let pixel_deg = 58.6 / nside;
    // discover() adds midpoint probes: 64 probes per mean pixel transit.
    let step_days = (pixel_deg / (32.0 * SIDEREAL_DEG_PER_DAY))
        .clamp(super::transitions::TOLERANCE, 1.0 / 1440.0);

    let pixel_at = |mjd: ModifiedJulianDate| -> Result<u64> {
        let time = tt_mjd_to_utc_time(mjd);
        let equatorial = resolve_target_direction(target, observer, time)?;
        let galactic: siderust::coordinates::spherical::direction::Galactic = equatorial.to_frame();
        let index = grid
            .direction_to_pixel(galactic.to_cartesian())
            .map_err(|err| NsbError::OutOfRange(err.to_string()))?;
        Ok(index.get())
    };

    super::transitions::discover(windows, step_days, pixel_at)
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
            &[tt_window("2024-06-18T22:00:00Z", "2024-06-19T04:00:00Z")],
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
            &[zero],
        )
        .unwrap()
        .is_empty());
        let inverted = TimePeriod::new(start + Days::new(1.0), start);
        assert!(starlight_healpix_transitions(
            &evaluator,
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            horizontal,
            &[inverted],
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
            &[tt_window("2024-06-18T22:00:00Z", "2024-06-19T06:00:00Z")],
        )
        .unwrap();
        assert!(
            !cuts.is_empty(),
            "fixed Alt/Az over eight hours should cross HEALPix pixels"
        );
    }
    #[test]
    fn dense_reference_pixel_transitions() {
        use siderust::catalogs::observatories::EL_PARANAL;
        use siderust::healpix::{HealpixGrid, Nside};
        let grid = HealpixGrid::ring(Nside::new(128).unwrap()).unwrap();
        let base_window = tt_window("2024-06-18T22:00:00Z", "2024-06-19T02:00:00Z");
        let mut short_visits = 0;
        for (observer, alt, az) in [
            (ROQUE_DE_LOS_MUCHACHOS.geodetic(), 55.0, 120.0),
            (EL_PARANAL.geodetic(), 30.0, 45.0),
            (ROQUE_DE_LOS_MUCHACHOS.geodetic(), 70.0, 270.0),
        ] {
            let window = if alt == 55.0 {
                TimePeriod::new(base_window.start, base_window.start + Days::new(0.25))
            } else {
                base_window
            };
            let target = TargetDirection::from(HorizontalTarget::new(alt * DEG, az * DEG));
            let pixel = |t| {
                let eq = target.resolve(observer, tt_mjd_to_utc_time(t)).unwrap();
                let gal: siderust::coordinates::spherical::direction::Galactic = eq.to_frame();
                grid.direction_to_pixel(gal.to_cartesian()).unwrap().get()
            };
            let cuts = healpix_transitions(grid, observer, target, &[window]).unwrap();
            let mut reference = Vec::new();
            let mut last = pixel(window.start);
            // Independent uniform 0.25 s scan, without discovery or refinement.
            for i in 1..=(window.length().value() * 86400.0 / 0.25).round() as usize {
                let t = window.start + Days::new(i as f64 * 0.25 / 86400.0);
                let next = pixel(t);
                if next != last {
                    reference.push(t);
                    last = next;
                }
            }
            assert_eq!(cuts.len(), reference.len(), "alt={alt}, az={az}");
            if alt == 55.0 {
                assert_eq!(reference.len(), 241, "frozen six-hour NSIDE128 regression");
            }
            for (cut, witness) in cuts.iter().zip(&reference) {
                assert!((*witness - *cut).value().abs() * 86400.0 <= 0.251);
            }
            short_visits += reference
                .windows(2)
                .filter(|p| (p[1] - p[0]).value() * 86400.0 < 27.4)
                .count();
            // Force two near-vertex transitions inside one old 27.4 s
            // discovery bracket. A single binary search can return only one.
            if let Some(pair) = reference
                .windows(2)
                .find(|p| (p[1] - p[0]).value() * 86400.0 < 20.0)
            {
                let center = pair[0] + Days::new((pair[1] - pair[0]).value() * 0.5);
                let bracket = TimePeriod::new(
                    center - Days::new(13.7 / 86400.0),
                    center + Days::new(13.7 / 86400.0),
                );
                let local = healpix_transitions(grid, observer, target, &[bracket]).unwrap();
                assert!(
                    local.len() >= 2,
                    "multiple transitions inside old coarse interval"
                );
            }
            // Splitting at a transition (stream boundary) cannot drop it.
            let split = cuts[cuts.len() / 2];
            let split_cuts = healpix_transitions(
                grid,
                observer,
                target,
                &[
                    TimePeriod::new(window.start, split),
                    TimePeriod::new(split, window.end),
                ],
            )
            .unwrap();
            assert_eq!(split_cuts.len(), cuts.len());
            for (a, b) in split_cuts.iter().zip(cuts.iter()) {
                assert!((*a - *b).value().abs() * 86400.0 <= 0.001);
            }
        }
        assert!(
            short_visits > 0,
            "fixtures must exercise near-vertex short transits"
        );
    }

    #[test]
    fn long_and_disjoint_windows_stream_without_sample_budget_rejection() {
        use siderust::healpix::{HealpixGrid, Nside};
        let grid = HealpixGrid::ring(Nside::new(128).unwrap()).unwrap();
        let window = tt_window("2024-06-18T22:00:00Z", "2024-06-28T22:00:00Z");
        let observer = ROQUE_DE_LOS_MUCHACHOS.geodetic();
        let target = TargetDirection::from(HorizontalTarget::new(55.0 * DEG, 120.0 * DEG));
        let cuts = healpix_transitions(grid, observer, target, &[window]).unwrap();
        assert!(cuts.len() > 1000);
        let windows = [
            TimePeriod::new(window.start, window.start + Days::new(0.1)),
            TimePeriod::new(window.end - Days::new(0.1), window.end),
        ];
        let selected = healpix_transitions(grid, observer, target, &windows).unwrap();
        assert!(!selected.is_empty());
        assert!(selected
            .iter()
            .all(|t| windows.iter().any(|w| w.start <= *t && *t <= w.end)));
        assert!(healpix_transitions(grid, observer, target, &[])
            .unwrap()
            .is_empty());
    }
}
