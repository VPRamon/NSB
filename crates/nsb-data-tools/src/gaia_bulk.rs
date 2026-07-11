//! Restartable downloader for the official Gaia DR3 sampled-XP bulk files.
//!
//! The ESA checksum manifest is authoritative. Files are streamed to
//! `<filename>.part`, resumed with HTTP ranges, checked with the official MD5,
//! and only then atomically renamed to their final name. The output manifest is
//! deliberately written with every entry pending before downloads start so an
//! interrupted run cannot be mistaken for a complete one.
#![allow(missing_docs)]

use anyhow::{bail, Context, Result};
use futures_util::{stream, StreamExt};
use md5::{Digest, Md5};
use reqwest::header::{
    HeaderMap, ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE,
    RETRY_AFTER,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex as AsyncMutex;

pub const OFFICIAL_GAIA_XP_SAMPLED_BASE_URL: &str =
    "https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_sampled_mean_spectrum/";
pub const OFFICIAL_CHECKSUM_MANIFEST: &str = "_MD5SUM.txt";

const OUTPUT_MANIFEST_SCHEMA_VERSION: u32 = 1;
const MAX_CHECKSUM_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
const MAX_ERROR_BODY_BYTES: usize = 1024 * 1024;
const PREFIX_CAPTURE_BYTES: usize = 8 * 1024;
const REPRESENTATIVE_ERROR_LIMIT: usize = 10;

#[derive(Debug, Clone)]
pub struct BulkConfig {
    pub concurrency: usize,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub progress_interval: Duration,
    /// Deterministically restrict the run to the first N sorted inventory entries.
    /// Production callers must reject this option.
    pub file_limit: Option<usize>,
}

impl Default for BulkConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            timeout: Duration::from_secs(60 * 60),
            connect_timeout: Duration::from_secs(30),
            max_attempts: 6,
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(2 * 60),
            progress_interval: Duration::from_secs(30),
            file_limit: None,
        }
    }
}

impl BulkConfig {
    pub fn validate(&self) -> Result<()> {
        if self.concurrency == 0 {
            bail!("Gaia bulk concurrency must be positive");
        }
        if self.timeout.is_zero() || self.connect_timeout.is_zero() {
            bail!("Gaia bulk timeouts must be positive");
        }
        if self.max_attempts == 0 {
            bail!("Gaia bulk max attempts must be positive");
        }
        if self.initial_backoff.is_zero() || self.max_backoff < self.initial_backoff {
            bail!("Gaia bulk backoff must be positive and max >= initial");
        }
        if self.progress_interval.is_zero() {
            bail!("Gaia bulk progress interval must be positive");
        }
        if self.file_limit == Some(0) {
            bail!("Gaia bulk file limit must be positive when set");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BulkPaths {
    pub download_dir: PathBuf,
    pub error_dir: PathBuf,
    pub checksum_manifest_path: PathBuf,
    pub output_manifest_path: PathBuf,
}

impl BulkPaths {
    pub fn new(download_dir: impl Into<PathBuf>) -> Self {
        let download_dir = download_dir.into();
        Self {
            error_dir: download_dir.join("errors"),
            checksum_manifest_path: download_dir.join(OFFICIAL_CHECKSUM_MANIFEST),
            output_manifest_path: download_dir.join("gaia_xp_sampled_bulk_manifest.json"),
            download_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkFile {
    pub filename: String,
    pub md5: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkFileStatus {
    Pending,
    Downloaded,
    Resumed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkOutputFile {
    pub filename: String,
    pub official_md5: String,
    pub size_bytes: Option<u64>,
    /// Path relative to `BulkPaths::download_dir`.
    pub local_path: String,
    pub status: BulkFileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BulkOutputManifest {
    pub schema_version: u32,
    pub product: String,
    pub source_url: String,
    pub checksum_algorithm: String,
    pub inventory_total_files: usize,
    pub requested_files: usize,
    pub complete: bool,
    pub complete_inventory: bool,
    pub files: Vec<BulkOutputFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkReport {
    pub inventory_total_files: usize,
    pub requested_files: usize,
    pub expected_files: usize,
    pub completed_files: usize,
    pub resumed_files: usize,
    pub downloaded_files: usize,
    pub failed_files: usize,
    pub partial_files: usize,
    pub requests_total: usize,
    pub retries_total: usize,
    pub http_status_counts: BTreeMap<String, usize>,
    /// All response-body bytes received, including the checksum manifest and retries.
    pub bytes_downloaded: u64,
    /// Sum of the sizes of all checksum-validated local `.csv.gz` files.
    pub compressed_bytes_total: u64,
    pub elapsed_seconds: f64,
    pub throughput_files_per_second: f64,
    pub throughput_recent_files_per_second: f64,
    pub throughput_bytes_per_second: f64,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub latency_p99_ms: Option<f64>,
    pub representative_errors: Vec<String>,
    pub failed_filenames: Vec<String>,
    pub checksum_manifest_path: String,
    pub output_manifest_path: String,
    pub complete: bool,
    pub complete_inventory: bool,
}

impl BulkReport {
    pub fn ensure_complete(&self) -> Result<()> {
        if !self.complete {
            bail!(
                "Gaia bulk download incomplete: {}/{} complete, {} failed, {} partial files",
                self.completed_files,
                self.expected_files,
                self.failed_files,
                self.partial_files
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BulkDownloader {
    base_url: reqwest::Url,
    client: reqwest::Client,
    config: BulkConfig,
    limiter: Arc<AdaptiveRateLimiter>,
}

impl BulkDownloader {
    pub fn new(base_url: impl AsRef<str>, config: BulkConfig) -> Result<Self> {
        config.validate()?;
        let mut base = base_url.as_ref().to_string();
        if !base.ends_with('/') {
            base.push('/');
        }
        let base_url = reqwest::Url::parse(&base)
            .with_context(|| format!("invalid Gaia bulk base URL {base:?}"))?;
        if !matches!(base_url.scheme(), "http" | "https") {
            bail!("Gaia bulk base URL must use HTTP or HTTPS");
        }
        if base_url.query().is_some() || base_url.fragment().is_some() {
            bail!("Gaia bulk base URL must not contain a query or fragment");
        }

        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout)
            .read_timeout(config.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(config.concurrency)
            .tcp_keepalive(Duration::from_secs(30))
            .user_agent(concat!(
                "NSB/",
                env!("CARGO_PKG_VERSION"),
                " Gaia-DR3-bulk-release-tool"
            ))
            .build()
            .context("failed to build Gaia bulk HTTP client")?;

        Ok(Self {
            base_url,
            client,
            limiter: Arc::new(AdaptiveRateLimiter::new(config.concurrency as f64)),
            config,
        })
    }

    /// Download every entry in the official checksum manifest.
    ///
    /// This method is fail-closed: it writes the final manifest, including any
    /// failed entries, and then returns an error unless every expected file is
    /// checksum-valid and no corresponding `.part` remains.
    pub async fn download(&self, paths: &BulkPaths, resume: bool) -> Result<BulkReport> {
        prepare_paths(paths)?;
        let started = Instant::now();
        let metrics = Arc::new(Mutex::new(ProgressMetrics::new()));

        let inventory = self
            .fetch_checksum_manifest(paths, Arc::clone(&metrics))
            .await?;
        let inventory_total_files = inventory.len();
        let requested_files = inventory
            .iter()
            .take(self.config.file_limit.unwrap_or(inventory_total_files))
            .cloned()
            .collect::<Vec<_>>();
        {
            let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
            state.expected_files = requested_files.len();
        }

        atomic_write(
            &paths.checksum_manifest_path,
            canonical_checksum_manifest(&inventory).as_bytes(),
        )?;
        write_output_manifest(
            &paths.output_manifest_path,
            pending_output_manifest(&self.base_url, &requested_files, inventory_total_files),
        )?;

        let tasks = stream::iter(requested_files.iter().cloned())
            .map(|file| {
                let metrics = Arc::clone(&metrics);
                async move { self.download_one(file, paths, resume, metrics).await }
            })
            .buffer_unordered(self.config.concurrency);
        tokio::pin!(tasks);

        let mut outcomes = Vec::with_capacity(requested_files.len());
        let mut progress = tokio::time::interval(self.config.progress_interval);
        progress.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = progress.tick().await;
        loop {
            tokio::select! {
                outcome = tasks.next() => {
                    match outcome {
                        Some(outcome) => outcomes.push(outcome),
                        None => break,
                    }
                }
                _ = progress.tick() => {
                    let state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
                    eprintln!("{}", state.progress_line());
                }
            }
        }

        outcomes.sort_by(|left, right| left.file.filename.cmp(&right.file.filename));
        if outcomes.len() != requested_files.len() {
            bail!(
                "internal Gaia bulk error: expected {} outcomes, received {}",
                requested_files.len(),
                outcomes.len()
            );
        }

        let partial_files = requested_files
            .iter()
            .filter(|file| part_path(&paths.download_dir, &file.filename).exists())
            .count();
        let output_manifest = completed_output_manifest(
            &self.base_url,
            &outcomes,
            partial_files,
            inventory_total_files,
        );
        write_output_manifest(&paths.output_manifest_path, output_manifest)?;

        let elapsed = started.elapsed();
        let report = {
            let state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
            state.finish(
                elapsed,
                partial_files,
                &outcomes,
                paths,
                inventory_total_files,
            )
        };
        {
            let state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
            eprintln!("{}", state.progress_line());
        }

        if !report.complete {
            bail!(
                "Gaia bulk download incomplete: {}/{} checksum-valid, {} failed, {} partial; inspect {}",
                report.completed_files,
                report.expected_files,
                report.failed_files,
                report.partial_files,
                paths.output_manifest_path.display()
            );
        }
        Ok(report)
    }

    async fn fetch_checksum_manifest(
        &self,
        paths: &BulkPaths,
        metrics: Arc<Mutex<ProgressMetrics>>,
    ) -> Result<Vec<BulkFile>> {
        let url = self
            .base_url
            .join(OFFICIAL_CHECKSUM_MANIFEST)
            .context("failed to construct Gaia checksum-manifest URL")?;
        let mut last_error = String::new();

        for attempt in 1..=self.config.max_attempts {
            self.limiter.acquire().await;
            note_request(&metrics, attempt);
            let attempt_started = Instant::now();
            let response = self
                .client
                .get(url.clone())
                .header(ACCEPT, "text/plain")
                .header(ACCEPT_ENCODING, "identity")
                .send()
                .await;

            let failure = match response {
                Err(error) => {
                    note_attempt(&metrics, "transport", attempt_started.elapsed());
                    AttemptFailure {
                        retryable: true,
                        retry_after: None,
                        status: None,
                        headers: HeaderMap::new(),
                        body: Vec::new(),
                        detail: format!(
                            "manifest request attempt {attempt} transport error: {error}"
                        ),
                    }
                }
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    let headers = response.headers().clone();
                    let retry_after = parse_retry_after(&headers);
                    let collected =
                        collect_limited_body(&mut response, MAX_CHECKSUM_MANIFEST_BYTES, &metrics)
                            .await;
                    note_attempt(&metrics, &status.to_string(), attempt_started.elapsed());

                    if status == 200 && collected.error.is_none() && !collected.truncated {
                        match std::str::from_utf8(&collected.bytes)
                            .context("Gaia checksum manifest is not UTF-8")
                            .and_then(parse_md5_manifest)
                        {
                            Ok(files) => {
                                self.limiter.record_success().await;
                                return Ok(files);
                            }
                            Err(error) => AttemptFailure {
                                retryable: true,
                                retry_after,
                                status: Some(status),
                                headers,
                                body: collected.bytes,
                                detail: format!(
                                    "manifest request attempt {attempt} returned invalid content: {error:#}"
                                ),
                            },
                        }
                    } else {
                        let detail = if let Some(ref error) = collected.error {
                            format!("manifest request attempt {attempt} body read failed: {error}")
                        } else if collected.truncated {
                            format!(
                                "manifest request attempt {attempt} exceeded {} bytes",
                                MAX_CHECKSUM_MANIFEST_BYTES
                            )
                        } else {
                            format!("manifest request attempt {attempt} returned HTTP {status}")
                        };
                        AttemptFailure {
                            retryable: is_retryable_status(status)
                                || status == 200
                                || collected.error.is_some(),
                            retry_after,
                            status: Some(status),
                            headers,
                            body: collected.bytes,
                            detail,
                        }
                    }
                }
            };

            last_error = failure.detail.clone();
            note_attempt_error(&metrics);
            let persisted = persist_attempt_error(
                &paths.error_dir,
                "_MD5SUM",
                attempt,
                failure.status,
                &failure.headers,
                &failure.body,
                &failure.detail,
            );
            note_persisted_error(&metrics, persisted);

            if matches!(failure.status, Some(429 | 503)) {
                self.limiter.record_throttle().await;
            }
            if !failure.retryable || attempt == self.config.max_attempts {
                break;
            }
            sleep_before_retry(
                &metrics,
                failure.retry_after,
                self.retry_delay(OFFICIAL_CHECKSUM_MANIFEST, attempt),
            )
            .await;
        }

        bail!("failed to fetch authoritative Gaia checksum manifest from {url}: {last_error}")
    }

    async fn download_one(
        &self,
        file: BulkFile,
        paths: &BulkPaths,
        resume: bool,
        metrics: Arc<Mutex<ProgressMetrics>>,
    ) -> FileOutcome {
        let final_path = paths.download_dir.join(&file.filename);
        let part_path = part_path(&paths.download_dir, &file.filename);

        match prepare_existing_file(
            &file,
            &final_path,
            &part_path,
            &paths.error_dir,
            resume,
            &metrics,
        ) {
            Ok(Some((status, size))) => {
                note_file_complete(&metrics, status, size);
                return FileOutcome {
                    file,
                    status,
                    size_bytes: Some(size),
                    error: None,
                };
            }
            Ok(None) => {}
            Err(error) => {
                note_file_failed(&metrics);
                return FileOutcome {
                    file,
                    status: BulkFileStatus::Failed,
                    size_bytes: None,
                    error: Some(format!("{error:#}")),
                };
            }
        }

        let url = match self.base_url.join(&file.filename) {
            Ok(url) => url,
            Err(error) => {
                note_file_failed(&metrics);
                return FileOutcome {
                    file,
                    status: BulkFileStatus::Failed,
                    size_bytes: None,
                    error: Some(format!("failed to construct download URL: {error}")),
                };
            }
        };
        let mut last_error = String::new();

        for attempt in 1..=self.config.max_attempts {
            self.limiter.acquire().await;
            note_request(&metrics, attempt);
            let attempt_started = Instant::now();
            let outcome = self
                .perform_file_attempt(&file, &url, &final_path, &part_path, Arc::clone(&metrics))
                .await;

            match outcome {
                FileAttemptOutcome::Complete { size, status } => {
                    note_attempt(&metrics, &status.to_string(), attempt_started.elapsed());
                    self.limiter.record_success().await;
                    note_file_complete(&metrics, BulkFileStatus::Downloaded, size);
                    return FileOutcome {
                        file,
                        status: BulkFileStatus::Downloaded,
                        size_bytes: Some(size),
                        error: None,
                    };
                }
                FileAttemptOutcome::Failure(failure) => {
                    let status_key = failure
                        .status
                        .map(|status| status.to_string())
                        .unwrap_or_else(|| "transport".to_string());
                    note_attempt(&metrics, &status_key, attempt_started.elapsed());
                    note_attempt_error(&metrics);
                    last_error = failure.detail.clone();
                    let persisted = persist_attempt_error(
                        &paths.error_dir,
                        &file.filename,
                        attempt,
                        failure.status,
                        &failure.headers,
                        &failure.body,
                        &failure.detail,
                    );
                    note_persisted_error(&metrics, persisted);

                    if matches!(failure.status, Some(429 | 503)) {
                        self.limiter.record_throttle().await;
                    }
                    if !failure.retryable || attempt == self.config.max_attempts {
                        break;
                    }
                    sleep_before_retry(
                        &metrics,
                        failure.retry_after,
                        self.retry_delay(&file.filename, attempt),
                    )
                    .await;
                }
            }
        }

        note_file_failed(&metrics);
        FileOutcome {
            file,
            status: BulkFileStatus::Failed,
            size_bytes: None,
            error: Some(last_error),
        }
    }

    async fn perform_file_attempt(
        &self,
        file: &BulkFile,
        url: &reqwest::Url,
        final_path: &Path,
        part_path: &Path,
        metrics: Arc<Mutex<ProgressMetrics>>,
    ) -> FileAttemptOutcome {
        let offset = match fs::metadata(part_path) {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => {
                return failure_without_response(
                    false,
                    format!("partial path {} is not a regular file", part_path.display()),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => {
                return failure_without_response(
                    false,
                    format!(
                        "failed to inspect partial file {}: {error}",
                        part_path.display()
                    ),
                );
            }
        };

        let mut request = self
            .client
            .get(url.clone())
            .header(ACCEPT, "application/gzip, application/octet-stream, */*")
            .header(ACCEPT_ENCODING, "identity");
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let mut response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return failure_without_response(
                    true,
                    format!(
                        "file={} offset={offset} transport error: {error}",
                        file.filename
                    ),
                );
            }
        };
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let retry_after = parse_retry_after(&headers);

        if status == 416 {
            let collected =
                collect_limited_body(&mut response, MAX_ERROR_BODY_BYTES, &metrics).await;
            return handle_range_not_satisfiable(
                file, final_path, part_path, offset, headers, collected,
            );
        }

        if !matches!(status, 200 | 206) {
            let collected =
                collect_limited_body(&mut response, MAX_ERROR_BODY_BYTES, &metrics).await;
            let body_error = collected
                .error
                .as_deref()
                .map(|error| format!("; body read error: {error}"))
                .unwrap_or_default();
            let truncated = if collected.truncated {
                format!("; body truncated at {} bytes", collected.bytes.len())
            } else {
                String::new()
            };
            return FileAttemptOutcome::Failure(AttemptFailure {
                retryable: is_retryable_status(status) || collected.error.is_some(),
                retry_after,
                status: Some(status),
                headers,
                body: collected.bytes,
                detail: format!(
                    "file={} offset={offset} HTTP {status}{body_error}{truncated}",
                    file.filename
                ),
            });
        }

        if let Some(encoding) = headers.get(CONTENT_ENCODING) {
            if !encoding
                .to_str()
                .is_ok_and(|value| value.eq_ignore_ascii_case("identity"))
            {
                return FileAttemptOutcome::Failure(AttemptFailure {
                    retryable: false,
                    retry_after,
                    status: Some(status),
                    headers,
                    body: Vec::new(),
                    detail: format!(
                        "file={} HTTP {status} used unsupported Content-Encoding; raw gzip bytes are required for MD5 validation",
                        file.filename
                    ),
                });
            }
        }

        let response_length = parse_content_length(&headers);
        let (write_offset, expected_total, expected_response_bytes, append) = if status == 206 {
            let content_range = match headers
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .map(parse_content_range)
            {
                Some(Ok(ContentRange::Satisfied { start, end, total })) => {
                    if start != offset {
                        return protocol_failure(
                            status,
                            headers,
                            retry_after,
                            format!(
                                "file={} requested range start {offset}, server returned {start}",
                                file.filename
                            ),
                        );
                    }
                    let span = end - start + 1;
                    if response_length.is_some_and(|length| length != span) {
                        return protocol_failure(
                            status,
                            headers,
                            retry_after,
                            format!(
                                "file={} Content-Length does not match Content-Range span {span}",
                                file.filename
                            ),
                        );
                    }
                    (offset, Some(total), Some(span), true)
                }
                Some(Ok(ContentRange::Unsatisfied { .. })) => {
                    return protocol_failure(
                        status,
                        headers,
                        retry_after,
                        format!(
                            "file={} returned an unsatisfied Content-Range with HTTP 206",
                            file.filename
                        ),
                    );
                }
                Some(Err(error)) => {
                    return protocol_failure(
                        status,
                        headers,
                        retry_after,
                        format!("file={} invalid Content-Range: {error:#}", file.filename),
                    );
                }
                None => {
                    return protocol_failure(
                        status,
                        headers,
                        retry_after,
                        format!("file={} HTTP 206 omitted Content-Range", file.filename),
                    );
                }
            };
            content_range
        } else {
            if headers.contains_key(CONTENT_RANGE) {
                return protocol_failure(
                    status,
                    headers,
                    retry_after,
                    format!(
                        "file={} HTTP 200 unexpectedly included Content-Range",
                        file.filename
                    ),
                );
            }
            // A 200 response to a Range request means the server ignored Range.
            // Restarting from byte zero is safe; appending would corrupt the file.
            (0, response_length, response_length, false)
        };

        let mut output = if append {
            match OpenOptions::new().create(true).append(true).open(part_path) {
                Ok(file) => file,
                Err(error) => {
                    return failure_with_response(
                        false,
                        status,
                        headers,
                        retry_after,
                        Vec::new(),
                        format!("failed to append {}: {error}", part_path.display()),
                    );
                }
            }
        } else {
            match File::create(part_path) {
                Ok(file) => file,
                Err(error) => {
                    return failure_with_response(
                        false,
                        status,
                        headers,
                        retry_after,
                        Vec::new(),
                        format!("failed to create {}: {error}", part_path.display()),
                    );
                }
            }
        };

        if append
            && fs::metadata(part_path)
                .map(|metadata| metadata.len())
                .unwrap_or(u64::MAX)
                != write_offset
        {
            return failure_with_response(
                false,
                status,
                headers,
                retry_after,
                Vec::new(),
                format!(
                    "partial file {} changed while opening it",
                    part_path.display()
                ),
            );
        }

        let mut received = 0_u64;
        let mut prefix = Vec::with_capacity(PREFIX_CAPTURE_BYTES);
        loop {
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(error) => {
                    let _ = output.flush();
                    let _ = output.sync_all();
                    return failure_with_response(
                        true,
                        status,
                        headers,
                        retry_after,
                        prefix,
                        format!(
                            "file={} offset={} response body failed after {} bytes: {error}",
                            file.filename, write_offset, received
                        ),
                    );
                }
            };
            note_bytes(&metrics, chunk.len() as u64);
            received = match received.checked_add(chunk.len() as u64) {
                Some(value) => value,
                None => {
                    return failure_with_response(
                        false,
                        status,
                        headers,
                        retry_after,
                        prefix,
                        format!("file={} response size overflow", file.filename),
                    );
                }
            };
            if prefix.len() < PREFIX_CAPTURE_BYTES {
                let take = (PREFIX_CAPTURE_BYTES - prefix.len()).min(chunk.len());
                prefix.extend_from_slice(&chunk[..take]);
            }
            if let Err(error) = output.write_all(&chunk) {
                return failure_with_response(
                    false,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!("failed writing {}: {error}", part_path.display()),
                );
            }
            if contains_service_error(&prefix) {
                drop(output);
                let _ = truncate_file(part_path, write_offset);
                return failure_with_response(
                    true,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!(
                        "file={} returned a SERVICE ERROR body with HTTP {status}",
                        file.filename
                    ),
                );
            }
        }

        if let Err(error) = output.flush().and_then(|_| output.sync_all()) {
            return failure_with_response(
                false,
                status,
                headers,
                retry_after,
                prefix,
                format!("failed to sync {}: {error}", part_path.display()),
            );
        }
        drop(output);

        if let Some(expected) = expected_response_bytes {
            if received != expected {
                if received > expected {
                    let _ = fs::remove_file(part_path);
                }
                return failure_with_response(
                    true,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!(
                        "file={} response length mismatch: expected {expected}, received {received}",
                        file.filename
                    ),
                );
            }
        }

        let size = match fs::metadata(part_path) {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                return failure_with_response(
                    false,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!("failed to stat {}: {error}", part_path.display()),
                );
            }
        };
        if let Some(total) = expected_total {
            if size < total {
                return failure_with_response(
                    true,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!(
                        "file={} partial range complete at {size} of {total} bytes",
                        file.filename
                    ),
                );
            }
            if size > total {
                let _ = fs::remove_file(part_path);
                return failure_with_response(
                    true,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!(
                        "file={} local size {size} exceeds server total {total}",
                        file.filename
                    ),
                );
            }
        }

        let actual_md5 = match md5_file(part_path) {
            Ok(md5) => md5,
            Err(error) => {
                return failure_with_response(
                    false,
                    status,
                    headers,
                    retry_after,
                    prefix,
                    format!("failed to checksum {}: {error:#}", part_path.display()),
                );
            }
        };
        if actual_md5 != file.md5 {
            // When the response declared a complete representation, the part
            // cannot be resumed into the expected object. Start the next attempt
            // from zero. Without a declared total, retain it and let Range/416
            // establish whether it was merely truncated.
            if expected_total.is_some() {
                let _ = fs::remove_file(part_path);
            }
            return failure_with_response(
                true,
                status,
                headers,
                retry_after,
                prefix,
                format!(
                    "file={} MD5 mismatch: expected {}, got {}",
                    file.filename, file.md5, actual_md5
                ),
            );
        }

        if let Err(error) = promote_part(part_path, final_path) {
            return failure_with_response(
                false,
                status,
                headers,
                retry_after,
                prefix,
                format!("file={} promotion failed: {error:#}", file.filename),
            );
        }
        FileAttemptOutcome::Complete { size, status }
    }

    fn retry_delay(&self, key: &str, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        let base_ms = self
            .config
            .initial_backoff
            .as_millis()
            .saturating_mul(1_u128 << exponent)
            .min(self.config.max_backoff.as_millis());
        let hash = key.bytes().fold(u64::from(attempt), |acc, byte| {
            acc.wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(byte))
        });
        let jitter_permille = 800 + hash % 401;
        let millis = base_ms
            .saturating_mul(u128::from(jitter_permille))
            .saturating_div(1000)
            .min(self.config.max_backoff.as_millis());
        Duration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
    }
}

#[derive(Debug)]
struct FileOutcome {
    file: BulkFile,
    status: BulkFileStatus,
    size_bytes: Option<u64>,
    error: Option<String>,
}

enum FileAttemptOutcome {
    Complete { size: u64, status: u16 },
    Failure(AttemptFailure),
}

struct AttemptFailure {
    retryable: bool,
    retry_after: Option<Duration>,
    status: Option<u16>,
    headers: HeaderMap,
    body: Vec<u8>,
    detail: String,
}

fn failure_without_response(retryable: bool, detail: String) -> FileAttemptOutcome {
    FileAttemptOutcome::Failure(AttemptFailure {
        retryable,
        retry_after: None,
        status: None,
        headers: HeaderMap::new(),
        body: Vec::new(),
        detail,
    })
}

fn failure_with_response(
    retryable: bool,
    status: u16,
    headers: HeaderMap,
    retry_after: Option<Duration>,
    body: Vec<u8>,
    detail: String,
) -> FileAttemptOutcome {
    FileAttemptOutcome::Failure(AttemptFailure {
        retryable,
        retry_after,
        status: Some(status),
        headers,
        body,
        detail,
    })
}

fn protocol_failure(
    status: u16,
    headers: HeaderMap,
    retry_after: Option<Duration>,
    detail: String,
) -> FileAttemptOutcome {
    failure_with_response(true, status, headers, retry_after, Vec::new(), detail)
}

fn handle_range_not_satisfiable(
    file: &BulkFile,
    final_path: &Path,
    part_path: &Path,
    offset: u64,
    headers: HeaderMap,
    collected: CollectedBody,
) -> FileAttemptOutcome {
    let parsed = headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .map(parse_content_range);
    let Some(Ok(ContentRange::Unsatisfied { total })) = parsed else {
        return failure_with_response(
            true,
            416,
            headers,
            None,
            collected.bytes,
            format!(
                "file={} HTTP 416 omitted a valid unsatisfied Content-Range",
                file.filename
            ),
        );
    };
    if offset == total && offset > 0 {
        match md5_file(part_path) {
            Ok(actual) if actual == file.md5 => {
                if let Err(error) = promote_part(part_path, final_path) {
                    return failure_with_response(
                        false,
                        416,
                        headers,
                        None,
                        collected.bytes,
                        format!("file={} promotion failed: {error:#}", file.filename),
                    );
                }
                return FileAttemptOutcome::Complete {
                    size: total,
                    status: 416,
                };
            }
            Ok(actual) => {
                let _ = fs::remove_file(part_path);
                return failure_with_response(
                    true,
                    416,
                    headers,
                    None,
                    collected.bytes,
                    format!(
                        "file={} complete partial failed MD5: expected {}, got {actual}",
                        file.filename, file.md5
                    ),
                );
            }
            Err(error) => {
                return failure_with_response(
                    false,
                    416,
                    headers,
                    None,
                    collected.bytes,
                    format!("file={} partial checksum failed: {error:#}", file.filename),
                );
            }
        }
    }

    let _ = fs::remove_file(part_path);
    failure_with_response(
        true,
        416,
        headers,
        None,
        collected.bytes,
        format!(
            "file={} partial offset {offset} disagrees with server total {total}; restarting",
            file.filename
        ),
    )
}

fn prepare_existing_file(
    file: &BulkFile,
    final_path: &Path,
    part_path: &Path,
    error_dir: &Path,
    resume: bool,
    metrics: &Arc<Mutex<ProgressMetrics>>,
) -> Result<Option<(BulkFileStatus, u64)>> {
    if !resume {
        remove_if_exists(final_path)?;
        remove_if_exists(part_path)?;
        return Ok(None);
    }

    if final_path.exists() {
        let actual = md5_file(final_path)?;
        if actual == file.md5 {
            if part_path.exists() {
                fs::remove_file(part_path).with_context(|| {
                    format!("failed to remove stale partial {}", part_path.display())
                })?;
            }
            let size = fs::metadata(final_path)?.len();
            return Ok(Some((BulkFileStatus::Resumed, size)));
        }
        let detail = format!(
            "existing file MD5 mismatch: expected {}, got {actual}; removing {}",
            file.md5,
            final_path.display()
        );
        let persisted = persist_attempt_error(
            error_dir,
            &file.filename,
            0,
            None,
            &HeaderMap::new(),
            detail.as_bytes(),
            &detail,
        );
        note_persisted_error(metrics, persisted);
        fs::remove_file(final_path)
            .with_context(|| format!("failed to remove invalid {}", final_path.display()))?;
    }

    if part_path.exists() {
        let actual = md5_file(part_path)?;
        if actual == file.md5 {
            let size = fs::metadata(part_path)?.len();
            promote_part(part_path, final_path)?;
            return Ok(Some((BulkFileStatus::Resumed, size)));
        }
    }
    Ok(None)
}

fn prepare_paths(paths: &BulkPaths) -> Result<()> {
    fs::create_dir_all(&paths.download_dir).with_context(|| {
        format!(
            "failed to create Gaia bulk download directory {}",
            paths.download_dir.display()
        )
    })?;
    fs::create_dir_all(&paths.error_dir).with_context(|| {
        format!(
            "failed to create Gaia bulk error directory {}",
            paths.error_dir.display()
        )
    })?;
    for path in [&paths.checksum_manifest_path, &paths.output_manifest_path] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    Ok(())
}

pub fn parse_md5_manifest(text: &str) -> Result<Vec<BulkFile>> {
    let mut files = Vec::new();
    let mut names = BTreeSet::new();
    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let split_at = line
            .find(char::is_whitespace)
            .with_context(|| format!("manifest line {line_number} has no filename"))?;
        let (checksum, remainder) = line.split_at(split_at);
        if checksum.len() != 32 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("manifest line {line_number} has an invalid MD5 checksum");
        }
        let mut filename = remainder.trim_start();
        if let Some(stripped) = filename.strip_prefix('*') {
            filename = stripped;
        }
        if filename.is_empty() || filename.trim() != filename {
            bail!("manifest line {line_number} has an invalid filename");
        }
        validate_manifest_filename(filename)
            .with_context(|| format!("invalid filename on manifest line {line_number}"))?;
        if !names.insert(filename.to_string()) {
            bail!("duplicate filename {filename:?} in checksum manifest");
        }
        files.push(BulkFile {
            filename: filename.to_string(),
            md5: checksum.to_ascii_lowercase(),
        });
    }
    if files.is_empty() {
        bail!("Gaia checksum manifest is empty");
    }
    files.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(files)
}

fn validate_manifest_filename(filename: &str) -> Result<()> {
    if filename == OFFICIAL_CHECKSUM_MANIFEST
        || filename.bytes().any(|byte| byte.is_ascii_control())
        || filename.contains(['/', '\\'])
    {
        bail!("unsafe manifest filename {filename:?}");
    }
    let path = Path::new(filename);
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("manifest filename must be one normal path component");
    }
    Ok(())
}

fn canonical_checksum_manifest(files: &[BulkFile]) -> String {
    let mut output = String::new();
    for file in files {
        output.push_str(&file.md5);
        output.push_str("  ");
        output.push_str(&file.filename);
        output.push('\n');
    }
    output
}

pub fn md5_file(path: &Path) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("failed to open {} for MD5", path.display()))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Md5::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("failed reading {} for MD5", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentRange {
    Satisfied { start: u64, end: u64, total: u64 },
    Unsatisfied { total: u64 },
}

fn parse_content_range(value: &str) -> Result<ContentRange> {
    let value = value.trim();
    let remainder = value
        .strip_prefix("bytes ")
        .context("Content-Range must start with `bytes `")?;
    if let Some(total) = remainder.strip_prefix("*/") {
        let total = total
            .parse::<u64>()
            .context("invalid unsatisfied Content-Range total")?;
        return Ok(ContentRange::Unsatisfied { total });
    }
    let (range, total) = remainder
        .split_once('/')
        .context("Content-Range is missing total size")?;
    let (start, end) = range
        .split_once('-')
        .context("Content-Range is missing byte bounds")?;
    let start = start
        .parse::<u64>()
        .context("invalid Content-Range start")?;
    let end = end.parse::<u64>().context("invalid Content-Range end")?;
    let total = total
        .parse::<u64>()
        .context("invalid Content-Range total")?;
    if start > end || end >= total {
        bail!("Content-Range bounds are inconsistent");
    }
    Ok(ContentRange::Satisfied { start, end, total })
}

fn parse_content_length(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let deadline = httpdate::parse_http_date(value).ok()?;
    Some(
        deadline
            .duration_since(SystemTime::now())
            .unwrap_or_default(),
    )
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

fn contains_service_error(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    text.to_ascii_uppercase().contains("SERVICE ERROR")
}

struct CollectedBody {
    bytes: Vec<u8>,
    truncated: bool,
    error: Option<String>,
}

async fn collect_limited_body(
    response: &mut reqwest::Response,
    limit: usize,
    metrics: &Arc<Mutex<ProgressMetrics>>,
) -> CollectedBody {
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                note_bytes(metrics, chunk.len() as u64);
                let remaining = limit.saturating_sub(bytes.len());
                let take = remaining.min(chunk.len());
                bytes.extend_from_slice(&chunk[..take]);
                if take < chunk.len() {
                    return CollectedBody {
                        bytes,
                        truncated: true,
                        error: None,
                    };
                }
            }
            Ok(None) => {
                return CollectedBody {
                    bytes,
                    truncated: false,
                    error: None,
                };
            }
            Err(error) => {
                return CollectedBody {
                    bytes,
                    truncated: false,
                    error: Some(error.to_string()),
                };
            }
        }
    }
}

fn pending_output_manifest(
    base_url: &reqwest::Url,
    files: &[BulkFile],
    inventory_total_files: usize,
) -> BulkOutputManifest {
    BulkOutputManifest {
        schema_version: OUTPUT_MANIFEST_SCHEMA_VERSION,
        product: "Gaia DR3 XP sampled mean spectrum".to_string(),
        source_url: base_url.as_str().to_string(),
        checksum_algorithm: "MD5 (official ESA manifest)".to_string(),
        inventory_total_files,
        requested_files: files.len(),
        complete: false,
        complete_inventory: false,
        files: files
            .iter()
            .map(|file| BulkOutputFile {
                filename: file.filename.clone(),
                official_md5: file.md5.clone(),
                size_bytes: None,
                local_path: file.filename.clone(),
                status: BulkFileStatus::Pending,
            })
            .collect(),
    }
}

fn completed_output_manifest(
    base_url: &reqwest::Url,
    outcomes: &[FileOutcome],
    partial_files: usize,
    inventory_total_files: usize,
) -> BulkOutputManifest {
    let files = outcomes
        .iter()
        .map(|outcome| BulkOutputFile {
            filename: outcome.file.filename.clone(),
            official_md5: outcome.file.md5.clone(),
            size_bytes: outcome.size_bytes,
            local_path: outcome.file.filename.clone(),
            status: outcome.status,
        })
        .collect::<Vec<_>>();
    let complete = partial_files == 0
        && files.iter().all(|file| {
            matches!(
                file.status,
                BulkFileStatus::Downloaded | BulkFileStatus::Resumed
            )
        });
    BulkOutputManifest {
        schema_version: OUTPUT_MANIFEST_SCHEMA_VERSION,
        product: "Gaia DR3 XP sampled mean spectrum".to_string(),
        source_url: base_url.as_str().to_string(),
        checksum_algorithm: "MD5 (official ESA manifest)".to_string(),
        inventory_total_files,
        requested_files: files.len(),
        complete,
        complete_inventory: complete && files.len() == inventory_total_files,
        files,
    }
}

fn write_output_manifest(path: &Path, manifest: BulkOutputManifest) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("atomic output path has no UTF-8 filename")?;
    let part = path.with_file_name(format!("{file_name}.part"));
    remove_if_exists(&part)?;
    let mut output =
        File::create(&part).with_context(|| format!("failed to create {}", part.display()))?;
    output
        .write_all(bytes)
        .with_context(|| format!("failed to write {}", part.display()))?;
    output
        .sync_all()
        .with_context(|| format!("failed to sync {}", part.display()))?;
    drop(output);
    fs::rename(&part, path).with_context(|| {
        format!(
            "failed atomic rename {} -> {}",
            part.display(),
            path.display()
        )
    })?;
    sync_parent(path)
}

fn promote_part(part_path: &Path, final_path: &Path) -> Result<()> {
    if final_path.exists() {
        bail!(
            "refusing to replace existing final file {} during promotion",
            final_path.display()
        );
    }
    fs::rename(part_path, final_path).with_context(|| {
        format!(
            "failed atomic rename {} -> {}",
            part_path.display(),
            final_path.display()
        )
    })?;
    sync_parent(final_path)
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("path has no parent directory")?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| format!("failed to sync directory {}", parent.display()))
}

fn truncate_file(path: &Path, size: u64) -> Result<()> {
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(size)?;
    file.sync_all()?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn part_path(download_dir: &Path, filename: &str) -> PathBuf {
    download_dir.join(format!("{filename}.part"))
}

fn persist_attempt_error(
    error_dir: &Path,
    stem: &str,
    attempt: u32,
    status: Option<u16>,
    headers: &HeaderMap,
    body: &[u8],
    detail: &str,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(error_dir)?;
    let prefix = format!("{stem}.attempt_{attempt:02}");
    let headers_path = error_dir.join(format!("{prefix}.headers.txt"));
    let body_path = error_dir.join(format!("{prefix}.body"));

    let mut header_lines = headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{}: {}",
                name.as_str(),
                value.to_str().unwrap_or("<non-UTF8>")
            )
        })
        .collect::<Vec<_>>();
    header_lines.sort();
    let mut metadata = format!(
        "attempt={attempt}\nstatus={}\ndetail={}\n",
        status
            .map(|value| value.to_string())
            .unwrap_or_else(|| "transport/local".to_string()),
        detail.replace(['\r', '\n'], " ")
    );
    for line in header_lines {
        metadata.push_str(&line);
        metadata.push('\n');
    }
    fs::write(&headers_path, metadata)?;
    fs::write(&body_path, body)?;
    Ok(vec![headers_path, body_path])
}

fn note_persisted_error(metrics: &Arc<Mutex<ProgressMetrics>>, result: Result<Vec<PathBuf>>) {
    let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
    match result {
        Ok(paths) => {
            for path in paths {
                state.note_error(path.display().to_string());
            }
        }
        Err(error) => state.note_error(format!("failed to persist error evidence: {error:#}")),
    }
}

fn note_request(metrics: &Arc<Mutex<ProgressMetrics>>, attempt: u32) {
    let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
    state.requests_total += 1;
    if attempt > 1 {
        state.retry_attempts_total += 1;
    }
}

fn note_attempt(metrics: &Arc<Mutex<ProgressMetrics>>, status: &str, latency: Duration) {
    let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
    *state
        .http_status_counts
        .entry(status.to_string())
        .or_default() += 1;
    state.latencies.push(latency);
}

fn note_attempt_error(metrics: &Arc<Mutex<ProgressMetrics>>) {
    metrics
        .lock()
        .expect("Gaia bulk metrics mutex poisoned")
        .attempt_errors += 1;
}

fn note_bytes(metrics: &Arc<Mutex<ProgressMetrics>>, count: u64) {
    let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
    state.bytes_downloaded = state.bytes_downloaded.saturating_add(count);
}

fn note_file_complete(metrics: &Arc<Mutex<ProgressMetrics>>, status: BulkFileStatus, size: u64) {
    let mut state = metrics.lock().expect("Gaia bulk metrics mutex poisoned");
    state.completed_files += 1;
    if status == BulkFileStatus::Resumed {
        state.resumed_files += 1;
    } else {
        state.downloaded_files += 1;
    }
    state.compressed_bytes_total = state.compressed_bytes_total.saturating_add(size);
    state.note_completion();
}

fn note_file_failed(metrics: &Arc<Mutex<ProgressMetrics>>) {
    metrics
        .lock()
        .expect("Gaia bulk metrics mutex poisoned")
        .failed_files += 1;
}

async fn sleep_before_retry(
    _metrics: &Arc<Mutex<ProgressMetrics>>,
    retry_after: Option<Duration>,
    backoff: Duration,
) {
    let delay = retry_after.map_or(backoff, |server| server.max(backoff));
    tokio::time::sleep(delay).await;
}

#[derive(Debug)]
struct ProgressMetrics {
    expected_files: usize,
    completed_files: usize,
    resumed_files: usize,
    downloaded_files: usize,
    failed_files: usize,
    attempt_errors: usize,
    requests_total: usize,
    retry_attempts_total: usize,
    http_status_counts: BTreeMap<String, usize>,
    bytes_downloaded: u64,
    compressed_bytes_total: u64,
    started: Instant,
    completion_times: VecDeque<Instant>,
    latencies: Vec<Duration>,
    representative_errors: Vec<String>,
}

impl ProgressMetrics {
    fn new() -> Self {
        Self {
            expected_files: 0,
            completed_files: 0,
            resumed_files: 0,
            downloaded_files: 0,
            failed_files: 0,
            attempt_errors: 0,
            requests_total: 0,
            retry_attempts_total: 0,
            http_status_counts: BTreeMap::new(),
            bytes_downloaded: 0,
            compressed_bytes_total: 0,
            started: Instant::now(),
            completion_times: VecDeque::new(),
            latencies: Vec::new(),
            representative_errors: Vec::new(),
        }
    }

    fn note_completion(&mut self) {
        let now = Instant::now();
        self.completion_times.push_back(now);
        while self
            .completion_times
            .front()
            .is_some_and(|time| now.duration_since(*time) > Duration::from_secs(60))
        {
            self.completion_times.pop_front();
        }
    }

    fn recent_throughput(&self) -> f64 {
        let Some(first) = self.completion_times.front() else {
            return 0.0;
        };
        let seconds = Instant::now().duration_since(*first).as_secs_f64().max(1.0);
        self.completion_times.len() as f64 / seconds
    }

    fn progress_line(&self) -> String {
        let elapsed = self.started.elapsed();
        let percent = if self.expected_files == 0 {
            0.0
        } else {
            100.0 * self.completed_files as f64 / self.expected_files as f64
        };
        let throughput = self.downloaded_files as f64 / elapsed.as_secs_f64().max(0.001);
        let remaining = self.expected_files.saturating_sub(self.completed_files);
        let eta = if throughput > 0.0 {
            format_duration(Duration::from_secs_f64(remaining as f64 / throughput))
        } else {
            "unknown".to_string()
        };
        let mib_per_second =
            self.bytes_downloaded as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64().max(0.001);
        format!(
            "{}/{} | {:.2}% | {:.2} files/s | {:.2} MiB/s | retries {} | errors {} | elapsed {} | ETA {} | validated {}",
            self.completed_files,
            self.expected_files,
            percent,
            throughput,
            mib_per_second,
            self.retry_attempts_total,
            self.attempt_errors + self.failed_files,
            format_duration(elapsed),
            eta,
            human_bytes(self.compressed_bytes_total),
        )
    }

    fn note_error(&mut self, error: String) {
        if self.representative_errors.len() < REPRESENTATIVE_ERROR_LIMIT
            && !self.representative_errors.contains(&error)
        {
            self.representative_errors.push(error);
        }
    }

    fn finish(
        &self,
        elapsed: Duration,
        partial_files: usize,
        outcomes: &[FileOutcome],
        paths: &BulkPaths,
        inventory_total_files: usize,
    ) -> BulkReport {
        let mut latencies = self
            .latencies
            .iter()
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .collect::<Vec<_>>();
        latencies.sort_by(f64::total_cmp);
        let mut representative_errors = self.representative_errors.clone();
        representative_errors.sort();
        let mut failed_filenames = outcomes
            .iter()
            .filter(|outcome| outcome.status == BulkFileStatus::Failed)
            .map(|outcome| outcome.file.filename.clone())
            .collect::<Vec<_>>();
        failed_filenames.sort();
        let complete = self.completed_files == self.expected_files
            && self.failed_files == 0
            && partial_files == 0
            && outcomes.iter().all(|outcome| outcome.error.is_none());
        let seconds = elapsed.as_secs_f64();
        BulkReport {
            inventory_total_files,
            requested_files: self.expected_files,
            expected_files: self.expected_files,
            completed_files: self.completed_files,
            resumed_files: self.resumed_files,
            downloaded_files: self.downloaded_files,
            failed_files: self.failed_files,
            partial_files,
            requests_total: self.requests_total,
            retries_total: self.retry_attempts_total,
            http_status_counts: self.http_status_counts.clone(),
            bytes_downloaded: self.bytes_downloaded,
            compressed_bytes_total: self.compressed_bytes_total,
            elapsed_seconds: seconds,
            throughput_files_per_second: self.downloaded_files as f64 / seconds.max(0.001),
            throughput_recent_files_per_second: self.recent_throughput(),
            throughput_bytes_per_second: self.bytes_downloaded as f64 / seconds.max(0.001),
            latency_p50_ms: percentile(&latencies, 0.50),
            latency_p95_ms: percentile(&latencies, 0.95),
            latency_p99_ms: percentile(&latencies, 0.99),
            representative_errors,
            failed_filenames,
            checksum_manifest_path: paths.checksum_manifest_path.display().to_string(),
            output_manifest_path: paths.output_manifest_path.display().to_string(),
            complete,
            complete_inventory: complete && self.expected_files == inventory_total_files,
        }
    }
}

#[derive(Debug)]
struct AdaptiveRateLimiter {
    maximum_rps: f64,
    state: AsyncMutex<RateState>,
}

#[derive(Debug)]
struct RateState {
    current_rps: f64,
    next_request: Instant,
}

impl AdaptiveRateLimiter {
    fn new(maximum_rps: f64) -> Self {
        Self {
            maximum_rps,
            state: AsyncMutex::new(RateState {
                current_rps: maximum_rps,
                next_request: Instant::now(),
            }),
        }
    }

    async fn acquire(&self) {
        let delay = {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let scheduled = state.next_request.max(now);
            state.next_request =
                scheduled + Duration::from_secs_f64(1.0 / state.current_rps.max(0.001));
            scheduled.saturating_duration_since(now)
        };
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    async fn record_throttle(&self) {
        let mut state = self.state.lock().await;
        state.current_rps = (state.current_rps * 0.7).max(0.25);
        state.next_request = state
            .next_request
            .max(Instant::now() + Duration::from_millis(250));
    }

    async fn record_success(&self) {
        let mut state = self.state.lock().await;
        state.current_rps = (state.current_rps * 1.005 + 0.005).min(self.maximum_rps);
    }
}

fn percentile(sorted_values: &[f64], quantile: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let rank = (quantile * sorted_values.len() as f64).ceil() as usize;
    Some(sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)])
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn parses_and_canonicalizes_official_md5_format() -> Result<()> {
        let parsed = parse_md5_manifest(
            "D41D8CD98F00B204E9800998ECF8427E  z.csv.gz\n\
             900150983cd24fb0d6963f7d28e17f72 *a.csv.gz\r\n",
        )?;
        assert_eq!(parsed[0].filename, "a.csv.gz");
        assert_eq!(parsed[0].md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(parsed[1].filename, "z.csv.gz");
        assert_eq!(
            canonical_checksum_manifest(&parsed),
            "900150983cd24fb0d6963f7d28e17f72  a.csv.gz\n\
             d41d8cd98f00b204e9800998ecf8427e  z.csv.gz\n"
        );
        Ok(())
    }

    #[test]
    fn rejects_unsafe_duplicate_or_invalid_manifest_entries() {
        assert!(parse_md5_manifest("abc  file.csv.gz\n").is_err());
        assert!(
            parse_md5_manifest("d41d8cd98f00b204e9800998ecf8427e  ../escape.csv.gz\n").is_err()
        );
        assert!(parse_md5_manifest(
            "d41d8cd98f00b204e9800998ecf8427e  same.csv.gz\n\
             d41d8cd98f00b204e9800998ecf8427e  same.csv.gz\n"
        )
        .is_err());
    }

    #[test]
    fn computes_known_md5() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture");
        fs::write(&path, b"abc")?;
        assert_eq!(md5_file(&path)?, "900150983cd24fb0d6963f7d28e17f72");
        Ok(())
    }

    #[test]
    fn validates_content_range_strictly() -> Result<()> {
        assert_eq!(
            parse_content_range("bytes 3-5/6")?,
            ContentRange::Satisfied {
                start: 3,
                end: 5,
                total: 6
            }
        );
        assert_eq!(
            parse_content_range("bytes */6")?,
            ContentRange::Unsatisfied { total: 6 }
        );
        assert!(parse_content_range("bytes 4-6/6").is_err());
        assert!(parse_content_range("items 3-5/6").is_err());
        Ok(())
    }

    #[tokio::test]
    async fn resumes_cut_response_with_validated_range_then_promotes() -> Result<()> {
        let payload = b"abcdef".to_vec();
        let md5 = format!("{:x}", Md5::digest(&payload));
        let manifest = format!("{md5}  fixture.csv.gz\n");
        let observed_ranges = Arc::new(Mutex::new(Vec::<String>::new()));
        let handler_ranges = Arc::clone(&observed_ranges);
        let payload_for_server = payload.clone();
        let manifest_for_server = manifest.clone();
        let (base_url, requests, server) = spawn_server(move |request, _index| {
            if request.starts_with("GET /_MD5SUM.txt ") {
                http_response(200, &[], manifest_for_server.as_bytes(), None)
            } else {
                let range = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("range:"))
                    .unwrap_or("")
                    .to_string();
                handler_ranges
                    .lock()
                    .expect("range mutex poisoned")
                    .push(range.clone());
                if range.is_empty() {
                    // Deliberately promise six bytes and close after three.
                    http_response(200, &[], &payload_for_server[..3], Some(6))
                } else {
                    assert_eq!(range.to_ascii_lowercase(), "range: bytes=3-");
                    http_response(
                        206,
                        &[("Content-Range", "bytes 3-5/6")],
                        &payload_for_server[3..],
                        None,
                    )
                }
            }
        })
        .await?;

        let dir = tempfile::tempdir()?;
        let paths = BulkPaths::new(dir.path());
        let downloader = BulkDownloader::new(base_url, test_config())?;
        let report = downloader.download(&paths, true).await?;
        server.abort();

        assert!(report.complete);
        assert_eq!(report.completed_files, 1);
        assert_eq!(report.downloaded_files, 1);
        assert_eq!(report.retries_total, 1);
        assert_eq!(fs::read(dir.path().join("fixture.csv.gz"))?, payload);
        assert!(!dir.path().join("fixture.csv.gz.part").exists());
        assert_eq!(requests.load(Ordering::SeqCst), 3);
        let ranges = observed_ranges.lock().expect("range mutex poisoned");
        assert_eq!(ranges.len(), 2);
        assert!(ranges[0].is_empty());
        assert_eq!(ranges[1].to_ascii_lowercase(), "range: bytes=3-");

        let output: BulkOutputManifest =
            serde_json::from_slice(&fs::read(&paths.output_manifest_path)?)?;
        assert!(output.complete);
        assert_eq!(output.files[0].status, BulkFileStatus::Downloaded);
        assert_eq!(output.files[0].size_bytes, Some(6));
        Ok(())
    }

    #[tokio::test]
    async fn resume_validates_existing_file_before_skipping_request() -> Result<()> {
        let payload = b"already complete".to_vec();
        let md5 = format!("{:x}", Md5::digest(&payload));
        let manifest = format!("{md5}  existing.csv.gz\n");
        let manifest_for_server = manifest.clone();
        let (base_url, requests, server) = spawn_server(move |request, _index| {
            assert!(request.starts_with("GET /_MD5SUM.txt "));
            http_response(200, &[], manifest_for_server.as_bytes(), None)
        })
        .await?;

        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("existing.csv.gz"), &payload)?;
        let paths = BulkPaths::new(dir.path());
        let downloader = BulkDownloader::new(base_url, test_config())?;
        let report = downloader.download(&paths, true).await?;
        server.abort();

        assert_eq!(report.completed_files, 1);
        assert_eq!(report.resumed_files, 1);
        assert_eq!(report.downloaded_files, 0);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn checksum_mismatch_never_promotes_part_and_fails_closed() -> Result<()> {
        let expected_md5 = format!("{:x}", Md5::digest(b"expected"));
        let manifest = format!("{expected_md5}  corrupt.csv.gz\n");
        let manifest_for_server = manifest.clone();
        let (base_url, requests, server) = spawn_server(move |request, _index| {
            if request.starts_with("GET /_MD5SUM.txt ") {
                http_response(200, &[], manifest_for_server.as_bytes(), None)
            } else {
                http_response(200, &[], b"wrong", None)
            }
        })
        .await?;

        let dir = tempfile::tempdir()?;
        let paths = BulkPaths::new(dir.path());
        let mut config = test_config();
        config.max_attempts = 2;
        let downloader = BulkDownloader::new(base_url, config)?;
        let result = downloader.download(&paths, true).await;
        server.abort();

        assert!(result.is_err());
        assert!(!dir.path().join("corrupt.csv.gz").exists());
        assert!(!dir.path().join("corrupt.csv.gz.part").exists());
        assert_eq!(requests.load(Ordering::SeqCst), 3);
        let output: BulkOutputManifest =
            serde_json::from_slice(&fs::read(&paths.output_manifest_path)?)?;
        assert!(!output.complete);
        assert_eq!(output.files[0].status, BulkFileStatus::Failed);
        Ok(())
    }

    #[tokio::test]
    async fn file_limit_is_deterministic_and_not_a_complete_inventory() -> Result<()> {
        let a = b"a".to_vec();
        let b = b"b".to_vec();
        let manifest = format!(
            "{:x}  b.csv.gz\n{:x}  a.csv.gz\n",
            Md5::digest(&b),
            Md5::digest(&a)
        );
        let manifest_for_server = manifest.clone();
        let a_for_server = a.clone();
        let (base_url, requests, server) = spawn_server(move |request, _index| {
            if request.starts_with("GET /_MD5SUM.txt ") {
                http_response(200, &[], manifest_for_server.as_bytes(), None)
            } else {
                assert!(request.starts_with("GET /a.csv.gz "));
                http_response(200, &[], &a_for_server, None)
            }
        })
        .await?;

        let dir = tempfile::tempdir()?;
        let paths = BulkPaths::new(dir.path());
        let mut config = test_config();
        config.file_limit = Some(1);
        let downloader = BulkDownloader::new(base_url, config)?;
        let report = downloader.download(&paths, true).await?;
        server.abort();

        assert!(report.complete);
        assert!(!report.complete_inventory);
        assert_eq!(report.inventory_total_files, 2);
        assert_eq!(report.requested_files, 1);
        assert_eq!(fs::read(dir.path().join("a.csv.gz"))?, a);
        assert!(!dir.path().join("b.csv.gz").exists());
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        let output: BulkOutputManifest =
            serde_json::from_slice(&fs::read(&paths.output_manifest_path)?)?;
        assert_eq!(output.inventory_total_files, 2);
        assert_eq!(output.requested_files, 1);
        assert!(!output.complete_inventory);
        Ok(())
    }

    fn test_config() -> BulkConfig {
        BulkConfig {
            concurrency: 2,
            timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(1),
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            progress_interval: Duration::from_secs(60),
            file_limit: None,
        }
    }

    async fn spawn_server<F>(
        handler: F,
    ) -> Result<(String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>)>
    where
        F: Fn(&str, usize) -> Vec<u8> + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let requests = Arc::new(AtomicUsize::new(0));
        let request_count = Arc::clone(&requests);
        let handler = Arc::new(handler);
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let handler = Arc::clone(&handler);
                let request_count = Arc::clone(&request_count);
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    let mut buffer = [0_u8; 1024];
                    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                        let Ok(count) = socket.read(&mut buffer).await else {
                            return;
                        };
                        if count == 0 || bytes.len() > 64 * 1024 {
                            return;
                        }
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    let request = String::from_utf8_lossy(&bytes);
                    let index = request_count.fetch_add(1, Ordering::SeqCst);
                    let response = handler(&request, index);
                    let _ = socket.write_all(&response).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Ok((format!("http://{address}/"), requests, task))
    }

    fn http_response(
        status: u16,
        headers: &[(&str, &str)],
        body: &[u8],
        declared_length: Option<usize>,
    ) -> Vec<u8> {
        let reason = match status {
            200 => "OK",
            206 => "Partial Content",
            503 => "Service Unavailable",
            _ => "Error",
        };
        let mut response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
            declared_length.unwrap_or(body.len())
        )
        .into_bytes();
        for (name, value) in headers {
            response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
        response.extend_from_slice(b"\r\n");
        response.extend_from_slice(body);
        response
    }
}
