// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
mod cli;
mod commands;
mod config;
mod error;
mod logging;
mod output;
mod parsing;
mod site_profiles;

use anyhow::{anyhow, Result};
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let level = logging::init(cli.log_level, cli.verbose)
        .map_err(|error| anyhow!("failed to initialize logging: {error}"))?;
    log::debug!("logging initialized at {level:?}");

    let result = match cli.command {
        Command::Point(args) => commands::point::run(args, cli.format),
        Command::Window(args) => commands::window::run(args, cli.format),
        Command::Sites(args) => commands::sites::run(args, cli.format),
        Command::Config(args) => commands::config::run(args),
    };

    if let Err(error) = &result {
        log::error!("command failed: {error:#}");
    }

    result
}
