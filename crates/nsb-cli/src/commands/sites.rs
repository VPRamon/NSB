use crate::cli::{OutputFormat, SitesArgs, SitesCommand};
use crate::output;
use crate::parsing::location::{resolve_site, SITE_PRESETS};
use anyhow::{bail, Result};

pub fn run(args: SitesArgs, format: OutputFormat) -> Result<()> {
    match args.command {
        SitesCommand::List => output::write_sites(format, SITE_PRESETS),
        SitesCommand::Show { alias } => {
            let Some(site) = resolve_site(&alias) else {
                bail!("unknown site alias {alias:?}");
            };
            output::write_sites(format, &[site])
        }
    }
}
