use crate::cli::{OutputFormat, SitesArgs, SitesCommand};
use crate::output;
use crate::parsing::location::{resolve_site, SITE_PRESETS};
use anyhow::{bail, Result};
use log::{debug, info, warn};

pub fn run(args: SitesArgs, format: OutputFormat) -> Result<()> {
    match args.command {
        SitesCommand::List => {
            info!("listing known site aliases: count={}", SITE_PRESETS.len());
            output::write_sites(format, SITE_PRESETS)
        }
        SitesCommand::Show { alias } => {
            debug!("resolving site alias: {alias}");
            let Some(site) = resolve_site(&alias) else {
                warn!("unknown site alias requested: {alias}");
                bail!("unknown site alias {alias:?}");
            };
            info!("showing site alias: {alias}");
            output::write_sites(format, &[site])
        }
    }
}
