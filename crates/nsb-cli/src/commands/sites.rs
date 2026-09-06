use crate::cli::{OutputFormat, SitesArgs, SitesCommand};
use crate::output;
use crate::parsing::location::{resolve_site, site_presets};
use anyhow::{bail, Result};
use log::{debug, info, warn};

pub fn run(args: SitesArgs, format: OutputFormat) -> Result<()> {
    match args.command {
        SitesCommand::List => {
            let sites = site_presets();
            info!("listing known site aliases: count={}", sites.len());
            output::write_sites(format, sites)
        }
        SitesCommand::Show { alias } => {
            debug!("resolving site alias: {alias}");
            let Some(site) = resolve_site(&alias) else {
                warn!("unknown site alias requested: {alias}");
                bail!("unknown site alias {alias:?}");
            };
            info!("showing site alias: {alias}");
            output::write_sites(format, std::slice::from_ref(site))
        }
    }
}
