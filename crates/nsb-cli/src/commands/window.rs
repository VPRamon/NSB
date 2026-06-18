use crate::cli::{OutputFormat, WindowArgs};
use crate::commands::point::{model_config, reject_unsupported_starlight};
use crate::output;
use crate::parsing::{components, location, radiance, target, time};
use anyhow::Result;
use nsb::{NsbEvaluator, ThresholdQuery};
use qtty::angular::Degrees;
use qtty::Second;
use tempoch::{Period, UTC};

pub fn run(args: WindowArgs, format: OutputFormat) -> Result<()> {
    let observer = location::resolve_observer(&args.observer)?;
    let target = target::resolve_target(&args.target);
    let start = time::parse_utc(&args.start)?;
    let end = time::parse_utc(&args.end)?;
    let max = radiance::parse_max_nsb(args.max_nsb)?;
    let min = radiance::parse_min_nsb(args.min_nsb, args.max_nsb)?;
    let components = components::parse_components(&args.model.components)?;
    reject_unsupported_starlight(components)?;
    let evaluator = NsbEvaluator::with_config(model_config(&args.model)?)?;

    let (sun_altitude_ceiling, target_altitude_floor) = if args.no_pre_filter {
        (None, None)
    } else {
        (
            Some(Degrees::new(args.sun_altitude_max)),
            Some(Degrees::new(args.target_altitude_min)),
        )
    };

    let base_query = ThresholdQuery {
        observer,
        target,
        window: Period::new(start, end),
        threshold: max,
        components,
        sample_step: Second::new(args.step),
        sun_altitude_ceiling,
        target_altitude_floor,
    };

    let max_result = evaluator.periods_below_threshold(&base_query)?;
    let periods = if let Some(min) = min {
        let min_query = ThresholdQuery {
            threshold: min,
            ..base_query.clone()
        };
        let below_min = evaluator.periods_below_threshold(&min_query)?;
        subtract_periods(&max_result.periods, &below_min.periods)
    } else {
        max_result.periods
    };

    output::write_window(format, start, end, min, max, components, &periods)
}

fn subtract_periods(base: &[Period<UTC>], remove: &[Period<UTC>]) -> Vec<Period<UTC>> {
    let mut out = Vec::new();
    for period in base {
        let mut fragments = vec![*period];
        for cut in remove {
            fragments = fragments
                .into_iter()
                .flat_map(|fragment| subtract_one(fragment, *cut))
                .collect();
        }
        out.extend(fragments);
    }
    out
}

fn subtract_one(period: Period<UTC>, cut: Period<UTC>) -> Vec<Period<UTC>> {
    if cut.end <= period.start || cut.start >= period.end {
        return vec![period];
    }
    let mut out = Vec::new();
    let left_end = cut.start.min(period.end);
    if period.start < left_end {
        out.push(Period::new(period.start, left_end));
    }
    let right_start = cut.end.max(period.start);
    if right_start < period.end {
        out.push(Period::new(right_start, period.end));
    }
    out
}
