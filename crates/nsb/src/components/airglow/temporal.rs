use chrono::Datelike;
use qtty::angular::Degrees;
use siderust::bodies::Sun as SunBody;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate, TT};
use tempoch::{Time, MJD, UTC};

const ASTRONOMICAL_TWILIGHT_DEG: f64 = -18.0;
const NIGHT_SEARCH_RADIUS_DAYS: f64 = 2.0;

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
/// Returns `None` when `time` is outside a complete astronomical night, or when
/// no dusk/dawn-bounded astronomical night exists in the search window.
pub(crate) fn time_of_night_bin(time: Time<UTC>, location: Geodetic<ECEF>) -> Option<usize> {
    let time_tt = utc_time_to_tt_mjd(time);
    let night = astronomical_night_containing(time_tt, location)?;

    let duration_days = night.end.raw().value() - night.start.raw().value();
    if !duration_days.is_finite() || duration_days <= 0.0 {
        return None;
    }

    let phase = ((time_tt.raw().value() - night.start.raw().value()) / duration_days)
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
) -> Option<TimePeriod<ModifiedJulianDate>> {
    let search_window = TimePeriod::new(
        ModifiedJulianDate::new(time_tt.raw().value() - NIGHT_SEARCH_RADIUS_DAYS),
        ModifiedJulianDate::new(time_tt.raw().value() + NIGHT_SEARCH_RADIUS_DAYS),
    );

    SunBody
        .below_threshold(
            &location,
            search_window,
            Degrees::new(ASTRONOMICAL_TWILIGHT_DEG),
            SearchOpts::default(),
        )
        .into_iter()
        // A valid phase requires a complete night, bounded by both
        // astronomical dusk and astronomical dawn inside the search window.
        .filter(|night| night.start > search_window.start && night.end < search_window.end)
        .find(|night| night.start < time_tt && time_tt < night.end)
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
    astronomical_night_containing(utc_time_to_tt_mjd(time), location)
}
