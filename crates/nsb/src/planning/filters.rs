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
    if windows.is_empty() {
        return Ok(windows);
    }
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
        cuts.extend(zodiacal_boundaries(evaluator, prepared, &windows)?);
    }
    if matches!(prepared.starlight, PreparedStarlight::Dynamic) {
        cuts.extend(super::healpix_partition::starlight_healpix_transitions(
            evaluator,
            prepared.observer,
            prepared.target,
            &windows,
        )?);
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).expect("finite TT instants"));
    cuts.dedup_by(|right, left| (*right - *left).value().abs() <= super::transitions::TOLERANCE);
    let mut out = Vec::new();
    for window in windows.drain(..) {
        let mut left = window.start;
        // Coalesce with eligible endpoints too: all retained physical edges
        // must leave resolvable interiors on both sides of the cut.
        for &cut in cuts.iter().filter(|&&c| {
            (c - window.start).value() > super::transitions::TOLERANCE
                && (window.end - c).value() > super::transitions::TOLERANCE
        }) {
            out.push(TimePeriod::new(left, cut));
            left = cut;
        }
        out.push(TimePeriod::new(left, window.end));
    }
    Ok(out)
}

fn zodiacal_boundaries(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    windows: &[TimePeriod<ModifiedJulianDate>],
) -> crate::Result<Vec<ModifiedJulianDate>> {
    match prepared.target {
        TargetDirection::Equatorial(target) => zodiacal_boundaries_fixed_ecliptic(prepared, target),
        TargetDirection::Horizontal(_) => {
            zodiacal_boundaries_moving_target(evaluator, prepared, windows)
        }
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

/// Partition the model's actual variables. Signed latitude and relative
/// longitude retain the |beta| and 0/180-degree folding cusps. All rectangular
/// corner-fill edges are already on the 5-degree grid. Reddening depends on
/// true elongation, and Noll extinction on the interpolated 500 nm brightness.
/// This is sampled discovery; sub-probe grazing excursions are not certified.
fn zodiacal_boundaries_moving_target(
    evaluator: &NsbEvaluator,
    prepared: &PreparedThresholdQuery,
    windows: &[TimePeriod<ModifiedJulianDate>],
) -> crate::Result<Vec<ModifiedJulianDate>> {
    use super::intervals::tt_mjd_to_utc_time;
    use siderust::coordinates::{
        frames::EclipticMeanJ2000, spherical::Direction, transform::TransformFrame,
    };
    let noll =
        evaluator.config().zodiacal_extinction() == crate::ZodiacalExtinction::Noll2012Approx;
    super::transitions::discover(windows, 30.0 / 86400.0, |mjd| {
        let time = tt_mjd_to_utc_time(mjd);
        let target = resolve_target_direction(prepared.target, prepared.observer, time)?;
        let ecl: Direction<EclipticMeanJ2000> = target.to_frame();
        let sun = prepared.astronomy.solar(time).value().to_degrees();
        let relative = (ecl.lon().value() - sun + 180.0).rem_euclid(360.0) - 180.0;
        zodiacal_regime(ecl.lat().value(), relative, noll)
    })
}

fn zodiacal_regime(beta: f64, relative: f64, noll: bool) -> crate::Result<[i32; 5]> {
    use crate::components::zodiacal::leinert::Leinert1998Grid;
    use crate::units::angular::Radians;
    let latitude = beta.to_radians();
    let delta = relative.abs().to_radians();
    let elongation = (latitude.cos() * delta.cos())
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    let dex = if noll {
        let s10 = Leinert1998Grid::lookup_s10(Radians::new(latitude), Radians::new(delta))?;
        (s10.value() * crate::units::S10_TO_W_M2_SR_UM.value() / 1e-8).log10()
    } else {
        0.0
    };
    Ok([
        (beta / 5.0).floor() as i32,
        (relative / 5.0).floor() as i32,
        if elongation < 30.0 {
            0
        } else if elongation < 90.0 {
            1
        } else {
            2
        },
        i32::from(dex > 2.244),
        i32::from(dex > 2.255),
    ])
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

#[cfg(test)]
mod zodiacal_partition_tests {
    use super::*;
    use siderust::qtty::Days;

    #[test]
    fn empty_candidates_skip_even_unrepresentably_expensive_partitions() {
        use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
        use crate::{HorizontalTarget, ThresholdQuery, DEG};
        use siderust::catalogs::observatories::ROQUE_DE_LOS_MUCHACHOS;
        let evaluator = super::super::test_fixtures::continuum_default_evaluator();
        let start = tempoch::Time::<tempoch::UTC>::from_chrono(
            "2024-06-18T22:00:00Z"
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap(),
        );
        let query = ThresholdQuery::new(
            ROQUE_DE_LOS_MUCHACHOS.geodetic(),
            HorizontalTarget::new(55.0 * DEG, 120.0 * DEG),
            tempoch::Period::new(start, start + Days::new(1.0)),
            Radiance::new(100.0),
        )
        .with_components(ComponentMask::ZODIACAL | ComponentMask::STARLIGHT)
        .with_sun_altitude_ceiling(None);
        let context = evaluator.prepare_site_window_context(&query).unwrap();
        let mut prepared =
            super::super::prepare::prepare_target_threshold(&evaluator, &context, &query).unwrap();
        // Simulate an empty Sun/altitude intersection without doing 137 years
        // of astronomical eligibility preparation. No physical probe is useful.
        prepared.candidate_windows.clear();
        prepared.tt_window.end = prepared.tt_window.start + Days::new(50000.0);
        assert!(adaptive_threshold_windows(&evaluator, &prepared)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn relative_edge_inside_unchanged_absolute_cells_is_partitioned() {
        let origin = ModifiedJulianDate::new(60000.0);
        let window = TimePeriod::new(origin, origin + Days::new(30.0 / 86400.0));
        let cuts = super::super::transitions::discover(&[window], 30.0 / 86400.0, |t| {
            let fraction = (t - origin).value() / window.length().value();
            let lon = 12.0 + 2.0 * fraction;
            let sun = 7.5 + fraction;
            assert_eq!((lon / 5.0).floor(), 2.0);
            assert_eq!((sun / 5.0).floor(), 1.0);
            zodiacal_regime(35.0, lon - sun, false)
        })
        .unwrap();
        assert_eq!(cuts.len(), 1);
        assert!(((cuts[0] - origin).value() * 86400.0 - 15.0).abs() < 0.001);
    }

    #[test]
    fn model_regimes_cover_folds_corner_edges_reddening_and_extinction() {
        use crate::components::zodiacal::leinert::test_support::reference_lookup_s10;
        // Independent scalar historical lookup provides the extinction reference.
        // Positive/negative latitude, signed longitude wrap/folds, corner edges,
        // and true (rather than longitude-only) elongation transitions.
        for (beta_a, beta_b, lon_a, lon_b) in [
            (22.0, 22.0, 1.0, 35.0),
            (-12.0, -28.0, 18.0, 18.0),
            (-1.0, 1.0, 40.0, 40.0),
            (20.0, 20.0, 20.0, 40.0),
            (40.0, 40.0, 85.0, 95.0),
            (35.0, 35.0, -2.0, 2.0),
            (35.0, 35.0, 178.0, 182.0),
            (40.0, 40.0, 30.0, 100.0),
        ] {
            let origin = ModifiedJulianDate::new(60000.0);
            let window = TimePeriod::new(origin, origin + Days::new(300.0 / 86400.0));
            let coords = |t: ModifiedJulianDate| {
                let f = (t - origin).value() / window.length().value();
                (
                    beta_a + (beta_b - beta_a) * f,
                    (lon_a + (lon_b - lon_a) * f + 180.0).rem_euclid(360.0) - 180.0,
                )
            };
            let cuts = super::super::transitions::discover(&[window], 30.0 / 86400.0, |t| {
                let (beta, lon) = coords(t);
                zodiacal_regime(beta, lon, true)
            })
            .unwrap();
            let reference = |t| {
                let (beta, lon) = coords(t);
                let s10 = reference_lookup_s10(beta.to_radians(), lon.abs().to_radians()).unwrap();
                let dex = (s10 * 1.28).log10();
                let elongation = (beta.to_radians().cos() * lon.to_radians().cos())
                    .acos()
                    .to_degrees();
                [
                    (beta / 5.0).floor() as i32,
                    (lon / 5.0).floor() as i32,
                    if elongation < 30.0 {
                        0
                    } else if elongation < 90.0 {
                        1
                    } else {
                        2
                    },
                    i32::from(dex > 2.244),
                    i32::from(dex > 2.255),
                ]
            };
            let mut last = reference(window.start);
            let mut witnessed = 0;
            // 10 ms uniform reference, 1500 times finer than discovery probes.
            for i in 1..30000 {
                let t = origin + Days::new(i as f64 * 0.01 / 86400.0);
                let next = reference(t);
                if next != last {
                    assert!(
                        cuts.iter()
                            .any(|cut| (t - *cut).value().abs() * 86400.0 < 0.011),
                        "missing model edge beta={beta_a}..{beta_b}, lon={lon_a}..{lon_b}, at {i}"
                    );
                    witnessed += 1;
                }
                last = next;
            }
            assert!(witnessed > 0);
        }
    }
}
