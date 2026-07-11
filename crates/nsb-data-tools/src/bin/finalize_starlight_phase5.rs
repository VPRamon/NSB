//! Finalize Phase 5 population reconciliation and checksum manifest.

use anyhow::{Context, Result};
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::gaia_datalink::datalink_raw_coefficient_path;
use nsb_data_tools::starlight_phase5::write_sha256sum;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    phase5_root: PathBuf,
    #[arg(long)]
    overlap_targets: PathBuf,
    #[arg(long)]
    continuous_only_targets: PathBuf,
    #[arg(long)]
    raw_dir: PathBuf,
    #[arg(long)]
    reconstructed_dir: PathBuf,
    #[arg(long)]
    overlap_validation_json: PathBuf,
    #[arg(long)]
    output_reconciliation: PathBuf,
    #[arg(long)]
    exclusions_csv: PathBuf,
}

#[derive(Debug, Deserialize)]
struct OverlapValidationSummary {
    global: MetricSummary,
}

#[derive(Debug, Deserialize)]
struct MetricSummary {
    sample_count: u64,
}

#[derive(Debug, Serialize)]
struct PopulationReconciliation {
    schema_version: u32,
    overlap_requested: u64,
    overlap_retrieved: u64,
    overlap_reconstructed: u64,
    overlap_valid: u64,
    overlap_excluded: u64,
    continuous_only_requested: u64,
    continuous_only_retrieved: u64,
    continuous_only_reconstructed: u64,
    continuous_only_valid: u64,
    continuous_only_excluded: u64,
}

fn count_targets(path: &Path) -> Result<u64> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    Ok(reader.records().count() as u64)
}

fn load_overlap_ids(path: &Path) -> Result<HashSet<String>> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    let headers = reader.headers()?.clone();
    let idx = headers
        .iter()
        .position(|h| h == "source_id")
        .context("source_id")?;
    let mut ids = HashSet::new();
    for row in reader.records() {
        ids.insert(row?.get(idx).context("source_id")?.to_string());
    }
    Ok(ids)
}

fn count_retrieved(raw_dir: &Path, ids: &HashSet<String>) -> u64 {
    ids.iter()
        .filter(|id| datalink_raw_coefficient_path(raw_dir, id).is_file())
        .count() as u64
}

fn count_reconstructed(reconstructed_dir: &Path, ids: &HashSet<String>) -> u64 {
    ids.iter()
        .filter(|id| reconstructed_dir.join(format!("{id}.csv")).is_file())
        .count() as u64
}

fn write_exclusions(path: &Path, rows: &[(String, String, String, String, String)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    writer.write_record(["source_id", "reason_code", "evidence", "impact", "fallback"])?;
    for (source_id, reason, evidence, impact, fallback) in rows {
        writer.write_record([source_id, reason, evidence, impact, fallback])?;
    }
    writer.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let overlap_ids = load_overlap_ids(&args.overlap_targets)?;
    let continuous_ids = load_overlap_ids(&args.continuous_only_targets)?;

    let overlap_requested = count_targets(&args.overlap_targets)?;
    let continuous_only_requested = count_targets(&args.continuous_only_targets)?;
    let overlap_retrieved = count_retrieved(&args.raw_dir, &overlap_ids);
    let continuous_retrieved = count_retrieved(&args.raw_dir, &continuous_ids);
    let overlap_reconstructed = count_reconstructed(&args.reconstructed_dir, &overlap_ids);
    let continuous_reconstructed = count_reconstructed(&args.reconstructed_dir, &continuous_ids);

    let validation: OverlapValidationSummary =
        serde_json::from_str(&fs::read_to_string(&args.overlap_validation_json)?)?;
    let overlap_valid = validation.global.sample_count;
    let overlap_excluded = overlap_requested.saturating_sub(overlap_valid);

    let contributions_path = args.phase5_root.join("phase5_continuous_only_336_650.csv");
    let mut continuous_valid = 0_u64;
    if contributions_path.is_file() {
        let mut reader = ReaderBuilder::new().from_path(&contributions_path)?;
        continuous_valid = reader.records().count() as u64;
    }
    let continuous_only_excluded = continuous_only_requested.saturating_sub(continuous_valid);

    let mut exclusions = Vec::new();
    for id in overlap_ids
        .iter()
        .filter(|id| !args.reconstructed_dir.join(format!("{id}.csv")).is_file())
    {
        let retrieved = datalink_raw_coefficient_path(&args.raw_dir, id).is_file();
        exclusions.push((
            id.clone(),
            if retrieved {
                "reconstruction_missing".to_string()
            } else {
                "retrieval_missing".to_string()
            },
            "no normalized reconstruction CSV".to_string(),
            "excluded from overlap validation".to_string(),
            "retry retrieval or reconstruction".to_string(),
        ));
    }
    for id in continuous_ids
        .iter()
        .filter(|id| !args.reconstructed_dir.join(format!("{id}.csv")).is_file())
    {
        exclusions.push((
            id.clone(),
            "retrieval_or_reconstruction_missing".to_string(),
            "no normalized reconstruction CSV".to_string(),
            "excluded from continuous-only contributions".to_string(),
            "retry retrieval or reconstruction".to_string(),
        ));
    }
    write_exclusions(&args.exclusions_csv, &exclusions)?;

    let reconciliation = PopulationReconciliation {
        schema_version: 1,
        overlap_requested,
        overlap_retrieved,
        overlap_reconstructed,
        overlap_valid,
        overlap_excluded,
        continuous_only_requested,
        continuous_only_retrieved: continuous_retrieved,
        continuous_only_reconstructed: continuous_reconstructed,
        continuous_only_valid: continuous_valid,
        continuous_only_excluded,
    };
    if let Some(parent) = args.output_reconciliation.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_reconciliation,
        serde_json::to_string_pretty(&reconciliation)? + "\n",
    )?;

    let artifact_files = [
        args.phase5_root.join("phase5_phase4_inputs.snapshot.json"),
        args.phase5_root.join("phase5_gaiaxpy_environment.json"),
        args.phase5_root.join("phase5_requests.manifest.json"),
        args.phase5_root.join("phase5_download_inventory.csv"),
        args.phase5_root.join("phase5_coefficients.manifest.json"),
        args.phase5_root.join("phase5_reconstruction.manifest.json"),
        args.phase5_root.join("phase5_overlap_validation.json"),
        args.phase5_root.join("phase5_overlap_validation.md"),
        args.phase5_root.join("phase5_overlap_predictions.csv"),
        args.phase5_root
            .join("phase5_overlap_stratified_metrics.csv"),
        args.phase5_root.join("phase5_continuous_only_336_650.csv"),
        args.exclusions_csv.clone(),
        args.output_reconciliation.clone(),
    ];
    let existing: Vec<PathBuf> = artifact_files.into_iter().filter(|p| p.is_file()).collect();
    write_sha256sum(&args.phase5_root, &existing)?;

    println!(
        "reconciliation: overlap valid={overlap_valid}/{overlap_requested} continuous valid={continuous_valid}/{continuous_only_requested}"
    );
    Ok(())
}
