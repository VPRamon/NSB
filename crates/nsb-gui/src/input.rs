use chrono::{FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use nsb::{ComponentMask, Observer, Target, DEG};
use siderust::catalogs::ObservatoryCatalog;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::SphericalDirectionAstroExt;
use siderust::qtty::{Degrees, Meters};
use siderust::time::{self, JulianDate, TT};
use std::fmt;
use tempoch::{Time, JD, UTC};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverMode {
    Coordinates,
    Map,
    Site,
}

impl ObserverMode {
    pub const ALL: [Self; 3] = [Self::Coordinates, Self::Map, Self::Site];
}

impl fmt::Display for ObserverMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Coordinates => "Coordinates",
            Self::Map => "Map",
            Self::Site => "Site",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeMode {
    Local,
    JulianDate,
    ModifiedJulianDate,
}

impl TimeMode {
    pub const ALL: [Self; 3] = [Self::Local, Self::JulianDate, Self::ModifiedJulianDate];
}

impl fmt::Display for TimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Local => "Local",
            Self::JulianDate => "JD (TT)",
            Self::ModifiedJulianDate => "MJD (TT)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFrame {
    Icrs,
    Horizontal,
}

impl TargetFrame {
    pub const ALL: [Self; 2] = [Self::Icrs, Self::Horizontal];
}

impl fmt::Display for TargetFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Icrs => "ICRS/J2000",
            Self::Horizontal => "Horizontal",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFormat {
    Sexagesimal,
    DecimalDegrees,
}

impl CoordinateFormat {
    pub const ALL: [Self; 2] = [Self::Sexagesimal, Self::DecimalDegrees];
}

impl fmt::Display for CoordinateFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Sexagesimal => "Sexagesimal",
            Self::DecimalDegrees => "Decimal degrees",
        })
    }
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub observer_mode: ObserverMode,
    pub longitude: String,
    pub latitude: String,
    pub altitude_m: String,
    pub site_name: String,

    pub time_mode: TimeMode,
    pub local_date: String,
    pub local_time: String,
    pub utc_offset: String,
    pub jd_tt: String,
    pub mjd_tt: String,

    pub target_frame: TargetFrame,
    pub coordinate_format: CoordinateFormat,
    pub ra_hms: String,
    pub dec_dms: String,
    pub ra_deg: String,
    pub dec_deg: String,
    pub az_dms: String,
    pub alt_dms: String,
    pub az_deg: String,
    pub target_alt_deg: String,

    pub zodiacal: bool,
    pub starlight: bool,
    pub airglow: bool,
    pub moonlight: bool,

    pub max_radiance: String,
    pub use_sun_ceiling: bool,
    pub sun_altitude_ceiling_deg: String,
    pub use_target_floor: bool,
    pub target_altitude_floor_deg: String,
    pub duration_hours: String,
    pub sample_step_seconds: String,
}

impl Default for InputState {
    fn default() -> Self {
        let default_mask = ComponentMask::ALL;
        Self {
            observer_mode: ObserverMode::Coordinates,
            longitude: "-70.316344".into(),
            latitude: "-24.683428".into(),
            altitude_m: "2184.6".into(),
            site_name: "El Paranal Observatory".into(),
            time_mode: TimeMode::Local,
            local_date: "2026-10-07".into(),
            local_time: "20:00".into(),
            utc_offset: "+02:00".into(),
            jd_tt: "2461321.25".into(),
            mjd_tt: "61320.75".into(),
            target_frame: TargetFrame::Icrs,
            coordinate_format: CoordinateFormat::Sexagesimal,
            ra_hms: "17 45 40.04".into(),
            dec_dms: "-29 00 28.1".into(),
            ra_deg: "266.41683".into(),
            dec_deg: "-29.00781".into(),
            az_dms: "180 00 00".into(),
            alt_dms: "45 00 00".into(),
            az_deg: "180.0".into(),
            target_alt_deg: "45.0".into(),
            zodiacal: default_mask.contains(ComponentMask::ZODIACAL),
            starlight: default_mask.contains(ComponentMask::STARLIGHT),
            airglow: default_mask.contains(ComponentMask::AIRGLOW),
            moonlight: default_mask.contains(ComponentMask::MOON),
            max_radiance: "0.25".into(),
            use_sun_ceiling: true,
            sun_altitude_ceiling_deg: "-18.0".into(),
            use_target_floor: true,
            target_altitude_floor_deg: "0.0".into(),
            duration_hours: "10".into(),
            sample_step_seconds: "600".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CalculationRequest {
    pub observer: Observer,
    pub start: Time<UTC>,
    pub target: Target,
    pub components: ComponentMask,
    pub max_radiance: f64,
    pub sun_altitude_ceiling_deg: Option<f64>,
    pub target_altitude_floor_deg: Option<f64>,
    pub duration_hours: f64,
    pub sample_step_seconds: f64,
}

impl InputState {
    pub fn site_names() -> Vec<String> {
        let mut names: Vec<String> = ObservatoryCatalog::builtin()
            .iter()
            .map(|observatory| observatory.name.to_string())
            .collect();
        names.sort();
        names
    }

    pub fn starlight_available() -> bool {
        ComponentMask::ALL.contains(ComponentMask::STARLIGHT)
    }

    pub fn resolve(&self) -> Result<CalculationRequest, String> {
        let observer = self.resolve_observer()?;
        let start = self.resolve_time()?;
        let target = self.resolve_target(observer, start)?;
        let components = self.resolve_components()?;

        let max_radiance = parse_finite(&self.max_radiance, "maximum NSB radiance")?;
        if max_radiance < 0.0 {
            return Err("maximum NSB radiance must be non-negative".into());
        }

        let sun_altitude_ceiling_deg = self
            .use_sun_ceiling
            .then(|| {
                parse_range(
                    &self.sun_altitude_ceiling_deg,
                    "Sun altitude ceiling",
                    -90.0,
                    90.0,
                )
            })
            .transpose()?;
        let target_altitude_floor_deg = self
            .use_target_floor
            .then(|| {
                parse_range(
                    &self.target_altitude_floor_deg,
                    "target altitude floor",
                    -90.0,
                    90.0,
                )
            })
            .transpose()?;

        let duration_hours = parse_finite(&self.duration_hours, "window duration")?;
        if !(0.0 < duration_hours && duration_hours <= 72.0) {
            return Err("window duration must be in (0, 72] hours".into());
        }

        let sample_step_seconds = parse_finite(&self.sample_step_seconds, "sample step")?;
        if !(60.0..=3600.0).contains(&sample_step_seconds) {
            return Err("sample step must be between 60 and 3600 seconds".into());
        }

        Ok(CalculationRequest {
            observer,
            start,
            target,
            components,
            max_radiance,
            sun_altitude_ceiling_deg,
            target_altitude_floor_deg,
            duration_hours,
            sample_step_seconds,
        })
    }

    fn resolve_observer(&self) -> Result<Observer, String> {
        match self.observer_mode {
            ObserverMode::Coordinates | ObserverMode::Map => {
                let lon = parse_range(&self.longitude, "longitude", -180.0, 180.0)?;
                let lat = parse_range(&self.latitude, "latitude", -90.0, 90.0)?;
                let height = parse_range(&self.altitude_m, "altitude", -500.0, 10_000.0)?;
                Ok(Geodetic::<ECEF>::new_raw(
                    Degrees::new(lon),
                    Degrees::new(lat),
                    Meters::new(height),
                ))
            }
            ObserverMode::Site => {
                let catalog = ObservatoryCatalog::builtin();
                catalog
                    .get(self.site_name.trim())
                    .map(|site| site.geodetic())
                    .ok_or_else(|| format!("unknown Siderust observatory: {:?}", self.site_name))
            }
        }
    }

    fn resolve_time(&self) -> Result<Time<UTC>, String> {
        match self.time_mode {
            TimeMode::Local => {
                parse_local_time(&self.local_date, &self.local_time, &self.utc_offset)
            }
            TimeMode::JulianDate => {
                let value = parse_finite(&self.jd_tt, "JD (TT)")?;
                let jd = time::try_jd_f64(value)
                    .map_err(|error| format!("invalid JD (TT): {error}"))?;
                Ok(Time::<TT>::from(jd).to::<UTC>())
            }
            TimeMode::ModifiedJulianDate => {
                let value = parse_finite(&self.mjd_tt, "MJD (TT)")?;
                let mjd = time::try_mjd_f64(value)
                    .map_err(|error| format!("invalid MJD (TT): {error}"))?;
                Ok(Time::<TT>::from(mjd).to::<UTC>())
            }
        }
    }

    fn resolve_target(&self, observer: Observer, start: Time<UTC>) -> Result<Target, String> {
        match self.target_frame {
            TargetFrame::Icrs => {
                let (ra_deg, dec_deg) = match self.coordinate_format {
                    CoordinateFormat::Sexagesimal => (
                        parse_ra_hms(&self.ra_hms)?,
                        parse_signed_dms(&self.dec_dms, "declination", -90.0, 90.0)?,
                    ),
                    CoordinateFormat::DecimalDegrees => (
                        parse_ra_degrees(&self.ra_deg)?,
                        parse_range(&self.dec_deg, "declination", -90.0, 90.0)?,
                    ),
                };
                Ok(Target::new(ra_deg * DEG, dec_deg * DEG))
            }
            TargetFrame::Horizontal => {
                let (az_deg, alt_deg) = match self.coordinate_format {
                    CoordinateFormat::Sexagesimal => (
                        parse_unsigned_dms(&self.az_dms, "azimuth", 0.0, 360.0)?,
                        parse_signed_dms(&self.alt_dms, "altitude", -90.0, 90.0)?,
                    ),
                    CoordinateFormat::DecimalDegrees => (
                        parse_azimuth(&self.az_deg)?,
                        parse_range(&self.target_alt_deg, "altitude", -90.0, 90.0)?,
                    ),
                };

                // NSB evaluates fixed J2000 equatorial directions. Horizontal input is
                // therefore interpreted as the local pointing at the selected window
                // start and converted once to the equivalent fixed J2000 direction.
                let horizontal = direction::Horizontal::new(alt_deg * DEG, az_deg * DEG);
                let jd_tt: JulianDate = start.to::<TT>().to::<JD>();
                let true_of_date = horizontal.to_equatorial(&jd_tt, &observer);
                let target: direction::EquatorialMeanJ2000 = true_of_date.to_frame(&jd_tt);
                Ok(target)
            }
        }
    }

    fn resolve_components(&self) -> Result<ComponentMask, String> {
        let mut components = ComponentMask::empty();
        if self.zodiacal {
            components |= ComponentMask::ZODIACAL;
        }
        if self.starlight {
            components |= ComponentMask::STARLIGHT;
        }
        if self.airglow {
            components |= ComponentMask::AIRGLOW;
        }
        if self.moonlight {
            components |= ComponentMask::MOON;
        }
        if components.is_empty() {
            return Err("select at least one NSB component".into());
        }
        Ok(components)
    }
}

fn parse_local_time(date: &str, time_text: &str, offset_text: &str) -> Result<Time<UTC>, String> {
    let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|error| format!("invalid local date: {error}"))?;
    let time = NaiveTime::parse_from_str(time_text.trim(), "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(time_text.trim(), "%H:%M"))
        .map_err(|error| format!("invalid local time: {error}"))?;
    let offset = parse_utc_offset(offset_text)?;
    let local = offset
        .from_local_datetime(&NaiveDateTime::new(date, time))
        .single()
        .ok_or_else(|| "local date/time could not be resolved".to_string())?;
    Ok(Time::<UTC>::from_chrono(local.with_timezone(&Utc)))
}

fn parse_utc_offset(input: &str) -> Result<FixedOffset, String> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("z") || trimmed == "+00:00" || trimmed == "-00:00" {
        return FixedOffset::east_opt(0).ok_or_else(|| "invalid UTC offset".into());
    }
    let (sign, rest) = match trimmed.as_bytes().first().copied() {
        Some(b'+') => (1_i32, &trimmed[1..]),
        Some(b'-') => (-1_i32, &trimmed[1..]),
        _ => return Err("UTC offset must use ±HH:MM, e.g. +02:00".into()),
    };
    let (hours, minutes) = rest
        .split_once(':')
        .ok_or_else(|| "UTC offset must use ±HH:MM, e.g. +02:00".to_string())?;
    let hours: i32 = hours
        .parse()
        .map_err(|_| "invalid UTC-offset hours".to_string())?;
    let minutes: i32 = minutes
        .parse()
        .map_err(|_| "invalid UTC-offset minutes".to_string())?;
    if hours > 23 || minutes > 59 {
        return Err("UTC offset is outside the valid ±23:59 range".into());
    }
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
        .ok_or_else(|| "invalid UTC offset".into())
}

fn parse_ra_hms(input: &str) -> Result<f64, String> {
    let values = parse_sexagesimal_parts(input, "right ascension")?;
    if values.negative || !(0.0..24.0).contains(&values.major) {
        return Err("right ascension hours must be in [0, 24)".into());
    }
    Ok((values.major + values.minutes / 60.0 + values.seconds / 3600.0) * 15.0)
}

fn parse_ra_degrees(input: &str) -> Result<f64, String> {
    let value = parse_finite(input, "right ascension")?;
    if !(0.0..360.0).contains(&value) {
        return Err("right ascension must be in [0, 360) degrees".into());
    }
    Ok(value)
}

fn parse_azimuth(input: &str) -> Result<f64, String> {
    let value = parse_finite(input, "azimuth")?;
    if !(0.0..360.0).contains(&value) {
        return Err("azimuth must be in [0, 360) degrees".into());
    }
    Ok(value)
}

fn parse_signed_dms(input: &str, label: &str, min: f64, max: f64) -> Result<f64, String> {
    let parts = parse_sexagesimal_parts(input, label)?;
    let unsigned = parts.major + parts.minutes / 60.0 + parts.seconds / 3600.0;
    let value = if parts.negative { -unsigned } else { unsigned };
    if !(min..=max).contains(&value) {
        return Err(format!("{label} must be in [{min}, {max}] degrees"));
    }
    if unsigned == max.abs() && (parts.minutes != 0.0 || parts.seconds != 0.0) {
        return Err(format!("{label} cannot exceed {max} degrees"));
    }
    Ok(value)
}

fn parse_unsigned_dms(
    input: &str,
    label: &str,
    min: f64,
    max_exclusive: f64,
) -> Result<f64, String> {
    let parts = parse_sexagesimal_parts(input, label)?;
    if parts.negative {
        return Err(format!("{label} cannot be negative"));
    }
    let value = parts.major + parts.minutes / 60.0 + parts.seconds / 3600.0;
    if !(min..max_exclusive).contains(&value) {
        return Err(format!(
            "{label} must be in [{min}, {max_exclusive}) degrees"
        ));
    }
    Ok(value)
}

struct SexagesimalParts {
    negative: bool,
    major: f64,
    minutes: f64,
    seconds: f64,
}

fn parse_sexagesimal_parts(input: &str, label: &str) -> Result<SexagesimalParts, String> {
    let normalized: String = input
        .trim()
        .chars()
        .map(|ch| match ch {
            ':' | 'h' | 'd' | '°' | '\u{27}' | '\u{22}' | 'm' | 's' => ' ',
            other => other,
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.len() != 3 {
        return Err(format!("{label} must contain three sexagesimal fields"));
    }
    let negative = tokens[0].starts_with('-');
    let major: f64 = tokens[0]
        .parse::<f64>()
        .map_err(|_| format!("invalid {label} major field"))?
        .abs();
    let minutes = tokens[1]
        .parse::<f64>()
        .map_err(|_| format!("invalid {label} minute field"))?;
    let seconds = tokens[2]
        .parse::<f64>()
        .map_err(|_| format!("invalid {label} second field"))?;
    if !major.is_finite() || !minutes.is_finite() || !seconds.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    if !(0.0..60.0).contains(&minutes) || !(0.0..60.0).contains(&seconds) {
        return Err(format!(
            "{label} minutes and seconds must be in [0, 60)"
        ));
    }
    Ok(SexagesimalParts {
        negative,
        major,
        minutes,
        seconds,
    })
}

fn parse_finite(input: &str, label: &str) -> Result<f64, String> {
    let value: f64 = input
        .trim()
        .parse()
        .map_err(|_| format!("invalid {label}: {input:?}"))?;
    if !value.is_finite() {
        return Err(format!("{label} must be finite"));
    }
    Ok(value)
}

fn parse_range(input: &str, label: &str, min: f64, max: f64) -> Result<f64, String> {
    let value = parse_finite(input, label)?;
    if !(min..=max).contains(&value) {
        return Err(format!("{label} must be in [{min}, {max}]"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hms_and_signed_dms() {
        assert!(
            (parse_ra_hms("17 45 40.04").unwrap() - 266.416_833_333).abs() < 1.0e-8
        );
        assert!(
            (parse_signed_dms("-29 00 28.1", "dec", -90.0, 90.0).unwrap()
                + 29.007_805_556)
                .abs()
                < 1.0e-8
        );
    }

    #[test]
    fn rejects_invalid_sexagesimal_minutes() {
        assert!(parse_ra_hms("12 60 00").is_err());
        assert!(parse_signed_dms("91 00 00", "dec", -90.0, 90.0).is_err());
    }

    #[test]
    fn local_time_respects_fixed_utc_offset() {
        let parsed = parse_local_time("2026-10-07", "20:00", "+02:00").unwrap();
        let utc = parsed.to_chrono().unwrap();
        assert_eq!(
            utc.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "2026-10-07T18:00:00Z"
        );
    }

    #[test]
    fn jd_and_mjd_represent_the_same_tt_instant() {
        let jd = InputState {
            time_mode: TimeMode::JulianDate,
            jd_tt: "2461321.25".into(),
            ..Default::default()
        };
        let mut mjd = jd.clone();
        mjd.time_mode = TimeMode::ModifiedJulianDate;
        mjd.mjd_tt = "61320.75".into();
        let jd_time = jd.resolve_time().unwrap().to_chrono().unwrap();
        let mjd_time = mjd.resolve_time().unwrap().to_chrono().unwrap();
        assert!((jd_time - mjd_time).num_milliseconds().abs() <= 1);
    }

    #[test]
    fn coordinate_validation_matches_domain_ranges() {
        let invalid_longitude = InputState {
            longitude: "181".into(),
            ..Default::default()
        };
        assert!(invalid_longitude.resolve_observer().is_err());

        let invalid_latitude = InputState {
            longitude: "0".into(),
            latitude: "-91".into(),
            ..Default::default()
        };
        assert!(invalid_latitude.resolve_observer().is_err());
    }

    #[test]
    fn icrs_sexagesimal_and_decimal_inputs_agree() {
        let mut state = InputState::default();
        let observer = state.resolve_observer().unwrap();
        let start = state.resolve_time().unwrap();
        let a = state.resolve_target(observer, start).unwrap();

        state.coordinate_format = CoordinateFormat::DecimalDegrees;
        state.ra_deg = "266.416833333".into();
        state.dec_deg = "-29.007805556".into();
        let b = state.resolve_target(observer, start).unwrap();

        assert!((a.ra().value() - b.ra().value()).abs() < 1.0e-8);
        assert!((a.dec().value() - b.dec().value()).abs() < 1.0e-8);
    }

    #[test]
    fn horizontal_input_resolves_to_finite_j2000_target() {
        let state = InputState {
            target_frame: TargetFrame::Horizontal,
            coordinate_format: CoordinateFormat::DecimalDegrees,
            ..Default::default()
        };
        let observer = state.resolve_observer().unwrap();
        let start = state.resolve_time().unwrap();
        let target = state.resolve_target(observer, start).unwrap();
        assert!(target.ra().value().is_finite());
        assert!(target.dec().value().is_finite());
    }
}
