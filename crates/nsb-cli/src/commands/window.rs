use crate::cli::{OutputFormat, WindowArgs};
use crate::commands::point::model_config;
use crate::output;
use crate::parsing::{components, location, radiance, target, time};
use anyhow::Result;
use log::{debug, info};
use nsb::{NsbEvaluator, ThresholdQuery};
use qtty::angular::Degrees;
use qtty::Second;
use std::time::Instant;
use tempoch::{Period, UTC};

pub fn run(args: WindowArgs, format: OutputFormat) -> Result<()> {
    let started = Instant::now();
    info!(
        "starting threshold window search: start={}, end={}, min_nsb={:?}, max_nsb={}, step_seconds={}, components={}, format={format:?}",
        args.start,
        args.end,
        args.min_nsb,
        args.max_nsb,
        args.step,
        args.model.components
    );

    let observer = location::resolve_observer(&args.observer)?;
    let target = target::resolve_target(&args.target);
    let start = time::parse_utc(&args.start)?;
    let end = time::parse_utc(&args.end)?;
    let max = radiance::parse_max_nsb(args.max_nsb)?;
    let min = radiance::parse_min_nsb(args.min_nsb, args.max_nsb)?;
    let selection = components::parse_components(&args.model.components)?;
    let components = selection.mask;
    debug!(
        "resolved window query: site={:?}, lon={:?}, lat={:?}, height={:?}, ra={}, dec={}, components={components:?}, pre_filter_enabled={}",
        args.observer.site,
        args.observer.lon,
        args.observer.lat,
        args.observer.height,
        args.target.ra,
        args.target.dec,
        !args.no_pre_filter
    );

    let evaluator = NsbEvaluator::with_config(model_config(
        &args.model,
        selection,
        args.model.site_profile.into(),
    )?)?;

    let (sun_altitude_ceiling, target_altitude_floor) = if args.no_pre_filter {
        info!("threshold pre-filters disabled");
        (None, None)
    } else {
        let filters = (
            Some(Degrees::new(args.sun_altitude_max)),
            Some(Degrees::new(args.target_altitude_min)),
        );
        debug!(
            "threshold pre-filters: sun_altitude_max_deg={}, target_altitude_min_deg={}",
            args.sun_altitude_max, args.target_altitude_min
        );
        filters
    };

    let base_query = ThresholdQuery::new(observer, target, Period::new(start, end), max)
        .with_components(components)
        .with_sample_step(Second::new(args.step))
        .with_sun_altitude_ceiling(sun_altitude_ceiling)
        .with_target_altitude_floor(target_altitude_floor);

    info!("running max-threshold search");
    let max_result = evaluator.periods_below_threshold(&base_query)?;
    let periods = if let Some(min) = min {
        info!("running min-threshold exclusion search");
        let mut min_query = base_query.clone();
        min_query.threshold = min;
        let below_min = evaluator.periods_below_threshold(&min_query)?;
        subtract_periods(&max_result.periods, &below_min.periods)
    } else {
        max_result.periods
    };

    let descriptions = evaluator.describe_components(observer, components)?;
    info!(
        "completed threshold window search: period_count={}, elapsed_ms={}",
        periods.len(),
        started.elapsed().as_millis()
    );
    output::write_window(
        format,
        &output::WindowOutput {
            start,
            end,
            min,
            max,
            components,
            config: &evaluator.config(),
            descriptions: &descriptions,
            periods: &periods,
        },
    )
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
