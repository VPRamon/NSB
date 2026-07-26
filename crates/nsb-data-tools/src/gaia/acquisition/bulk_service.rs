//! Library service for the supported Gaia XP continuous bulk downloader.

use crate::gaia::acquisition::bulk::{BulkConfig, BulkDownloader, BulkPaths, BulkReport};
use crate::platform::artifact_io;
use anyhow::{bail, Result};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

/// Typed configuration for one Gaia XP continuous bulk acquisition run.
#[derive(Debug, Clone)]
pub struct ContinuousBulkDownloadConfig {
    /// Destination containing official partitions and manifests.
    pub download_dir: PathBuf,
    /// Reuse only checksum-verified completed files and resumable partials.
    pub resume: bool,
    /// Optional deterministic non-zero inventory prefix for pilots/tests.
    pub file_limit: Option<NonZeroUsize>,
    /// Maximum concurrent partition requests.
    pub concurrency: usize,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Connection timeout.
    pub connect_timeout: Duration,
    /// Maximum attempts per request.
    pub max_attempts: u32,
    /// Initial retry backoff.
    pub initial_backoff: Duration,
    /// Maximum retry backoff.
    pub max_backoff: Duration,
    /// Progress reporting interval.
    pub progress_interval: Duration,
    /// Optional report path; defaults inside `download_dir`.
    pub report_json: Option<PathBuf>,
}

impl ContinuousBulkDownloadConfig {
    /// Validate command-facing configuration before any filesystem or network work.
    pub fn validate(&self) -> Result<()> {
        if self.download_dir.as_os_str().is_empty() {
            bail!("download_dir must not be empty");
        }
        let bulk = self.bulk_config();
        bulk.validate()
    }

    /// Stable report path for this run.
    pub fn report_path(&self) -> PathBuf {
        self.report_json.clone().unwrap_or_else(|| {
            self.download_dir
                .join("gaia_xp_continuous_bulk_report.json")
        })
    }

    fn bulk_config(&self) -> BulkConfig {
        BulkConfig {
            concurrency: self.concurrency,
            timeout: self.timeout,
            connect_timeout: self.connect_timeout,
            max_attempts: self.max_attempts,
            initial_backoff: self.initial_backoff,
            max_backoff: self.max_backoff,
            progress_interval: self.progress_interval,
            file_limit: self.file_limit.map(NonZeroUsize::get),
            filename_allowlist: None,
        }
    }
}

/// Execute acquisition, atomically persist the typed report, and fail closed
/// unless every requested partition is checksum verified.
pub async fn run_continuous_bulk_download(
    config: ContinuousBulkDownloadConfig,
) -> Result<BulkReport> {
    config.validate()?;
    let paths = BulkPaths::continuous(&config.download_dir);
    let downloader = BulkDownloader::continuous(config.bulk_config())?;
    let report = downloader.download(&paths, config.resume).await?;
    artifact_io::write_json_atomic(&config.report_path(), &report)?;
    report.ensure_complete()?;
    Ok(report)
}
