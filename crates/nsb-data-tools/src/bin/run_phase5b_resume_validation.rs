//! Compare uninterrupted and resumed Phase 5B mini-pilot outputs.

use anyhow::{bail, Result};
use clap::Parser;
use nsb_data_tools::gaia_xp_continuous_healpix::XpContinuousHealpixAccumulator;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Validate Phase 5B mini-pilot resume equivalence against uninterrupted run")]
struct Args {
    #[arg(long)]
    uninterrupted_dir: PathBuf,
    #[arg(long)]
    resumed_dir: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
    #[arg(long)]
    reference_merge_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ResumeValidation {
    uninterrupted_rows_scanned: u64,
    resumed_rows_scanned: u64,
    uninterrupted_valid: u64,
    resumed_valid: u64,
    uninterrupted_excluded: u64,
    resumed_excluded: u64,
    healpix_identical: bool,
    flux_identical: bool,
    processed_counts_equal: bool,
    duplicate_source_ids: Vec<String>,
    missing_in_resumed: Vec<String>,
    extra_in_resumed: Vec<String>,
    multi_worker_identical: Option<bool>,
    passed: bool,
}

fn read_metrics(dir: &Path) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir.join("phase5b_mini_pilot_metrics.json"),
    )?)?)
}

fn read_manifest(dir: &Path) -> Result<serde_json::Value> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir.join("phase5b_mini_pilot_manifest.json"),
    )?)?)
}

fn read_healpix(dir: &Path) -> Result<XpContinuousHealpixAccumulator> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dir.join("phase5b_healpix_accumulator.json"),
    )?)?)
}

fn main() -> Result<()> {
    let args = Args::parse();
    let left_metrics = read_metrics(&args.uninterrupted_dir)?;
    let right_metrics = read_metrics(&args.resumed_dir)?;
    let left_manifest = read_manifest(&args.uninterrupted_dir)?;
    let right_manifest = read_manifest(&args.resumed_dir)?;
    let left_healpix = read_healpix(&args.uninterrupted_dir)?;
    let right_healpix = read_healpix(&args.resumed_dir)?;

    let left_ids: HashSet<String> = left_manifest["processed_source_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    let right_ids: HashSet<String> = right_manifest["processed_source_ids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();

    let duplicate_source_ids = right_ids
        .iter()
        .filter(|id| {
            right_manifest["processed_source_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|value| value.as_str() == Some(id.as_str()))
                .count()
                > 1
        })
        .cloned()
        .collect::<Vec<_>>();

    let missing_in_resumed = left_ids.difference(&right_ids).cloned().collect::<Vec<_>>();
    let extra_in_resumed = right_ids.difference(&left_ids).cloned().collect::<Vec<_>>();

    let healpix_identical = left_healpix.checksum() == right_healpix.checksum();
    let flux_identical = left_metrics["flux_checksum"] == right_metrics["flux_checksum"];
    let processed_counts_equal = left_metrics["sources_reconstructed"]
        == right_metrics["sources_reconstructed"]
        && left_metrics["rows_valid"] == right_metrics["rows_valid"]
        && left_metrics["rows_excluded"] == right_metrics["rows_excluded"];

    let multi_worker_identical = args.reference_merge_dir.as_ref().map(|merge_dir| {
        read_metrics(merge_dir)
            .ok()
            .and_then(|metrics| metrics["healpix_checksum"].as_str().map(str::to_string))
            .is_some_and(|checksum| checksum == left_healpix.checksum())
    });

    let passed = healpix_identical
        && flux_identical
        && processed_counts_equal
        && duplicate_source_ids.is_empty()
        && missing_in_resumed.is_empty()
        && extra_in_resumed.is_empty();

    let report = ResumeValidation {
        uninterrupted_rows_scanned: left_metrics["rows_scanned"].as_u64().unwrap_or(0),
        resumed_rows_scanned: right_metrics["rows_scanned"].as_u64().unwrap_or(0),
        uninterrupted_valid: left_metrics["rows_valid"].as_u64().unwrap_or(0),
        resumed_valid: right_metrics["rows_valid"].as_u64().unwrap_or(0),
        uninterrupted_excluded: left_metrics["rows_excluded"].as_u64().unwrap_or(0),
        resumed_excluded: right_metrics["rows_excluded"].as_u64().unwrap_or(0),
        healpix_identical,
        flux_identical,
        processed_counts_equal,
        duplicate_source_ids,
        missing_in_resumed,
        extra_in_resumed,
        multi_worker_identical,
        passed,
    };

    if let Some(parent) = args.output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!(
        "phase5b resume validation passed={} healpix_identical={} -> {}",
        report.passed,
        report.healpix_identical,
        args.output_json.display()
    );
    if !passed {
        bail!("resume validation failed");
    }
    Ok(())
}
