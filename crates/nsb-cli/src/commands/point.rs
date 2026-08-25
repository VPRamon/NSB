use crate::cli::{OutputFormat, PointArgs};
use crate::output;
use crate::parsing::{components, location, target, time};
use anyhow::Result;
use log::{debug, info};
use nsb::{
    MoonlightModel, NsbEvaluator, NsbModelConfig, PointQuery, SolarFluxUnits, ZodiacalExtinction,
};
use std::time::Instant;

pub fn run(args: PointArgs, format: OutputFormat) -> Result<()> {
    let started = Instant::now();
    info!(
        "starting point evaluation: time={}, components={}, format={format:?}",
        args.time, args.model.components
    );

    let observer = location::resolve_observer(&args.observer)?;
    let time = time::parse_utc(&args.time)?;
    let target = target::resolve_target(&args.target);
    let selection = components::parse_components(&args.model.components)?;
    let components = selection.mask;
    debug!(
        "resolved point query: site={:?}, lon={:?}, lat={:?}, height={:?}, ra={}, dec={}, components={components:?}",
        args.observer.site,
        args.observer.lon,
        args.observer.lat,
        args.observer.height,
        args.target.ra,
        args.target.dec
    );

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

    info!(
        "completed point evaluation: component_count={}, elapsed_ms={}",
        result.components.len(),
        started.elapsed().as_millis()
    );

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
        debug!("using explicit solar radio flux: {sfu} sfu");
        config.solar_radio_flux = SolarFluxUnits::new(sfu);
    }
    config.zodiacal_extinction = match args.zodiacal_extinction {
        crate::cli::ZodiacalExtinctionArg::Noll2012 => ZodiacalExtinction::Noll2012Approx,
        crate::cli::ZodiacalExtinctionArg::None => ZodiacalExtinction::None,
    };
    match components.starlight {
        Some(components::StarlightSelection::Production) => {
            config.starlight_model = Some(match (&args.starlight_map, &args.starlight_manifest) {
                (Some(_), Some(_)) => validated_external_starlight(args)?,
                (None, None) => {
                    debug!("using bundled production Gaia DR3 starlight model");
                    nsb::StarlightModel::bundled_production_gaia_dr3()
                }
                _ => anyhow::bail!(
                    "--starlight-map and --starlight-manifest must be provided together"
                ),
            });
        }
        None => {
            if args.starlight_map.is_some() || args.starlight_manifest.is_some() {
                if components.mask.contains(nsb::ComponentMask::STARLIGHT) {
                    config.starlight_model = Some(validated_external_starlight(args)?);
                } else {
                    anyhow::bail!(
                        "--starlight-map/--starlight-manifest require --components starlight"
                    );
                }
            }
        }
    }
    debug!("resolved model config: {config:?}");
    Ok(config)
}

fn validated_external_starlight(args: &crate::cli::ModelArgs) -> Result<nsb::StarlightModel> {
    let (Some(map_path), Some(manifest_path)) = (&args.starlight_map, &args.starlight_manifest)
    else {
        anyhow::bail!("--starlight-map and --starlight-manifest must be provided together");
    };
    info!(
        "loading validated external starlight map: map={}, manifest={}",
        map_path.display(),
        manifest_path.display()
    );
    let map = nsb::ValidatedStarlightMap::from_files(map_path, manifest_path)?;
    Ok(nsb::StarlightModel::validated_external(map))
}
