//! Candidate-window smoothing and airglow phase helpers for threshold search.

use super::types::PreparedThresholdQuery;
use crate::components::airglow;
use crate::evaluator::ComponentMask;
use siderust::time::{Interval as TimePeriod, ModifiedJulianDate};

pub(crate) fn contains_time(
    periods: &[TimePeriod<ModifiedJulianDate>],
    time: ModifiedJulianDate,
) -> bool {
    periods
        .iter()
        .any(|period| period.start <= time && time <= period.end)
}

pub(crate) fn smooth_threshold_windows(
    prepared: &PreparedThresholdQuery,
) -> Vec<TimePeriod<ModifiedJulianDate>> {
    let mut out = Vec::new();
    for candidate in &prepared.candidate_windows {
        let mut boundaries = vec![candidate.start, candidate.end];
        if prepared.components.contains(ComponentMask::AIRGLOW) {
            for phase in prepared.airglow_phase_periods.iter() {
                collect_internal_boundaries(&mut boundaries, phase.period, *candidate);
            }
        }
        if let Some(moon_visible_periods) = &prepared.moon_visible_periods {
            for moon_period in moon_visible_periods.iter() {
                collect_internal_boundaries(&mut boundaries, *moon_period, *candidate);
            }
        }

        boundaries.sort_by(|lhs, rhs| lhs.raw().value().total_cmp(&rhs.raw().value()));
        boundaries
            .dedup_by(|lhs, rhs| (lhs.raw().value() - rhs.raw().value()).abs() <= f64::EPSILON);
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
