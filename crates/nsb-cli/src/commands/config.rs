use crate::cli::{ConfigArgs, ConfigCommand};
use crate::config::CliConfig;
use anyhow::{Context, Result};
use std::fs;

pub fn run(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Init => {
            let text = toml::to_string_pretty(&CliConfig::default())?;
            println!("{text}");
            Ok(())
        }
        ConfigCommand::Validate { path } => {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read config file {}", path.display()))?;
            let _: CliConfig = toml::from_str(&text)
                .with_context(|| format!("invalid config file {}", path.display()))?;
            println!("ok: {}", path.display());
            Ok(())
        }
    }
}
