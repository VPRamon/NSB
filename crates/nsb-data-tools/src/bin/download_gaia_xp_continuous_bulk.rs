//! Resumable downloader for the official Gaia DR3 XP continuous coefficient bulk.

use anyhow::{bail, Result};
use clap::Parser;
use nsb_data_tools::gaia_bulk::{BulkConfig, BulkDownloader, BulkPaths, BulkReport};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    about = "Download the official Gaia DR3 XP continuous coefficient bulk with MD5 validation"
)]
struct Args {
    #[arg(long)]
    download_dir: PathBuf,
    #[arg(long)]
    resume: bool,
    /// Deterministic prefix of the official inventory for pilots and tests.
    #[arg(long)]
    file_limit: Option<usize>,
    #[arg(long, default_value_t = 4)]
    concurrency: usize,
    #[arg(long, default_value_t = 3600)]
    timeout_secs: u64,
    #[arg(long, default_value_t = 30)]
    connect_timeout_secs: u64,
    #[arg(long, default_value_t = 6)]
    max_attempts: u32,
    #[arg(long, default_value_t = 1000)]
    initial_backoff_ms: u64,
    #[arg(long, default_value_t = 120)]
    max_backoff_secs: u64,
    #[arg(long, default_value_t = 30)]
    progress_interval_secs: u64,
    #[arg(long)]
    report_json: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if args.download_dir.as_os_str().is_empty() {
        bail!("download_dir must not be empty");
    }

    let config = BulkConfig {
        concurrency: args.concurrency,
        timeout: Duration::from_secs(args.timeout_secs),
        connect_timeout: Duration::from_secs(args.connect_timeout_secs),
        max_attempts: args.max_attempts,
        initial_backoff: Duration::from_millis(args.initial_backoff_ms),
        max_backoff: Duration::from_secs(args.max_backoff_secs),
        progress_interval: Duration::from_secs(args.progress_interval_secs),
        file_limit: args.file_limit,
    };
    let paths = BulkPaths::continuous(&args.download_dir);
    let downloader = BulkDownloader::continuous(config)?;
    let report = downloader.download(&paths, args.resume).await?;
    write_report(
        &args.report_json.unwrap_or_else(|| {
            args.download_dir
                .join("gaia_xp_continuous_bulk_report.json")
        }),
        &report,
    )?;
    report.ensure_complete()?;
    Ok(())
}

fn write_report(path: &PathBuf, report: &BulkReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    println!(
        "Gaia XP continuous bulk: {}/{} files complete, {:.2} MiB/s, report -> {}",
        report.completed_files,
        report.expected_files,
        report.throughput_bytes_per_second / (1024.0 * 1024.0),
        path.display()
    );
    Ok(())
}
