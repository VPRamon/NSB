//! Production orchestrator for Gaia DR3 XP continuous bulk (issue #47 PR A).
//!
//! Runs USB/internal preflight, official inventory audit, representative rehearsal,
//! kill/resume validation, and storage-plan generation. Full bulk is blocked until
//! all preflight gates pass.

use crate::gaia_storage_preflight::{
    audit_official_inventory, directory_size_bytes, run_storage_preflight, write_storage_plan_json,
    write_storage_plan_markdown, StoragePreflightConfig,
};
use crate::gaia_usb_cache::{
    append_session_log, read_or_create_cache_root_marker, verify_usb_identity, UsbCacheLayout,
    OFFICIAL_CHECKSUM_MANIFEST,
};
use crate::gaia_usb_cache_rotator::{
    bulk_filename, filenames_checksum_verified, filenames_for_production, UsbCacheRotator,
    UsbCacheRotatorConfig,
};
use crate::gaia_xp_continuous_bulk_healpix_merge::{
    bulk_accumulator_path, merge_all_partition_checkpoints, BulkHealpixMergeReport,
};
use crate::gaia_xp_continuous_bulk_reconciliation::{
    backfill_reconciliation_from_verified_cache, build_partition_from_processing_output,
    sync_ledger_from_merge_state, write_root_manifest, PartitionReconciliationManifest,
};
use crate::gaia_xp_continuous_pilot_io::atomic_write_json;
use crate::gaia_xp_continuous_tool_launch::run_download_bulk_command;
use crate::gaia_xp_continuous_tool_launch::run_mini_pilot_command;
use crate::starlight::xp_continuous::partition_claim::claim_partitions;
use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

#[derive(Debug, Parser)]
#[command(about = "Gaia DR3 XP continuous bulk production pipeline (preflight + rehearsal)")]
struct Args {
    #[arg(long)]
    work_dir: Option<PathBuf>,
    #[arg(long)]
    checkpoint_dir: Option<PathBuf>,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    manifest_dir: Option<PathBuf>,
    #[arg(long)]
    reconciliation_dir: Option<PathBuf>,
    #[arg(long)]
    input_cache_dir: Option<PathBuf>,
    #[arg(long)]
    usb_mountpoint: Option<PathBuf>,
    #[arg(long)]
    usb_cache_root: Option<PathBuf>,
    #[arg(long, default_value_t = 20 * 1024 * 1024 * 1024)]
    max_cache_bytes: u64,
    #[arg(long)]
    frozen_policy: Option<PathBuf>,
    #[arg(long)]
    official_checksum_manifest: Option<PathBuf>,
    #[arg(long)]
    rehearsal_bulk_gz: Option<PathBuf>,
    #[arg(long, default_value_t = 2_000)]
    rehearsal_row_limit: usize,
    #[arg(long, default_value_t = 500)]
    rehearsal_batch_size: usize,
    /// Production streaming row limit (0 = entire bulk partition file).
    #[arg(long, default_value_t = 0)]
    production_row_limit: usize,
    #[arg(long, default_value_t = 1000)]
    production_batch_size: usize,
    /// Parallel reconstruction workers per partition (0 = auto: cores - headroom).
    #[arg(long, default_value_t = 0)]
    production_workers: usize,
    /// Cores reserved for OS/IO when auto-sizing workers (0 = 4 with USB, else 2).
    #[arg(long, default_value_t = 0)]
    production_worker_headroom: usize,
    /// Concurrent partitions per invocation (1 = serial; claim-locked for multi-process safety).
    #[arg(long, default_value_t = 1)]
    production_parallel_partitions: usize,
    /// Save mini-pilot checkpoint every N batch waves (0 = auto: max(4, workers/2)).
    #[arg(long, default_value_t = 0)]
    production_checkpoint_interval: usize,
    #[arg(long)]
    gaiaxpy_environment: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    preflight_only: bool,
    #[arg(long, default_value_t = false)]
    resume: bool,
    #[arg(long, default_value_t = false)]
    cleanup_verified_inputs: bool,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    #[arg(long, default_value_t = false)]
    init_usb_marker: bool,
    #[arg(long, default_value_t = false)]
    skip_rehearsal: bool,
    #[arg(long, default_value_t = false)]
    skip_resume_test: bool,
    #[arg(long, default_value = "xp-continuous")]
    cache_subdir: String,
    /// Process up to N checksum-verified USB cache files through mini-pilot and mark releasable.
    #[arg(long)]
    process_verified_cache_limit: Option<usize>,
    /// Per-file production loop: download (if needed) → process → HEALPix checkpoint → releasable.
    #[arg(long)]
    file_limit: Option<usize>,
    /// Limit live cleanup deletes to N releasable files (dry-run still lists all candidates).
    #[arg(long)]
    cleanup_limit: Option<usize>,
    /// Merge discovered partition HEALPix checkpoints into bulk_healpix_accumulator.json.
    #[arg(long, default_value_t = false)]
    merge_partition_checkpoints: bool,
    /// Backfill partition reconciliation manifests from verified_cache_process outputs.
    #[arg(long, default_value_t = false)]
    backfill_reconciliation: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionManifest {
    schema_version: u32,
    session_id: String,
    software_commit: String,
    work_dir: String,
    checkpoint_dir: String,
    output_dir: String,
    manifest_dir: String,
    input_cache_dir: String,
    usb_mountpoint: Option<String>,
    usb_cache_root: Option<String>,
    max_cache_bytes: u64,
    preflight_only: bool,
    resume: bool,
    cleanup_verified_inputs: bool,
    dry_run: bool,
    gates: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RehearsalReport {
    bulk_gz: String,
    row_limit: usize,
    batch_size: usize,
    wall_elapsed_seconds: f64,
    sources_per_second: f64,
    peak_rss_kib: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix_checksum: String,
    flux_checksum: String,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProcessedCacheFileReport {
    filename: String,
    bulk_gz: String,
    output_dir: String,
    rows_valid: u64,
    rows_failed: u64,
    final_state: String,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProductionReconciliationReport {
    status: String,
    partition_manifest: String,
    ledger_path: String,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix_checksum: String,
    reconciliation_ok: bool,
    population_status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProductionStreamMetrics {
    rows_scanned: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    sources_per_second: f64,
    wall_elapsed_seconds: f64,
    peak_rss_kib: u64,
    healpix_checksum: String,
    flux_checksum: String,
    row_limit: usize,
    batch_size: usize,
    full_file: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProductionFileReport {
    filename: String,
    initial_state: String,
    downloaded: bool,
    processed: bool,
    stream_metrics: Option<ProductionStreamMetrics>,
    healpix_checkpoint: Option<String>,
    reconciliation: Option<ProductionReconciliationReport>,
    final_state: String,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProductionLoopReport {
    file_limit: usize,
    files: Vec<ProductionFileReport>,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PipelineReport {
    schema_version: u32,
    session_manifest: SessionManifest,
    storage_plan_passed: bool,
    rehearsal: Option<RehearsalReport>,
    resume_validation: Option<serde_json::Value>,
    deterministic_merge: Option<serde_json::Value>,
    processed_cache_files: Vec<ProcessedCacheFileReport>,
    production_loop: Option<ProductionLoopReport>,
    bulk_healpix_merge: Option<BulkHealpixMergeReport>,
    reconciliation_backfill: Option<serde_json::Value>,
    cleanup_simulation: Option<serde_json::Value>,
    all_preflight_gates_passed: bool,
    ready_for_full_bulk: bool,
    blockers: Vec<String>,
}

fn apply_env_defaults(args: &mut Args) {
    if args.work_dir.is_none() {
        args.work_dir = Some(env_path("STARLIGHT_WORK"));
    }
    if args.checkpoint_dir.is_none() {
        args.checkpoint_dir = Some(env_path("STARLIGHT_CHECKPOINTS"));
    }
    if args.output_dir.is_none() {
        args.output_dir = Some(env_path("STARLIGHT_OUTPUTS"));
    }
    if args.manifest_dir.is_none() {
        args.manifest_dir = Some(env_path("GAIA_USB_MANIFESTS"));
    }
    if args.reconciliation_dir.is_none() {
        args.reconciliation_dir = Some(env_path("GAIA_USB_RECONCILIATION"));
    }
    if args.input_cache_dir.is_none() {
        args.input_cache_dir = Some(env_path("GAIA_USB_CACHE"));
    }
    if args.usb_mountpoint.is_none() {
        args.usb_mountpoint = std::env::var_os("GAIA_USB_MOUNT").map(PathBuf::from);
    }
    if args.usb_cache_root.is_none() {
        args.usb_cache_root = std::env::var_os("GAIA_USB_ROOT").map(PathBuf::from);
    }
    if args.frozen_policy.is_none() {
        args.frozen_policy = std::env::var_os("STARLIGHT_FROZEN_POLICY").map(PathBuf::from);
    }
    if args.gaiaxpy_environment.is_none() {
        args.gaiaxpy_environment = std::env::var_os("STARLIGHT_GAIAXPY_ENV").map(PathBuf::from);
    }
}

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Standalone binary entrypoint.
pub fn run_standalone() -> Result<()> {
    let mut args = Args::parse();
    apply_env_defaults(&mut args);
    let work_dir = args.work_dir.clone().expect("work_dir");
    let checkpoint_dir = args.checkpoint_dir.clone().expect("checkpoint_dir");
    let output_dir = args.output_dir.clone().expect("output_dir");
    let manifest_dir = args.manifest_dir.clone().expect("manifest_dir");
    let input_cache_dir = args.input_cache_dir.clone().expect("input_cache_dir");
    fs::create_dir_all(&work_dir)?;
    fs::create_dir_all(&checkpoint_dir)?;
    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(&manifest_dir)?;
    fs::create_dir_all(&input_cache_dir)?;

    let official_checksum_manifest = args.official_checksum_manifest.clone().unwrap_or_else(|| {
        if let Some(root) = &args.usb_cache_root {
            root.join(&args.cache_subdir)
                .join(OFFICIAL_CHECKSUM_MANIFEST)
        } else {
            input_cache_dir.join(OFFICIAL_CHECKSUM_MANIFEST)
        }
    });

    let mut rotator = load_usb_rotator(&args, &official_checksum_manifest)?;

    let measured_internal = directory_size_bytes(
        &work_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| work_dir.clone()),
    )
    .ok();

    let preflight_config = StoragePreflightConfig {
        work_dir: work_dir.clone(),
        checkpoint_dir: checkpoint_dir.clone(),
        output_dir: output_dir.clone(),
        manifest_dir: manifest_dir.clone(),
        input_cache_dir: input_cache_dir.clone(),
        usb_mountpoint: args.usb_mountpoint.clone(),
        usb_cache_root: args.usb_cache_root.clone(),
        max_cache_bytes: args.max_cache_bytes,
        frozen_policy: args.frozen_policy.clone(),
        official_checksum_manifest: official_checksum_manifest.clone(),
        measured_internal_existing_bytes: measured_internal,
    };

    let storage_plan = run_storage_preflight(&preflight_config)?;
    let storage_plan_json = manifest_dir.join("storage_plan.json");
    let storage_plan_md = manifest_dir.join("storage_plan.md");
    write_storage_plan_json(&storage_plan_json, &storage_plan)?;
    write_storage_plan_markdown(&storage_plan_md, &storage_plan)?;
    println!(
        "storage plan: {} ({})",
        storage_plan.conclusion,
        storage_plan_json.display()
    );

    let inventory = audit_official_inventory(&official_checksum_manifest)?;
    let session = SessionManifest {
        schema_version: 1,
        session_id: format!("xp-continuous-bulk-{}", storage_plan.timestamp_utc),
        software_commit: software_commit()?,
        work_dir: work_dir.display().to_string(),
        checkpoint_dir: checkpoint_dir.display().to_string(),
        output_dir: output_dir.display().to_string(),
        manifest_dir: manifest_dir.display().to_string(),
        input_cache_dir: input_cache_dir.display().to_string(),
        usb_mountpoint: args
            .usb_mountpoint
            .as_ref()
            .map(|p| p.display().to_string()),
        usb_cache_root: args
            .usb_cache_root
            .as_ref()
            .map(|p| p.display().to_string()),
        max_cache_bytes: args.max_cache_bytes,
        preflight_only: args.preflight_only,
        resume: args.resume,
        cleanup_verified_inputs: args.cleanup_verified_inputs,
        dry_run: args.dry_run,
        gates: storage_plan
            .preflight_gates
            .iter()
            .map(|gate| format!("{}:{}", gate.name, gate.passed))
            .collect(),
    };
    let session_path = manifest_dir.join("session_manifest.json");
    atomic_write_json(
        &session_path,
        &(serde_json::to_string_pretty(&session)? + "\n"),
    )?;

    let mut blockers = storage_plan
        .preflight_gates
        .iter()
        .filter(|gate| !gate.passed)
        .map(|gate| format!("{}: {}", gate.name, gate.detail))
        .collect::<Vec<_>>();

    let mut rehearsal = None;
    let mut resume_validation: Option<serde_json::Value> = None;
    let mut deterministic_merge = None;
    let mut processed_cache_files = Vec::new();
    let mut production_loop = None;
    let mut bulk_healpix_merge = None;
    let mut reconciliation_backfill = None;
    let mut cleanup_simulation = None;

    if !args.preflight_only && !args.skip_rehearsal {
        let bulk_gz =
            resolve_rehearsal_bulk(&input_cache_dir, &work_dir, args.rehearsal_bulk_gz.as_ref())?;
        let rehearsal_dir = work_dir.join("rehearsal");
        if args.resume
            && rehearsal_dir
                .join("phase5b_mini_pilot_checkpoint.json")
                .exists()
        {
            run_mini_pilot(
                &bulk_gz,
                &rehearsal_dir,
                args.rehearsal_row_limit,
                args.rehearsal_batch_size,
                1,
                false,
                true,
                1,
                &args,
            )?;
        } else if rehearsal_dir.exists() {
            fs::remove_dir_all(&rehearsal_dir)?;
        }
        run_mini_pilot(
            &bulk_gz,
            &rehearsal_dir,
            args.rehearsal_row_limit,
            args.rehearsal_batch_size,
            1,
            false,
            false,
            1,
            &args,
        )?;
        if let Some(rotator) = rotator.as_mut() {
            advance_cache_after_mini_pilot(rotator, &bulk_gz)?;
        }
        rehearsal = Some(read_rehearsal_report(
            &rehearsal_dir,
            &bulk_gz,
            args.rehearsal_row_limit,
            args.rehearsal_batch_size,
        )?);
        if let Some(report) = &rehearsal {
            if !report.passed {
                blockers.push(format!(
                    "representative_rehearsal: throughput={:.2} src/s peak_rss_kib={}",
                    report.sources_per_second, report.peak_rss_kib
                ));
            }
        }

        if !args.skip_resume_test {
            let uninterrupted = work_dir.join("resume_test_uninterrupted");
            let resumed = work_dir.join("resume_test_resumed");
            for dir in [&uninterrupted, &resumed] {
                if dir.exists() {
                    fs::remove_dir_all(dir)?;
                }
            }
            let half = args.rehearsal_row_limit / 2;
            run_mini_pilot(
                &bulk_gz,
                &uninterrupted,
                args.rehearsal_row_limit,
                args.rehearsal_batch_size,
                1,
                false,
                false,
                1,
                &args,
            )?;
            run_mini_pilot(
                &bulk_gz,
                &resumed,
                half,
                args.rehearsal_batch_size,
                1,
                false,
                false,
                1,
                &args,
            )?;
            run_mini_pilot(
                &bulk_gz,
                &resumed,
                half,
                args.rehearsal_batch_size,
                1,
                false,
                true,
                1,
                &args,
            )?;
            let resume_output = manifest_dir.join("resume_validation.json");
            resume_validation = Some(run_resume_validation(
                &uninterrupted,
                &resumed,
                &resume_output,
            )?);
            if let Some(value) = &resume_validation {
                if value.get("passed").and_then(|v| v.as_bool()) != Some(true) {
                    blockers.push("kill_resume: resume validation failed".to_string());
                }
            }
        }

        if storage_plan.passed {
            deterministic_merge = read_existing_merge_report(&work_dir);
        }
    }

    if !args.preflight_only {
        if let (Some(rotator), Some(limit)) = (rotator.as_mut(), args.process_verified_cache_limit)
        {
            processed_cache_files = process_verified_cache_files(rotator, &work_dir, limit, &args)?;
            println!(
                "processed {} verified USB cache files",
                processed_cache_files.len()
            );
        }

        if let (Some(rotator), Some(limit)) = (rotator.as_mut(), args.file_limit) {
            let reconciliation_dir = args
                .reconciliation_dir
                .clone()
                .unwrap_or_else(|| rotator.layout.reconciliation_dir.clone());
            production_loop = Some(run_production_loop(
                rotator,
                &work_dir,
                &checkpoint_dir,
                &reconciliation_dir,
                limit,
                &args,
            )?);
            if let Some(loop_report) = &production_loop {
                println!(
                    "production loop: {} file(s), passed={}",
                    loop_report.files.len(),
                    loop_report.passed
                );
            }
        }
    }

    if args.backfill_reconciliation {
        let reconciliation_dir = args.reconciliation_dir.clone().unwrap_or_else(|| {
            args.usb_cache_root
                .as_ref()
                .map(|root| root.join("reconciliation"))
                .unwrap_or_else(|| checkpoint_dir.join("reconciliation"))
        });
        let cache_uuid = rotator
            .as_ref()
            .map(|rotator| rotator.manifest.cache_uuid.clone())
            .unwrap_or_else(|| "unknown".to_string());
        reconciliation_backfill = Some(run_reconciliation_backfill(
            &reconciliation_dir,
            &cache_uuid,
            &work_dir,
            &checkpoint_dir,
        )?);
    }

    if args.merge_partition_checkpoints || production_loop.is_some() {
        let reconciliation_dir = args.reconciliation_dir.clone().unwrap_or_else(|| {
            args.usb_cache_root
                .as_ref()
                .map(|root| root.join("reconciliation"))
                .unwrap_or_else(|| checkpoint_dir.join("reconciliation"))
        });
        bulk_healpix_merge = Some(run_bulk_healpix_merge(
            &checkpoint_dir,
            &work_dir,
            &reconciliation_dir,
        )?);
        if let Some(report) = &bulk_healpix_merge {
            println!(
                "bulk HEALPix merge: {} partitions merged ({} new), checksum {} passed={}",
                report.total_partitions_merged,
                report.partitions_merged_this_run,
                report.global_healpix_checksum,
                report.passed
            );
        }
    }

    if args.cleanup_verified_inputs {
        if let Some(rotator) = rotator.as_mut() {
            rotator.reload_manifest()?;
            let cleanup = rotator.run_input_cleanup(args.dry_run, args.cleanup_limit)?;
            let cleanup_path = manifest_dir.join("cleanup_simulation.json");
            atomic_write_json(
                &cleanup_path,
                &(serde_json::to_string_pretty(&cleanup)? + "\n"),
            )?;
            println!(
                "cleanup {}: {} candidates, {} bytes reclaimable -> {}",
                if args.dry_run { "dry-run" } else { "live" },
                cleanup.candidates.len(),
                cleanup.bytes_reclaimed,
                cleanup_path.display()
            );
            for candidate in &cleanup.candidates {
                let verb = if args.dry_run {
                    "would delete"
                } else {
                    "deleted"
                };
                println!("  {verb}: {candidate}");
            }
            cleanup_simulation = Some(serde_json::to_value(&cleanup)?);
        } else {
            blockers.push("cleanup_verified_inputs: USB cache rotator not configured".to_string());
        }
    }

    let rehearsal_gate = rehearsal
        .as_ref()
        .map(|r| r.passed)
        .unwrap_or(args.skip_rehearsal);
    let resume_gate = resume_validation
        .as_ref()
        .and_then(|v| v.get("passed").and_then(|b| b.as_bool()))
        .unwrap_or(args.skip_resume_test);
    let all_preflight_gates_passed = storage_plan.passed
        && inventory.passed
        && rehearsal_gate
        && resume_gate
        && blockers.is_empty();
    let ready_for_full_bulk = all_preflight_gates_passed;

    let report = PipelineReport {
        schema_version: 1,
        session_manifest: session,
        storage_plan_passed: storage_plan.passed,
        rehearsal,
        resume_validation,
        deterministic_merge,
        processed_cache_files,
        production_loop,
        bulk_healpix_merge,
        reconciliation_backfill,
        cleanup_simulation,
        all_preflight_gates_passed,
        ready_for_full_bulk,
        blockers: blockers.clone(),
    };
    let report_path = manifest_dir.join("pipeline_report.json");
    atomic_write_json(
        &report_path,
        &(serde_json::to_string_pretty(&report)? + "\n"),
    )?;

    if args.preflight_only {
        println!("preflight-only complete; rehearsal skipped");
    }

    if !ready_for_full_bulk {
        println!("NOT READY for full bulk. Blockers:");
        for blocker in &blockers {
            println!("  - {blocker}");
        }
        if !storage_plan.passed {
            std::process::exit(2);
        }
    } else {
        println!("ALL PREFLIGHT GATES PASSED — ready for full bulk orchestration");
    }

    Ok(())
}

fn resolve_rehearsal_bulk(
    input_cache_dir: &Path,
    work_dir: &Path,
    rehearsal_bulk_gz: Option<&PathBuf>,
) -> Result<PathBuf> {
    if let Some(path) = rehearsal_bulk_gz {
        return Ok(path.clone());
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(input_cache_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("gz")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("XpContinuousMeanSpectrum_"))
        })
        .collect();
    if candidates.is_empty() {
        let pilot_bulk = work_dir
            .parent()
            .map(|parent| parent.join("pilot-xp-continuous-bulk/bulk"))
            .filter(|path| path.exists());
        if let Some(path) = pilot_bulk {
            candidates = fs::read_dir(&path)?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("gz"))
                .collect();
        }
    }
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .with_context(|| "no rehearsal bulk .csv.gz found in input cache or pilot directory")
}

fn available_cores() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(8)
}

fn production_worker_headroom(args: &Args) -> usize {
    if args.production_worker_headroom > 0 {
        return args.production_worker_headroom;
    }
    if let Ok(value) = std::env::var("PRODUCTION_WORKER_HEADROOM") {
        if let Ok(parsed) = value.parse::<usize>() {
            return parsed.max(0);
        }
    }
    if args.usb_cache_root.is_some() {
        4
    } else {
        2
    }
}

fn production_parallel_partitions(args: &Args) -> usize {
    args.production_parallel_partitions.max(1)
}

fn production_workers(args: &Args, parallel_partitions: usize) -> usize {
    if args.production_workers > 0 {
        return args.production_workers.max(1);
    }
    if let Ok(value) = std::env::var("PRODUCTION_WORKERS") {
        if let Ok(parsed) = value.parse::<usize>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    let headroom = production_worker_headroom(args);
    let budget = available_cores().saturating_sub(headroom).max(1);
    let parallel = parallel_partitions.max(1);
    (budget / parallel).max(1)
}

fn production_checkpoint_interval(args: &Args, workers: usize) -> usize {
    if args.production_checkpoint_interval > 0 {
        return args.production_checkpoint_interval;
    }
    if let Ok(value) = std::env::var("PRODUCTION_CHECKPOINT_INTERVAL") {
        if let Ok(parsed) = value.parse::<usize>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    (workers / 2).max(4)
}

#[allow(clippy::too_many_arguments)]
fn run_mini_pilot(
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    workers: usize,
    skip_normalized_output: bool,
    resume: bool,
    checkpoint_interval: usize,
    args: &Args,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    run_mini_pilot_command(
        bulk_gz,
        output_dir,
        row_limit,
        batch_size,
        workers,
        skip_normalized_output,
        resume,
        args.frozen_policy.as_deref(),
        args.gaiaxpy_environment.as_deref(),
        checkpoint_interval,
    )
}

fn read_rehearsal_report(
    output_dir: &Path,
    bulk_gz: &Path,
    row_limit: usize,
    batch_size: usize,
) -> Result<RehearsalReport> {
    let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_metrics.json"),
    )?)?;
    let wall = metrics["wall_elapsed_seconds"].as_f64().unwrap_or(0.0);
    let rows_valid = metrics["rows_valid"].as_u64().unwrap_or(0);
    let sources_per_second = if wall > 0.0 {
        rows_valid as f64 / wall
    } else {
        0.0
    };
    Ok(RehearsalReport {
        bulk_gz: bulk_gz.display().to_string(),
        row_limit,
        batch_size,
        wall_elapsed_seconds: wall,
        sources_per_second,
        peak_rss_kib: metrics["peak_rss_kib"].as_u64().unwrap_or(0),
        rows_valid,
        rows_excluded: metrics["rows_excluded"].as_u64().unwrap_or(0),
        rows_failed: metrics["rows_failed"].as_u64().unwrap_or(0),
        healpix_checksum: metrics["healpix_checksum"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        flux_checksum: metrics["flux_checksum"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        passed: rows_valid > 0 && sources_per_second > 0.0,
    })
}

fn run_resume_validation(
    uninterrupted_dir: &Path,
    resumed_dir: &Path,
    output_json: &Path,
) -> Result<serde_json::Value> {
    let status = Command::new("cargo")
        .args([
            "run",
            "--locked",
            "-q",
            "-p",
            "nsb-data-tools",
            "--bin",
            "run_phase5b_resume_validation",
            "--",
            "--uninterrupted-dir",
        ])
        .arg(uninterrupted_dir)
        .args(["--resumed-dir"])
        .arg(resumed_dir)
        .args(["--output-json"])
        .arg(output_json)
        .status()
        .context("failed to launch run_phase5b_resume_validation")?;
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output_json).with_context(|| {
            format!(
                "failed to read resume validation report at {}",
                output_json.display()
            )
        })?)?;
    if !status.success() && report.get("passed").and_then(|v| v.as_bool()) != Some(true) {
        // Non-zero exit is expected when validation fails; report is still authoritative.
    }
    Ok(report)
}

fn read_existing_merge_report(work_dir: &Path) -> Option<serde_json::Value> {
    let path = work_dir.parent().map(|parent| {
        parent
            .join("pilot-xp-continuous-bulk/multifile_pilot/phase5b_multifile_pilot_manifest.json")
    })?;
    serde_json::from_str(&fs::read_to_string(path).ok()?)
        .ok()
        .and_then(|manifest: serde_json::Value| manifest.get("deterministic_merge").cloned())
}

fn software_commit() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .context("failed to resolve git HEAD")?;
    if !output.status.success() {
        bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn load_usb_rotator(
    args: &Args,
    official_checksum_manifest: &Path,
) -> Result<Option<UsbCacheRotator>> {
    let (Some(mount), Some(root)) = (&args.usb_mountpoint, &args.usb_cache_root) else {
        return Ok(None);
    };

    let layout = UsbCacheLayout::from_env(mount, root, &args.cache_subdir);
    if args.init_usb_marker {
        let marker = read_or_create_cache_root_marker(&layout, true)?;
        append_session_log(
            &layout,
            &format!("initialized cache root marker uuid={}", marker.cache_uuid),
        )?;
        println!(
            "USB cache root marker initialized: {} ({})",
            layout.marker_path().display(),
            marker.cache_uuid
        );
    }
    let identity = verify_usb_identity(&layout)?;
    if !identity.passed {
        bail!(
            "USB identity preflight failed: {}",
            identity.failures.join("; ")
        );
    }

    let rotator = UsbCacheRotator::prepare(UsbCacheRotatorConfig {
        layout,
        max_cache_bytes: args.max_cache_bytes,
        init_usb_marker: false,
    })?;

    if !official_checksum_manifest.is_file() {
        bail!(
            "official checksum manifest missing at {}",
            official_checksum_manifest.display()
        );
    }

    Ok(Some(rotator))
}

fn advance_cache_after_mini_pilot(rotator: &mut UsbCacheRotator, bulk_gz: &Path) -> Result<()> {
    let Some(filename) = bulk_filename(bulk_gz) else {
        return Ok(());
    };
    if rotator.entry_state(&filename).is_none() {
        return Ok(());
    }
    rotator.advance_after_mini_pilot(&filename)
}

fn process_verified_cache_files(
    rotator: &mut UsbCacheRotator,
    work_dir: &Path,
    limit: usize,
    args: &Args,
) -> Result<Vec<ProcessedCacheFileReport>> {
    let filenames = filenames_checksum_verified(&rotator.manifest, Some(limit));
    let mut reports = Vec::with_capacity(filenames.len());
    let process_root = work_dir.join("verified_cache_process");
    fs::create_dir_all(&process_root)?;

    for filename in filenames {
        let bulk_gz = rotator.layout.cache_dir.join(&filename);
        if !bulk_gz.is_file() {
            bail!(
                "checksum_verified cache file missing on disk: {}",
                bulk_gz.display()
            );
        }
        let output_dir = process_root.join(filename.trim_end_matches(".csv.gz"));
        if output_dir.exists() {
            fs::remove_dir_all(&output_dir)?;
        }
        rotator.mark_processing(&filename)?;
        run_mini_pilot(
            &bulk_gz,
            &output_dir,
            args.rehearsal_row_limit,
            args.rehearsal_batch_size,
            1,
            false,
            false,
            1,
            args,
        )?;
        rotator.advance_after_mini_pilot(&filename)?;
        let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
            output_dir.join("phase5b_mini_pilot_metrics.json"),
        )?)?;
        let rows_valid = metrics["rows_valid"].as_u64().unwrap_or(0);
        let rows_failed = metrics["rows_failed"].as_u64().unwrap_or(0);
        let final_state = rotator
            .entry_state(&filename)
            .map(cache_state_label)
            .unwrap_or_else(|| "unknown".to_string());
        reports.push(ProcessedCacheFileReport {
            filename: filename.clone(),
            bulk_gz: bulk_gz.display().to_string(),
            output_dir: output_dir.display().to_string(),
            rows_valid,
            rows_failed,
            final_state: final_state.clone(),
            passed: rows_valid > 0 && rows_failed == 0 && final_state == "releasable",
        });
    }
    Ok(reports)
}

fn mini_pilot_checkpoint_path(output_dir: &Path) -> PathBuf {
    output_dir.join("phase5b_mini_pilot_checkpoint.json")
}

fn prepare_production_output_dir(output_dir: &Path, resume: bool) -> Result<()> {
    let can_resume = resume && mini_pilot_checkpoint_path(output_dir).is_file();
    if output_dir.exists() && !can_resume {
        fs::remove_dir_all(output_dir)?;
    }
    Ok(())
}

fn run_production_loop(
    rotator: &mut UsbCacheRotator,
    work_dir: &Path,
    checkpoint_dir: &Path,
    reconciliation_dir: &Path,
    file_limit: usize,
    args: &Args,
) -> Result<ProductionLoopReport> {
    if args
        .frozen_policy
        .as_ref()
        .is_none_or(|path| !path.is_file())
    {
        bail!("production loop requires --frozen-policy (or STARLIGHT_FROZEN_POLICY)");
    }
    if args
        .gaiaxpy_environment
        .as_ref()
        .is_none_or(|path| !path.is_file())
    {
        bail!("production loop requires --gaiaxpy-environment (or STARLIGHT_GAIAXPY_ENV)");
    }

    let parallel = production_parallel_partitions(args);
    let workers = production_workers(args, parallel);
    let checkpoint_interval = production_checkpoint_interval(args, workers);
    println!(
        "production packing: parallel_partitions={parallel} workers_per_partition={workers} checkpoint_interval={checkpoint_interval} cores={}",
        available_cores()
    );

    rotator.reload_manifest()?;
    let candidates = filenames_for_production(&rotator.manifest, Some(file_limit));
    let claims = claim_partitions(checkpoint_dir, &candidates, file_limit.max(1))?;
    if claims.is_empty() {
        bail!(
            "no claimable production partitions (candidates={}, file_limit={file_limit}); another worker may hold all leases",
            candidates.len()
        );
    }
    let filenames: Vec<String> = claims
        .iter()
        .map(|claim| claim.filename().to_string())
        .collect();
    println!(
        "claimed {}/{} partition(s) for this invocation",
        filenames.len(),
        candidates.len()
    );

    let production_root = work_dir.join("production_loop");
    fs::create_dir_all(&production_root)?;
    fs::create_dir_all(checkpoint_dir)?;
    fs::create_dir_all(reconciliation_dir)?;

    let cache_uuid = rotator.manifest.cache_uuid.clone();
    let partitions_pending = rotator
        .manifest
        .entries
        .iter()
        .filter(|entry| {
            !matches!(
                entry.state,
                crate::gaia_usb_cache::CacheInputState::Releasable
                    | crate::gaia_usb_cache::CacheInputState::Deleted
            )
        })
        .count() as u32;

    let files = if parallel <= 1 || filenames.len() <= 1 {
        let mut files = Vec::with_capacity(filenames.len());
        for filename in &filenames {
            files.push(process_one_production_file(
                rotator,
                filename,
                &production_root,
                checkpoint_dir,
                reconciliation_dir,
                &cache_uuid,
                partitions_pending,
                workers,
                checkpoint_interval,
                args,
            )?);
        }
        files
    } else {
        process_production_files_parallel(
            rotator,
            &filenames,
            parallel,
            &production_root,
            checkpoint_dir,
            reconciliation_dir,
            &cache_uuid,
            partitions_pending,
            workers,
            checkpoint_interval,
            args,
        )?
    };

    // Keep claims alive until processing finishes.
    drop(claims);

    let passed = !files.is_empty() && files.iter().all(|file| file.passed);
    Ok(ProductionLoopReport {
        file_limit,
        files,
        passed,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_production_files_parallel(
    rotator: &mut UsbCacheRotator,
    filenames: &[String],
    parallel: usize,
    production_root: &Path,
    checkpoint_dir: &Path,
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partitions_pending: u32,
    workers: usize,
    checkpoint_interval: usize,
    args: &Args,
) -> Result<Vec<ProductionFileReport>> {
    let mut reports = Vec::with_capacity(filenames.len());
    for chunk in filenames.chunks(parallel.max(1)) {
        let mut prepared = Vec::with_capacity(chunk.len());
        for filename in chunk {
            prepared.push(prepare_production_file(
                rotator,
                filename,
                production_root,
                args,
            )?);
        }

        let stream_results = thread::scope(
            |scope| -> Result<Vec<(usize, Option<ProductionStreamMetrics>, Option<String>)>> {
                let mut handles = Vec::with_capacity(prepared.len());
                for (idx, prep) in prepared.iter().enumerate() {
                    if !prep.ready_to_process {
                        handles.push(scope.spawn(move || Ok((idx, None, None))));
                        continue;
                    }
                    let bulk_gz = prep.bulk_gz.clone();
                    let output_dir = prep.output_dir.clone();
                    let filename = prep.filename.clone();
                    let can_resume = prep.can_resume;
                    let handle = scope.spawn(move || -> Result<(usize, Option<ProductionStreamMetrics>, Option<String>)> {
                    let metrics = run_production_stream(
                        &bulk_gz,
                        &output_dir,
                        args.production_row_limit,
                        args.production_batch_size,
                        workers,
                        can_resume,
                        checkpoint_interval,
                        args,
                    )?;
                    let checkpoint =
                        write_healpix_checkpoint(&filename, &output_dir, checkpoint_dir)?;
                    Ok((idx, Some(metrics), Some(checkpoint)))
                });
                    handles.push(handle);
                }
                let mut out: Vec<Option<(Option<ProductionStreamMetrics>, Option<String>)>> =
                    (0..prepared.len()).map(|_| None).collect();
                for handle in handles {
                    let (idx, metrics, checkpoint) = handle
                        .join()
                        .map_err(|_| anyhow::anyhow!("production worker thread panicked"))??;
                    out[idx] = Some((metrics, checkpoint));
                }
                Ok(out
                    .into_iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let (metrics, checkpoint) = item.unwrap_or((None, None));
                        (idx, metrics, checkpoint)
                    })
                    .collect())
            },
        )?;

        for (idx, metrics, healpix_checkpoint) in stream_results {
            let prep = &prepared[idx];
            let mut processed = false;
            let mut reconciliation = None;
            let stream_metrics = metrics;
            if prep.ready_to_process {
                if stream_metrics.is_none() {
                    bail!("missing stream metrics for {}", prep.filename);
                }
                let (manifest, manifest_path, ledger_path) =
                    build_partition_from_processing_output(
                        reconciliation_dir,
                        cache_uuid,
                        &prep.filename,
                        &prep.output_dir,
                        partitions_pending,
                    )?;
                reconciliation = Some(production_reconciliation_report(
                    &manifest,
                    &manifest_path,
                    &ledger_path,
                ));
                processed = true;
                if prep.output_dir.exists() {
                    fs::remove_dir_all(&prep.output_dir)?;
                }
                rotator.advance_after_mini_pilot(&prep.filename)?;
            }
            let final_state = rotator
                .entry_state(&prep.filename)
                .map(cache_state_label)
                .unwrap_or_else(|| "unknown".to_string());
            let passed = final_state == "releasable"
                && processed
                && reconciliation
                    .as_ref()
                    .is_some_and(|report| report.reconciliation_ok);
            reports.push(ProductionFileReport {
                filename: prep.filename.clone(),
                initial_state: prep.initial_state.clone(),
                downloaded: prep.downloaded,
                processed,
                stream_metrics,
                healpix_checkpoint,
                reconciliation,
                final_state,
                passed,
            });
        }
    }
    Ok(reports)
}

struct PreparedProductionFile {
    filename: String,
    initial_state: String,
    downloaded: bool,
    bulk_gz: PathBuf,
    output_dir: PathBuf,
    can_resume: bool,
    ready_to_process: bool,
}

fn prepare_production_file(
    rotator: &mut UsbCacheRotator,
    filename: &str,
    production_root: &Path,
    args: &Args,
) -> Result<PreparedProductionFile> {
    let initial_state = rotator
        .entry_state(filename)
        .map(cache_state_label)
        .unwrap_or_else(|| "unknown".to_string());
    let mut downloaded = false;
    if matches!(
        rotator.entry_state(filename),
        Some(crate::gaia_usb_cache::CacheInputState::Planned)
            | Some(crate::gaia_usb_cache::CacheInputState::Failed)
    ) {
        download_cache_file(filename, args)?;
        rotator.reload_manifest()?;
        downloaded = true;
    }
    let bulk_gz = rotator.layout.cache_dir.join(filename);
    let output_dir = production_root.join(filename.trim_end_matches(".csv.gz"));
    let entry_state = rotator.entry_state(filename);
    let can_resume = args.resume && mini_pilot_checkpoint_path(&output_dir).is_file();
    let ready_to_process = matches!(
        entry_state,
        Some(crate::gaia_usb_cache::CacheInputState::ChecksumVerified)
            | Some(crate::gaia_usb_cache::CacheInputState::Processing)
    );
    if ready_to_process {
        if !bulk_gz.is_file() {
            bail!(
                "production input missing on disk for {filename}: {}",
                bulk_gz.display()
            );
        }
        prepare_production_output_dir(&output_dir, args.resume)?;
        if entry_state == Some(crate::gaia_usb_cache::CacheInputState::ChecksumVerified) {
            rotator.mark_processing(filename)?;
        }
    }
    Ok(PreparedProductionFile {
        filename: filename.to_string(),
        initial_state,
        downloaded,
        bulk_gz,
        output_dir,
        can_resume,
        ready_to_process,
    })
}

#[allow(clippy::too_many_arguments)]
fn process_one_production_file(
    rotator: &mut UsbCacheRotator,
    filename: &str,
    production_root: &Path,
    checkpoint_dir: &Path,
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partitions_pending: u32,
    workers: usize,
    checkpoint_interval: usize,
    args: &Args,
) -> Result<ProductionFileReport> {
    let prep = prepare_production_file(rotator, filename, production_root, args)?;
    let mut processed = false;
    let mut stream_metrics = None;
    let mut healpix_checkpoint = None;
    let mut reconciliation = None;
    if prep.ready_to_process {
        stream_metrics = Some(run_production_stream(
            &prep.bulk_gz,
            &prep.output_dir,
            args.production_row_limit,
            args.production_batch_size,
            workers,
            prep.can_resume,
            checkpoint_interval,
            args,
        )?);
        healpix_checkpoint = Some(write_healpix_checkpoint(
            &prep.filename,
            &prep.output_dir,
            checkpoint_dir,
        )?);
        let (manifest, manifest_path, ledger_path) = build_partition_from_processing_output(
            reconciliation_dir,
            cache_uuid,
            &prep.filename,
            &prep.output_dir,
            partitions_pending,
        )?;
        reconciliation = Some(production_reconciliation_report(
            &manifest,
            &manifest_path,
            &ledger_path,
        ));
        processed = true;
        if prep.output_dir.exists() {
            fs::remove_dir_all(&prep.output_dir)?;
        }
        rotator.advance_after_mini_pilot(&prep.filename)?;
    }
    let final_state = rotator
        .entry_state(&prep.filename)
        .map(cache_state_label)
        .unwrap_or_else(|| "unknown".to_string());
    let passed = final_state == "releasable"
        && processed
        && reconciliation
            .as_ref()
            .is_some_and(|report| report.reconciliation_ok);
    Ok(ProductionFileReport {
        filename: prep.filename,
        initial_state: prep.initial_state,
        downloaded: prep.downloaded,
        processed,
        stream_metrics,
        healpix_checkpoint,
        reconciliation,
        final_state,
        passed,
    })
}

fn production_reconciliation_report(
    manifest: &PartitionReconciliationManifest,
    manifest_path: &Path,
    ledger_path: &Path,
) -> ProductionReconciliationReport {
    ProductionReconciliationReport {
        status: "partition_complete".to_string(),
        partition_manifest: manifest_path.display().to_string(),
        ledger_path: ledger_path.display().to_string(),
        rows_valid: manifest.source_counts.rows_valid,
        rows_excluded: manifest.source_counts.rows_excluded,
        rows_failed: manifest.source_counts.rows_failed,
        healpix_checksum: manifest.accumulator.healpix_checksum.clone(),
        reconciliation_ok: manifest.reconciliation_ok,
        population_status: manifest.population_status.clone(),
    }
}

/// Stream a bulk partition through the canonical adapter, GaiaXPy, and HEALPix
/// accumulator (`run_phase5b_mini_pilot` with production row/batch limits).
fn run_production_stream(
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    workers: usize,
    resume: bool,
    checkpoint_interval: usize,
    args: &Args,
) -> Result<ProductionStreamMetrics> {
    run_mini_pilot(
        bulk_gz,
        output_dir,
        row_limit,
        batch_size,
        workers,
        true,
        resume,
        checkpoint_interval,
        args,
    )?;

    let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_metrics.json"),
    )?)?;
    Ok(ProductionStreamMetrics {
        rows_scanned: metrics["rows_scanned"].as_u64().unwrap_or(0),
        rows_valid: metrics["rows_valid"].as_u64().unwrap_or(0),
        rows_excluded: metrics["rows_excluded"].as_u64().unwrap_or(0),
        rows_failed: metrics["rows_failed"].as_u64().unwrap_or(0),
        sources_per_second: metrics["sources_per_second"].as_f64().unwrap_or(0.0),
        wall_elapsed_seconds: metrics["wall_elapsed_seconds"].as_f64().unwrap_or(0.0),
        peak_rss_kib: metrics["peak_rss_kib"].as_u64().unwrap_or(0),
        healpix_checksum: metrics["healpix_checksum"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        flux_checksum: metrics["flux_checksum"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        row_limit,
        batch_size,
        full_file: row_limit == 0,
    })
}

fn download_cache_file(filename: &str, args: &Args) -> Result<()> {
    run_download_bulk_command(
        filename,
        args.usb_mountpoint.as_deref(),
        args.usb_cache_root.as_deref(),
        &args.cache_subdir,
        args.max_cache_bytes,
        args.resume,
    )
}

fn write_healpix_checkpoint(
    filename: &str,
    output_dir: &Path,
    checkpoint_dir: &Path,
) -> Result<String> {
    let source = output_dir.join("phase5b_healpix_accumulator.json");
    if !source.is_file() {
        bail!(
            "HEALPix accumulator missing at {} after processing {}",
            source.display(),
            filename
        );
    }
    let stem = filename.trim_end_matches(".csv.gz");
    let dest = checkpoint_dir.join(format!("{stem}_healpix_accumulator.json"));
    fs::copy(&source, &dest).with_context(|| {
        format!(
            "failed to copy HEALPix checkpoint from {} to {}",
            source.display(),
            dest.display()
        )
    })?;
    Ok(dest.display().to_string())
}

fn run_reconciliation_backfill(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    work_dir: &Path,
    checkpoint_dir: &Path,
) -> Result<serde_json::Value> {
    let search_roots = reconciliation_search_roots(work_dir);
    let (ledger, backfilled) = backfill_reconciliation_from_verified_cache(
        reconciliation_dir,
        cache_uuid,
        &search_roots,
        3381,
    )?;
    let merge_checksum = fs::read_to_string(
        crate::gaia_xp_continuous_bulk_healpix_merge::merge_state_path(checkpoint_dir),
    )
    .ok()
    .and_then(|text| {
        serde_json::from_str::<crate::gaia_xp_continuous_bulk_healpix_merge::BulkHealpixMergeState>(
            &text,
        )
        .ok()
        .map(|state| state.global_healpix_checksum)
    });
    write_root_manifest(
        reconciliation_dir,
        &ledger,
        merge_checksum.as_deref(),
        Some(&bulk_accumulator_path(checkpoint_dir)),
    )?;
    println!(
        "reconciliation backfill: {} manifests, ledger valid={} progress {:.6}%",
        backfilled.len(),
        ledger.population_accumulated_valid,
        ledger.population_progress_fraction * 100.0
    );
    Ok(serde_json::json!({
        "partitions_backfilled": backfilled.iter().map(|manifest| {
            serde_json::json!({
                "partition_filename": manifest.partition_filename,
                "rows_valid": manifest.source_counts.rows_valid,
                "rows_excluded": manifest.source_counts.rows_excluded,
                "rows_failed": manifest.source_counts.rows_failed,
                "healpix_checksum": manifest.accumulator.healpix_checksum,
            })
        }).collect::<Vec<_>>(),
        "ledger_valid": ledger.population_accumulated_valid,
        "ledger_progress_fraction": ledger.population_progress_fraction,
    }))
}

fn reconciliation_search_roots(work_dir: &Path) -> Vec<PathBuf> {
    let mut search_roots = vec![work_dir.to_path_buf()];
    if let Some(parent) = work_dir.parent() {
        let sibling_work = parent.join("work");
        if sibling_work.is_dir() && sibling_work != *work_dir {
            search_roots.push(sibling_work);
        }
        if let Some(starlight_root) = parent.parent() {
            let legacy_work = starlight_root.join("work");
            if legacy_work.is_dir() && !search_roots.contains(&legacy_work) {
                search_roots.push(legacy_work);
            }
        }
    }
    search_roots
}

fn run_bulk_healpix_merge(
    checkpoint_dir: &Path,
    work_dir: &Path,
    reconciliation_dir: &Path,
) -> Result<BulkHealpixMergeReport> {
    let lock_path = checkpoint_dir.join("bulk_healpix_merge.lock");
    let _lock = crate::platform::file_lock::lock_exclusive(&lock_path)?;
    let search_roots = reconciliation_search_roots(work_dir);
    let report = merge_all_partition_checkpoints(checkpoint_dir, &search_roots)?;
    let merge_state: crate::gaia_xp_continuous_bulk_healpix_merge::BulkHealpixMergeState =
        serde_json::from_str(&fs::read_to_string(
            crate::gaia_xp_continuous_bulk_healpix_merge::merge_state_path(checkpoint_dir),
        )?)?;
    let (ledger, root_path) = sync_ledger_from_merge_state(
        reconciliation_dir,
        &merge_state,
        &bulk_accumulator_path(checkpoint_dir),
    )?;
    write_root_manifest(
        reconciliation_dir,
        &ledger,
        Some(&report.global_healpix_checksum),
        Some(&bulk_accumulator_path(checkpoint_dir)),
    )?;
    println!(
        "reconciliation root manifest: {} (progress {:.6}%)",
        root_path.display(),
        ledger.population_progress_fraction * 100.0
    );
    Ok(report)
}

fn cache_state_label(state: crate::gaia_usb_cache::CacheInputState) -> String {
    match state {
        crate::gaia_usb_cache::CacheInputState::Planned => "planned".to_string(),
        crate::gaia_usb_cache::CacheInputState::Downloading => "downloading".to_string(),
        crate::gaia_usb_cache::CacheInputState::Downloaded => "downloaded".to_string(),
        crate::gaia_usb_cache::CacheInputState::ChecksumVerified => "checksum_verified".to_string(),
        crate::gaia_usb_cache::CacheInputState::Processing => "processing".to_string(),
        crate::gaia_usb_cache::CacheInputState::Processed => "processed".to_string(),
        crate::gaia_usb_cache::CacheInputState::OutputVerified => "output_verified".to_string(),
        crate::gaia_usb_cache::CacheInputState::Releasable => "releasable".to_string(),
        crate::gaia_usb_cache::CacheInputState::Deleted => "deleted".to_string(),
        crate::gaia_usb_cache::CacheInputState::Failed => "failed".to_string(),
    }
}
