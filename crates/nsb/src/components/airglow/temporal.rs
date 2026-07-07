use chrono::Datelike;
use qtty::angular::Degrees;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
#[cfg(test)]
use std::cell::Cell;
use tempoch::{Time, MJD, UTC};

pub(crate) const ASTRONOMICAL_TWILIGHT_DEG: f64 = -18.0;
const INITIAL_NIGHT_SEARCH_RADIUS_DAYS: f64 = 2.0;
const MAX_NIGHT_SEARCH_RADIUS_DAYS: f64 = 200.0;
const NIGHT_SEARCH_EXPANSION_FACTOR: f64 = 4.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AstronomicalNightPeriod {
    pub(crate) period: TimePeriod<ModifiedJulianDate>,
    pub(crate) phase_bounded: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AirglowPhasePeriod {
    pub(crate) period: TimePeriod<ModifiedJulianDate>,
    pub(crate) time_bin: usize,
}

#[cfg(test)]
thread_local! {
    static POINT_NIGHT_SEARCH_FORBIDDEN: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn season_bin(time: Time<UTC>, location: Geodetic<ECEF>) -> usize {
    let Some(dt) = local_solar_datetime(time, location) else {
        return 0;
    };
    match dt.month() {
        12 | 1 => 1,
        2 | 3 => 2,
        4 | 5 => 3,
        6 | 7 => 4,
        8 | 9 => 5,
        10 | 11 => 6,
        _ => 0,
    }
}

/// Site-aware airglow time bin based on astronomical-night phase.
///
/// The SkyCalc-derived airglow calibration table defines three equal time
/// ranges over the full astronomical-night interval (`alt_sun < -18°`). We
/// therefore compute the complete astronomical night containing `time` from
/// Siderust solar-altitude events, normalize `time` to phase in that interval,
/// and map `[0, 1/3)`, `[1/3, 2/3)`, `[2/3, 1]` to rows 1, 2, and 3.
///
/// The search expands adaptively so high-latitude winter nights are not
/// mistaken for missing airglow merely because the first local window is clipped.
/// If a final expanded search still sees continuous night without both bounding
/// crossings, row 0 (full-night correction) is used rather than returning zero.
pub(crate) fn time_of_night_bin(time: Time<UTC>, location: Geodetic<ECEF>) -> Option<usize> {
    let time_tt = utc_time_to_tt_mjd(time);
    let night = astronomical_night_containing(time_tt, location)?;
    time_of_night_bin_from_night(time_tt, &night)
}

pub(crate) fn is_astronomical_twilight(threshold: Degrees) -> bool {
    (threshold.value() - ASTRONOMICAL_TWILIGHT_DEG).abs() <= f64::EPSILON
}

pub(crate) fn astronomical_nights_for_window(
    window: TimePeriod<ModifiedJulianDate>,
    location: Geodetic<ECEF>,
) -> Vec<AstronomicalNightPeriod> {
    if window.start >= window.end {
        return Vec::new();
    }

    let search_window = expand_window(window, MAX_NIGHT_SEARCH_RADIUS_DAYS);
    SunBody
        .below_threshold(
            &location,
            search_window,
            Degrees::new(ASTRONOMICAL_TWILIGHT_DEG),
            SearchOpts::default(),
        )
        .into_iter()
        .filter(|night| night.end > window.start && night.start < window.end)
        .map(|night| AstronomicalNightPeriod {
            phase_bounded: night.start > search_window.start && night.end < search_window.end,
            period: night,
        })
        .collect()
}

pub(crate) fn clipped_night_periods(
    nights: &[AstronomicalNightPeriod],
    window: TimePeriod<ModifiedJulianDate>,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    nights
        .iter()
        .filter_map(|night| intersect_period(night.period, window))
        .collect()
}

pub(crate) fn airglow_phase_periods_for_window(
    nights: &[AstronomicalNightPeriod],
    window: TimePeriod<ModifiedJulianDate>,
) -> Vec<AirglowPhasePeriod> {
    let mut periods = Vec::new();
    for night in nights {
        if !night.phase_bounded {
            if let Some(period) = intersect_period(night.period, window) {
                periods.push(AirglowPhasePeriod {
                    period,
                    time_bin: 0,
                });
            }
            continue;
        }

        let start = night.period.start.raw().value();
        let end = night.period.end.raw().value();
        let duration = end - start;
        if !duration.is_finite() || duration <= 0.0 {
            continue;
        }

        let first = ModifiedJulianDate::new(start + duration / 3.0);
        let second = ModifiedJulianDate::new(start + duration * 2.0 / 3.0);
        for (period, time_bin) in [
            (TimePeriod::new(night.period.start, first), 1),
            (TimePeriod::new(first, second), 2),
            (TimePeriod::new(second, night.period.end), 3),
        ] {
            if let Some(period) = intersect_period(period, window) {
                periods.push(AirglowPhasePeriod { period, time_bin });
            }
        }
    }
    periods
}

pub(crate) fn time_of_night_bin_from_nights(
    time_tt: ModifiedJulianDate,
    nights: &[AstronomicalNightPeriod],
) -> Option<usize> {
    nights
        .iter()
        .find(|night| night.period.start < time_tt && time_tt < night.period.end)
        .and_then(|night| time_of_night_bin_from_night(time_tt, night))
}

pub(crate) fn time_of_night_bin_from_phase_periods(
    time_tt: ModifiedJulianDate,
    phases: &[AirglowPhasePeriod],
) -> Option<usize> {
    phases
        .iter()
        .find(|phase| phase.period.start < time_tt && time_tt < phase.period.end)
        .map(|phase| phase.time_bin)
}

fn time_of_night_bin_from_night(
    time_tt: ModifiedJulianDate,
    night: &AstronomicalNightPeriod,
) -> Option<usize> {
    if !(night.period.start < time_tt && time_tt < night.period.end) {
        return None;
    }

    if !night.phase_bounded {
        return Some(0);
    }

    let duration_days = night.period.end.raw().value() - night.period.start.raw().value();
    if !duration_days.is_finite() || duration_days <= 0.0 {
        return None;
    }

    let phase = ((time_tt.raw().value() - night.period.start.raw().value()) / duration_days)
        .clamp(0.0, 1.0);

    Some(if phase < 1.0 / 3.0 {
        1
    } else if phase < 2.0 / 3.0 {
        2
    } else {
        3
    })
}

fn astronomical_night_containing(
    time_tt: ModifiedJulianDate,
    location: Geodetic<ECEF>,
) -> Option<AstronomicalNightPeriod> {
    #[cfg(test)]
    POINT_NIGHT_SEARCH_FORBIDDEN.with(|forbidden| {
        assert!(
            !forbidden.get(),
            "threshold sampling must use precomputed airglow night context"
        );
    });

    let mut radius_days = INITIAL_NIGHT_SEARCH_RADIUS_DAYS;

    loop {
        let search_window = TimePeriod::new(
            ModifiedJulianDate::new(time_tt.raw().value() - radius_days),
            ModifiedJulianDate::new(time_tt.raw().value() + radius_days),
        );

        let night = SunBody
            .below_threshold(
                &location,
                search_window,
                Degrees::new(ASTRONOMICAL_TWILIGHT_DEG),
                SearchOpts::default(),
            )
            .into_iter()
            .find(|night| night.start < time_tt && time_tt < night.end)?;

        if night.start > search_window.start && night.end < search_window.end {
            return Some(AstronomicalNightPeriod {
                period: night,
                phase_bounded: true,
            });
        }

        if radius_days >= MAX_NIGHT_SEARCH_RADIUS_DAYS {
            return Some(AstronomicalNightPeriod {
                period: night,
                phase_bounded: false,
            });
        }

        radius_days =
            (radius_days * NIGHT_SEARCH_EXPANSION_FACTOR).min(MAX_NIGHT_SEARCH_RADIUS_DAYS);
    }
}

fn expand_window(
    window: TimePeriod<ModifiedJulianDate>,
    radius_days: f64,
) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        ModifiedJulianDate::new(window.start.raw().value() - radius_days),
        ModifiedJulianDate::new(window.end.raw().value() + radius_days),
    )
}

fn intersect_period(
    lhs: TimePeriod<ModifiedJulianDate>,
    rhs: TimePeriod<ModifiedJulianDate>,
) -> Option<TimePeriod<ModifiedJulianDate>> {
    let start = lhs.start.max(rhs.start);
    let end = lhs.end.min(rhs.end);
    (start < end).then(|| TimePeriod::new(start, end))
}

fn utc_time_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn local_solar_datetime(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let dt = time.to_chrono()?;
    let offset_seconds = (location.lon.value() / 15.0 * 3600.0).round() as i64;
    Some(dt + chrono::Duration::seconds(offset_seconds))
}

#[cfg(test)]
pub(crate) fn time_of_night_bin_for_test(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
) -> Option<usize> {
    time_of_night_bin(time, location)
}

#[cfg(test)]
pub(crate) fn astronomical_night_for_test(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
) -> Option<TimePeriod<ModifiedJulianDate>> {
    astronomical_night_containing(utc_time_to_tt_mjd(time), location).map(|night| night.period)
}

#[cfg(test)]
pub(crate) fn forbid_point_night_search_for_test<R>(f: impl FnOnce() -> R) -> R {
    struct ResetForbidden;

    impl Drop for ResetForbidden {
        fn drop(&mut self) {
            POINT_NIGHT_SEARCH_FORBIDDEN.with(|forbidden| forbidden.set(false));
        }
    }

    POINT_NIGHT_SEARCH_FORBIDDEN.with(|forbidden| {
        forbidden.set(true);
    });
    let _reset = ResetForbidden;
    f()
}
