//! Prepare Phase 5 targets and freeze Phase 4 inputs.

use anyhow::Result;
use clap::Parser;
use nsb_data_tools::starlight_phase5::{
    extract_phase5_targets, verify_phase4_inputs, write_phase4_snapshot, write_targets_csv,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Freeze Phase 4 inputs and extract XP continuous Phase 5 targets")]
struct Args {
    #[arg(long, default_value = "~/nsb-data/starlight-gaia-release/missing-flux")]
    missing_flux_root: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5"
    )]
    phase5_root: PathBuf,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn main() -> Result<()> {
    let args = Args::parse();
    let missing_flux_root = expand(args.missing_flux_root);
    let phase5_root = expand(args.phase5_root);
    std::fs::create_dir_all(&phase5_root)?;

    let snapshot = verify_phase4_inputs(&missing_flux_root)?;
    write_phase4_snapshot(
        &phase5_root.join("phase5_phase4_inputs.snapshot.json"),
        &snapshot,
    )?;

    let overlap = extract_phase5_targets(&missing_flux_root, "xp_sampled_overlap")?;
    let continuous_only = extract_phase5_targets(&missing_flux_root, "xp_continuous_only")?;
    write_targets_csv(&phase5_root.join("phase5_overlap_targets.csv"), &overlap)?;
    write_targets_csv(
        &phase5_root.join("phase5_continuous_only_targets.csv"),
        &continuous_only,
    )?;
    let mut all = overlap.clone();
    all.extend(continuous_only.clone());
    write_targets_csv(
        &phase5_root.join("phase5_all_xp_continuous_targets.csv"),
        &all,
    )?;

    println!(
        "phase5 prepare: overlap={}, continuous_only={}",
        overlap.len(),
        continuous_only.len()
    );
    Ok(())
}
