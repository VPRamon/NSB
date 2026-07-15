//! Resumable downloader for the official Gaia DR3 XP continuous coefficient bulk.
//!
//! When USB cache arguments are supplied, downloads run through the rotating USB
//! cache state machine with vfat-safe size limits and transactional writes.

use anyhow::Result;
use clap::Parser;
use nsb_data_tools::gaia_bulk::{BulkConfig, BulkDownloader, BulkPaths, BulkReport};
use nsb_data_tools::gaia_usb_cache::UsbCacheLayout;
use nsb_data_tools::gaia_usb_cache_rotator::{
    filenames_for_download, UsbCacheRotator, UsbCacheRotatorConfig,
};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    about = "Download the official Gaia DR3 XP continuous coefficient bulk with MD5 validation"
)]
struct Args {
    #[arg(long)]
    download_dir: Option<PathBuf>,
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
    /// USB mountpoint for rotating cache (enables cache rotator when set with --usb-cache-root).
    #[arg(long)]
    usb_mountpoint: Option<PathBuf>,
    #[arg(long)]
    usb_cache_root: Option<PathBuf>,
    #[arg(long, default_value = "xp-continuous")]
    cache_subdir: String,
    #[arg(long, default_value_t = 20 * 1024 * 1024 * 1024)]
    max_cache_bytes: u64,
    #[arg(long, default_value_t = false)]
    init_usb_marker: bool,
    /// Download only this inventory filename (USB cache mode).
    #[arg(long)]
    only_filename: Option<String>,
}

#[derive(Debug, Serialize)]
struct UsbCacheDownloadReport {
    bulk_report: BulkReport,
    cache_sync: nsb_data_tools::gaia_usb_cache_rotator::UsbCacheSyncReport,
    session_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = Args::parse();
    apply_env_defaults(&mut args);

    let config = BulkConfig {
        concurrency: args.concurrency,
        timeout: Duration::from_secs(args.timeout_secs),
        connect_timeout: Duration::from_secs(args.connect_timeout_secs),
        max_attempts: args.max_attempts,
        initial_backoff: Duration::from_millis(args.initial_backoff_ms),
        max_backoff: Duration::from_secs(args.max_backoff_secs),
        progress_interval: Duration::from_secs(args.progress_interval_secs),
        file_limit: args.file_limit,
        filename_allowlist: None,
    };

    if let (Some(mount), Some(root)) = (&args.usb_mountpoint, &args.usb_cache_root) {
        return run_usb_cache_download(&args, config, mount, root).await;
    }

    let download_dir = args
        .download_dir
        .clone()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("download_dir is required when USB cache mode is not enabled")
        })?;

    let paths = BulkPaths::continuous(&download_dir);
    let downloader = BulkDownloader::continuous(config)?;
    let report = downloader.download(&paths, args.resume).await?;
    write_report(
        &args
            .report_json
            .unwrap_or_else(|| download_dir.join("gaia_xp_continuous_bulk_report.json")),
        &report,
    )?;
    report.ensure_complete()?;
    Ok(())
}

async fn run_usb_cache_download(
    args: &Args,
    config: BulkConfig,
    mount: &PathBuf,
    root: &PathBuf,
) -> Result<()> {
    let layout = UsbCacheLayout::from_env(mount, root, &args.cache_subdir);
    let mut rotator = UsbCacheRotator::prepare(UsbCacheRotatorConfig {
        layout: layout.clone(),
        max_cache_bytes: args.max_cache_bytes,
        init_usb_marker: args.init_usb_marker,
    })?;

    let session = rotator.write_session_manifest(args.file_limit, args.resume)?;
    let pending = if let Some(filename) = &args.only_filename {
        vec![filename.clone()]
    } else {
        filenames_for_download(&rotator.manifest, args.file_limit)
    };
    if pending.is_empty() {
        println!(
            "USB cache: no pending files (limit={:?}); manifest already satisfied",
            args.file_limit
        );
        return Ok(());
    }
    rotator.mark_files_downloading(&pending)?;

    let mut download_config = config;
    download_config.file_limit = None;
    download_config.filename_allowlist = Some(pending);
    let paths = rotator.bulk_paths();
    let downloader = BulkDownloader::continuous(download_config)?;
    let bulk_report = downloader.download(&paths, args.resume).await?;
    let cache_sync = rotator.apply_bulk_report(&bulk_report)?;

    let combined = UsbCacheDownloadReport {
        bulk_report: bulk_report.clone(),
        cache_sync: cache_sync.clone(),
        session_id: session.session_id,
    };

    let report_path = args.report_json.clone().unwrap_or_else(|| {
        layout
            .manifests_dir
            .join("gaia_xp_continuous_usb_cache_download_report.json")
    });
    write_usb_report(&report_path, &combined)?;

    println!(
        "USB cache download: {}/{} bulk files, {} checksum_verified, {} failed, footprint {} bytes -> {}",
        bulk_report.completed_files,
        bulk_report.expected_files,
        cache_sync.checksum_verified,
        cache_sync.failed,
        cache_sync.footprint_bytes,
        report_path.display()
    );

    if !cache_sync.passed {
        bail!("USB cache sync failed: {}", cache_sync.failures.join("; "));
    }
    if args.file_limit.is_none() {
        bulk_report.ensure_complete()?;
    }
    Ok(())
}

fn apply_env_defaults(args: &mut Args) {
    if args.usb_mountpoint.is_none() {
        args.usb_mountpoint = std::env::var_os("GAIA_USB_MOUNT").map(PathBuf::from);
    }
    if args.usb_cache_root.is_none() {
        args.usb_cache_root = std::env::var_os("GAIA_USB_ROOT").map(PathBuf::from);
    }
    if args.download_dir.is_none() {
        args.download_dir = std::env::var_os("GAIA_USB_CACHE").map(PathBuf::from);
    }
}

fn write_report(path: &PathBuf, report: &BulkReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn write_usb_report(path: &PathBuf, report: &UsbCacheDownloadReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
