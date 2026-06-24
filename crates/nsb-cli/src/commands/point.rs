use crate::cli::{OutputFormat, PointArgs};
use crate::output;
use crate::parsing::{components, location, target, time};
use anyhow::Result;
use nsb::{
    MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, ZodiacalExtinction,
};

pub fn run(args: PointArgs, format: OutputFormat) -> Result<()> {
    let observer = location::resolve_observer(&args.observer)?;
    let time = time::parse_utc(&args.time)?;
    let target = target::resolve_target(&args.target);
    let selection = components::parse_components(&args.model.components)?;
    let components = selection.mask;
    let evaluator = NsbEvaluator::with_config(model_config(
        &args.model,
        selection,
        location::site_profile(&args.observer),
    )?)?;

    let result = evaluator.evaluate(&PointQuery {
        observer,
        time,
        target,
        components,
    })?;

    output::write_point(format, time, observer, target, &evaluator.config(), &result)
}

pub(crate) fn model_config(
    args: &crate::cli::ModelArgs,
    components: components::ParsedComponents,
    site_profile: nsb::SiteProfileId,
) -> Result<NsbModelConfig> {
    let mut config = NsbModelConfig::generic_clear_sky();
    config.site_profile = site_profile;
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
    match components.starlight {
        Some(components::StarlightSelection::ExperimentalSeed) => {
            if args.starlight_map.is_some() || args.starlight_manifest.is_some() {
                anyhow::bail!(
                    "--starlight-map/--starlight-manifest are only valid with --components starlight"
                );
            }
            config.starlight_model = Some(nsb::StarlightModel::bundled_experimental_seed());
        }
        Some(components::StarlightSelection::ValidatedExternal) => {
            let map_path = args.starlight_map.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "validated starlight requires --starlight-map and --starlight-manifest"
                )
            })?;
            let manifest_path = args.starlight_manifest.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "validated starlight requires --starlight-map and --starlight-manifest"
                )
            })?;
            let map = nsb::ValidatedStarlightMap::from_files(map_path, manifest_path)?;
            config.starlight_model = Some(nsb::StarlightModel::validated_external(map));
        }
        None => {
            if args.starlight_map.is_some() || args.starlight_manifest.is_some() {
                anyhow::bail!(
                    "--starlight-map/--starlight-manifest require --components starlight"
                );
            }
        }
    }
    Ok(config)
}
