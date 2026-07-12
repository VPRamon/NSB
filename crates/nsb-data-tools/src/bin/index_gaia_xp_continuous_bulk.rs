//! Build and query the Gaia DR3 XP continuous bulk file index.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nsb_data_tools::gaia_xp_continuous_bulk_index::{
    build_index, locate_and_verify_row, locate_source_id, write_index_csv,
};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Index Gaia DR3 XP continuous bulk files and locate source_id routes")]
struct Args {
    #[arg(long, default_value = "bulk/_MD5SUM.txt")]
    md5_manifest: PathBuf,
    #[arg(long, default_value = "bulk")]
    download_dir: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Resolve one source_id to its bulk file entry.
    Locate {
        #[arg(long)]
        source_id: String,
        #[arg(long, default_value_t = false)]
        verify_row: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    let index = build_index(
        &args.md5_manifest,
        &args.download_dir,
        Some(
            &args
                .download_dir
                .join("gaia_xp_continuous_bulk_manifest.json"),
        ),
    )?;
    let json_path = args.output_dir.join("phase5b_bulk_file_index.json");
    fs::write(&json_path, serde_json::to_string_pretty(&index)? + "\n")?;
    write_index_csv(&args.output_dir.join("phase5b_bulk_file_index.csv"), &index)?;
    if let Some(Command::Locate {
        source_id,
        verify_row,
    }) = args.command
    {
        let source_id = source_id
            .parse::<u64>()
            .with_context(|| format!("invalid source_id {source_id}"))?;
        let located = if verify_row {
            locate_and_verify_row(&index, source_id)?
        } else {
            locate_source_id(&index, source_id)?
        };
        println!("{}", serde_json::to_string_pretty(&located)?);
    } else {
        println!(
            "indexed {} bulk files -> {}",
            index.inventory_total_files,
            json_path.display()
        );
    }
    Ok(())
}
