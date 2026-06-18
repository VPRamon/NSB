mod cli;
mod commands;
mod config;
mod error;
mod output;
mod parsing;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Point(args) => commands::point::run(args, cli.format),
        Command::Window(args) => commands::window::run(args, cli.format),
        Command::Sites(args) => commands::sites::run(args, cli.format),
        Command::Config(args) => commands::config::run(args),
    }
}
