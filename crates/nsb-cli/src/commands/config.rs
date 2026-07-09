use crate::cli::{ConfigArgs, ConfigCommand};
use crate::config::CliConfig;
use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;

pub fn run(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Init => {
            info!("printing starter configuration");
            let text = toml::to_string_pretty(&CliConfig::default())?;
            println!("{text}");
            Ok(())
        }
        ConfigCommand::Validate { path } => {
            info!("validating configuration file: {}", path.display());
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read config file {}", path.display()))?;
            debug!(
                "read configuration file: path={}, bytes={}",
                path.display(),
                text.len()
            );
            let _: CliConfig = toml::from_str(&text)
                .with_context(|| format!("invalid config file {}", path.display()))?;
            println!("ok: {}", path.display());
            Ok(())
        }
    }
}
