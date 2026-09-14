use super::domain::{AirglowNightPhase, AirglowSeason};
use chrono::Datelike;
use qtty::angular::Degrees;
use rayon::prelude::*;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::event::altitude::{AltitudeEventsExt, AltitudeProvider, SearchOpts};
use siderust::qtty::{Day, Days, Hours, Radian};
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
#[cfg(test)]
use std::cell::Cell;
use tempoch::{Time, MJD, UTC};

pub(crate) const ASTRONOMICAL_TWILIGHT: Degrees = Degrees::new(-18.0);
const INITIAL_NIGHT_SEARCH_RADIUS: Days = Days::new(2.0);
const MAX_NIGHT_SEARCH_RADIUS: Days = Days::new(200.0);
const NIGHT_SEARCH_EXPANSION_FACTOR: f64 = 4.0;
const WINDOW_EVENT_TOLERANCE: Days = Days::new(1.0e-5);
const SOLAR_DISCOVERY_STEP: Days = Hours::new(6.0).to_const::<Day>();
const SOLAR_DISCOVERY_TOLERANCE_DAYS: f64 = 1.0e-7;
const SOLAR_GRAZING_GUARD: f64 = 0.01;
const SOLAR_EXACT_BRACKET_GUARD: Days = Hours::new(1.0).to_const::<Day>();
const MAX_SOLAR_POLISH_STEPS: usize = 4;

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

    let mut radius = INITIAL_NIGHT_SEARCH_RADIUS;
    loop {
        let search_window = expand_window(window, radius);
        let nights: Vec<_> =
            sun_below_threshold_periods(search_window, location, ASTRONOMICAL_TWILIGHT)
                .into_iter()
                .filter(|night| night.end > window.start && night.start < window.end)
                .map(|night| AstronomicalNightPeriod {
                    phase_bounded: night.start > search_window.start
                        && night.end < search_window.end,
                    period: night,
                })
                .collect();

        if nights.iter().all(|night| night.phase_bounded) || radius >= MAX_NIGHT_SEARCH_RADIUS {
            return nights;
        }
        radius = (radius * NIGHT_SEARCH_EXPANSION_FACTOR).min(MAX_NIGHT_SEARCH_RADIUS);
    }
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

        let night = sun_below_threshold_periods(search_window, location, ASTRONOMICAL_TWILIGHT)
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

#[derive(Clone, Copy)]
struct SolarCrossingCandidate {
    approximate_root: ModifiedJulianDate,
    bracket: TimePeriod<ModifiedJulianDate>,
    rising: bool,
}

/// Fast solar-period preparation with authoritative boundary validation.
///
/// A compact Meeus-style signal discovers ordinary crossings. Each retained
/// root is polished and accepted using precise Siderust solar altitudes. A
/// near-tangent interval is intentionally delegated to Siderust's full event
/// search, preserving polar and grazing behavior.
pub(crate) fn sun_below_threshold_periods(
    window: TimePeriod<ModifiedJulianDate>,
    location: Geodetic<ECEF>,
    threshold: Degrees,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    if window.start >= window.end {
        return Vec::new();
    }
    let threshold_sin = threshold.to::<Radian>().sin();
    let approximate = |time| approximate_solar_sin_altitude(time, location) - threshold_sin;
    let exact =
        |time: ModifiedJulianDate| SunBody.altitude_at(&location, time).sin() - threshold_sin;

    let mut samples = Vec::new();
    let mut time = window.start;
    loop {
        samples.push((time, approximate(time)));
        if time >= window.end {
            break;
        }
        let next = ModifiedJulianDate::new(
            (time.raw().value() + SOLAR_DISCOVERY_STEP.value()).min(window.end.raw().value()),
        );
        if next <= time {
            break;
        }
        time = next;
    }

    if samples.windows(3).any(|triple| {
        let [(_, left), (_, middle), (_, right)] = triple else {
            unreachable!()
        };
        middle.abs() < SOLAR_GRAZING_GUARD
            && middle.abs() < left.abs()
            && middle.abs() < right.abs()
            && left.signum() == right.signum()
    }) {
        return SunBody.below_threshold(&location, window, threshold, SearchOpts::default());
    }

    let mut candidates = Vec::new();
    for pair in samples.windows(2) {
        let (lo, f_lo) = pair[0];
        let (hi, f_hi) = pair[1];
        if f_lo.signum() == f_hi.signum() {
            continue;
        }
        candidates.push(SolarCrossingCandidate {
            approximate_root: refine_approximate_solar_root(lo, f_lo, hi, f_hi, &approximate),
            bracket: TimePeriod::new(
                ModifiedJulianDate::new(
                    (lo.raw().value() - SOLAR_EXACT_BRACKET_GUARD.value())
                        .max(window.start.raw().value()),
                ),
                ModifiedJulianDate::new(
                    (hi.raw().value() + SOLAR_EXACT_BRACKET_GUARD.value())
                        .min(window.end.raw().value()),
                ),
            ),
            rising: f_hi > f_lo,
        });
    }

    let roots: Option<Vec<_>> = candidates
        .par_iter()
        .map(|candidate| {
            polish_solar_root(*candidate, &approximate, &exact).map(|root| (root, candidate.rising))
        })
        .collect();
    let Some(roots) = roots else {
        return SunBody.below_threshold(
            &location,
            window,
            threshold,
            SearchOpts {
                time_tolerance: WINDOW_EVENT_TOLERANCE,
            },
        );
    };

    let mut periods = Vec::new();
    let mut open = (exact(window.start) < 0.0).then_some(window.start);
    for (root, rising) in roots {
        if rising {
            if let Some(start) = open.take() {
                if start < root {
                    periods.push(TimePeriod::new(start, root));
                }
            }
        } else if open.is_none() {
            open = Some(root);
        }
    }
    if let Some(start) = open {
        if start < window.end {
            periods.push(TimePeriod::new(start, window.end));
        }
    }
    periods
}

fn refine_approximate_solar_root<F>(
    mut lo: ModifiedJulianDate,
    mut f_lo: f64,
    mut hi: ModifiedJulianDate,
    mut f_hi: f64,
    approximate: &F,
) -> ModifiedJulianDate
where
    F: Fn(ModifiedJulianDate) -> f64,
{
    for _ in 0..32 {
        if hi.raw().value() - lo.raw().value() <= SOLAR_DISCOVERY_TOLERANCE_DAYS {
            break;
        }
        let lo_value = lo.raw().value();
        let hi_value = hi.raw().value();
        let secant = hi_value - f_hi * (hi_value - lo_value) / (f_hi - f_lo);
        let candidate = if secant.is_finite() && secant > lo_value && secant < hi_value {
            ModifiedJulianDate::new(secant)
        } else {
            ModifiedJulianDate::new(0.5 * (lo_value + hi_value))
        };
        let value = approximate(candidate);
        if value.signum() == f_lo.signum() {
            lo = candidate;
            f_lo = value;
        } else {
            hi = candidate;
            f_hi = value;
        }
    }
    ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()))
}

fn polish_solar_root<A, E>(
    candidate: SolarCrossingCandidate,
    approximate: &A,
    exact: &E,
) -> Option<ModifiedJulianDate>
where
    A: Fn(ModifiedJulianDate) -> f64,
    E: Fn(ModifiedJulianDate) -> f64,
{
    let mut previous_time = candidate.approximate_root;
    let mut previous_residual = exact(previous_time);
    let derivative_delta = 1.0e-4;
    let derivative = (approximate(ModifiedJulianDate::new(
        previous_time.raw().value() + derivative_delta,
    )) - approximate(ModifiedJulianDate::new(
        previous_time.raw().value() - derivative_delta,
    ))) / (2.0 * derivative_delta);
    if !derivative.is_finite() || derivative.abs() < 1.0e-8 {
        return refine_exact_solar_bracket(candidate.bracket, exact);
    }

    let mut next_time =
        ModifiedJulianDate::new(previous_time.raw().value() - previous_residual / derivative);
    for _ in 0..MAX_SOLAR_POLISH_STEPS {
        if next_time < candidate.bracket.start || next_time > candidate.bracket.end {
            return refine_exact_solar_bracket(candidate.bracket, exact);
        }
        let next_residual = exact(next_time);
        let dt = next_time.raw().value() - previous_time.raw().value();
        let exact_slope = (next_residual - previous_residual) / dt;
        if !exact_slope.is_finite() || exact_slope.abs() < 1.0e-8 {
            return refine_exact_solar_bracket(candidate.bracket, exact);
        }
        let correction = next_residual / exact_slope;
        if correction.abs() <= WINDOW_EVENT_TOLERANCE.value() {
            return Some(next_time);
        }
        previous_time = next_time;
        previous_residual = next_residual;
        next_time = ModifiedJulianDate::new(next_time.raw().value() - correction);
    }
    refine_exact_solar_bracket(candidate.bracket, exact)
}

fn refine_exact_solar_bracket<E>(
    bracket: TimePeriod<ModifiedJulianDate>,
    exact: &E,
) -> Option<ModifiedJulianDate>
where
    E: Fn(ModifiedJulianDate) -> f64,
{
    let mut lo = bracket.start;
    let mut hi = bracket.end;
    let mut f_lo = exact(lo);
    let f_hi = exact(hi);
    if f_lo.signum() == f_hi.signum() {
        return None;
    }
    for _ in 0..20 {
        if hi.raw().value() - lo.raw().value() <= WINDOW_EVENT_TOLERANCE.value() {
            break;
        }
        let mid = ModifiedJulianDate::new(0.5 * (lo.raw().value() + hi.raw().value()));
        let f_mid = exact(mid);
        if f_mid.signum() == f_lo.signum() {
            lo = mid;
            f_lo = f_mid;
        } else {
            hi = mid;
        }
    }
    Some(ModifiedJulianDate::new(
        0.5 * (lo.raw().value() + hi.raw().value()),
    ))
}

#[inline]
fn approximate_solar_sin_altitude(time: ModifiedJulianDate, location: Geodetic<ECEF>) -> f64 {
    let j2000_mjd = 51_544.5;
    let days = time.raw().value() - j2000_mjd;
    let mean_longitude = (280.466_46 + 0.985_647_36 * days).to_radians();
    let mean_anomaly = (357.529_11 + 0.985_600_28 * days).to_radians();
    let ecliptic_longitude = mean_longitude
        + 1.914_602_f64.to_radians() * mean_anomaly.sin()
        + 0.019_993_f64.to_radians() * (2.0 * mean_anomaly).sin()
        + 0.000_289_f64.to_radians() * (3.0 * mean_anomaly).sin();
    let obliquity = (23.439_291 - 0.000_000_36 * days).to_radians();
    let (sin_longitude, cos_longitude) = ecliptic_longitude.sin_cos();
    let right_ascension = (obliquity.cos() * sin_longitude).atan2(cos_longitude);
    let sin_declination = obliquity.sin() * sin_longitude;
    let cos_declination = (1.0 - sin_declination * sin_declination).sqrt();
    let gmst = (4.894_961 + 6.300_388 * days).rem_euclid(std::f64::consts::TAU);
    let hour_angle = (gmst + location.lon.to::<Radian>().value() - right_ascension)
        .rem_euclid(std::f64::consts::TAU);
    let latitude = location.lat.to::<Radian>().value();
    latitude.sin() * sin_declination + latitude.cos() * cos_declination * hour_angle.cos()
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

    fn cta_south() -> Geodetic<ECEF> {
        Geodetic::new_raw(
            Degrees::new(-70.3147),
            Degrees::new(-24.6834),
            Meters::new(2_147.0),
        )
    }

    fn period(start: (i32, u32, u32), days: i64) -> TimePeriod<ModifiedJulianDate> {
        let start = utc(start.0, start.1, start.2);
        let end =
            Time::<UTC>::from_chrono(start.to_chrono().unwrap() + chrono::Duration::days(days));
        TimePeriod::new(utc_time_to_tt_mjd(start), utc_time_to_tt_mjd(end))
    }

    fn assert_solar_periods_match_reference(
        location: Geodetic<ECEF>,
        window: TimePeriod<ModifiedJulianDate>,
        threshold: Degrees,
    ) {
        let reference =
            SunBody.below_threshold(&location, window, threshold, SearchOpts::default());
        let actual = sun_below_threshold_periods(window, location, threshold);
        assert_eq!(
            actual.len(),
            reference.len(),
            "actual={actual:?} reference={reference:?}"
        );
        for (actual, reference) in actual.iter().zip(reference) {
            let start_error_seconds =
                (actual.start.raw().value() - reference.start.raw().value()).abs() * 86_400.0;
            let end_error_seconds =
                (actual.end.raw().value() - reference.end.raw().value()).abs() * 86_400.0;
            assert!(
                start_error_seconds <= 1.0,
                "solar start error {start_error_seconds}s"
            );
            assert!(
                end_error_seconds <= 1.0,
                "solar end error {end_error_seconds}s"
            );
        }
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

    #[test]
    fn fast_solar_boundaries_match_precise_reference_across_regimes() {
        for (location, window, threshold) in [
            (cta_south(), period((2026, 1, 1), 31), Degrees::new(-18.0)),
            (cta_south(), period((2026, 6, 1), 31), Degrees::new(0.0)),
            (equator(), period((2026, 3, 15), 20), Degrees::new(-12.0)),
            (
                Geodetic::new_raw(
                    Degrees::new(18.9553),
                    Degrees::new(69.6492),
                    Meters::new(0.0),
                ),
                period((2026, 5, 10), 40),
                Degrees::new(-18.0),
            ),
        ] {
            assert_solar_periods_match_reference(location, window, threshold);
        }
    }
}
