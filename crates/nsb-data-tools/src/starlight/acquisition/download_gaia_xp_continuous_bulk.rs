//! Resumable downloader for the official Gaia DR3 XP continuous coefficient bulk.

use anyhow::Result;
use clap::Parser;
use nsb_data_tools::gaia::acquisition::bulk_service::{
    run_continuous_bulk_download, ContinuousBulkDownloadConfig,
};
use std::num::NonZeroUsize;
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
    /// Deterministic non-zero prefix of the official inventory for pilots and tests.
    #[arg(long)]
    file_limit: Option<NonZeroUsize>,
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

/// Run the XP continuous bulk downloader with hierarchical command arguments.
pub fn run_cli() -> Result<()> {
    let args = crate::parse_command_args();
    tokio::runtime::Runtime::new()?.block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let report_path = args.report_json.clone().unwrap_or_else(|| {
        args.download_dir
            .join("gaia_xp_continuous_bulk_report.json")
    });
    let report = run_continuous_bulk_download(ContinuousBulkDownloadConfig {
        download_dir: args.download_dir,
        resume: args.resume,
        file_limit: args.file_limit,
        concurrency: args.concurrency,
        timeout: Duration::from_secs(args.timeout_secs),
        connect_timeout: Duration::from_secs(args.connect_timeout_secs),
        max_attempts: args.max_attempts,
        initial_backoff: Duration::from_millis(args.initial_backoff_ms),
        max_backoff: Duration::from_secs(args.max_backoff_secs),
        progress_interval: Duration::from_secs(args.progress_interval_secs),
        report_json: args.report_json,
    })
    .await?;
    println!(
        "Gaia XP continuous bulk: {}/{} files complete, {:.2} MiB/s, report -> {}",
        report.completed_files,
        report.expected_files,
        report.throughput_bytes_per_second / (1024.0 * 1024.0),
        report_path.display()
    );
    Ok(())
}
