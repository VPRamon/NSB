use crate::error::{NsbError, Result};
use chrono::{DateTime, Duration, Utc};
use qtty::{Quantity, Second, Unit};
use tempoch::{Period, Time, UTC};

const MAX_CROSSING_REFINEMENTS: usize = 24;
const CROSSING_TOLERANCE_MICROSECONDS: i64 = 1_000_000;

/// Find UTC sub-periods where a sampled scalar quantity stays within `[min, max]`.
///
/// The search brackets transitions with `sample_step` and refines each detected
/// range boundary by bisection. It deliberately works over typed quantities so
/// component models can reuse the same temporal-search semantics without
/// duplicating unit-erasing code.
pub(crate) fn periods_in_range<V, F>(
    window: Period<UTC>,
    sample_step: Second,
    min: Quantity<V>,
    max: Quantity<V>,
    value_at: F,
) -> Result<Vec<Period<UTC>>>
where
    V: Unit,
    F: Fn(Time<UTC>) -> Result<Quantity<V>>,
{
    validate_range_inputs(window, sample_step, min, max)?;

    let start = utc_to_chrono(window.start)?;
    let end = utc_to_chrono(window.end)?;
    let chrono_window = (start, end);
    if start >= end {
        return Ok(Vec::new());
    }

    let step = duration_from_seconds(sample_step)?;
    let mut periods = Vec::new();
    let mut t0 = start;
    let mut y0 = value_at_chrono(t0, &value_at)?;
    let mut inside0 = inside_range(y0, min, max)?;
    let mut open_start = inside0.then_some(start);

    while t0 < end {
        let t1 = add_step_clamped(t0, step, end);
        if t1 <= t0 {
            break;
        }

        let y1 = value_at_chrono(t1, &value_at)?;
        let inside1 = inside_range(y1, min, max)?;
        if inside0 != inside1 {
            let crossing = refine_range_crossing(t0, y0, t1, y1, &value_at, (min, max), inside0)?;
            if inside0 {
                if let Some(start) = open_start.take() {
                    push_non_empty_period(&mut periods, start, crossing, window, chrono_window);
                }
            } else {
                open_start = Some(crossing);
            }
        } else if let Some((entry, exit)) = complete_band_straddle_boundaries(y0, y1, min, max) {
            let entry_time = refine_threshold_crossing(t0, y0, t1, y1, &value_at, entry)?;
            let exit_time = refine_threshold_crossing(t0, y0, t1, y1, &value_at, exit)?;
            let (start, end) = if entry_time <= exit_time {
                (entry_time, exit_time)
            } else {
                (exit_time, entry_time)
            };
            push_non_empty_period(&mut periods, start, end, window, chrono_window);
        }

        t0 = t1;
        y0 = y1;
        inside0 = inside1;
    }

    if let Some(start) = open_start {
        push_non_empty_period(&mut periods, start, end, window, chrono_window);
    }

    Ok(periods)
}

fn validate_range_inputs<V>(
    window: Period<UTC>,
    sample_step: Second,
    min: Quantity<V>,
    max: Quantity<V>,
) -> Result<()>
where
    V: Unit,
{
    if window.start > window.end {
        return Err(NsbError::OutOfRange(
            "query window start must not be after end".to_string(),
        ));
    }
    if !sample_step.is_finite() || sample_step <= Second::new(0.0) {
        return Err(NsbError::OutOfRange(
            "sample_step must be finite and greater than zero".to_string(),
        ));
    }
    if !min.is_finite() || !max.is_finite() {
        return Err(NsbError::OutOfRange(
            "range bounds must be finite".to_string(),
        ));
    }
    if min < Quantity::new(0.0) || max < Quantity::new(0.0) {
        return Err(NsbError::OutOfRange(
            "radiance range bounds must be non-negative".to_string(),
        ));
    }
    if min > max {
        return Err(NsbError::OutOfRange(
            "minimum radiance must be less than or equal to maximum radiance".to_string(),
        ));
    }
    Ok(())
}

fn value_at_chrono<V, F>(time: DateTime<Utc>, value_at: &F) -> Result<Quantity<V>>
where
    V: Unit,
    F: Fn(Time<UTC>) -> Result<Quantity<V>>,
{
    let value = value_at(Time::<UTC>::from_chrono(time))?;
    if !value.is_finite() {
        return Err(NsbError::OutOfRange(
            "sampled radiance must be finite".to_string(),
        ));
    }
    Ok(value)
}

fn inside_range<V>(value: Quantity<V>, min: Quantity<V>, max: Quantity<V>) -> Result<bool>
where
    V: Unit,
{
    if !value.is_finite() {
        return Err(NsbError::OutOfRange(
            "sampled radiance must be finite".to_string(),
        ));
    }
    Ok(value >= min && value <= max)
}

fn complete_band_straddle_boundaries<V>(
    y0: Quantity<V>,
    y1: Quantity<V>,
    min: Quantity<V>,
    max: Quantity<V>,
) -> Option<(Quantity<V>, Quantity<V>)>
where
    V: Unit,
{
    if y0 < min && y1 > max {
        Some((min, max))
    } else if y0 > max && y1 < min {
        Some((max, min))
    } else {
        None
    }
}

fn refine_range_crossing<V, F>(
    mut lo: DateTime<Utc>,
    mut y_lo: Quantity<V>,
    mut hi: DateTime<Utc>,
    mut y_hi: Quantity<V>,
    value_at: &F,
    bounds: (Quantity<V>, Quantity<V>),
    lo_inside: bool,
) -> Result<DateTime<Utc>>
where
    V: Unit,
    F: Fn(Time<UTC>) -> Result<Quantity<V>>,
{
    let (min, max) = bounds;
    for _ in 0..MAX_CROSSING_REFINEMENTS {
        let mid = midpoint(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = value_at_chrono(mid, value_at)?;
        let mid_inside = inside_range(y_mid, min, max)?;
        if mid_inside == lo_inside {
            lo = mid;
            y_lo = y_mid;
        } else {
            hi = mid;
            y_hi = y_mid;
        }
        if microseconds_between(lo, hi).is_some_and(|us| us <= CROSSING_TOLERANCE_MICROSECONDS) {
            break;
        }
    }

    Ok(linear_boundary_estimate(lo, y_lo, hi, y_hi, min, max))
}

fn refine_threshold_crossing<V, F>(
    mut lo: DateTime<Utc>,
    mut y_lo: Quantity<V>,
    mut hi: DateTime<Utc>,
    mut y_hi: Quantity<V>,
    value_at: &F,
    threshold: Quantity<V>,
) -> Result<DateTime<Utc>>
where
    V: Unit,
    F: Fn(Time<UTC>) -> Result<Quantity<V>>,
{
    let lo_below = y_lo < threshold;
    for _ in 0..MAX_CROSSING_REFINEMENTS {
        let mid = midpoint(lo, hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let y_mid = value_at_chrono(mid, value_at)?;
        if (y_mid < threshold) == lo_below {
            lo = mid;
            y_lo = y_mid;
        } else {
            hi = mid;
            y_hi = y_mid;
        }
        if microseconds_between(lo, hi).is_some_and(|us| us <= CROSSING_TOLERANCE_MICROSECONDS) {
            break;
        }
    }

    Ok(linear_threshold_estimate(lo, y_lo, hi, y_hi, threshold))
}

fn linear_boundary_estimate<V>(
    lo: DateTime<Utc>,
    y_lo: Quantity<V>,
    hi: DateTime<Utc>,
    y_hi: Quantity<V>,
    min: Quantity<V>,
    max: Quantity<V>,
) -> DateTime<Utc>
where
    V: Unit,
{
    let Some(threshold) = crossed_boundary_value(y_lo, y_hi, min, max) else {
        return midpoint(lo, hi);
    };
    linear_boundary_value_estimate(lo, y_lo, hi, y_hi, threshold)
}

fn linear_threshold_estimate<V>(
    lo: DateTime<Utc>,
    y_lo: Quantity<V>,
    hi: DateTime<Utc>,
    y_hi: Quantity<V>,
    threshold: Quantity<V>,
) -> DateTime<Utc>
where
    V: Unit,
{
    linear_boundary_value_estimate(lo, y_lo, hi, y_hi, threshold.value())
}

fn linear_boundary_value_estimate<V>(
    lo: DateTime<Utc>,
    y_lo: Quantity<V>,
    hi: DateTime<Utc>,
    y_hi: Quantity<V>,
    threshold: f64,
) -> DateTime<Utc>
where
    V: Unit,
{
    let denom = y_hi.value() - y_lo.value();
    if !denom.is_finite() || denom == 0.0 {
        return midpoint(lo, hi);
    }
    let Some(total_us) = microseconds_between(lo, hi) else {
        return midpoint(lo, hi);
    };
    let frac = ((threshold - y_lo.value()) / denom).clamp(0.0, 1.0);
    let offset_us = (total_us as f64 * frac).round() as i64;
    let candidate = lo + Duration::microseconds(offset_us);
    candidate.clamp(lo, hi)
}

fn crossed_boundary_value<V>(
    y0: Quantity<V>,
    y1: Quantity<V>,
    min: Quantity<V>,
    max: Quantity<V>,
) -> Option<f64>
where
    V: Unit,
{
    let (a, b) = (y0.value(), y1.value());
    let min = min.value();
    let max = max.value();
    if (a < min && b >= min) || (b < min && a >= min) {
        Some(min)
    } else if (a > max && b <= max) || (b > max && a <= max) {
        Some(max)
    } else {
        None
    }
}

fn duration_from_seconds(step: Second) -> Result<Duration> {
    let microseconds = (step.value() * 1.0e6).round();
    if !microseconds.is_finite() || microseconds <= 0.0 || microseconds > i64::MAX as f64 {
        return Err(NsbError::OutOfRange(
            "sample_step is outside the representable UTC duration range".to_string(),
        ));
    }
    Ok(Duration::microseconds(microseconds as i64))
}

fn add_step_clamped(time: DateTime<Utc>, step: Duration, end: DateTime<Utc>) -> DateTime<Utc> {
    let next = time + step;
    if next > end {
        end
    } else {
        next
    }
}

fn midpoint(lo: DateTime<Utc>, hi: DateTime<Utc>) -> DateTime<Utc> {
    let Some(us) = microseconds_between(lo, hi) else {
        return lo;
    };
    lo + Duration::microseconds(us / 2)
}

fn microseconds_between(lo: DateTime<Utc>, hi: DateTime<Utc>) -> Option<i64> {
    hi.signed_duration_since(lo).num_microseconds()
}

fn utc_to_chrono(time: Time<UTC>) -> Result<DateTime<Utc>> {
    time.to_chrono().ok_or_else(|| {
        NsbError::OutOfRange("UTC instant is outside chrono's representable range".to_string())
    })
}

fn push_non_empty_period(
    periods: &mut Vec<Period<UTC>>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    window: Period<UTC>,
    chrono_window: (DateTime<Utc>, DateTime<Utc>),
) {
    if start < end {
        let start = if start == chrono_window.0 {
            window.start
        } else {
            Time::<UTC>::from_chrono(start)
        };
        let end = if end == chrono_window.1 {
            window.end
        } else {
            Time::<UTC>::from_chrono(end)
        };
        periods.push(Period::new(start, end));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;

    fn parse_utc(input: &str) -> Time<UTC> {
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339(input)
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    fn ten_minute_window() -> Period<UTC> {
        Period::new(
            parse_utc("2023-09-04T02:00:00Z"),
            parse_utc("2023-09-04T02:10:00Z"),
        )
    }

    fn assert_detects_complete_band_straddle(slope_sign: f64) {
        let window = ten_minute_window();
        let start = utc_to_chrono(window.start).unwrap();

        let periods = periods_in_range(
            window,
            Second::new(600.0),
            Radiance::new(8.0),
            Radiance::new(12.0),
            |time| {
                let elapsed = utc_to_chrono(time)?
                    .signed_duration_since(start)
                    .num_microseconds()
                    .expect("ten-minute test interval fits in i64 microseconds")
                    as f64
                    / 1.0e6;
                let ascending = elapsed / 30.0;
                let value = if slope_sign.is_sign_positive() {
                    ascending
                } else {
                    20.0 - ascending
                };
                Ok(Radiance::new(value))
            },
        )
        .unwrap();

        assert_eq!(periods.len(), 1);
        let expected_start = start + Duration::seconds(240);
        let expected_end = start + Duration::seconds(360);
        assert!(
            utc_to_chrono(periods[0].start)
                .unwrap()
                .signed_duration_since(expected_start)
                .abs()
                <= Duration::milliseconds(1)
        );
        assert!(
            utc_to_chrono(periods[0].end)
                .unwrap()
                .signed_duration_since(expected_end)
                .abs()
                <= Duration::milliseconds(1)
        );
    }

    #[test]
    fn periods_in_range_detects_ascending_complete_band_straddle_between_samples() {
        assert_detects_complete_band_straddle(1.0);
    }

    #[test]
    fn periods_in_range_detects_descending_complete_band_straddle_between_samples() {
        assert_detects_complete_band_straddle(-1.0);
    }
}
