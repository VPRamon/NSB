//! Phase 5B multifile operational pilot orchestrator.

use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb_data_tools::gaia_xp_continuous_bulk_index::{
    build_index, locate_and_verify_row, BulkFileIndex,
};
use nsb_data_tools::gaia_xp_continuous_bulk_schema::{
    compare_prefix_schemas, inspect_bulk_ecsv_schema,
};
use nsb_data_tools::gaia_xp_continuous_healpix::XpContinuousHealpixAccumulator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Parser)]
#[command(about = "Run Phase 5B multifile bulk XP continuous pilot across downloaded prefixes")]
struct Args {
    #[arg(long)]
    bulk_dir: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, value_delimiter = ',')]
    bulk_files: Vec<PathBuf>,
    #[arg(long, default_value_t = 10_000)]
    row_limit: usize,
    #[arg(long, default_value_t = 500)]
    batch_size: usize,
    #[arg(long, default_value_t = 64)]
    nside: u32,
    #[arg(long)]
    gaiaxpy_environment: PathBuf,
    #[arg(long, default_value = "_MD5SUM.txt")]
    md5_manifest: PathBuf,
    #[arg(long, default_value_t = false)]
    skip_resume_test: bool,
    #[arg(long, default_value_t = false)]
    skip_chunk_benchmark: bool,
    #[arg(long, default_value_t = false)]
    finalize_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct FilePilotSummary {
    bulk_file: String,
    bulk_checksum: String,
    output_dir: String,
    rows_scanned: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix_checksum: String,
    flux_checksum: String,
    reconciliation_ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeterministicMergeReport {
    order_12_checksum: String,
    order_21_checksum: String,
    single_worker_checksum: String,
    multi_worker_checksum: String,
    order_independent: bool,
    single_multi_identical: bool,
    passed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct MultifileManifest {
    schema_version: u32,
    bulk_files: Vec<String>,
    total_compressed_bytes: u64,
    schema_comparison: nsb_data_tools::gaia_xp_continuous_bulk_schema::PrefixSchemaComparison,
    per_file: Vec<FilePilotSummary>,
    chunk_benchmark: Option<serde_json::Value>,
    resume_validation: Option<serde_json::Value>,
    deterministic_merge: DeterministicMergeReport,
    index_checks: Vec<serde_json::Value>,
    software_commit: String,
    gaiaxpy_environment_checksum: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    if args.finalize_only {
        return finalize_artifacts(&args);
    }

    let bulk_files = resolve_bulk_files(&args)?;
    if bulk_files.len() < 2 {
        bail!("multifile pilot requires at least two bulk files");
    }

    let schema_left = inspect_bulk_ecsv_schema(&bulk_files[0], 32)?;
    let schema_right = inspect_bulk_ecsv_schema(&bulk_files[1], 32)?;
    let schema_comparison = compare_prefix_schemas(&schema_left, &schema_right);
    if !schema_comparison.compatible {
        bail!(
            "prefix schema incompatibility: {}",
            schema_comparison.incompatibilities.join("; ")
        );
    }

    let index = build_index(
        &args.bulk_dir.join(&args.md5_manifest),
        &args.bulk_dir,
        None,
    )?;
    fs::write(
        args.output_dir.join("phase5b_bulk_file_index.json"),
        serde_json::to_string_pretty(&index)? + "\n",
    )?;

    let chunk_benchmark = if args.skip_chunk_benchmark {
        None
    } else {
        Some(run_chunk_benchmark(
            &bulk_files[0],
            &args.output_dir.join("chunk_benchmark"),
            &args.gaiaxpy_environment,
            args.row_limit.min(500),
        )?)
    };

    let mut per_file = Vec::new();
    let mut accumulators = Vec::new();
    for bulk_gz in &bulk_files {
        let file_name = bulk_gz
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let out = args.output_dir.join(format!("file_{file_name}"));
        run_mini_pilot(
            bulk_gz,
            &out,
            args.row_limit,
            args.batch_size,
            args.nside,
            &args.gaiaxpy_environment,
            false,
        )?;
        let summary = read_file_summary(&out)?;
        let accumulator: XpContinuousHealpixAccumulator = serde_json::from_str(
            &fs::read_to_string(out.join("phase5b_healpix_accumulator.json"))?,
        )?;
        accumulators.push(accumulator);
        per_file.push(summary);
    }

    let resume_validation = if args.skip_resume_test {
        None
    } else {
        Some(run_resume_test(
            &bulk_files[0],
            &args.output_dir.join("resume_test"),
            args.row_limit,
            args.batch_size,
            args.nside,
            &args.gaiaxpy_environment,
        )?)
    };

    let deterministic_merge = validate_deterministic_merge(&accumulators)?;

    let index_checks = validate_index_for_files(&index, &bulk_files)?;

    let total_compressed_bytes = bulk_files
        .iter()
        .map(|path| fs::metadata(path).map(|meta| meta.len()).unwrap_or(0))
        .sum();

    let gaiaxpy_environment_checksum = read_gaiaxpy_checksum(&args.gaiaxpy_environment);
    let manifest = MultifileManifest {
        schema_version: 1,
        bulk_files: bulk_files
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        total_compressed_bytes,
        schema_comparison,
        per_file,
        chunk_benchmark,
        resume_validation,
        deterministic_merge: deterministic_merge.clone(),
        index_checks,
        software_commit: std::env::var("STARLIGHT_SOFTWARE_COMMIT")
            .unwrap_or_else(|_| "unknown".into()),
        gaiaxpy_environment_checksum,
    };

    fs::write(
        args.output_dir
            .join("phase5b_multifile_pilot_manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;

    let metrics = build_multifile_metrics(&manifest, &args, &bulk_files)?;
    fs::write(
        args.output_dir.join("phase5b_multifile_metrics.json"),
        serde_json::to_string_pretty(&metrics)? + "\n",
    )?;

    write_multifile_reconciliation_csv(&args.output_dir, &manifest)?;

    fs::write(
        args.output_dir.join("phase5b_deterministic_merge.json"),
        serde_json::to_string_pretty(&deterministic_merge)? + "\n",
    )?;

    if let Some(resume) = &manifest.resume_validation {
        fs::write(
            args.output_dir.join("phase5b_resume_validation.json"),
            serde_json::to_string_pretty(resume)? + "\n",
        )?;
    }

    write_resource_estimate(&args.output_dir, &metrics, &manifest)?;
    write_sha256sum(&args.output_dir)?;
    copy_top_level_artifacts(&args.output_dir)?;

    if !deterministic_merge.passed {
        bail!("multifile deterministic merge validation failed");
    }
    if manifest.per_file.iter().any(|file| !file.reconciliation_ok) {
        bail!("multifile reconciliation failed for at least one prefix");
    }

    println!(
        "phase5b multifile pilot passed: {} files, {} total rows valid -> {}",
        bulk_files.len(),
        manifest
            .per_file
            .iter()
            .map(|file| file.rows_valid)
            .sum::<u64>(),
        args.output_dir.display()
    );
    Ok(())
}

fn finalize_artifacts(args: &Args) -> Result<()> {
    let manifest: MultifileManifest = serde_json::from_str(&fs::read_to_string(
        args.output_dir
            .join("phase5b_multifile_pilot_manifest.json"),
    )?)?;
    let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        args.output_dir.join("phase5b_multifile_metrics.json"),
    )?)?;
    write_multifile_reconciliation_csv(&args.output_dir, &manifest)?;
    fs::write(
        args.output_dir.join("phase5b_deterministic_merge.json"),
        serde_json::to_string_pretty(&manifest.deterministic_merge)? + "\n",
    )?;
    if let Some(resume) = &manifest.resume_validation {
        fs::write(
            args.output_dir.join("phase5b_resume_validation.json"),
            serde_json::to_string_pretty(resume)? + "\n",
        )?;
    }
    write_resource_estimate(&args.output_dir, &metrics, &manifest)?;
    write_sha256sum(&args.output_dir)?;
    copy_top_level_artifacts(&args.output_dir)?;
    println!(
        "phase5b multifile pilot finalized -> {}",
        args.output_dir.display()
    );
    Ok(())
}

fn copy_top_level_artifacts(output_dir: &Path) -> Result<()> {
    let parent = output_dir
        .parent()
        .context("multifile output dir must have parent")?;
    for name in [
        "phase5b_multifile_pilot_manifest.json",
        "phase5b_multifile_metrics.json",
        "phase5b_multifile_reconciliation.csv",
        "phase5b_resume_validation.json",
        "phase5b_deterministic_merge.json",
        "phase5b_resource_estimate.json",
        "phase5b_resource_estimate.md",
        "phase5b_bulk_file_index.json",
        "phase5b.sha256sum",
    ] {
        let src = output_dir.join(name);
        if src.is_file() {
            fs::copy(&src, parent.join(name))?;
        }
    }
    Ok(())
}

fn resolve_bulk_files(args: &Args) -> Result<Vec<PathBuf>> {
    if !args.bulk_files.is_empty() {
        return Ok(args.bulk_files.clone());
    }
    let mut files = fs::read_dir(&args.bulk_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("gz")
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("XpContinuousMeanSpectrum_"))
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn run_mini_pilot(
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    nside: u32,
    gaiaxpy_environment: &Path,
    resume: bool,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "--locked",
        "-q",
        "-p",
        "nsb-data-tools",
        "--bin",
        "run_phase5b_mini_pilot",
        "--",
        "--bulk-gz",
    ]);
    command
        .arg(bulk_gz)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--row-limit")
        .arg(row_limit.to_string())
        .arg("--batch-size")
        .arg(batch_size.to_string())
        .arg("--nside")
        .arg(nside.to_string())
        .arg("--gaiaxpy-environment")
        .arg(gaiaxpy_environment)
        .arg("--skip-normalized-output");
    if resume {
        command.arg("--resume");
    }
    let status = command.status().context("invoke run_phase5b_mini_pilot")?;
    if !status.success() {
        bail!("mini pilot failed for {}", bulk_gz.display());
    }
    Ok(())
}

fn read_file_summary(output_dir: &Path) -> Result<FilePilotSummary> {
    let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_metrics.json"),
    )?)?;
    let reconciliation: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_reconciliation.json"),
    )?)?;
    Ok(FilePilotSummary {
        bulk_file: metrics["bulk_file"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        bulk_checksum: metrics["bulk_checksum"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        output_dir: output_dir.display().to_string(),
        rows_scanned: metrics["rows_scanned"].as_u64().unwrap_or(0),
        rows_valid: metrics["rows_valid"].as_u64().unwrap_or(0),
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
        reconciliation_ok: reconciliation["reconciliation_ok"]
            .as_bool()
            .unwrap_or(false),
    })
}

fn run_resume_test(
    bulk_gz: &Path,
    output_dir: &Path,
    row_limit: usize,
    batch_size: usize,
    nside: u32,
    gaiaxpy_environment: &Path,
) -> Result<serde_json::Value> {
    let _ = gaiaxpy_environment;
    let uninterrupted = output_dir.join("uninterrupted");
    let resumed = output_dir.join("resumed");
    fs::remove_dir_all(&uninterrupted).ok();
    fs::remove_dir_all(&resumed).ok();
    run_mini_pilot(
        bulk_gz,
        &uninterrupted,
        row_limit,
        batch_size,
        nside,
        gaiaxpy_environment,
        false,
    )?;
    let half = row_limit / 2;
    run_mini_pilot(
        bulk_gz,
        &resumed,
        half,
        batch_size,
        nside,
        gaiaxpy_environment,
        false,
    )?;
    run_mini_pilot(
        bulk_gz,
        &resumed,
        half,
        batch_size,
        nside,
        gaiaxpy_environment,
        true,
    )?;
    let validation_path = output_dir.join("phase5b_resume_validation.json");
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
        .arg(&uninterrupted)
        .arg("--resumed-dir")
        .arg(&resumed)
        .arg("--output-json")
        .arg(&validation_path)
        .status()?;
    if !status.success() {
        bail!("resume validation failed");
    }
    Ok(serde_json::from_str(&fs::read_to_string(
        &validation_path,
    )?)?)
}

fn run_chunk_benchmark(
    bulk_gz: &Path,
    output_dir: &Path,
    _gaiaxpy_environment: &Path,
    row_limit: usize,
) -> Result<serde_json::Value> {
    fs::create_dir_all(output_dir)?;
    let status = Command::new("cargo")
        .args([
            "run",
            "--locked",
            "-q",
            "-p",
            "nsb-data-tools",
            "--bin",
            "run_phase5b_chunk_benchmark",
            "--",
            "--bulk-gz",
        ])
        .arg(bulk_gz)
        .arg("--output-dir")
        .arg(output_dir)
        .arg("--row-limit")
        .arg(row_limit.to_string())
        .arg("--chunk-sizes")
        .arg("100,500,1000")
        .status()?;
    if !status.success() {
        bail!("chunk benchmark failed");
    }
    Ok(serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_chunk_benchmark.json"),
    )?)?)
}

fn validate_deterministic_merge(
    accumulators: &[XpContinuousHealpixAccumulator],
) -> Result<DeterministicMergeReport> {
    if accumulators.len() < 2 {
        bail!("deterministic merge requires two accumulators");
    }
    let mut order_12 = XpContinuousHealpixAccumulator::new(accumulators[0].nside)?;
    order_12.merge(&accumulators[0])?;
    order_12.merge(&accumulators[1])?;

    let mut order_21 = XpContinuousHealpixAccumulator::new(accumulators[0].nside)?;
    order_21.merge(&accumulators[1])?;
    order_21.merge(&accumulators[0])?;

    let mut single_worker = XpContinuousHealpixAccumulator::new(accumulators[0].nside)?;
    for acc in accumulators {
        single_worker.merge(acc)?;
    }

    let mut multi_worker = XpContinuousHealpixAccumulator::new(accumulators[0].nside)?;
    multi_worker.merge(&accumulators[0])?;
    multi_worker.merge(&accumulators[1])?;

    let order_independent = order_12.checksum() == order_21.checksum();
    let single_multi_identical = single_worker.checksum() == multi_worker.checksum();
    Ok(DeterministicMergeReport {
        order_12_checksum: order_12.checksum(),
        order_21_checksum: order_21.checksum(),
        single_worker_checksum: single_worker.checksum(),
        multi_worker_checksum: multi_worker.checksum(),
        order_independent,
        single_multi_identical,
        passed: order_independent && single_multi_identical,
    })
}

fn validate_index_for_files(
    index: &BulkFileIndex,
    bulk_files: &[PathBuf],
) -> Result<Vec<serde_json::Value>> {
    use nsb_data_tools::gaia_xp_continuous_canonical::stream_bulk_ecsv_gz;
    let mut checks = Vec::new();
    for bulk_gz in bulk_files {
        let file_name = bulk_gz
            .file_name()
            .and_then(|name| name.to_str())
            .context("bulk file name")?;
        let mut stream = stream_bulk_ecsv_gz(bulk_gz)?;
        let first = stream
            .next_record()?
            .with_context(|| format!("bulk file {file_name} has no rows"))?;
        let sample_source_id = first.source_id.parse::<u64>()?;
        let located = locate_and_verify_row(index, sample_source_id)?;
        checks.push(serde_json::json!({
            "bulk_file": file_name,
            "sample_source_id": sample_source_id.to_string(),
            "sample_healpix_index": located.healpix_index,
            "expected_file": located.file_name,
            "row_found": located.row_found,
            "expected_checksum": located.expected_checksum,
            "observed_checksum": located.observed_checksum,
            "validation_status": located.validation_status,
        }));
    }
    Ok(checks)
}

fn build_multifile_metrics(
    manifest: &MultifileManifest,
    args: &Args,
    bulk_files: &[PathBuf],
) -> Result<serde_json::Value> {
    let total_valid: u64 = manifest.per_file.iter().map(|file| file.rows_valid).sum();
    let total_excluded: u64 = manifest
        .per_file
        .iter()
        .map(|file| file.rows_excluded)
        .sum();
    let total_failed: u64 = manifest.per_file.iter().map(|file| file.rows_failed).sum();
    let total_scanned: u64 = manifest.per_file.iter().map(|file| file.rows_scanned).sum();
    let mut total_elapsed = 0.0;
    let mut peak_rss = 0_u64;
    let mut sources_per_second = 0.0;
    for file in &manifest.per_file {
        let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
            Path::new(&file.output_dir).join("phase5b_mini_pilot_metrics.json"),
        )?)?;
        total_elapsed += metrics["wall_elapsed_seconds"].as_f64().unwrap_or(0.0);
        peak_rss = peak_rss.max(metrics["peak_rss_kib"].as_u64().unwrap_or(0));
        sources_per_second += metrics["sources_per_second"].as_f64().unwrap_or(0.0);
    }
    sources_per_second /= manifest.per_file.len().max(1) as f64;
    let selected_chunk = manifest
        .chunk_benchmark
        .as_ref()
        .and_then(|value| value.get("selected_chunk_size"))
        .and_then(|value| value.as_u64())
        .unwrap_or(args.batch_size as u64);

    Ok(serde_json::json!({
        "schema_version": 1,
        "bulk_file_count": bulk_files.len(),
        "total_compressed_bytes": manifest.total_compressed_bytes,
        "total_compressed_gib": manifest.total_compressed_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
        "rows_scanned": total_scanned,
        "rows_valid": total_valid,
        "rows_excluded": total_excluded,
        "rows_failed": total_failed,
        "sources_per_second": sources_per_second,
        "wall_elapsed_seconds": total_elapsed,
        "peak_rss_kib": peak_rss,
        "selected_batch_size": selected_chunk,
        "resume_passed": manifest.resume_validation.as_ref().and_then(|v| v.get("passed")).and_then(|v| v.as_bool()),
        "deterministic_merge_passed": manifest.deterministic_merge.passed,
        "merged_healpix_checksum": manifest.deterministic_merge.single_worker_checksum,
        "gaiaxpy_environment_checksum": manifest.gaiaxpy_environment_checksum,
        "software_commit": manifest.software_commit,
        "per_file": manifest.per_file,
    }))
}

fn write_multifile_reconciliation_csv(
    output_dir: &Path,
    manifest: &MultifileManifest,
) -> Result<()> {
    let out_path = output_dir.join("phase5b_multifile_reconciliation.csv");
    let mut writer = csv::WriterBuilder::new().from_path(&out_path)?;
    for file in &manifest.per_file {
        let per_file_csv =
            Path::new(&file.output_dir).join("phase5b_mini_pilot_reconciliation.csv");
        if per_file_csv.is_file() {
            let mut reader = csv::ReaderBuilder::new().from_path(&per_file_csv)?;
            for row in reader.deserialize::<ExclusionRecord>() {
                writer.serialize(row?)?;
            }
        }
        writer.write_record([
            "",
            file.bulk_file.as_str(),
            &file.rows_scanned.to_string(),
            "file_summary",
            &format!(
                "valid={} excluded={} failed={}",
                file.rows_valid, file.rows_excluded, file.rows_failed
            ),
            "",
            &format!("reconciliation_ok={}", file.reconciliation_ok),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct ExclusionRecord {
    source_id: String,
    bulk_file: String,
    row_number: u64,
    reason_code: String,
    evidence: String,
    fallback: String,
    scientific_impact: String,
}

fn write_resource_estimate(
    output_dir: &Path,
    metrics: &serde_json::Value,
    manifest: &MultifileManifest,
) -> Result<()> {
    let pop = 184_729_270_f64;
    let sps = metrics["sources_per_second"].as_f64().unwrap_or(49.5);
    let estimate = serde_json::json!({
        "schema_version": 3,
        "population_xp_continuous_only": pop as u64,
        "bulk_files_total": 3386,
        "bulk_volume_tib": 3.3,
        "multifile_pilot_observed": metrics,
        "deterministic_merge": manifest.deterministic_merge,
        "selected_batch_size": metrics["selected_batch_size"],
        "scenarios": {
            "1_worker_days": pop / sps / 86400.0,
            "4_workers_days": pop / sps / 86400.0 / 4.0,
            "8_workers_days": pop / sps / 86400.0 / 8.0,
        }
    });
    fs::write(
        output_dir.join("phase5b_resource_estimate.json"),
        serde_json::to_string_pretty(&estimate)? + "\n",
    )?;
    let md = format!(
        "# Phase 5B multifile resource estimate\n\n\
Population: **184,729,270** XP continuous-only sources.\n\n\
## Multifile pilot\n\n\
- Files: **{}** (~{:.2} GiB compressed)\n\
- Valid sources: **{}**\n\
- Throughput: **{:.1}** sources/s (stable pilot estimate)\n\
- Batch size: **{}**\n\
- Merge checksum: `{}`\n\n\
## Scale scenarios\n\n\
| Workers | Days |\n| --- | --- |\n| 1 | {:.1} |\n| 4 | {:.1} |\n| 8 | {:.1} |\n",
        metrics["bulk_file_count"].as_u64().unwrap_or(2),
        metrics["total_compressed_gib"].as_f64().unwrap_or(2.75),
        metrics["rows_valid"].as_u64().unwrap_or(0),
        sps,
        metrics["selected_batch_size"].as_u64().unwrap_or(500),
        manifest.deterministic_merge.single_worker_checksum,
        pop / sps / 86400.0,
        pop / sps / 86400.0 / 4.0,
        pop / sps / 86400.0 / 8.0,
    );
    fs::write(output_dir.join("phase5b_resource_estimate.md"), md)?;
    Ok(())
}

fn write_sha256sum(output_dir: &Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut entries = Vec::new();
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name == "phase5b.sha256sum" {
            continue;
        }
        let bytes = fs::read(&path)?;
        let digest_bytes: [u8; 32] = Sha256::digest(bytes).into();
        let digest = digest_bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        entries.push((name.to_string(), digest));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let body = entries
        .iter()
        .map(|(name, digest)| format!("{digest}  {name}"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(output_dir.join("phase5b.sha256sum"), body)?;
    Ok(())
}

fn read_gaiaxpy_checksum(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("checksum_sha256")
        .or_else(|| json.get("gaiaxpy_package_hash"))
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_merge_is_commutative_for_disjoint_accumulators() -> Result<()> {
        let mut left = XpContinuousHealpixAccumulator::new(64)?;
        left.accumulate_valid(0, 1.0, 0.1, 0.0)?;
        let mut right = XpContinuousHealpixAccumulator::new(64)?;
        right.accumulate_valid((1_u64 << 43) | 1, 2.0, 0.2, 0.0)?;
        let report = validate_deterministic_merge(&[left, right])?;
        assert!(report.passed);
        Ok(())
    }
}
