//! Batch-download Gaia XP continuous coefficients for Phase 5 targets.

use anyhow::{Context, Result};
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_datalink::{
    datalink_raw_coefficient_path, DatalinkConfig, DatalinkDownloader, DatalinkRetrievalType,
    DownloadPaths,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(about = "Download XP_CONTINUOUS coefficients for Phase 5 target lists")]
struct Args {
    #[arg(long)]
    targets_csv: PathBuf,
    #[arg(long)]
    raw_dir: PathBuf,
    #[arg(long)]
    checkpoint: PathBuf,
    #[arg(long)]
    inventory_csv: PathBuf,
    #[arg(long)]
    manifest_json: PathBuf,
    #[arg(long, default_value = "https://gea.esac.esa.int/data-server/data")]
    datalink_url: String,
    #[arg(long, default_value_t = 4)]
    concurrency: usize,
    #[arg(long, default_value_t = 2.0)]
    max_rps: f64,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    retry_failed_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct DownloadManifest {
    schema_version: u32,
    gaia_service_endpoint: String,
    retrieval_type: String,
    batch_id: String,
    population: String,
    source_ids_requested: usize,
    generation_timestamp_utc: String,
}

fn load_source_ids(path: &Path, limit: Option<usize>) -> Result<Vec<String>> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    let headers = reader.headers()?.clone();
    let idx = headers
        .iter()
        .position(|h| h == "source_id")
        .context("source_id column")?;
    let pop_idx = headers.iter().position(|h| h == "population");
    let split_idx = headers.iter().position(|h| h == "split");
    let mut ids = Vec::new();
    for row in reader.records() {
        let row = row?;
        ids.push(row.get(idx).context("source_id")?.to_string());
        let _population = pop_idx.and_then(|i| row.get(i));
        let _split = split_idx.and_then(|i| row.get(i));
        if limit.is_some_and(|cap| ids.len() >= cap) {
            break;
        }
    }
    Ok(ids)
}

fn write_inventory(
    path: &Path,
    raw_dir: &Path,
    requested: &[String],
    report: &nsb_data_tools::gaia_datalink::DownloadReport,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let failed: HashSet<_> = report.failed_source_ids.iter().cloned().collect();
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    writer.write_record([
        "source_id",
        "requested",
        "retrieved",
        "raw_path",
        "raw_sha256",
        "status",
    ])?;
    for source_id in requested {
        let raw_path = datalink_raw_coefficient_path(raw_dir, source_id);
        let (status, sha) = if failed.contains(source_id) {
            ("failed", String::new())
        } else if raw_path.is_file() {
            ("valid", sha256_file(&raw_path).unwrap_or_default())
        } else {
            ("missing", String::new())
        };
        writer.write_record([
            source_id.as_str(),
            "true",
            if raw_path.is_file() { "true" } else { "false" },
            raw_path.display().to_string().as_str(),
            sha.as_str(),
            status,
        ])?;
    }
    writer.flush()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.raw_dir)?;
    if let Some(parent) = args.checkpoint.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut source_ids = load_source_ids(&args.targets_csv, args.limit)?;
    source_ids.sort();
    source_ids.dedup();

    let population = args
        .targets_csv
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("targets")
        .to_string();
    let batch_id = format!(
        "{population}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    );

    let config = DatalinkConfig {
        concurrency: args.concurrency,
        max_rps: args.max_rps,
        timeout: Duration::from_secs(120),
        connect_timeout: Duration::from_secs(30),
        max_attempts: 5,
        initial_backoff: Duration::from_millis(500),
        max_backoff: Duration::from_secs(30),
        progress_interval: Duration::from_secs(10),
    };
    let downloader = Arc::new(
        DatalinkDownloader::new(&args.datalink_url, config)?
            .with_retrieval_type(DatalinkRetrievalType::XpContinuous),
    );
    let paths = DownloadPaths {
        raw_dir: args.raw_dir.clone(),
        error_dir: args.raw_dir.join("_errors"),
        checkpoint: args.checkpoint.clone(),
    };
    let report = downloader
        .download(&source_ids, &paths, args.resume, args.retry_failed_only)
        .await?;

    write_inventory(&args.inventory_csv, &args.raw_dir, &source_ids, &report)?;

    let manifest = DownloadManifest {
        schema_version: 1,
        gaia_service_endpoint: args.datalink_url,
        retrieval_type: "XP_CONTINUOUS".to_string(),
        batch_id,
        population,
        source_ids_requested: source_ids.len(),
        generation_timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };
    if let Some(parent) = args.manifest_json.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &args.manifest_json,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;

    println!(
        "download complete: requested={} completed={} failed={} pending={}",
        report.selected_sources,
        report.completed_sources,
        report.failed_sources,
        report.pending_sources
    );
    Ok(())
}
