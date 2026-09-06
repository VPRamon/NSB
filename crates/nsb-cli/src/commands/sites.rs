use crate::cli::{OutputFormat, SitesArgs, SitesCommand};
use crate::output;
use crate::parsing::location::{catalog_output, load_catalog, resolve_site, ObservatoryOutput};
use anyhow::{bail, Result};
use log::{debug, info, warn};

pub fn run(args: SitesArgs, format: OutputFormat) -> Result<()> {
    let catalog = load_catalog(args.observatory_catalog.as_deref())?;
    match args.command {
        SitesCommand::List => {
            info!("listing observatory catalog: count={}", catalog.len());
            output::write_sites(format, &catalog_output(&catalog))
        }
        SitesCommand::Show { alias } => {
            debug!("resolving site alias: {alias}");
            let Some(site) = resolve_site(&catalog, &alias) else {
                warn!("unknown site alias requested: {alias}");
                bail!("unknown observatory name or alias {alias:?}");
            };
            info!("showing site alias: {alias}");
            output::write_sites(format, &[ObservatoryOutput::from_observatory(site)])
        }
    }
}
