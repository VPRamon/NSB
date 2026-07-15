//! Phase 5B operational mini-pilot: stream bulk ECSV with in-process Rust calibration and HEALPix accumulation.

use anyhow::{Context, Result};
use clap::Parser;
use md5::{Digest, Md5};
use nsb_data_tools::gaia_xp_continuous_bulk_index::gaia_source_healpix_index;
use nsb_data_tools::gaia_xp_continuous_calibrate::GaiaXpContinuousCalibrator;
use nsb_data_tools::gaia_xp_continuous_canonical::{
    stream_bulk_ecsv_gz, CanonicalXpContinuousRecord, CANONICAL_XP_CONTINUOUS_SCHEMA,
};
use nsb_data_tools::gaia_xp_continuous_healpix::{
    XpContinuousHealpixAccumulator, DEFAULT_PILOT_NSIDE,
};
use nsb_data_tools::gaia_xp_continuous_pilot_io::{
    atomic_write_json, checkpoint_state_checksum, verify_checkpoint_state_checksum,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(
    about = "Stream Gaia bulk XP continuous rows through canonical adapter, Rust calibrate, and HEALPix accumulation"
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
    /// Parallel reconstruction batch workers (wave size).
    #[arg(long, default_value_t = 1)]
    workers: usize,
    #[arg(long, default_value_t = 64)]
    nside: u32,
    #[arg(long, default_value_t = 0)]
    start_row: u64,
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    frozen_policy: Option<PathBuf>,
    #[arg(long)]
    gaiaxpy_environment: Option<PathBuf>,
    #[arg(long)]
    skip_normalized_output: bool,
    #[arg(long)]
    design_fixture: Option<PathBuf>,
    /// Omit per-source flux map from checkpoint JSON (production default with --skip-normalized-output).
    #[arg(long)]
    light_checkpoint: bool,
    /// Save checkpoint every N batch waves (1 = every wave).
    #[arg(long, default_value_t = 1)]
    checkpoint_interval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExclusionRecord {
    source_id: String,
    bulk_file: String,
    row_number: u64,
    reason_code: String,
    evidence: String,
    fallback: String,
    scientific_impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    processed_count: u64,
    valid_count: u64,
    excluded_count: u64,
    failed_count: u64,
    healpix: XpContinuousHealpixAccumulator,
    healpix_checksum: String,
    nside: u32,
    exclusions: Vec<ExclusionRecord>,
    adapter_version: u32,
    software_commit: String,
    gaiaxpy_version: Option<String>,
    gaiaxpy_environment_checksum: Option<String>,
    state_checksum: String,
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
    if let Ok(value) = std::env::var("STARLIGHT_SOFTWARE_COMMIT") {
        return value;
    }
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_gaiaxpy_environment(explicit: Option<&Path>) -> Option<(String, String)> {
    gaiaxpy_environment_paths(explicit)
        .into_iter()
        .find_map(|path| {
            if !path.is_file() {
                return None;
            }
            let text = fs::read_to_string(&path).ok()?;
            let json: serde_json::Value = serde_json::from_str(&text).ok()?;
            let checksum = json
                .get("checksum_sha256")
                .or_else(|| json.get("gaiaxpy_package_hash"))
                .and_then(|value| value.as_str())
                .map(str::to_string)?;
            let version = json
                .get("gaiaxpy_version")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            Some((checksum, version.unwrap_or_else(|| "unknown".to_string())))
        })
}

fn gaiaxpy_environment_paths(explicit: Option<&Path>) -> Vec<PathBuf> {
    explicit
        .map(|path| vec![path.to_path_buf()])
        .unwrap_or_default()
        .into_iter()
        .chain([
            PathBuf::from(
                "/home/valles/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/gaiaxpy_environment.json",
            ),
        ])
        .collect()
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
        if checkpoint.schema_version >= 3 && !checkpoint.state_checksum.is_empty() {
            verify_checkpoint_state_checksum(
                &checkpoint.bulk_checksum,
                checkpoint.row_index,
                checkpoint.rows_valid,
                checkpoint.rows_excluded,
                checkpoint.rows_failed,
                &checkpoint.healpix_checksum,
                &checkpoint.state_checksum,
            )?;
        }
        return Ok(checkpoint);
    }
    let gaiaxpy = read_gaiaxpy_environment(gaiaxpy_environment);
    Ok(MiniPilotCheckpoint {
        schema_version: 3,
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
        processed_count: 0,
        valid_count: 0,
        excluded_count: 0,
        failed_count: 0,
        healpix: XpContinuousHealpixAccumulator::new(nside)?,
        healpix_checksum: String::new(),
        nside,
        exclusions: Vec::new(),
        adapter_version: CANONICAL_XP_CONTINUOUS_SCHEMA,
        software_commit: software_commit(),
        gaiaxpy_version: gaiaxpy.as_ref().map(|(_, version)| version.clone()),
        gaiaxpy_environment_checksum: gaiaxpy.map(|(checksum, _)| checksum),
        state_checksum: String::new(),
        timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    })
}

fn sync_checkpoint_fields(checkpoint: &mut MiniPilotCheckpoint) {
    checkpoint.processed_count = checkpoint.processed_source_ids.len() as u64;
    checkpoint.valid_count = checkpoint.rows_valid;
    checkpoint.excluded_count = checkpoint.rows_excluded;
    checkpoint.failed_count = checkpoint.rows_failed;
    checkpoint.rows_read = checkpoint.row_index;
    checkpoint.healpix_checksum = checkpoint.healpix.checksum();
    checkpoint.state_checksum = checkpoint_state_checksum(
        &checkpoint.bulk_checksum,
        checkpoint.row_index,
        checkpoint.rows_valid,
        checkpoint.rows_excluded,
        checkpoint.rows_failed,
        &checkpoint.healpix_checksum,
    );
}

fn save_checkpoint(
    path: &Path,
    accumulator_path: &Path,
    checkpoint: &mut MiniPilotCheckpoint,
    light_checkpoint: bool,
) -> Result<()> {
    sync_checkpoint_fields(checkpoint);
    if light_checkpoint {
        let mut light = checkpoint.clone();
        light.flux_by_source_id.clear();
        atomic_write_json(path, &(serde_json::to_string_pretty(&light)? + "\n"))?;
    } else {
        atomic_write_json(path, &(serde_json::to_string_pretty(checkpoint)? + "\n"))?;
    }
    atomic_write_json(
        accumulator_path,
        &(serde_json::to_string_pretty(&checkpoint.healpix)? + "\n"),
    )?;
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
    let light_checkpoint = args.light_checkpoint || args.skip_normalized_output;
    let checkpoint_interval = args.checkpoint_interval.max(1);
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(
        args.design_fixture.as_deref(),
        args.gaiaxpy_environment.as_deref(),
    );
    let calibrator = Arc::new(
        GaiaXpContinuousCalibrator::from_design_fixture(&fixture).with_context(|| {
            format!(
                "load GaiaXPy design fixture for rust calibrate ({})",
                fixture.display()
            )
        })?,
    );

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
    let workers = args.workers.max(1);
    let mut batch: Vec<CanonicalXpContinuousRecord> = Vec::with_capacity(args.batch_size);
    let mut batch_index = checkpoint
        .processed_source_ids
        .len()
        .div_ceil(args.batch_size) as u64;
    let mut peak_rss = peak_rss_kib();
    let mut waves_since_checkpoint = 0_usize;

    let mut rows_in_window = 0_u64;
    let row_window = if args.row_limit == 0 {
        u64::MAX
    } else {
        args.row_limit as u64
    };

    while rows_in_window < row_window {
        let mut wave: Vec<(u64, Vec<CanonicalXpContinuousRecord>)> = Vec::with_capacity(workers);
        while wave.len() < workers && rows_in_window < row_window {
            while batch.len() < args.batch_size && rows_in_window < row_window {
                let Some(record) = stream.next_record()? else {
                    rows_in_window = row_window;
                    break;
                };
                checkpoint.row_index += 1;
                rows_in_window += 1;
                if processed.contains(&record.source_id) {
                    continue;
                }
                batch.push(record);
            }
            if batch.is_empty() {
                break;
            }
            wave.push((batch_index, std::mem::take(&mut batch)));
            batch_index += 1;
        }
        if wave.is_empty() {
            break;
        }

        let calibrator_ref = calibrator.clone();

        let mut wave_results = if workers == 1 {
            let (index, records) = wave.pop().expect("non-empty wave");
            vec![run_batch_rust_calibrate(
                calibrator_ref.as_ref(),
                index,
                records,
            )]
        } else {
            std::thread::scope(|scope| {
                wave.into_iter()
                    .map(|(index, records)| {
                        let calibrator_ref = calibrator_ref.clone();
                        scope.spawn(move || {
                            run_batch_rust_calibrate(calibrator_ref.as_ref(), index, records)
                        })
                    })
                    .map(|handle| handle.join().expect("batch worker panicked"))
                    .collect::<Vec<_>>()
            })
        };
        wave_results.sort_by_key(|result| result.batch_index);
        for result in wave_results {
            apply_batch_outcomes(&mut checkpoint, result, light_checkpoint)?;
        }
        checkpoint.timestamp_utc = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        waves_since_checkpoint += 1;
        if waves_since_checkpoint >= checkpoint_interval {
            save_checkpoint(&ckpt_path, &acc_path, &mut checkpoint, light_checkpoint)?;
            waves_since_checkpoint = 0;
        }
        processed.extend(checkpoint.processed_source_ids.iter().cloned());
        peak_rss = peak_rss.max(peak_rss_kib());
    }

    if waves_since_checkpoint > 0 {
        save_checkpoint(&ckpt_path, &acc_path, &mut checkpoint, light_checkpoint)?;
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
        "workers": workers,
        "reconstruct_backend": "rust",
        "light_checkpoint": light_checkpoint,
        "checkpoint_interval": checkpoint_interval,
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
            "bulk_file": bulk_file_name(&checkpoint),
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
    write_reconciliation_csv(&args.output_dir, &checkpoint)?;
    println!(
        "phase5b mini pilot: {} valid / {} excluded / {} total sources -> {}",
        checkpoint.rows_valid,
        checkpoint.rows_excluded,
        checkpoint.processed_source_ids.len(),
        args.output_dir.display()
    );
    Ok(())
}

struct BatchResult {
    batch_index: u64,
    records: Vec<CanonicalXpContinuousRecord>,
    outcomes: Result<HashMap<String, ReconstructionOutcome>>,
}

fn run_batch_rust_calibrate(
    calibrator: &GaiaXpContinuousCalibrator,
    batch_index: u64,
    records: Vec<CanonicalXpContinuousRecord>,
) -> BatchResult {
    let outcomes = (|| -> Result<HashMap<String, ReconstructionOutcome>> {
        let mut outcomes = HashMap::with_capacity(records.len());
        for record in &records {
            match calibrator.calibrate_record(record) {
                Ok(flux) => {
                    outcomes.insert(
                        record.source_id.clone(),
                        ReconstructionOutcome {
                            flux: flux.flux_336_650_ph_m2_s,
                            uncertainty: flux.statistical_uncertainty_336_650_ph_m2_s,
                        },
                    );
                }
                Err(error) => {
                    let source_id = record.source_id.clone();
                    eprintln!("rust calibrate failed for {source_id}: {error:#}");
                }
            }
        }
        Ok(outcomes)
    })();
    BatchResult {
        batch_index,
        records,
        outcomes,
    }
}

fn apply_batch_outcomes(
    checkpoint: &mut MiniPilotCheckpoint,
    result: BatchResult,
    light_checkpoint: bool,
) -> Result<()> {
    match result.outcomes {
        Ok(outcomes) => {
            for record in result.records {
                let healpix_index = gaia_source_healpix_index(record.source_id.parse::<u64>()?);
                match outcomes.get(&record.source_id) {
                    Some(outcome) if outcome.flux.is_finite() && outcome.flux > 0.0 => {
                        checkpoint.healpix.accumulate_valid(
                            healpix_index,
                            outcome.flux,
                            outcome.uncertainty,
                            0.0,
                        )?;
                        if !light_checkpoint {
                            checkpoint
                                .flux_by_source_id
                                .insert(record.source_id.clone(), outcome.flux);
                        }
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
                            "missing_reconstruction_outcome",
                            "batch did not return this source".to_string(),
                        )?;
                    }
                }
            }
        }
        Err(error) => {
            for record in result.records {
                register_failure(
                    checkpoint,
                    &record,
                    "reconstruction_batch_failed",
                    error.to_string(),
                )?;
            }
        }
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

fn bulk_file_name(checkpoint: &MiniPilotCheckpoint) -> String {
    Path::new(&checkpoint.bulk_file)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn write_reconciliation_csv(output_dir: &Path, checkpoint: &MiniPilotCheckpoint) -> Result<()> {
    let path = output_dir.join("phase5b_mini_pilot_reconciliation.csv");
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    for row in &checkpoint.exclusions {
        writer.serialize(row)?;
    }
    writer.flush()?;
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
        bulk_file: bulk_file_name(checkpoint),
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
        bulk_file: bulk_file_name(checkpoint),
        row_number: checkpoint.row_index,
        reason_code: reason_code.to_string(),
        evidence,
        fallback: "exclude_from_valid_accumulation".to_string(),
        scientific_impact: "source excluded from HEALPix valid accumulation".to_string(),
    });
    Ok(())
}
