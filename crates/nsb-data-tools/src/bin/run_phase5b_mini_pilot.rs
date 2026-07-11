//! Phase 5B operational mini-pilot: stream bulk ECSV with HEALPix accumulation.

use anyhow::{Context, Result};
use clap::Parser;
use md5::{Digest, Md5};
use nsb_data_tools::gaia_xp_continuous_bulk_index::gaia_source_healpix_index;
use nsb_data_tools::gaia_xp_continuous_canonical::{
    stream_bulk_ecsv_gz, write_gaiaxpy_datalink_csv_batch, CanonicalXpContinuousRecord,
    CANONICAL_XP_CONTINUOUS_SCHEMA,
};
use nsb_data_tools::gaia_xp_continuous_healpix::{
    XpContinuousHealpixAccumulator, DEFAULT_PILOT_NSIDE,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(
    about = "Stream Gaia bulk XP continuous rows through canonical adapter, GaiaXPy, and HEALPix accumulation"
)]
struct Args {
    #[arg(long)]
    bulk_gz: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 10_000)]
    row_limit: usize,
    #[arg(long, default_value_t = 500)]
    batch_size: usize,
    #[arg(long, default_value_t = 64)]
    nside: u32,
    #[arg(long, default_value_t = 0)]
    start_row: u64,
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    python: Option<PathBuf>,
    #[arg(long)]
    reconstruct_script: Option<PathBuf>,
    #[arg(long)]
    frozen_policy: Option<PathBuf>,
    #[arg(long)]
    gaiaxpy_environment: Option<PathBuf>,
    #[arg(long)]
    skip_normalized_output: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExclusionRecord {
    source_id: String,
    row_number: u64,
    reason_code: String,
    evidence: String,
    fallback: String,
    scientific_impact: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MiniPilotCheckpoint {
    schema_version: u32,
    bulk_file: String,
    bulk_checksum: String,
    row_index: u64,
    last_source_id: Option<String>,
    processed_source_ids: Vec<String>,
    flux_by_source_id: HashMap<String, f64>,
    rows_read: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix: XpContinuousHealpixAccumulator,
    exclusions: Vec<ExclusionRecord>,
    adapter_version: u32,
    software_commit: String,
    gaiaxpy_environment_checksum: Option<String>,
    timestamp_utc: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReconstructionOutcome {
    flux: f64,
    uncertainty: f64,
}

fn checkpoint_path(output_dir: &Path) -> PathBuf {
    output_dir.join("phase5b_mini_pilot_checkpoint.json")
}

fn accumulator_path(output_dir: &Path) -> PathBuf {
    output_dir.join("phase5b_healpix_accumulator.json")
}

fn peak_rss_kib() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0)
}

fn file_md5(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Md5::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn software_commit() -> String {
    std::env::var("STARLIGHT_SOFTWARE_COMMIT").unwrap_or_else(|_| "unknown".to_string())
}

#[allow(clippy::too_many_arguments)]
fn load_checkpoint(
    path: &Path,
    bulk_gz: &Path,
    nside: u32,
    resume: bool,
    gaiaxpy_environment: Option<&Path>,
) -> Result<MiniPilotCheckpoint> {
    if resume && path.is_file() {
        let checkpoint: MiniPilotCheckpoint = serde_json::from_str(&fs::read_to_string(path)?)?;
        if checkpoint.bulk_file != bulk_gz.display().to_string() {
            anyhow::bail!("checkpoint bulk file mismatch");
        }
        return Ok(checkpoint);
    }
    Ok(MiniPilotCheckpoint {
        schema_version: 2,
        bulk_file: bulk_gz.display().to_string(),
        bulk_checksum: file_md5(bulk_gz)?,
        row_index: 0,
        last_source_id: None,
        processed_source_ids: Vec::new(),
        flux_by_source_id: HashMap::new(),
        rows_read: 0,
        rows_valid: 0,
        rows_excluded: 0,
        rows_failed: 0,
        healpix: XpContinuousHealpixAccumulator::new(nside)?,
        exclusions: Vec::new(),
        adapter_version: CANONICAL_XP_CONTINUOUS_SCHEMA,
        software_commit: software_commit(),
        gaiaxpy_environment_checksum: read_gaiaxpy_checksum(gaiaxpy_environment),
        timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    })
}

fn read_gaiaxpy_checksum(explicit: Option<&Path>) -> Option<String> {
    let candidates: Vec<PathBuf> = explicit
        .map(|path| vec![path.to_path_buf()])
        .unwrap_or_default()
        .into_iter()
        .chain([
            PathBuf::from("tools/starlight-xp-continuous/gaiaxpy_environment.json"),
            PathBuf::from(
                "/home/valles/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/gaiaxpy_environment.json",
            ),
        ])
        .collect();
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        return json
            .get("checksum_sha256")
            .or_else(|| json.get("gaiaxpy_package_hash"))
            .and_then(|value| value.as_str())
            .map(str::to_string);
    }
    None
}

fn save_checkpoint(
    path: &Path,
    accumulator_path: &Path,
    checkpoint: &MiniPilotCheckpoint,
) -> Result<()> {
    let ckpt_part = path.with_extension("json.part");
    fs::write(&ckpt_part, serde_json::to_string_pretty(checkpoint)? + "\n")?;
    fs::rename(&ckpt_part, path)?;
    let acc_part = accumulator_path.with_extension("json.part");
    fs::write(
        &acc_part,
        serde_json::to_string_pretty(&checkpoint.healpix)? + "\n",
    )?;
    fs::rename(&acc_part, accumulator_path)?;
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
    skip_output: bool,
) -> Result<HashMap<String, ReconstructionOutcome>> {
    if !skip_output {
        fs::create_dir_all(output_dir)?;
    }
    let manifest = output_dir.join("batch_manifest.json");
    let mut command = Command::new(python);
    command
        .arg(script)
        .arg("--coefficient-file")
        .arg(coefficient_csv)
        .arg("--manifest")
        .arg(&manifest)
        .arg("--output-dir")
        .arg(output_dir);
    let status = command
        .status()
        .with_context(|| format!("invoke {}", script.display()))?;
    if !status.success() {
        anyhow::bail!(
            "python reconstruction failed for {}",
            coefficient_csv.display()
        );
    }
    let entries: serde_json::Value = serde_json::from_str(&fs::read_to_string(&manifest)?)?;
    let mut outcomes = HashMap::new();
    for entry in entries["entries"].as_array().into_iter().flatten() {
        let source_id = entry["source_id"].as_str().unwrap_or_default().to_string();
        if source_id.is_empty() {
            continue;
        }
        if let (Some(flux), Some(uncertainty)) = (
            entry.get("flux_336_650_ph_m2_s").and_then(|v| v.as_f64()),
            entry
                .get("statistical_uncertainty_336_650_ph_m2_s")
                .and_then(|v| v.as_f64()),
        ) {
            outcomes.insert(source_id, ReconstructionOutcome { flux, uncertainty });
        }
    }
    Ok(outcomes)
}

fn verify_frozen_policy(path: &Path) -> Result<()> {
    let _policy: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.nside != DEFAULT_PILOT_NSIDE {
        eprintln!("warning: pilot default nside is {DEFAULT_PILOT_NSIDE}");
    }
    if let Some(policy) = &args.frozen_policy {
        verify_frozen_policy(policy)?;
    }
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
    let acc_path = accumulator_path(&args.output_dir);
    let mut checkpoint = load_checkpoint(
        &ckpt_path,
        &args.bulk_gz,
        args.nside,
        args.resume,
        args.gaiaxpy_environment.as_deref(),
    )?;
    let mut processed: HashSet<String> = checkpoint.processed_source_ids.iter().cloned().collect();

    let started = Instant::now();
    let initial_processed = checkpoint.processed_source_ids.len();
    let mut stream = stream_bulk_ecsv_gz(&args.bulk_gz)?;
    if args.start_row == 0 && args.resume && checkpoint.row_index > 0 {
        skip_stream_rows(&mut stream, checkpoint.row_index)?;
    } else if args.start_row > 0 {
        skip_stream_rows(&mut stream, args.start_row)?;
    }
    let mut batch: Vec<CanonicalXpContinuousRecord> = Vec::with_capacity(args.batch_size);
    let mut batch_index = checkpoint
        .processed_source_ids
        .len()
        .div_ceil(args.batch_size) as u64;
    let mut peak_rss = peak_rss_kib();

    let mut rows_in_window = 0_u64;

    while rows_in_window < args.row_limit as u64 {
        let Some(record) = stream.next_record()? else {
            break;
        };
        checkpoint.row_index += 1;
        rows_in_window += 1;
        if processed.contains(&record.source_id) {
            continue;
        }
        batch.push(record);
        let pending = batch.len();
        if pending >= args.batch_size || rows_in_window >= args.row_limit as u64 {
            process_batch(
                &args.output_dir,
                &coefficients_root,
                &python,
                &reconstruct_script,
                &mut batch,
                batch_index,
                &mut checkpoint,
                args.skip_normalized_output,
            )?;
            batch_index += 1;
            processed.extend(checkpoint.processed_source_ids.iter().cloned());
            checkpoint.timestamp_utc = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
            save_checkpoint(&ckpt_path, &acc_path, &checkpoint)?;
            peak_rss = peak_rss.max(peak_rss_kib());
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
            args.skip_normalized_output,
        )?;
        save_checkpoint(&ckpt_path, &acc_path, &checkpoint)?;
        peak_rss = peak_rss.max(peak_rss_kib());
    }

    let elapsed = started.elapsed().as_secs_f64();
    let new_sources = checkpoint
        .processed_source_ids
        .len()
        .saturating_sub(initial_processed);
    let metrics = serde_json::json!({
        "schema_version": 2,
        "bulk_file": args.bulk_gz.display().to_string(),
        "bulk_checksum": checkpoint.bulk_checksum,
        "rows_scanned": checkpoint.row_index,
        "rows_processed": checkpoint.rows_valid + checkpoint.rows_excluded + checkpoint.rows_failed,
        "rows_valid": checkpoint.rows_valid,
        "rows_excluded": checkpoint.rows_excluded,
        "rows_failed": checkpoint.rows_failed,
        "sources_reconstructed": checkpoint.processed_source_ids.len(),
        "sources_per_second": new_sources as f64 / elapsed.max(1e-6),
        "megabytes_per_second": fs::metadata(&args.bulk_gz).map(|m| m.len()).unwrap_or(0) as f64
            / (1024.0 * 1024.0)
            / elapsed.max(1e-6),
        "peak_rss_kib": peak_rss,
        "checkpoint_interval": args.batch_size,
        "chunk_size": args.batch_size,
        "flux_checksum": flux_checksum(&checkpoint.flux_by_source_id),
        "healpix_checksum": checkpoint.healpix.checksum(),
        "integrated_flux_checksum": flux_checksum(&checkpoint.flux_by_source_id),
        "gaiaxpy_environment_checksum": checkpoint.gaiaxpy_environment_checksum,
        "software_commit": checkpoint.software_commit,
        "wall_elapsed_seconds": elapsed,
        "nside": args.nside,
    });
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
            "rows_scanned": checkpoint.row_index,
            "rows_valid": checkpoint.rows_valid,
            "rows_excluded": checkpoint.rows_excluded,
            "rows_failed": checkpoint.rows_failed,
            "processed_unique": checkpoint.processed_source_ids.len(),
            "reconciliation_ok": checkpoint.rows_valid + checkpoint.rows_excluded + checkpoint.rows_failed
                == checkpoint.processed_source_ids.len() as u64,
            "exclusions": checkpoint.exclusions,
        }))? + "\n",
    )?;
    println!(
        "phase5b mini pilot: {} valid / {} excluded / {} total sources -> {}",
        checkpoint.rows_valid,
        checkpoint.rows_excluded,
        checkpoint.processed_source_ids.len(),
        args.output_dir.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_batch(
    output_dir: &Path,
    coefficients_root: &Path,
    python: &Path,
    reconstruct_script: &Path,
    batch: &mut Vec<CanonicalXpContinuousRecord>,
    batch_index: u64,
    checkpoint: &mut MiniPilotCheckpoint,
    skip_normalized_output: bool,
) -> Result<()> {
    let batch_csv = coefficients_root.join(format!("batch_{batch_index:05}.csv"));
    write_gaiaxpy_datalink_csv_batch(&batch_csv, batch)?;
    let recon_dir = output_dir.join(format!("normalized_batch_{batch_index:05}"));
    let outcomes = match run_python_batch(
        python,
        reconstruct_script,
        &batch_csv,
        &recon_dir,
        skip_normalized_output,
    ) {
        Ok(outcomes) => outcomes,
        Err(error) => {
            for record in batch.drain(..) {
                register_failure(
                    checkpoint,
                    &record,
                    "gaiaxpy_batch_failed",
                    error.to_string(),
                )?;
            }
            return Ok(());
        }
    };
    for record in batch.drain(..) {
        let healpix_index = gaia_source_healpix_index(record.source_id.parse::<u64>()?);
        match outcomes.get(&record.source_id) {
            Some(outcome) if outcome.flux.is_finite() && outcome.flux > 0.0 => {
                checkpoint.healpix.accumulate_valid(
                    healpix_index,
                    outcome.flux,
                    outcome.uncertainty,
                    0.0,
                )?;
                checkpoint
                    .flux_by_source_id
                    .insert(record.source_id.clone(), outcome.flux);
                checkpoint
                    .processed_source_ids
                    .push(record.source_id.clone());
                checkpoint.rows_valid += 1;
                checkpoint.last_source_id = Some(record.source_id);
            }
            Some(outcome) => {
                register_exclusion(
                    checkpoint,
                    &record,
                    "non_positive_flux",
                    format!("flux={}", outcome.flux),
                )?;
            }
            None => {
                register_exclusion(
                    checkpoint,
                    &record,
                    "missing_gaiaxpy_outcome",
                    "GaiaXPy batch did not return this source".to_string(),
                )?;
            }
        }
    }
    if skip_normalized_output {
        let _ = fs::remove_dir_all(recon_dir);
    }
    Ok(())
}

fn skip_stream_rows(
    stream: &mut nsb_data_tools::gaia_xp_continuous_canonical::BulkEcsvStream,
    rows: u64,
) -> Result<()> {
    for _ in 0..rows {
        if stream.next_record()?.is_none() {
            break;
        }
    }
    Ok(())
}

fn register_failure(
    checkpoint: &mut MiniPilotCheckpoint,
    record: &CanonicalXpContinuousRecord,
    reason_code: &str,
    evidence: String,
) -> Result<()> {
    checkpoint.rows_failed += 1;
    if !checkpoint
        .processed_source_ids
        .iter()
        .any(|seen| seen == &record.source_id)
    {
        checkpoint
            .processed_source_ids
            .push(record.source_id.clone());
    }
    checkpoint.exclusions.push(ExclusionRecord {
        source_id: record.source_id.clone(),
        row_number: checkpoint.row_index,
        reason_code: reason_code.to_string(),
        evidence,
        fallback: "exclude_from_valid_accumulation".to_string(),
        scientific_impact:
            "source failed reconstruction and is excluded from HEALPix valid accumulation"
                .to_string(),
    });
    Ok(())
}

fn register_exclusion(
    checkpoint: &mut MiniPilotCheckpoint,
    record: &CanonicalXpContinuousRecord,
    reason_code: &str,
    evidence: String,
) -> Result<()> {
    let source_id = record.source_id.parse::<u64>()?;
    checkpoint
        .healpix
        .record_exclusion(gaia_source_healpix_index(source_id), reason_code)?;
    checkpoint.rows_excluded += 1;
    if !checkpoint
        .processed_source_ids
        .iter()
        .any(|seen| seen == &record.source_id)
    {
        checkpoint
            .processed_source_ids
            .push(record.source_id.clone());
    }
    checkpoint.exclusions.push(ExclusionRecord {
        source_id: record.source_id.clone(),
        row_number: checkpoint.row_index,
        reason_code: reason_code.to_string(),
        evidence,
        fallback: "exclude_from_valid_accumulation".to_string(),
        scientific_impact: "source excluded from HEALPix valid accumulation".to_string(),
    });
    Ok(())
}
