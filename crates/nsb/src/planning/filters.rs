// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Physical regime partitions and eligibility edges for numerical discovery.

use super::types::{PreparedStarlight, PreparedThresholdQuery};
use crate::components::airglow;
use crate::evaluator::{resolve_target_direction, ComponentMask, TargetDirection};
use crate::NsbEvaluator;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};

/// Additional regimes used by practical discovery only. The independent reference scanner
/// retains the original windows and therefore the original grid phase.
pub(crate) fn adaptive_threshold_windows(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
) -> crate::Result<Vec<TimePeriod<ModifiedJulianDate>>> {
    use super::intervals::{tt_mjd_to_utc_time, utc_time_to_tt_mjd};
    use siderust::coordinates::{spherical::direction, transform::TransformFrame};
    use siderust::event::altitude::{above_threshold, SearchOpts};
    let mut windows = threshold_windows(prepared, false);
    // Constant equatorial Starlight alone needs no extra regime cuts. Dynamic
    // horizontal Starlight still requires HEALPix pixel-transition partitions.
    if prepared
        .components
        .difference(ComponentMask::STARLIGHT)
        .is_empty()
        && !matches!(prepared.starlight, PreparedStarlight::Dynamic)
    {
        return Ok(windows);
    }
    let mut cuts: Vec<ModifiedJulianDate> = Vec::new();

    // Source and Moonlight have a physical horizon branch even if the caller
    // clears the visibility filter or requests a negative altitude floor.
    // Fixed horizontal targets have constant altitude: either always above or
    // always below the geometric horizon — no time-varying horizon cuts.
    match prepared.target {
        TargetDirection::Equatorial(target) => {
            let target_icrs: direction::ICRS = target.to_frame();
            let horizon = above_threshold(
                &target_icrs,
                &prepared.observer,
                prepared.tt_window,
                crate::units::angular::Degrees::new(0.0),
                SearchOpts::default(),
            );
            cuts.extend(horizon.iter().flat_map(|p| [p.start, p.end]));
        }
        TargetDirection::Horizontal(_) => {}
    }

    let start = tt_mjd_to_utc_time(prepared.tt_window.start)
        .to_chrono()
        .ok_or_else(|| {
            crate::NsbError::Unsupported("adaptive calendar partition is not representable".into())
        })?;
    let end = tt_mjd_to_utc_time(prepared.tt_window.end)
        .to_chrono()
        .ok_or_else(|| {
            crate::NsbError::Unsupported("adaptive calendar partition is not representable".into())
        })?;
    let mut day = start
        .date_naive()
        .and_time(chrono::NaiveTime::MIN)
        .and_utc();
    while day < end {
        cuts.push(utc_time_to_tt_mjd(
            tempoch::Time::<tempoch::UTC>::from_chrono(day),
        ));
        day = day
            .checked_add_signed(chrono::Duration::days(1))
            .ok_or_else(|| {
                crate::NsbError::Unsupported("adaptive calendar partition overflow".into())
            })?;
        if cuts.len() > 25_000 {
            return Err(crate::NsbError::Unsupported(
                "adaptive physical partition budget".into(),
            ));
        }
    }
    if let Some(cache) = &prepared.solar_activity_cache {
        cuts.extend(cache.transitions().iter().copied().map(utc_time_to_tt_mjd));
    }
    if prepared.components.contains(ComponentMask::ZODIACAL) {
        cuts.extend(zodiacal_boundaries(prepared)?);
    }
    if matches!(prepared.starlight, PreparedStarlight::Dynamic) {
        cuts.extend(super::healpix_partition::starlight_healpix_transitions(
            evaluator,
            prepared.observer,
            prepared.target,
            prepared.tt_window,
        )?);
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).expect("finite TT instants"));
    cuts.dedup();
    let mut out = Vec::new();
    for window in windows.drain(..) {
        let mut left = window.start;
        for &cut in cuts.iter().filter(|&&c| window.start < c && c < window.end) {
            out.push(TimePeriod::new(left, cut));
            left = cut;
        }
        out.push(TimePeriod::new(left, window.end));
    }
    Ok(out)
}

fn zodiacal_boundaries(
    prepared: &PreparedThresholdQuery,
) -> crate::Result<Vec<ModifiedJulianDate>> {
    match prepared.target {
        TargetDirection::Equatorial(target) => {
            zodiacal_boundaries_fixed_ecliptic(prepared, target)
        }
        TargetDirection::Horizontal(_) => zodiacal_boundaries_moving_target(prepared),
    }
}

fn zodiacal_boundaries_fixed_ecliptic(
    prepared: &PreparedThresholdQuery,
    target: crate::Target,
) -> crate::Result<Vec<ModifiedJulianDate>> {
    use siderust::bodies::Sun;
    use siderust::coordinates::{
        frames::EclipticMeanJ2000, spherical::Direction, transform::TransformFrame,
    };
    let ecl: Direction<EclipticMeanJ2000> = target.to_frame();
    let lon = ecl.lon().value();
    let beta = ecl.lat().value().to_radians();
    // All 5-degree Leinert cells, folding points, and constant-fill corner
    // edges. Latitude is fixed. Reddening changes slope at elongation 30/90.
    let mut edges: Vec<_> = (0..72)
        .map(|i| (lon + i as f64 * 5.0).rem_euclid(360.0))
        .collect();
    if beta.abs() < 30.0_f64.to_radians() {
        let delta = (30.0_f64.to_radians().cos() / beta.cos())
            .acos()
            .to_degrees();
        edges.extend([
            (lon - delta).rem_euclid(360.0),
            (lon + delta).rem_euclid(360.0),
        ]);
    }
    // Root coordinates are elapsed TT days, never a flattened absolute MJD.
    let origin = prepared.tt_window.start;
    let solar = |offset_days: f64| {
        Sun::ecliptic_longitude_geocentric(
            (origin + siderust::qtty::Days::new(offset_days)).to::<tempoch::JD>(),
        )
        .value()
        .to_degrees()
    };
    let mut out = Vec::new();
    let mut a = 0.0;
    let end = prepared.tt_window.length().value();
    while a < end {
        let b = (a + 1.0).min(end);
        let base = solar(a);
        let advance = (solar(b) - base).rem_euclid(360.0);
        if !advance.is_finite() || advance > 2.0 {
            return Err(crate::NsbError::Unsupported(
                "unresolved Zodiacal solar-longitude regime".into(),
            ));
        }
        for &edge in &edges {
            let delta = (edge - base).rem_euclid(360.0);
            if delta > 0.0 && delta < advance {
                let (mut lo, mut hi) = (a, b);
                while hi - lo > 0.0001 / 86_400.0 {
                    let mid = lo + (hi - lo) * 0.5;
                    if mid == lo || mid == hi {
                        break;
                    }
                    if (solar(mid) - base).rem_euclid(360.0) < delta {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                out.push(origin + siderust::qtty::Days::new(lo + (hi - lo) * 0.5));
            }
        }
        a = b;
    }
    Ok(out)
}

/// Zodiacal table cells for a moving (horizontal) target.
///
/// Both solar longitude and the target's ecliptic coordinates evolve. Sample
/// the `(λ_target, β_target, λ_sun)` Leinert cell identity and cut where it
/// changes. This is an empirical partition aligned with the adaptive contract,
/// not a continuous certificate of every infinitesimal table edge.
fn zodiacal_boundaries_moving_target(
    prepared: &PreparedThresholdQuery,
) -> crate::Result<Vec<ModifiedJulianDate>> {
    use super::intervals::tt_mjd_to_utc_time;
    use siderust::bodies::Sun;
    use siderust::coordinates::{
        frames::EclipticMeanJ2000, spherical::Direction, transform::TransformFrame,
    };
    use siderust::qtty::Days;

    let origin = prepared.tt_window.start;
    let end = prepared.tt_window.length().value();
    // ~2 min samples: finer than typical solar-longitude cell motion and
    // comparable to equatorial HEALPix transit times near nside=128.
    let step = (2.0_f64 / 1_440.0).min(end.max(1e-12));

    let cell_at = |offset_days: f64| -> crate::Result<(i32, i32, i32)> {
        let mjd = origin + Days::new(offset_days);
        let time = tt_mjd_to_utc_time(mjd);
        let target = resolve_target_direction(prepared.target, prepared.observer, time)?;
        let ecl: Direction<EclipticMeanJ2000> = target.to_frame();
        let lon = ecl.lon().value().rem_euclid(360.0);
        let lat = ecl.lat().value();
        let sun = Sun::ecliptic_longitude_geocentric(mjd.to::<tempoch::JD>())
            .value()
            .to_degrees()
            .rem_euclid(360.0);
        // 5° Leinert longitude cells; 5° latitude bands; solar elongation proxy.
        Ok((
            (lon / 5.0).floor() as i32,
            (lat / 5.0).floor() as i32,
            (sun / 5.0).floor() as i32,
        ))
    };

    let mut cuts = Vec::new();
    let mut a = 0.0;
    let mut prev = cell_at(0.0)?;
    while a < end {
        let b = (a + step).min(end);
        let next = cell_at(b)?;
        if next != prev {
            let (mut lo, mut hi) = (a, b);
            let left = prev;
            while hi - lo > 0.1 / 86_400.0 {
                let mid = lo + (hi - lo) * 0.5;
                if mid == lo || mid == hi {
                    break;
                }
                if cell_at(mid)? == left {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let cut = origin + Days::new(lo + (hi - lo) * 0.5);
            if prepared.tt_window.start < cut && cut < prepared.tt_window.end {
                cuts.push(cut);
            }
            prev = next;
        }
        a = b;
        if cuts.len() > 25_000 {
            return Err(crate::NsbError::Unsupported(
                "horizontal Zodiacal partition budget".into(),
            ));
        }
    }
    Ok(cuts)
}

pub(crate) fn contains_time(
    periods: &[TimePeriod<ModifiedJulianDate>],
    time: ModifiedJulianDate,
) -> bool {
    periods
        .iter()
        .any(|period| period.start <= time && time <= period.end)
}

pub(super) fn threshold_windows(
    prepared: &PreparedThresholdQuery,
    split_phase: bool,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let mut out = Vec::new();
    for candidate in &prepared.candidate_windows {
        let mut boundaries = vec![candidate.start, candidate.end];
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            if split_phase {
                for phase in prepared.airglow_phase_periods.iter() {
                    collect_internal_boundaries(&mut boundaries, phase.period, *candidate);
                }
            } else {
                // PALACE uses month/hour rows; the retained night-third identity
                // is ignored by its radiance formula. Only night edges jump.
                for night in prepared.astronomical_night_periods.iter() {
                    collect_internal_boundaries(&mut boundaries, night.period, *candidate);
                }
            }
            for night in prepared.astronomical_night_periods.iter() {
                let start = candidate.start.max(night.period.start);
                let end = candidate.end.min(night.period.end);
                if start < end {
                    boundaries.extend(airglow::temporal::palace_climatology_boundaries_for_window(
                        TimePeriod::new(start, end),
                        prepared.observer,
                    ));
                }
            }
        }
        if let Some(moon_visible_periods) = &prepared.moon_visible_periods {
            for moon_period in moon_visible_periods.iter() {
                collect_internal_boundaries(&mut boundaries, *moon_period, *candidate);
            }
        }

        boundaries.sort_by(|lhs, rhs| lhs.partial_cmp(rhs).expect("finite TT instants"));
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start < end {
                out.push(TimePeriod::new(start, end));
            }
        }
    }
    out
}

fn collect_internal_boundaries(
    boundaries: &mut Vec<ModifiedJulianDate>,
    period: TimePeriod<ModifiedJulianDate>,
    window: TimePeriod<ModifiedJulianDate>,
) {
    for boundary in [period.start, period.end] {
        if window.start < boundary && boundary < window.end {
            boundaries.push(boundary);
        }
    }
}

pub(crate) fn airglow_night_phase(
    prepared: &PreparedThresholdQuery,
    time: ModifiedJulianDate,
) -> Option<airglow::AirglowNightPhase> {
    let phase =
        airglow::temporal::night_phase_from_nights(time, &prepared.astronomical_night_periods);
    #[cfg(debug_assertions)]
    {
        let precomputed_phase = airglow::temporal::night_phase_from_phase_periods(
            time,
            &prepared.airglow_phase_periods,
        );
        if let (Some(phase), Some(precomputed_phase)) = (phase, precomputed_phase) {
            debug_assert_eq!(phase, precomputed_phase);
        }
    }
    phase
}
