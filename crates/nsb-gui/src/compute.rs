use crate::input::CalculationRequest;
use chrono::{DateTime, Duration, Utc};
use nsb::units::angular::Degrees;
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use nsb::units::Second;
use nsb::{NsbEvaluator, PointQuery, ThresholdQuery};
use siderust::bodies::Sun as SunBody;
use siderust::event::altitude::{AltitudeEventsExt, SearchOpts};
use siderust::time::{Interval, ModifiedJulianDate};
use tempoch::{Period, Time, MJD, TT, UTC};

#[derive(Debug, Clone)]
pub struct Sample {
    pub time: DateTime<Utc>,
    pub integrated_radiance: f64,
    pub b_mag_arcsec2: f64,
    pub v_mag_arcsec2: f64,
}

#[derive(Debug, Clone)]
pub struct ComponentContribution {
    pub name: String,
    pub integrated_radiance: f64,
    pub share: f64,
}

#[derive(Debug, Clone)]
pub struct ObservingWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl ObservingWindow {
    pub fn duration_seconds(&self) -> f64 {
        (self.end - self.start).num_milliseconds() as f64 / 1000.0
    }

    fn contains(&self, time: DateTime<Utc>) -> bool {
        self.start <= time && time <= self.end
    }
}

#[derive(Debug, Clone)]
pub struct CalculationOutput {
    pub samples: Vec<Sample>,
    pub windows: Vec<ObservingWindow>,
    pub sun_below_horizon: Vec<ObservingWindow>,
    pub astronomical_night: Vec<ObservingWindow>,
    pub reference_time: DateTime<Utc>,
    pub reference_radiance: f64,
    pub reference_b_mag_arcsec2: f64,
    pub reference_v_mag_arcsec2: f64,
    pub components: Vec<ComponentContribution>,
    pub threshold: f64,
    pub sun_altitude_ceiling_deg: Option<f64>,
    pub target_altitude_floor_deg: Option<f64>,
    pub sample_step_seconds: f64,
    pub reference_satisfies_criteria: bool,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

pub fn calculate(request: &CalculationRequest) -> Result<CalculationOutput, String> {
    let evaluator = NsbEvaluator::new().map_err(|error| error.to_string())?;
    let start = request
        .start
        .to_chrono()
        .ok_or_else(|| "window start cannot be represented as a chrono UTC instant".to_string())?;
    let duration_ms = (request.duration_hours * 3_600_000.0).round() as i64;
    let end = start + Duration::milliseconds(duration_ms);
    let end_time = Time::<UTC>::from_chrono(end);

    let threshold_query = ThresholdQuery::new(
        request.observer,
        request.target,
        Period::new(request.start, end_time),
        BandPhotonRadiance::new(request.max_radiance),
    )
    .with_components(request.components)
    .with_sample_step(Second::new(request.sample_step_seconds))
    .with_sun_altitude_ceiling(request.sun_altitude_ceiling_deg.map(Degrees::new))
    .with_target_altitude_floor(request.target_altitude_floor_deg.map(Degrees::new));

    let threshold_result = evaluator
        .periods_below_threshold(&threshold_query)
        .map_err(|error| error.to_string())?;
    let windows = utc_periods_to_windows(&threshold_result.periods, "observing window")?;

    // Use the same Siderust solar-event API as NSB's planning pre-filter. The
    // broad interval is painted as twilight, then astronomical night is
    // overlaid, leaving the visible 0°..-18° portions as twilight.
    let tt_window = Interval::new(utc_to_tt_mjd(request.start), utc_to_tt_mjd(end_time));
    let sun_below_horizon = SunBody.below_threshold(
        &request.observer,
        tt_window,
        Degrees::new(0.0),
        SearchOpts::default(),
    );
    let astronomical_night = SunBody.below_threshold(
        &request.observer,
        tt_window,
        Degrees::new(-18.0),
        SearchOpts::default(),
    );
    let sun_below_horizon = tt_periods_to_windows(&sun_below_horizon, "twilight")?;
    let astronomical_night = tt_periods_to_windows(&astronomical_night, "astronomical night")?;

    let step_ms = (request.sample_step_seconds * 1000.0).round() as i64;
    let step = Duration::milliseconds(step_ms.max(1));
    let mut current = start;
    let mut samples = Vec::new();
    let mut reference = None;

    while current <= end {
        let time = Time::<UTC>::from_chrono(current);
        let result = evaluator
            .evaluate(
                &PointQuery::new(request.observer, time, request.target)
                    .with_components(request.components),
            )
            .map_err(|error| format!("NSB evaluation at {current}: {error}"))?;
        let integrated = result.integrated.value();
        let sample = Sample {
            time: current,
            integrated_radiance: integrated,
            b_mag_arcsec2: result.b_mag.value(),
            v_mag_arcsec2: result.v_mag.value(),
        };

        let should_replace =
            reference
                .as_ref()
                .is_none_or(|(_, best): &(DateTime<Utc>, nsb::NsbResult)| {
                    integrated < best.integrated.value()
                });
        if should_replace {
            reference = Some((current, result));
        }
        samples.push(sample);
        current += step;
    }

    if samples.last().is_some_and(|sample| sample.time < end) {
        let time = Time::<UTC>::from_chrono(end);
        let result = evaluator
            .evaluate(
                &PointQuery::new(request.observer, time, request.target)
                    .with_components(request.components),
            )
            .map_err(|error| format!("NSB evaluation at {end}: {error}"))?;
        let integrated = result.integrated.value();
        let should_replace = reference
            .as_ref()
            .is_none_or(|(_, best)| integrated < best.integrated.value());
        if should_replace {
            reference = Some((end, result.clone()));
        }
        samples.push(Sample {
            time: end,
            integrated_radiance: integrated,
            b_mag_arcsec2: result.b_mag.value(),
            v_mag_arcsec2: result.v_mag.value(),
        });
    }

    let (reference_time, reference_result) =
        reference.ok_or_else(|| "calculation produced no samples".to_string())?;
    let reference_satisfies_criteria = windows.iter().any(|window| window.contains(reference_time));
    let total = reference_result.integrated.value();
    let components = reference_result
        .components
        .iter()
        .map(|component| {
            let value = component.integrated.value();
            let share = if total > 0.0 {
                (value / total).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ComponentContribution {
                name: humanize_component(component.name),
                integrated_radiance: value,
                share,
            }
        })
        .collect();

    Ok(CalculationOutput {
        samples,
        windows,
        sun_below_horizon,
        astronomical_night,
        reference_time,
        reference_radiance: total,
        reference_b_mag_arcsec2: reference_result.b_mag.value(),
        reference_v_mag_arcsec2: reference_result.v_mag.value(),
        components,
        threshold: request.max_radiance,
        sun_altitude_ceiling_deg: request.sun_altitude_ceiling_deg,
        target_altitude_floor_deg: request.target_altitude_floor_deg,
        sample_step_seconds: request.sample_step_seconds,
        reference_satisfies_criteria,
        start,
        end,
    })
}

fn utc_to_tt_mjd(time: Time<UTC>) -> ModifiedJulianDate {
    ModifiedJulianDate::from(time.to::<TT>().to::<MJD>())
}

fn utc_periods_to_windows(
    periods: &[Period<UTC>],
    label: &str,
) -> Result<Vec<ObservingWindow>, String> {
    periods
        .iter()
        .map(|period| {
            let start = period
                .start
                .to_chrono()
                .ok_or_else(|| format!("{label} start is outside chrono range"))?;
            let end = period
                .end
                .to_chrono()
                .ok_or_else(|| format!("{label} end is outside chrono range"))?;
            Ok(ObservingWindow { start, end })
        })
        .collect()
}

fn tt_periods_to_windows(
    periods: &[Interval<ModifiedJulianDate>],
    label: &str,
) -> Result<Vec<ObservingWindow>, String> {
    periods
        .iter()
        .map(|period| {
            let start = Time::<TT>::from(period.start)
                .to::<UTC>()
                .to_chrono()
                .ok_or_else(|| format!("{label} start is outside chrono range"))?;
            let end = Time::<TT>::from(period.end)
                .to::<UTC>()
                .to_chrono()
                .ok_or_else(|| format!("{label} end is outside chrono range"))?;
            Ok(ObservingWindow { start, end })
        })
        .collect()
}

fn humanize_component(name: &str) -> String {
    match name {
        "zodiacal" => "Zodiacal light",
        "starlight" => "Integrated starlight",
        "airglow" => "Airglow",
        "moon" => "Moonlight",
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputState;

    #[test]
    fn duration_seconds_preserves_subminute_windows() {
        let start = DateTime::parse_from_rfc3339("2026-10-07T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + Duration::seconds(90);
        assert_eq!(ObservingWindow { start, end }.duration_seconds(), 90.0);
    }

    #[test]
    fn observing_window_contains_both_authoritative_boundaries() {
        let start = DateTime::parse_from_rfc3339("2026-10-07T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + Duration::minutes(10);
        let window = ObservingWindow { start, end };

        assert!(window.contains(start));
        assert!(window.contains(end));
        assert!(!window.contains(start - Duration::milliseconds(1)));
        assert!(!window.contains(end + Duration::milliseconds(1)));
    }

    #[test]
    fn component_names_are_presentable() {
        assert_eq!(humanize_component("zodiacal"), "Zodiacal light");
        assert_eq!(humanize_component("moon"), "Moonlight");
    }

    #[test]
    fn short_default_request_runs_through_authoritative_library_apis() {
        let mut request = InputState::default().resolve().unwrap();
        request.duration_hours = 0.05;
        request.sample_step_seconds = 60.0;

        let output = calculate(&request).unwrap();
        assert_eq!(output.samples.len(), 4);
        assert!(output
            .samples
            .iter()
            .all(|sample| sample.integrated_radiance.is_finite()));
        assert!(output.windows.iter().all(|window| {
            request.start.to_chrono().unwrap() <= window.start && window.end <= output.end
        }));
    }
}
