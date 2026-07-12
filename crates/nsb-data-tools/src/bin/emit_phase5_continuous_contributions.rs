//! Emit normalized 336–650 nm XP continuous-only contributions.

use anyhow::{Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, WriterBuilder};
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_xp_continuous::{integral_to_contribution, integrate_reconstructed_csv};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    continuous_only_targets: PathBuf,
    #[arg(long)]
    reconstructed_dir: PathBuf,
    #[arg(long)]
    output_csv: PathBuf,
    #[arg(long)]
    reconciliation_json: PathBuf,
    #[arg(long, default_value = "")]
    calibration_checksum: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Reconciliation {
    schema_version: u32,
    continuous_only_requested: u64,
    continuous_only_reconstructed: u64,
    continuous_only_valid: u64,
    continuous_only_excluded: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut requested = 0_u64;
    let mut reconstructed = 0_u64;
    let mut valid = 0_u64;
    let mut excluded = 0_u64;

    if let Some(parent) = args.output_csv.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = WriterBuilder::new().from_path(&args.output_csv)?;
    writer.write_record([
        "source_or_bin_id",
        "healpix_index",
        "multiplicity",
        "flux_336_650_ph_m2_s",
        "statistical_uncertainty_336_650_ph_m2_s",
        "systematic_uncertainty_336_650_ph_m2_s",
        "flags_extrapolation",
        "flags_crowding",
        "quality_flags",
        "branch",
    ])?;

    let mut reader = ReaderBuilder::new().from_path(&args.continuous_only_targets)?;
    let headers = reader.headers()?.clone();
    let sid_idx = headers.iter().position(|h| h == "source_id").unwrap();
    let cell_idx = headers.iter().position(|h| h == "spatial_cell").unwrap();

    for row in reader.records() {
        let row = row?;
        requested += 1;
        let source_id = row.get(sid_idx).context("source_id")?;
        let cell = row.get(cell_idx).context("spatial_cell")?;
        let path = args.reconstructed_dir.join(format!("{source_id}.csv"));
        if !path.is_file() {
            excluded += 1;
            continue;
        }
        reconstructed += 1;
        let input_checksum = sha256_file(&path)?;
        let (sid, integral) = integrate_reconstructed_csv(&path)?;
        if !integral.total_ph_m2_s.is_finite() {
            excluded += 1;
            continue;
        }
        let contribution =
            integral_to_contribution(&sid, &integral, &input_checksum, &args.calibration_checksum);
        valid += 1;
        writer.write_record([
            contribution.source_id.as_str(),
            cell,
            "1",
            contribution.flux_336_650_ph_m2_s.to_string().as_str(),
            contribution
                .statistical_uncertainty_336_650_ph_m2_s
                .map(|v| v.to_string())
                .unwrap_or_default()
                .as_str(),
            contribution
                .systematic_uncertainty_336_650_ph_m2_s
                .to_string()
                .as_str(),
            contribution.extrapolated.to_string().as_str(),
            "",
            contribution.quality_flags.as_str(),
            contribution.branch.as_str(),
        ])?;
    }
    writer.flush()?;

    let reconciliation = Reconciliation {
        schema_version: 1,
        continuous_only_requested: requested,
        continuous_only_reconstructed: reconstructed,
        continuous_only_valid: valid,
        continuous_only_excluded: excluded,
    };
    if let Some(parent) = args.reconciliation_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.reconciliation_json,
        serde_json::to_string_pretty(&reconciliation)? + "\n",
    )?;
    println!(
        "continuous-only contributions: valid={valid} excluded={excluded} requested={requested}"
    );
    Ok(())
}
