use chrono::{Datelike, Timelike};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Time, UTC};

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

/// Site-aware approximate night bin based on local solar time.
///
/// This intentionally does not use raw UTC hour. It is a pragmatic fallback:
/// local night is approximated as 18:00-06:00 local solar time and split into
/// three equal bins. TODO: replace with astronomical-night event boundaries.
pub(crate) fn time_of_night_bin(time: Time<UTC>, location: Geodetic<ECEF>) -> usize {
    let Some(dt) = local_solar_datetime(time, location) else {
        return 0;
    };
    let hour = dt.hour() as f64 + dt.minute() as f64 / 60.0 + dt.second() as f64 / 3600.0;
    let night_hour = if hour >= 18.0 {
        hour - 18.0
    } else if hour < 6.0 {
        hour + 6.0
    } else {
        return 0;
    };
    if night_hour < 4.0 {
        1
    } else if night_hour < 8.0 {
        2
    } else {
        3
    }
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
pub(crate) fn time_of_night_bin_for_test(time: Time<UTC>, location: Geodetic<ECEF>) -> usize {
    time_of_night_bin(time, location)
}
