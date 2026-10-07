use crate::input::CalculationRequest;
use chrono::{DateTime, Duration, Utc};
use nsb::units::angular::Degrees;
use nsb::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance;
use nsb::units::Second;
use nsb::{NsbEvaluator, PointQuery, ThresholdQuery};
use tempoch::{Period, Time, UTC};

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
}

#[derive(Debug, Clone)]
pub struct CalculationOutput {
    pub samples: Vec<Sample>,
    pub windows: Vec<ObservingWindow>,
    pub reference_time: DateTime<Utc>,
    pub reference_radiance: f64,
    pub reference_b_mag_arcsec2: f64,
    pub reference_v_mag_arcsec2: f64,
    pub components: Vec<ComponentContribution>,
    pub threshold: f64,
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
    let windows = threshold_result
        .periods
        .iter()
        .map(|period| {
            let start = period
                .start
                .to_chrono()
                .ok_or_else(|| "observing-window start is outside chrono range".to_string())?;
            let end = period
                .end
                .to_chrono()
                .ok_or_else(|| "observing-window end is outside chrono range".to_string())?;
            Ok(ObservingWindow { start, end })
        })
        .collect::<Result<Vec<_>, String>>()?;

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
        reference_time,
        reference_radiance: total,
        reference_b_mag_arcsec2: reference_result.b_mag.value(),
        reference_v_mag_arcsec2: reference_result.v_mag.value(),
        components,
        threshold: request.max_radiance,
        start,
        end,
    })
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

    #[test]
    fn duration_seconds_preserves_subminute_windows() {
        let start = DateTime::parse_from_rfc3339("2026-10-07T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = start + Duration::seconds(90);
        assert_eq!(ObservingWindow { start, end }.duration_seconds(), 90.0);
    }

    #[test]
    fn component_names_are_presentable() {
        assert_eq!(humanize_component("zodiacal"), "Zodiacal light");
        assert_eq!(humanize_component("moon"), "Moonlight");
    }
}
