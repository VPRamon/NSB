use crate::cli::{OutputFormat, PointArgs};
use crate::output;
use crate::parsing::{components, location, target, time};
use anyhow::Result;
use nsb::{MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, ZodiacalExtinction};

pub fn run(args: PointArgs, format: OutputFormat) -> Result<()> {
    let observer = location::resolve_observer(&args.observer)?;
    let time = time::parse_utc(&args.time)?;
    let target = target::resolve_target(&args.target);
    let components = components::parse_components(&args.model.components)?;
    let evaluator = NsbEvaluator::with_config(model_config(&args.model)?)?;

    let result = evaluator.evaluate(&PointQuery {
        observer,
        time,
        target,
        components,
    })?;

    output::write_point(format, time, observer, target, &result)
}

pub(crate) fn model_config(args: &crate::cli::ModelArgs) -> Result<NsbModelConfig> {
    let mut config = NsbModelConfig::standard();
    config.moonlight_model = match args.moonlight_model {
        crate::cli::MoonlightModelArg::Jones2013 => MoonlightModel::Jones2013Spectral,
        crate::cli::MoonlightModelArg::Ks1991 => MoonlightModel::KrisciunasSchaefer1991,
    };
    if let Some(sfu) = args.solar_radio_flux_sfu {
        config.solar_radio_flux = SolarFluxUnits::new(sfu);
    }
    config.zodiacal_extinction = match args.zodiacal_extinction {
        crate::cli::ZodiacalExtinctionArg::Noll2012 => ZodiacalExtinction::Noll2012Approx,
        crate::cli::ZodiacalExtinctionArg::None => ZodiacalExtinction::None,
    };
    Ok(config)
}
