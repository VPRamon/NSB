//! Validate deterministic merge of split Phase 5B mini-pilot HEALPix accumulators.

use anyhow::Result;
use clap::Parser;
use nsb_data_tools::gaia_xp_continuous_healpix::XpContinuousHealpixAccumulator;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    left_accumulator: PathBuf,
    #[arg(long)]
    right_accumulator: PathBuf,
    #[arg(long)]
    reference_accumulator: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
}

#[derive(Debug, Serialize)]
struct MergeValidation {
    left_checksum: String,
    right_checksum: String,
    merged_checksum: String,
    reference_checksum: String,
    identical: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let left: XpContinuousHealpixAccumulator =
        serde_json::from_str(&fs::read_to_string(&args.left_accumulator)?)?;
    let right: XpContinuousHealpixAccumulator =
        serde_json::from_str(&fs::read_to_string(&args.right_accumulator)?)?;
    let reference: XpContinuousHealpixAccumulator =
        serde_json::from_str(&fs::read_to_string(&args.reference_accumulator)?)?;
    let mut merged = left.clone();
    merged.merge(&right)?;
    let report = MergeValidation {
        left_checksum: left.checksum(),
        right_checksum: right.checksum(),
        merged_checksum: merged.checksum(),
        reference_checksum: reference.checksum(),
        identical: merged.checksum() == reference.checksum(),
    };
    if let Some(parent) = args.output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!(
        "phase5b merge validation identical={} -> {}",
        report.identical,
        args.output_json.display()
    );
    if !report.identical {
        anyhow::bail!("merged accumulator does not match reference");
    }
    Ok(())
}
