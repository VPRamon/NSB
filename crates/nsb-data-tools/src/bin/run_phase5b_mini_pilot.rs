//! Phase 5B mini-pilot: stream bulk ECSV, emit GaiaXPy CSVs, checkpoint/resume.

use anyhow::{Context, Result};
use clap::Parser;
use md5::{Digest, Md5};
use nsb_data_tools::gaia_xp_continuous_canonical::{
    stream_bulk_ecsv_gz, write_gaiaxpy_datalink_csv_batch, CanonicalXpContinuousRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(
    about = "Stream Gaia bulk XP continuous rows into GaiaXPy CSV batches with checkpoint/resume"
)]
struct Args {
    #[arg(long)]
    bulk_gz: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 1000)]
    row_limit: usize,
    #[arg(long, default_value_t = 100)]
    batch_size: usize,
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    python: Option<PathBuf>,
    #[arg(long)]
    reconstruct_script: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MiniPilotCheckpoint {
    schema_version: u32,
    processed_source_ids: Vec<String>,
    flux_by_source_id: HashMap<String, f64>,
    rows_read: u64,
    rows_valid: u64,
    rows_invalid: u64,
    last_source_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MiniPilotMetrics {
    schema_version: u32,
    rows_read: u64,
    rows_valid: u64,
    rows_invalid: u64,
    sources_reconstructed: u64,
    sources_per_second: f64,
    megabytes_per_second: f64,
    peak_rss_kib: u64,
    checkpoint_interval: usize,
    flux_checksum: String,
    wall_elapsed_seconds: f64,
}

fn checkpoint_path(output_dir: &Path) -> PathBuf {
    output_dir.join("phase5b_mini_pilot_checkpoint.json")
}

fn load_checkpoint(path: &Path, resume: bool) -> Result<MiniPilotCheckpoint> {
    if resume && path.is_file() {
        let text = fs::read_to_string(path)?;
        return Ok(serde_json::from_str(&text)?);
    }
    Ok(MiniPilotCheckpoint {
        schema_version: 1,
        ..Default::default()
    })
}

fn save_checkpoint(path: &Path, checkpoint: &MiniPilotCheckpoint) -> Result<()> {
    let part = path.with_extension("json.part");
    fs::write(&part, serde_json::to_string_pretty(checkpoint)? + "\n")?;
    fs::rename(part, path)?;
    Ok(())
}

fn flux_checksum(flux_by_source_id: &HashMap<String, f64>) -> String {
    let mut ids = flux_by_source_id.keys().collect::<Vec<_>>();
    ids.sort();
    let mut hasher = Md5::new();
    for id in ids {
        hasher.update(id.as_bytes());
        hasher.update(flux_by_source_id[id].to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn run_python_batch(
    python: &Path,
    script: &Path,
    coefficient_csv: &Path,
    output_dir: &Path,
) -> Result<HashMap<String, f64>> {
    fs::create_dir_all(output_dir)?;
    let manifest = output_dir.join("batch_manifest.json");
    let status = Command::new(python)
        .arg(script)
        .arg("--coefficient-file")
        .arg(coefficient_csv)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--manifest")
        .arg(&manifest)
        .status()
        .with_context(|| format!("invoke {}", script.display()))?;
    if !status.success() {
        anyhow::bail!(
            "python reconstruction failed for {}",
            coefficient_csv.display()
        );
    }
    let entries: serde_json::Value = serde_json::from_str(&fs::read_to_string(manifest)?)?;
    let mut flux = HashMap::new();
    for entry in entries["entries"].as_array().into_iter().flatten() {
        let source_id = entry["source_id"].as_str().unwrap_or_default().to_string();
        if source_id.is_empty() {
            continue;
        }
        if let Some(value) = entry.get("flux_336_650_ph_m2_s").and_then(|v| v.as_f64()) {
            flux.insert(source_id, value);
        }
    }
    Ok(flux)
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    let coefficients_root = args.output_dir.join("coefficients");
    fs::create_dir_all(&coefficients_root)?;

    let python = args
        .python
        .unwrap_or_else(|| PathBuf::from("tools/starlight-xp-continuous/.venv/bin/python"));
    let reconstruct_script = args.reconstruct_script.unwrap_or_else(|| {
        PathBuf::from("tools/starlight-xp-continuous/reconstruct_and_integrate.py")
    });

    let ckpt_path = checkpoint_path(&args.output_dir);
    let mut checkpoint = load_checkpoint(&ckpt_path, args.resume)?;
    let mut processed: HashSet<String> = checkpoint.processed_source_ids.iter().cloned().collect();

    let started = Instant::now();
    let initial_processed = checkpoint.processed_source_ids.len();
    let mut stream = stream_bulk_ecsv_gz(&args.bulk_gz)?;
    let mut batch: Vec<CanonicalXpContinuousRecord> = Vec::with_capacity(args.batch_size);
    let mut batch_index = checkpoint
        .processed_source_ids
        .len()
        .div_ceil(args.batch_size) as u64;

    while checkpoint.processed_source_ids.len() + batch.len() < args.row_limit {
        let Some(record) = stream.next_record()? else {
            break;
        };
        checkpoint.rows_read += 1;
        if processed.contains(&record.source_id) {
            continue;
        }
        batch.push(record);
        let total_in_flight = checkpoint.processed_source_ids.len() + batch.len();
        if batch.len() >= args.batch_size || total_in_flight >= args.row_limit {
            process_batch(
                &args.output_dir,
                &coefficients_root,
                &python,
                &reconstruct_script,
                &mut batch,
                batch_index,
                &mut checkpoint,
            )?;
            batch_index += 1;
            processed.extend(checkpoint.processed_source_ids.iter().cloned());
            save_checkpoint(&ckpt_path, &checkpoint)?;
        }
    }

    if !batch.is_empty() {
        process_batch(
            &args.output_dir,
            &coefficients_root,
            &python,
            &reconstruct_script,
            &mut batch,
            batch_index,
            &mut checkpoint,
        )?;
        save_checkpoint(&ckpt_path, &checkpoint)?;
    }

    let elapsed = started.elapsed().as_secs_f64();
    let file_size = fs::metadata(&args.bulk_gz)?.len();
    let metrics = MiniPilotMetrics {
        schema_version: 1,
        rows_read: checkpoint.rows_read,
        rows_valid: checkpoint.rows_valid,
        rows_invalid: checkpoint.rows_invalid,
        sources_reconstructed: checkpoint.processed_source_ids.len() as u64,
        sources_per_second: (checkpoint
            .processed_source_ids
            .len()
            .saturating_sub(initial_processed)) as f64
            / elapsed.max(1e-6),
        megabytes_per_second: (file_size as f64 / (1024.0 * 1024.0)) / elapsed.max(1e-6),
        peak_rss_kib: 0,
        checkpoint_interval: args.batch_size,
        flux_checksum: flux_checksum(&checkpoint.flux_by_source_id),
        wall_elapsed_seconds: elapsed,
    };
    fs::write(
        args.output_dir.join("phase5b_mini_pilot_metrics.json"),
        serde_json::to_string_pretty(&metrics)? + "\n",
    )?;
    fs::write(
        args.output_dir.join("phase5b_mini_pilot_manifest.json"),
        serde_json::to_string_pretty(&checkpoint)? + "\n",
    )?;
    fs::write(
        args.output_dir
            .join("phase5b_mini_pilot_reconciliation.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "rows_read": checkpoint.rows_read,
            "rows_valid": checkpoint.rows_valid,
            "rows_invalid": checkpoint.rows_invalid,
            "processed_unique": checkpoint.processed_source_ids.len(),
            "reconciliation_ok": checkpoint.rows_valid + checkpoint.rows_invalid == checkpoint.rows_read,
        }))? + "\n",
    )?;
    println!(
        "phase5b mini pilot: {} sources in {:.1}s -> {}",
        metrics.sources_reconstructed,
        elapsed,
        args.output_dir.display()
    );
    Ok(())
}

fn process_batch(
    output_dir: &Path,
    coefficients_root: &Path,
    python: &Path,
    reconstruct_script: &Path,
    batch: &mut Vec<CanonicalXpContinuousRecord>,
    batch_index: u64,
    checkpoint: &mut MiniPilotCheckpoint,
) -> Result<()> {
    let batch_csv = coefficients_root.join(format!("batch_{batch_index:05}.csv"));
    write_gaiaxpy_datalink_csv_batch(&batch_csv, batch)?;
    checkpoint.rows_valid += batch.len() as u64;
    if let Some(last) = batch.last() {
        checkpoint.last_source_id = Some(last.source_id.clone());
    }
    let recon_dir = output_dir.join(format!("normalized_batch_{batch_index:05}"));
    fs::create_dir_all(&recon_dir)?;
    let flux = run_python_batch(python, reconstruct_script, &batch_csv, &recon_dir)?;
    for record in batch.drain(..) {
        if let Some(value) = flux.get(&record.source_id) {
            checkpoint
                .flux_by_source_id
                .insert(record.source_id.clone(), *value);
            checkpoint.processed_source_ids.push(record.source_id);
        }
    }
    Ok(())
}
