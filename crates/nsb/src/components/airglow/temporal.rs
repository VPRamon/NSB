use super::domain::{AirglowNightPhase, AirglowSeason};
use chrono::Datelike;
use qtty::angular::Degrees;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::qtty::Days;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
#[cfg(test)]
use std::cell::Cell;
use tempoch::{Time, MJD, UTC};

pub(crate) const ASTRONOMICAL_TWILIGHT: Degrees = Degrees::new(-18.0);
const INITIAL_NIGHT_SEARCH_RADIUS: Days = Days::new(2.0);
const MAX_NIGHT_SEARCH_RADIUS: Days = Days::new(200.0);
const NIGHT_SEARCH_EXPANSION_FACTOR: f64 = 4.0;

#[derive(Debug, Clone, Copy)]
pub(crate) struct AstronomicalNightPeriod {
    pub(crate) period: TimePeriod<ModifiedJulianDate>,
    pub(crate) phase_bounded: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AirglowPhasePeriod {
    pub(crate) period: TimePeriod<ModifiedJulianDate>,
    pub(crate) phase: AirglowNightPhase,
}

#[cfg(test)]
thread_local! {
    static POINT_NIGHT_SEARCH_FORBIDDEN: Cell<bool> = const { Cell::new(false) };
}

/// Return the empirical Airglow season for the observer's local-solar month.
///
/// `FullYear` preserves the existing aggregate fallback when the UTC instant
/// cannot be represented by `chrono`; normal month mappings always select one
/// of the six named double-month seasons.
pub(crate) fn season(time: Time<UTC>, location: Geodetic<ECEF>) -> AirglowSeason {
    let Some(dt) = local_solar_datetime(time, location) else {
        return AirglowSeason::FullYear;
    };
    match dt.month() {
        12 | 1 => AirglowSeason::DecJan,
        2 | 3 => AirglowSeason::FebMar,
        4 | 5 => AirglowSeason::AprMay,
        6 | 7 => AirglowSeason::JunJul,
        8 | 9 => AirglowSeason::AugSep,
        10 | 11 => AirglowSeason::OctNov,
        _ => AirglowSeason::FullYear,
    }
}

/// Site-aware Airglow night phase based on astronomical-night thirds.
///
/// The SkyCalc-derived Airglow calibration table defines three equal periods
/// over the full astronomical-night interval (`alt_sun < -18°`). We compute the
/// complete astronomical night containing `time`, normalize the instant to that
/// interval, and return the corresponding semantic phase.
///
/// The search expands adaptively so high-latitude winter nights are not
/// mistaken for missing Airglow merely because the first local window is clipped.
/// If the final expanded search still sees continuous night without both bounding
/// crossings, [`AirglowNightPhase::FullNight`] is used explicitly.
pub(crate) fn night_phase(time: Time<UTC>, location: Geodetic<ECEF>) -> Option<AirglowNightPhase> {
    let time_tt = utc_time_to_tt_mjd(time);
    let night = astronomical_night_containing(time_tt, location)?;
    night_phase_from_night(time_tt, &night)
}

pub(crate) fn is_astronomical_twilight(threshold: Degrees) -> bool {
    (threshold - ASTRONOMICAL_TWILIGHT).abs() <= Degrees::new(f64::EPSILON)
}

pub(crate) fn astronomical_nights_for_window(
    window: TimePeriod<ModifiedJulianDate>,
    location: Geodetic<ECEF>,
) -> Vec<AstronomicalNightPeriod> {
    if window.start >= window.end {
        return Vec::new();
    }

    let search_window = expand_window(window, MAX_NIGHT_SEARCH_RADIUS);
    SunBody
        .below_threshold(
            &location,
            search_window,
            ASTRONOMICAL_TWILIGHT,
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
                    phase: AirglowNightPhase::FullNight,
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
        for (period, phase) in [
            (
                TimePeriod::new(night.period.start, first),
                AirglowNightPhase::FirstThird,
            ),
            (
                TimePeriod::new(first, second),
                AirglowNightPhase::MiddleThird,
            ),
            (
                TimePeriod::new(second, night.period.end),
                AirglowNightPhase::LastThird,
            ),
        ] {
            if let Some(period) = intersect_period(period, window) {
                periods.push(AirglowPhasePeriod { period, phase });
            }
        }
    }
    periods
}

pub(crate) fn night_phase_from_nights(
    time_tt: ModifiedJulianDate,
    nights: &[AstronomicalNightPeriod],
) -> Option<AirglowNightPhase> {
    nights
        .iter()
        .find(|night| night.period.start < time_tt && time_tt < night.period.end)
        .and_then(|night| night_phase_from_night(time_tt, night))
}

pub(crate) fn night_phase_from_phase_periods(
    time_tt: ModifiedJulianDate,
    phases: &[AirglowPhasePeriod],
) -> Option<AirglowNightPhase> {
    phases
        .iter()
        .find(|phase| phase.period.start < time_tt && time_tt < phase.period.end)
        .map(|phase| phase.phase)
}

fn night_phase_from_night(
    time_tt: ModifiedJulianDate,
    night: &AstronomicalNightPeriod,
) -> Option<AirglowNightPhase> {
    if !(night.period.start < time_tt && time_tt < night.period.end) {
        return None;
    }

    if !night.phase_bounded {
        return Some(AirglowNightPhase::FullNight);
    }

    let duration_days = night.period.end.raw().value() - night.period.start.raw().value();
    if !duration_days.is_finite() || duration_days <= 0.0 {
        return None;
    }

    let phase = ((time_tt.raw().value() - night.period.start.raw().value()) / duration_days)
        .clamp(0.0, 1.0);

    Some(if phase < 1.0 / 3.0 {
        AirglowNightPhase::FirstThird
    } else if phase < 2.0 / 3.0 {
        AirglowNightPhase::MiddleThird
    } else {
        AirglowNightPhase::LastThird
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

    let mut radius = INITIAL_NIGHT_SEARCH_RADIUS;

    loop {
        let search_window = TimePeriod::new(
            ModifiedJulianDate::new(time_tt.raw().value() - radius.value()),
            ModifiedJulianDate::new(time_tt.raw().value() + radius.value()),
        );

        let night = SunBody
            .below_threshold(
                &location,
                search_window,
                ASTRONOMICAL_TWILIGHT,
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

        if radius >= MAX_NIGHT_SEARCH_RADIUS {
            return Some(AstronomicalNightPeriod {
                period: night,
                phase_bounded: false,
            });
        }

        radius = (radius * NIGHT_SEARCH_EXPANSION_FACTOR).min(MAX_NIGHT_SEARCH_RADIUS);
    }
}

fn expand_window(
    window: TimePeriod<ModifiedJulianDate>,
    radius: Days,
) -> TimePeriod<ModifiedJulianDate> {
    TimePeriod::new(
        ModifiedJulianDate::new(window.start.raw().value() - radius.value()),
        ModifiedJulianDate::new(window.end.raw().value() + radius.value()),
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
pub(crate) fn night_phase_for_test(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
) -> Option<AirglowNightPhase> {
    night_phase(time, location)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use qtty::angular::Degrees;
    use siderust::coordinates::centers::Geodetic;
    use siderust::coordinates::frames::ECEF;
    use siderust::qtty::Meters;

    fn utc(year: i32, month: u32, day: u32) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
                .single()
                .unwrap(),
        )
    }

    fn equator() -> Geodetic<ECEF> {
        Geodetic::new_raw(Degrees::new(0.0), Degrees::new(0.0), Meters::new(0.0))
    }

    #[test]
    fn season_maps_all_named_double_months() {
        let location = equator();
        for (month, expected) in [
            (12, AirglowSeason::DecJan),
            (1, AirglowSeason::DecJan),
            (2, AirglowSeason::FebMar),
            (3, AirglowSeason::FebMar),
            (4, AirglowSeason::AprMay),
            (5, AirglowSeason::AprMay),
            (6, AirglowSeason::JunJul),
            (7, AirglowSeason::JunJul),
            (8, AirglowSeason::AugSep),
            (9, AirglowSeason::AugSep),
            (10, AirglowSeason::OctNov),
            (11, AirglowSeason::OctNov),
        ] {
            assert_eq!(season(utc(2023, month, 15), location), expected);
        }
    }

    #[test]
    fn bounded_night_phase_uses_current_third_boundaries() {
        let night = AstronomicalNightPeriod {
            period: TimePeriod::new(ModifiedJulianDate::new(0.0), ModifiedJulianDate::new(3.0)),
            phase_bounded: true,
        };

        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(0.5), &night),
            Some(AirglowNightPhase::FirstThird)
        );
        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(1.0), &night),
            Some(AirglowNightPhase::MiddleThird)
        );
        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(1.5), &night),
            Some(AirglowNightPhase::MiddleThird)
        );
        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(2.0), &night),
            Some(AirglowNightPhase::LastThird)
        );
        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(2.5), &night),
            Some(AirglowNightPhase::LastThird)
        );
    }

    #[test]
    fn unbounded_astronomical_night_uses_explicit_full_night_phase() {
        let night = AstronomicalNightPeriod {
            period: TimePeriod::new(ModifiedJulianDate::new(0.0), ModifiedJulianDate::new(3.0)),
            phase_bounded: false,
        };
        assert_eq!(
            night_phase_from_night(ModifiedJulianDate::new(1.5), &night),
            Some(AirglowNightPhase::FullNight)
        );
    }
}
