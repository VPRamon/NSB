//! Concurrent, restartable Gaia DataLink XP retrieval.
#![allow(missing_docs)]

use crate::gaia::xp::continuous::validate_continuous_coefficient_csv;
use crate::gaia::xp::sampled::{
    contains_service_error, format_series, parse_gaia_datalink_csv, XpProduct,
    NORMALIZED_FLUX_COLUMN, NORMALIZED_FLUX_ERROR_COLUMN, NORMALIZED_WAVELENGTH_COLUMN,
};
use anyhow::{bail, Context, Result};
use futures_util::{stream, StreamExt};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone)]
pub struct DatalinkConfig {
    pub concurrency: usize,
    pub max_rps: f64,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub progress_interval: Duration,
}

impl DatalinkConfig {
    pub fn validate(&self) -> Result<()> {
        if self.concurrency == 0 {
            bail!("DataLink concurrency must be positive");
        }
        if !self.max_rps.is_finite() || self.max_rps <= 0.0 {
            bail!("DataLink max RPS must be finite and positive");
        }
        if self.timeout.is_zero() || self.connect_timeout.is_zero() {
            bail!("DataLink timeouts must be positive");
        }
        if self.max_attempts == 0 {
            bail!("DataLink max attempts must be positive");
        }
        if self.initial_backoff.is_zero() || self.max_backoff < self.initial_backoff {
            bail!("DataLink backoff must be positive and max >= initial");
        }
        if self.progress_interval.is_zero() {
            bail!("progress interval must be positive");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DownloadPaths {
    pub raw_dir: PathBuf,
    pub error_dir: PathBuf,
    pub checkpoint: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Pending,
    Downloading,
    Retrying,
    Downloaded,
    Validated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointEntry {
    pub unix_millis: u128,
    pub source_id: String,
    pub state: SourceState,
    pub attempt: u32,
    pub detail: String,
}

type CheckpointEvent = CheckpointEntry;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DownloadReport {
    pub selected_sources: usize,
    pub completed_sources: usize,
    pub resumed_sources: usize,
    pub retried_sources: usize,
    pub failed_sources: usize,
    pub pending_sources: usize,
    pub requests_total: usize,
    pub retry_attempts_total: usize,
    pub http_status_counts: BTreeMap<String, usize>,
    pub bytes_downloaded: u64,
    pub elapsed_seconds: f64,
    pub throughput_overall_sources_per_second: f64,
    pub throughput_recent_sources_per_second: f64,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub latency_p99_ms: Option<f64>,
    pub configured_max_rps: f64,
    pub final_adaptive_rps: f64,
    pub checkpoint_path: String,
    pub representative_error_files: Vec<String>,
    pub failed_source_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizationReport {
    pub chunks_requested: usize,
    pub chunks_completed: usize,
    pub chunks_failed: usize,
    pub products_parsed: usize,
    pub partial_files: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatalinkRetrievalType {
    XpSampled,
    XpContinuous,
}

impl DatalinkRetrievalType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::XpSampled => "XP_SAMPLED",
            Self::XpContinuous => "XP_CONTINUOUS",
        }
    }
}

#[derive(Debug)]
pub struct DatalinkDownloader {
    base_url: String,
    client: reqwest::Client,
    config: DatalinkConfig,
    limiter: Arc<AdaptiveRateLimiter>,
    retrieval_type: DatalinkRetrievalType,
}

impl DatalinkDownloader {
    pub fn new(base_url: impl Into<String>, config: DatalinkConfig) -> Result<Self> {
        config.validate()?;
        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(config.concurrency)
            .tcp_keepalive(Duration::from_secs(30))
            .user_agent(concat!(
                "NSB/",
                env!("CARGO_PKG_VERSION"),
                " Gaia-DR3-release-tool"
            ))
            .build()
            .context("failed to build Gaia DataLink HTTP client")?;
        Ok(Self {
            base_url: base_url.into(),
            client,
            limiter: Arc::new(AdaptiveRateLimiter::new(config.max_rps)),
            config,
            retrieval_type: DatalinkRetrievalType::XpSampled,
        })
    }

    pub fn with_retrieval_type(mut self, retrieval_type: DatalinkRetrievalType) -> Self {
        self.retrieval_type = retrieval_type;
        self
    }

    pub async fn download(
        self: Arc<Self>,
        source_ids: &[String],
        paths: &DownloadPaths,
        resume: bool,
        retry_failed_only: bool,
    ) -> Result<DownloadReport> {
        if retry_failed_only && !resume {
            bail!("retry-failed-only requires resume");
        }
        fs::create_dir_all(&paths.raw_dir)?;
        fs::create_dir_all(&paths.error_dir)?;
        if let Some(parent) = paths.checkpoint.parent() {
            fs::create_dir_all(parent)?;
        }
        let latest = load_checkpoint(&paths.checkpoint)?;
        if retry_failed_only && latest.is_empty() {
            bail!(
                "retry-failed-only requires an existing non-empty checkpoint at {}",
                paths.checkpoint.display()
            );
        }
        let checkpoint = Arc::new(CheckpointLog::open(&paths.checkpoint)?);
        let shared = Arc::new(Mutex::new(SharedReport::new(
            source_ids.len(),
            self.config.max_rps,
            paths.checkpoint.display().to_string(),
        )));
        let mut pending = Vec::new();
        for source_id in source_ids {
            let final_path = raw_path(&paths.raw_dir, source_id);
            let part_path = part_path(&paths.raw_dir, source_id);
            if resume {
                match validate_existing(&final_path, source_id, self.retrieval_type) {
                    Ok(size) => {
                        checkpoint.append(
                            source_id,
                            SourceState::Validated,
                            0,
                            "resume-valid-raw",
                        )?;
                        let mut report = shared.lock().expect("download report mutex poisoned");
                        report.completed_sources += 1;
                        report.resumed_sources += 1;
                        report.known_disk_bytes += size;
                        report.note_completion();
                        continue;
                    }
                    Err(err) if final_path.exists() => {
                        let quarantined = quarantine_file(
                            &final_path,
                            &paths.error_dir,
                            source_id,
                            "invalid-raw",
                        )?;
                        checkpoint.append(
                            source_id,
                            SourceState::Pending,
                            0,
                            &format!(
                                "existing raw invalid ({err:#}); quarantined {}",
                                quarantined.display()
                            ),
                        )?;
                    }
                    Err(_) => {}
                }
                if part_path.exists() {
                    match validate_existing(&part_path, source_id, self.retrieval_type) {
                        Ok(size) => {
                            atomic_replace(&part_path, &final_path)?;
                            checkpoint.append(
                                source_id,
                                SourceState::Validated,
                                0,
                                "resume-promoted-valid-part",
                            )?;
                            let mut report = shared.lock().expect("download report mutex poisoned");
                            report.completed_sources += 1;
                            report.resumed_sources += 1;
                            report.known_disk_bytes += size;
                            report.note_completion();
                            continue;
                        }
                        Err(err) => {
                            let quarantined = quarantine_file(
                                &part_path,
                                &paths.error_dir,
                                source_id,
                                "invalid-part",
                            )?;
                            checkpoint.append(
                                source_id,
                                SourceState::Pending,
                                0,
                                &format!(
                                    "stale part invalid ({err:#}); quarantined {}",
                                    quarantined.display()
                                ),
                            )?;
                        }
                    }
                }
            }
            if retry_failed_only
                && latest.get(source_id.as_str()).copied() != Some(SourceState::Failed)
            {
                continue;
            }
            checkpoint.append(source_id, SourceState::Pending, 0, "scheduled")?;
            pending.push(source_id.clone());
        }

        let started = Instant::now();
        let tasks = stream::iter(pending)
            .map(|source_id| {
                let downloader = Arc::clone(&self);
                let paths = paths.clone();
                let checkpoint = Arc::clone(&checkpoint);
                let shared = Arc::clone(&shared);
                async move {
                    downloader
                        .download_one(source_id, &paths, checkpoint, shared)
                        .await
                }
            })
            .buffer_unordered(self.config.concurrency);
        tokio::pin!(tasks);
        let mut progress = tokio::time::interval(self.config.progress_interval);
        progress.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let _ = progress.tick().await;
        loop {
            tokio::select! {
                outcome = tasks.next() => {
                    match outcome {
                        Some(Ok(())) => {}
                        Some(Err(err)) => {
                            let mut report = shared.lock().expect("download report mutex poisoned");
                            report.infrastructure_errors.push(format!("{err:#}"));
                        }
                        None => break,
                    }
                }
                _ = progress.tick() => {
                    let report = shared.lock().expect("download report mutex poisoned");
                    eprintln!("{}", report.progress_line());
                }
            }
        }

        let adaptive_rps = self.limiter.current_rps().await;
        let mut report = shared.lock().expect("download report mutex poisoned");
        report.elapsed_override = Some(started.elapsed());
        report.final_adaptive_rps = adaptive_rps;
        if !report.infrastructure_errors.is_empty() {
            for error in report.infrastructure_errors.clone() {
                report.note_error_file(error);
            }
        }
        let result = report.finish();
        eprintln!("{}", report.progress_line());
        Ok(result)
    }

    async fn download_one(
        &self,
        source_id: String,
        paths: &DownloadPaths,
        checkpoint: Arc<CheckpointLog>,
        shared: Arc<Mutex<SharedReport>>,
    ) -> Result<()> {
        let final_path = raw_path(&paths.raw_dir, &source_id);
        let part_path = part_path(&paths.raw_dir, &source_id);
        let mut last_error = String::new();

        for attempt in 1..=self.config.max_attempts {
            checkpoint.append(&source_id, SourceState::Downloading, attempt, "request")?;
            self.limiter.acquire().await;
            let requested = Instant::now();
            {
                let mut report = shared.lock().expect("download report mutex poisoned");
                report.requests_total += 1;
                if attempt > 1 {
                    report.retry_attempts_total += 1;
                    report.retried_source_ids.insert(source_id.clone());
                }
            }
            let response = self
                .client
                .get(&self.base_url)
                .query(&[
                    ("ID", format!("Gaia DR3 {source_id}")),
                    ("RETRIEVAL_TYPE", self.retrieval_type.as_str().to_string()),
                    ("DATA_STRUCTURE", "INDIVIDUAL".to_string()),
                    ("FORMAT", "csv".to_string()),
                ])
                .send()
                .await;
            let latency = requested.elapsed();
            {
                let mut report = shared.lock().expect("download report mutex poisoned");
                report.latencies.push(latency);
            }

            let result = match response {
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let retry_after_value = retry_after(&headers);
                    match response.bytes().await {
                        Ok(body) => {
                            let body = body.to_vec();
                            if !status.is_success() {
                                let detail = format!(
                                    "source_id={source_id} attempt={attempt} status={status} HTTP failure"
                                );
                                AttemptResult::Failure {
                                    retryable: is_retryable_status(status.as_u16()),
                                    retry_after: retry_after_value,
                                    status: Some(status.as_u16()),
                                    headers,
                                    body,
                                    detail,
                                }
                            } else if contains_service_error(&body) {
                                AttemptResult::Failure {
                                    retryable: true,
                                    retry_after: retry_after_value,
                                    status: Some(status.as_u16()),
                                    headers,
                                    body,
                                    detail: format!(
                                        "source_id={source_id} attempt={attempt} status={status} SERVICE ERROR response"
                                    ),
                                }
                            } else {
                                match self.retrieval_type {
                                    DatalinkRetrievalType::XpSampled => {
                                        match parse_gaia_datalink_csv(&body, &source_id) {
                                            Ok(product) => AttemptResult::Success {
                                                detail: format!(
                                                    "{} samples",
                                                    product.wavelengths_nm.len()
                                                ),
                                                body,
                                                status: status.as_u16(),
                                            },
                                            Err(err) => AttemptResult::Failure {
                                                retryable: true,
                                                retry_after: retry_after_value,
                                                status: Some(status.as_u16()),
                                                headers,
                                                body,
                                                detail: format!(
                                                    "source_id={source_id} attempt={attempt} status={status} invalid CSV: {err:#}"
                                                ),
                                            },
                                        }
                                    }
                                    DatalinkRetrievalType::XpContinuous => {
                                        match validate_continuous_coefficient_csv(&body, &source_id)
                                        {
                                            Ok(()) => AttemptResult::Success {
                                                detail: "XP continuous coefficients".to_string(),
                                                body,
                                                status: status.as_u16(),
                                            },
                                            Err(err) => AttemptResult::Failure {
                                                retryable: true,
                                                retry_after: retry_after_value,
                                                status: Some(status.as_u16()),
                                                headers,
                                                body,
                                                detail: format!(
                                                    "source_id={source_id} attempt={attempt} status={status} invalid coefficient CSV: {err:#}"
                                                ),
                                            },
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            AttemptResult::Failure {
                                retryable: err.is_timeout()
                                    || err.is_connect()
                                    || err.is_request()
                                    || err.is_body(),
                                retry_after: retry_after_value,
                                status: Some(status.as_u16()),
                                headers,
                                body: Vec::new(),
                                detail: format!(
                                    "source_id={source_id} attempt={attempt} status={status} body error: {err}"
                                ),
                            }
                        }
                    }
                }
                Err(err) => {
                    last_error =
                        format!("source_id={source_id} attempt={attempt} transport error: {err}");
                    AttemptResult::Failure {
                        retryable: err.is_timeout()
                            || err.is_connect()
                            || err.is_request()
                            || err.is_body(),
                        retry_after: None,
                        status: None,
                        headers: HeaderMap::new(),
                        body: Vec::new(),
                        detail: last_error.clone(),
                    }
                }
            };

            match result {
                AttemptResult::Success {
                    detail,
                    body,
                    status,
                } => {
                    write_part(&part_path, &body)?;
                    checkpoint.append(
                        &source_id,
                        SourceState::Downloaded,
                        attempt,
                        "atomic-part-written",
                    )?;
                    validate_existing(&part_path, &source_id, self.retrieval_type)?;
                    atomic_replace(&part_path, &final_path)?;
                    checkpoint.append(&source_id, SourceState::Validated, attempt, &detail)?;
                    self.limiter.record_success().await;
                    let mut report = shared.lock().expect("download report mutex poisoned");
                    report.bytes_downloaded += body.len() as u64;
                    *report
                        .http_status_counts
                        .entry(status.to_string())
                        .or_default() += 1;
                    report.completed_sources += 1;
                    report.known_disk_bytes += body.len() as u64;
                    report.note_completion();
                    return Ok(());
                }
                AttemptResult::Failure {
                    retryable,
                    retry_after,
                    status,
                    headers,
                    body,
                    detail,
                } => {
                    last_error = detail;
                    {
                        let mut report = shared.lock().expect("download report mutex poisoned");
                        report.bytes_downloaded += body.len() as u64;
                        let status_key = status
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "transport".to_string());
                        *report.http_status_counts.entry(status_key).or_default() += 1;
                    }
                    let error_files = persist_error_attempt(
                        &paths.error_dir,
                        &source_id,
                        attempt,
                        status,
                        &headers,
                        &body,
                        &last_error,
                    )?;
                    {
                        let mut report = shared.lock().expect("download report mutex poisoned");
                        for path in error_files {
                            report.note_error_file(path.display().to_string());
                        }
                    }
                    if matches!(status, Some(429 | 503)) {
                        self.limiter.record_throttle().await;
                    }
                    if retryable && attempt < self.config.max_attempts {
                        let delay =
                            retry_after.unwrap_or_else(|| self.retry_delay(&source_id, attempt));
                        checkpoint.append(
                            &source_id,
                            SourceState::Retrying,
                            attempt,
                            &format!("{}; sleep_ms={}", last_error, delay.as_millis()),
                        )?;
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    break;
                }
            }
        }

        if part_path.exists() {
            let _ = quarantine_file(&part_path, &paths.error_dir, &source_id, "failed-part");
        }
        checkpoint.append(
            &source_id,
            SourceState::Failed,
            self.config.max_attempts,
            &last_error,
        )?;
        let mut report = shared.lock().expect("download report mutex poisoned");
        report.failed_source_ids.push(source_id);
        Ok(())
    }

    fn retry_delay(&self, source_id: &str, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        let base_ms = self
            .config
            .initial_backoff
            .as_millis()
            .saturating_mul(1_u128 << exponent)
            .min(self.config.max_backoff.as_millis());
        let hash = source_id.bytes().fold(u64::from(attempt), |acc, byte| {
            acc.wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(byte))
        });
        let jitter_permille = 800 + hash % 401;
        let jittered = base_ms
            .saturating_mul(u128::from(jitter_permille))
            .saturating_div(1000)
            .min(self.config.max_backoff.as_millis());
        Duration::from_millis(u64::try_from(jittered).unwrap_or(u64::MAX))
    }
}

enum AttemptResult {
    Success {
        detail: String,
        body: Vec<u8>,
        status: u16,
    },
    Failure {
        retryable: bool,
        retry_after: Option<Duration>,
        status: Option<u16>,
        headers: HeaderMap,
        body: Vec<u8>,
        detail: String,
    },
}

#[derive(Debug)]
struct AdaptiveRateLimiter {
    max_rps: f64,
    state: AsyncMutex<RateState>,
}

#[derive(Debug)]
struct RateState {
    current_rps: f64,
    next_request: Instant,
}

impl AdaptiveRateLimiter {
    fn new(max_rps: f64) -> Self {
        Self {
            max_rps,
            state: AsyncMutex::new(RateState {
                current_rps: max_rps,
                next_request: Instant::now(),
            }),
        }
    }

    async fn acquire(&self) {
        let delay = {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let scheduled = state.next_request.max(now);
            state.next_request = scheduled + Duration::from_secs_f64(1.0 / state.current_rps);
            scheduled.saturating_duration_since(now)
        };
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
    }

    async fn record_throttle(&self) {
        let mut state = self.state.lock().await;
        state.current_rps = (state.current_rps * 0.7).max(1.0);
        state.next_request = state
            .next_request
            .max(Instant::now() + Duration::from_millis(250));
    }

    async fn record_success(&self) {
        let mut state = self.state.lock().await;
        state.current_rps = (state.current_rps * 1.005 + 0.005).min(self.max_rps);
    }

    async fn current_rps(&self) -> f64 {
        self.state.lock().await.current_rps
    }
}

#[derive(Debug)]
struct CheckpointLog {
    file: Mutex<File>,
}

impl CheckpointLog {
    fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open checkpoint {}", path.display()))?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn append(
        &self,
        source_id: &str,
        state: SourceState,
        attempt: u32,
        detail: &str,
    ) -> Result<()> {
        let event = CheckpointEvent {
            unix_millis: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            source_id: source_id.to_string(),
            state,
            attempt,
            detail: detail.replace(['\n', '\r'], " "),
        };
        let mut file = self.file.lock().expect("checkpoint mutex poisoned");
        serde_json::to_writer(&mut *file, &event)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }
}

/// Read all well-formed checkpoint JSONL records (ignores one torn tail line).
pub fn read_checkpoint_entries(path: &Path) -> Result<Vec<CheckpointEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<CheckpointEntry>(&line) {
            Ok(event) => entries.push(event),
            Err(_) => break,
        }
    }
    Ok(entries)
}

/// Latest checkpoint row per source_id.
pub fn latest_checkpoint_entries(path: &Path) -> Result<BTreeMap<String, CheckpointEntry>> {
    let mut latest = BTreeMap::new();
    for event in read_checkpoint_entries(path)? {
        latest.insert(event.source_id.clone(), event);
    }
    Ok(latest)
}

fn load_checkpoint(path: &Path) -> Result<BTreeMap<String, SourceState>> {
    Ok(latest_checkpoint_entries(path)?
        .into_iter()
        .map(|(source_id, entry)| (source_id, entry.state))
        .collect())
}

#[derive(Debug)]
struct SharedReport {
    selected_sources: usize,
    completed_sources: usize,
    resumed_sources: usize,
    requests_total: usize,
    retry_attempts_total: usize,
    http_status_counts: BTreeMap<String, usize>,
    bytes_downloaded: u64,
    known_disk_bytes: u64,
    started: Instant,
    elapsed_override: Option<Duration>,
    completion_times: VecDeque<Instant>,
    latencies: Vec<Duration>,
    retried_source_ids: BTreeSet<String>,
    failed_source_ids: Vec<String>,
    representative_error_files: Vec<String>,
    infrastructure_errors: Vec<String>,
    configured_max_rps: f64,
    final_adaptive_rps: f64,
    checkpoint_path: String,
}

impl SharedReport {
    fn new(selected_sources: usize, configured_max_rps: f64, checkpoint_path: String) -> Self {
        Self {
            selected_sources,
            completed_sources: 0,
            resumed_sources: 0,
            requests_total: 0,
            retry_attempts_total: 0,
            http_status_counts: BTreeMap::new(),
            bytes_downloaded: 0,
            known_disk_bytes: 0,
            started: Instant::now(),
            elapsed_override: None,
            completion_times: VecDeque::new(),
            latencies: Vec::new(),
            retried_source_ids: BTreeSet::new(),
            failed_source_ids: Vec::new(),
            representative_error_files: Vec::new(),
            infrastructure_errors: Vec::new(),
            configured_max_rps,
            final_adaptive_rps: configured_max_rps,
            checkpoint_path,
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
        let span = Instant::now().duration_since(*first).as_secs_f64().max(1.0);
        self.completion_times.len() as f64 / span
    }

    fn elapsed(&self) -> Duration {
        self.elapsed_override
            .unwrap_or_else(|| self.started.elapsed())
    }

    fn progress_line(&self) -> String {
        let completed = self.completed_sources.min(self.selected_sources);
        let percent = if self.selected_sources == 0 {
            100.0
        } else {
            100.0 * completed as f64 / self.selected_sources as f64
        };
        let elapsed = self.elapsed().as_secs_f64();
        let throughput = if elapsed > 0.0 {
            completed.saturating_sub(self.resumed_sources) as f64 / elapsed
        } else {
            0.0
        };
        let remaining = self.selected_sources.saturating_sub(completed);
        let eta = if throughput > 0.0 {
            format_duration(Duration::from_secs_f64(remaining as f64 / throughput))
        } else {
            "unknown".to_string()
        };
        format!(
            "{completed}/{} | {percent:.2}% | {throughput:.2} sources/s | retries {} | errors {} | elapsed {} | ETA {eta} | disk {}",
            self.selected_sources,
            self.retry_attempts_total,
            self.failed_source_ids.len(),
            format_duration(self.elapsed()),
            human_bytes(self.known_disk_bytes),
        )
    }

    fn note_error_file(&mut self, path: String) {
        if self.representative_error_files.len() < 10
            && !self.representative_error_files.contains(&path)
        {
            self.representative_error_files.push(path);
        }
    }

    fn finish(&self) -> DownloadReport {
        let elapsed = self.elapsed().as_secs_f64();
        let network_completed = self.completed_sources.saturating_sub(self.resumed_sources);
        let mut latency_ms = self
            .latencies
            .iter()
            .map(|duration| duration.as_secs_f64() * 1000.0)
            .collect::<Vec<_>>();
        latency_ms.sort_by(f64::total_cmp);
        let failed_sources = self.failed_source_ids.len() + self.infrastructure_errors.len();
        DownloadReport {
            selected_sources: self.selected_sources,
            completed_sources: self.completed_sources,
            resumed_sources: self.resumed_sources,
            retried_sources: self.retried_source_ids.len(),
            failed_sources,
            pending_sources: self
                .selected_sources
                .saturating_sub(self.completed_sources + failed_sources),
            requests_total: self.requests_total,
            retry_attempts_total: self.retry_attempts_total,
            http_status_counts: self.http_status_counts.clone(),
            bytes_downloaded: self.bytes_downloaded,
            elapsed_seconds: elapsed,
            throughput_overall_sources_per_second: if elapsed > 0.0 {
                network_completed as f64 / elapsed
            } else {
                0.0
            },
            throughput_recent_sources_per_second: self.recent_throughput(),
            latency_p50_ms: percentile(&latency_ms, 0.50),
            latency_p95_ms: percentile(&latency_ms, 0.95),
            latency_p99_ms: percentile(&latency_ms, 0.99),
            configured_max_rps: self.configured_max_rps,
            final_adaptive_rps: self.final_adaptive_rps,
            checkpoint_path: self.checkpoint_path.clone(),
            representative_error_files: self.representative_error_files.clone(),
            failed_source_ids: self.failed_source_ids.clone(),
        }
    }
}

pub fn rebuild_normalized_chunks(
    source_ids: &[String],
    chunk_size: usize,
    raw_dir: &Path,
    output_dir: &Path,
) -> Result<NormalizationReport> {
    if chunk_size == 0 {
        bail!("chunk size must be positive");
    }
    fs::create_dir_all(output_dir)?;
    let mut report = NormalizationReport::default();
    let expected_chunks = source_ids.len().div_ceil(chunk_size);
    remove_stale_chunks(output_dir, expected_chunks)?;

    for (chunk_index, chunk) in source_ids.chunks(chunk_size).enumerate() {
        report.chunks_requested += 1;
        let output = output_dir.join(format!("xp_chunk_{chunk_index:06}.csv"));
        let part = output.with_extension("csv.part");
        if part.exists() {
            fs::remove_file(&part)?;
        }
        let mut products = Vec::with_capacity(chunk.len());
        let mut failures = Vec::new();
        for source_id in chunk {
            let raw = raw_path(raw_dir, source_id);
            match fs::read(&raw)
                .with_context(|| format!("failed to read {}", raw.display()))
                .and_then(|bytes| parse_gaia_datalink_csv(&bytes, source_id))
            {
                Ok(product) => products.push(product),
                Err(err) => failures.push(format!("source_id={source_id}: {err:#}")),
            }
        }
        if !failures.is_empty() || products.len() != chunk.len() {
            report.chunks_failed += 1;
            report.failures.extend(failures);
            if output.exists() {
                fs::remove_file(&output)?;
            }
            continue;
        }
        write_normalized_part(&part, &products)?;
        validate_normalized_chunk(&part, chunk)?;
        atomic_replace(&part, &output)?;
        report.chunks_completed += 1;
        report.products_parsed += products.len();
    }
    report.partial_files = count_part_files(output_dir)?;
    Ok(report)
}

fn write_normalized_part(path: &Path, products: &[XpProduct]) -> Result<()> {
    let file = File::create(path)?;
    let mut writer = csv::WriterBuilder::new().from_writer(file);
    writer.write_record([
        "source_id",
        NORMALIZED_WAVELENGTH_COLUMN,
        NORMALIZED_FLUX_COLUMN,
        NORMALIZED_FLUX_ERROR_COLUMN,
    ])?;
    for product in products {
        let errors = product
            .flux_error_w_m2_nm
            .as_deref()
            .context("Gaia XP normalized chunks require flux_error")?;
        writer.write_record([
            product.source_id.as_str(),
            &format_series(&product.wavelengths_nm, false),
            &format_series(&product.flux_w_m2_nm, true),
            &format_series(errors, true),
        ])?;
    }
    writer.flush()?;
    let file = writer.into_inner()?;
    file.sync_all()?;
    Ok(())
}

fn validate_normalized_chunk(path: &Path, expected_source_ids: &[String]) -> Result<()> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)?;
    let headers = reader.headers()?.clone();
    let mut actual = Vec::new();
    for row in reader.records() {
        let row = row?;
        let product = crate::gaia::xp::sampled::parse_normalized_record(&headers, &row)?;
        actual.push(product.source_id);
    }
    if actual != expected_source_ids {
        bail!(
            "normalized chunk is incomplete or non-deterministic: expected {} ordered sources, found {}",
            expected_source_ids.len(),
            actual.len()
        );
    }
    Ok(())
}

fn remove_stale_chunks(output_dir: &Path, expected_chunks: usize) -> Result<()> {
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(index) = name
            .strip_prefix("xp_chunk_")
            .and_then(|name| name.strip_suffix(".csv"))
            .and_then(|index| index.parse::<usize>().ok())
        else {
            continue;
        };
        if index >= expected_chunks {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn count_part_files(dir: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "part")
        {
            count += 1;
        }
    }
    Ok(count)
}

fn validate_existing(
    path: &Path,
    source_id: &str,
    retrieval_type: DatalinkRetrievalType,
) -> Result<u64> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    match retrieval_type {
        DatalinkRetrievalType::XpSampled => {
            parse_gaia_datalink_csv(&bytes, source_id)?;
        }
        DatalinkRetrievalType::XpContinuous => {
            validate_continuous_coefficient_csv(&bytes, source_id)?;
        }
    }
    Ok(bytes.len() as u64)
}

fn write_part(part: &Path, body: &[u8]) -> Result<()> {
    if part.exists() {
        fs::remove_file(part)?;
    }
    let mut file = File::create(part)?;
    file.write_all(body)?;
    file.sync_all()?;
    Ok(())
}

fn atomic_replace(part: &Path, final_path: &Path) -> Result<()> {
    fs::rename(part, final_path).with_context(|| {
        format!(
            "failed atomic rename {} -> {}",
            part.display(),
            final_path.display()
        )
    })
}

fn quarantine_file(
    path: &Path,
    error_dir: &Path,
    source_id: &str,
    reason: &str,
) -> Result<PathBuf> {
    fs::create_dir_all(error_dir)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let target = error_dir.join(format!(
        "xp_source_{source_id}.{reason}.{timestamp}.{extension}"
    ));
    fs::rename(path, &target)?;
    Ok(target)
}

fn persist_error_attempt(
    error_dir: &Path,
    source_id: &str,
    attempt: u32,
    status: Option<u16>,
    headers: &HeaderMap,
    body: &[u8],
    detail: &str,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(error_dir)?;
    let stem = format!("xp_source_{source_id}.attempt_{attempt:02}");
    let header_path = error_dir.join(format!("{stem}.headers.txt"));
    let body_path = error_dir.join(format!("{stem}.body"));
    let mut header_text =
        format!("source_id={source_id}\nattempt={attempt}\nstatus={status:?}\n{detail}\n");
    for (name, value) in headers {
        header_text.push_str(name.as_str());
        header_text.push_str(": ");
        header_text.push_str(value.to_str().unwrap_or("<non-UTF8>"));
        header_text.push('\n');
    }
    fs::write(&header_path, header_text)?;
    fs::write(&body_path, body)?;
    Ok(vec![header_path, body_path])
}

pub fn datalink_raw_coefficient_path(raw_dir: &Path, source_id: &str) -> PathBuf {
    raw_dir.join(format!("xp_source_{source_id}.csv"))
}

fn raw_path(raw_dir: &Path, source_id: &str) -> PathBuf {
    datalink_raw_coefficient_path(raw_dir, source_id)
}

fn part_path(raw_dir: &Path, source_id: &str) -> PathBuf {
    raw_dir.join(format!("xp_source_{source_id}.csv.part"))
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = httpdate::parse_http_date(value).ok()?;
    Some(date.duration_since(SystemTime::now()).unwrap_or_default())
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

fn percentile(sorted: &[f64], quantile: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    sorted.get(index).copied()
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes as f64 >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::task::JoinHandle;

    #[derive(Clone)]
    struct MockResponse {
        status: u16,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
        delay: Duration,
        disconnect: bool,
    }

    impl MockResponse {
        fn status(status: u16) -> Self {
            Self {
                status,
                body: format!("HTTP {status}").into_bytes(),
                headers: Vec::new(),
                delay: Duration::ZERO,
                disconnect: false,
            }
        }

        fn valid() -> Self {
            Self {
                status: 200,
                body: Vec::new(),
                headers: vec![("Content-Type".into(), "text/csv".into())],
                delay: Duration::ZERO,
                disconnect: false,
            }
        }
    }

    struct MockServer {
        url: String,
        accepted_connections: Arc<AtomicUsize>,
        requests: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        task: JoinHandle<()>,
    }

    impl Drop for MockServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    impl MockServer {
        async fn start(responses: Vec<MockResponse>, default: MockResponse) -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let responses = Arc::new(AsyncMutex::new(VecDeque::from(responses)));
            let accepted_connections = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(AtomicUsize::new(0));
            let active = Arc::new(AtomicUsize::new(0));
            let max_active = Arc::new(AtomicUsize::new(0));
            let task = {
                let responses = Arc::clone(&responses);
                let accepted = Arc::clone(&accepted_connections);
                let requests_count = Arc::clone(&requests);
                let active_count = Arc::clone(&active);
                let max_count = Arc::clone(&max_active);
                tokio::spawn(async move {
                    while let Ok((stream, _)) = listener.accept().await {
                        accepted.fetch_add(1, Ordering::SeqCst);
                        let responses = Arc::clone(&responses);
                        let requests = Arc::clone(&requests_count);
                        let active = Arc::clone(&active_count);
                        let max_active = Arc::clone(&max_count);
                        let default = default.clone();
                        tokio::spawn(async move {
                            let _ = serve_connection(
                                stream, responses, default, requests, active, max_active,
                            )
                            .await;
                        });
                    }
                })
            };
            Ok(Self {
                url: format!("http://{address}/data"),
                accepted_connections,
                requests,
                active,
                max_active,
                task,
            })
        }
    }

    async fn serve_connection(
        mut stream: TcpStream,
        responses: Arc<AsyncMutex<VecDeque<MockResponse>>>,
        default: MockResponse,
        requests: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
    ) -> Result<()> {
        let mut pending = Vec::new();
        loop {
            while !pending.windows(4).any(|window| window == b"\r\n\r\n") {
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).await?;
                if read == 0 {
                    return Ok(());
                }
                pending.extend_from_slice(&buffer[..read]);
                if pending.len() > 64 * 1024 {
                    bail!("mock request header too large");
                }
            }
            let end = pending
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .expect("checked header terminator")
                + 4;
            let request = String::from_utf8_lossy(&pending[..end]).to_string();
            pending.drain(..end);
            requests.fetch_add(1, Ordering::SeqCst);
            let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
            max_active.fetch_max(now_active, Ordering::SeqCst);
            let mut response = responses
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| default.clone());
            if response.status == 200 && response.body.is_empty() {
                response.body = valid_csv(&request).into_bytes();
            }
            if !response.delay.is_zero() {
                tokio::time::sleep(response.delay).await;
            }
            if response.disconnect {
                active.fetch_sub(1, Ordering::SeqCst);
                return Ok(());
            }
            let reason = match response.status {
                200 => "OK",
                400 => "Bad Request",
                408 => "Request Timeout",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                502 => "Bad Gateway",
                503 => "Service Unavailable",
                504 => "Gateway Timeout",
                _ => "Mock",
            };
            let mut header = format!(
                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n",
                response.status,
                reason,
                response.body.len()
            );
            for (name, value) in response.headers {
                header.push_str(&format!("{name}: {value}\r\n"));
            }
            header.push_str("\r\n");
            stream.write_all(header.as_bytes()).await?;
            stream.write_all(&response.body).await?;
            stream.flush().await?;
            active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn valid_csv(request: &str) -> String {
        let first_line = request.lines().next().unwrap_or_default();
        let source_id = first_line
            .split(['?', '&', ' '])
            .find_map(|part| {
                part.strip_prefix("ID=Gaia+DR3+")
                    .or_else(|| part.strip_prefix("ID=Gaia%20DR3%20"))
            })
            .unwrap_or("42");
        format!(
            concat!(
                "source_id,solution_id,ra,dec,wavelength,flux,flux_error\n",
                "{0},1,0,0,336,1e-12,1e-14\n",
                "{0},1,0,0,650,1e-12,1e-14\n"
            ),
            source_id
        )
    }

    fn test_config() -> DatalinkConfig {
        DatalinkConfig {
            concurrency: 4,
            max_rps: 1_000.0,
            timeout: Duration::from_secs(2),
            connect_timeout: Duration::from_secs(1),
            max_attempts: 2,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
            progress_interval: Duration::from_secs(60),
        }
    }

    fn test_paths(dir: &tempfile::TempDir) -> DownloadPaths {
        DownloadPaths {
            raw_dir: dir.path().join("raw"),
            error_dir: dir.path().join("errors"),
            checkpoint: dir.path().join("checkpoint.jsonl"),
        }
    }

    #[test]
    fn retry_status_contract_is_exact() {
        for status in [408, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status));
        }
        for status in [200, 400, 401, 403, 404, 501] {
            assert!(!is_retryable_status(status));
        }
    }

    #[test]
    fn retry_after_supports_seconds_and_http_dates() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, "2".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(2)));
        headers.insert(
            RETRY_AFTER,
            httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(2))
                .parse()
                .unwrap(),
        );
        assert!(retry_after(&headers).is_some_and(|delay| delay <= Duration::from_secs(2)));
    }

    #[test]
    fn normalized_chunk_is_atomic_complete_and_keeps_uncertainty() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let raw = dir.path().join("raw");
        fs::create_dir_all(&raw)?;
        for id in ["1", "2"] {
            fs::write(
                raw_path(&raw, id),
                format!(
                    "source_id,solution_id,ra,dec,wavelength,flux,flux_error\n{id},1,0,0,336,1e-12,1e-14\n{id},1,0,0,650,1e-12,1e-14\n"
                ),
            )?;
        }
        let output = dir.path().join("chunks");
        let ids = vec!["1".to_string(), "2".to_string()];
        let report = rebuild_normalized_chunks(&ids, 2, &raw, &output)?;
        assert_eq!(report.chunks_completed, 1);
        assert_eq!(report.products_parsed, 2);
        assert_eq!(report.partial_files, 0);
        let text = fs::read_to_string(output.join("xp_chunk_000000.csv"))?;
        assert!(text.contains(NORMALIZED_FLUX_ERROR_COLUMN));
        Ok(())
    }

    #[test]
    fn incomplete_chunk_is_never_promoted() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let raw = dir.path().join("raw");
        fs::create_dir_all(&raw)?;
        let output = dir.path().join("chunks");
        let ids = vec!["missing".to_string()];
        let report = rebuild_normalized_chunks(&ids, 1, &raw, &output)?;
        assert_eq!(report.chunks_failed, 1);
        assert!(!output.join("xp_chunk_000000.csv").exists());
        assert!(!output.join("xp_chunk_000000.csv.part").exists());
        Ok(())
    }

    #[tokio::test]
    async fn concurrency_is_bounded_and_connections_are_reused() -> Result<()> {
        let mut response = MockResponse::valid();
        response.delay = Duration::from_millis(40);
        let server = MockServer::start(Vec::new(), response).await?;
        let dir = tempfile::tempdir()?;
        let ids = (1..=12).map(|id| id.to_string()).collect::<Vec<_>>();
        let downloader = Arc::new(DatalinkDownloader::new(&server.url, test_config())?);
        let report = downloader
            .download(&ids, &test_paths(&dir), false, false)
            .await?;
        assert_eq!(report.completed_sources, ids.len());
        assert_eq!(report.failed_sources, 0);
        assert!(server.max_active.load(Ordering::SeqCst) <= 4);
        assert!(server.max_active.load(Ordering::SeqCst) > 1);
        assert!(server.accepted_connections.load(Ordering::SeqCst) < ids.len());
        assert_eq!(server.active.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn global_rate_limit_is_enforced() -> Result<()> {
        let server = MockServer::start(Vec::new(), MockResponse::valid()).await?;
        let dir = tempfile::tempdir()?;
        let ids = (1..=4).map(|id| id.to_string()).collect::<Vec<_>>();
        let mut config = test_config();
        config.max_rps = 5.0;
        let started = Instant::now();
        Arc::new(DatalinkDownloader::new(&server.url, config)?)
            .download(&ids, &test_paths(&dir), false, false)
            .await?;
        assert!(started.elapsed() >= Duration::from_millis(580));
        Ok(())
    }

    #[tokio::test]
    async fn all_transient_statuses_retry_and_eventually_succeed() -> Result<()> {
        for status in [408, 429, 500, 502, 503, 504] {
            let server = MockServer::start(
                vec![MockResponse::status(status), MockResponse::valid()],
                MockResponse::valid(),
            )
            .await?;
            let dir = tempfile::tempdir()?;
            let report = Arc::new(DatalinkDownloader::new(&server.url, test_config())?)
                .download(&["42".to_string()], &test_paths(&dir), false, false)
                .await?;
            assert_eq!(report.completed_sources, 1, "status {status}");
            assert_eq!(report.retry_attempts_total, 1, "status {status}");
            assert_eq!(server.requests.load(Ordering::SeqCst), 2, "status {status}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn retry_after_is_respected() -> Result<()> {
        let mut throttled = MockResponse::status(429);
        throttled.headers.push(("Retry-After".into(), "1".into()));
        let server = MockServer::start(
            vec![throttled, MockResponse::valid()],
            MockResponse::valid(),
        )
        .await?;
        let dir = tempfile::tempdir()?;
        let started = Instant::now();
        let report = Arc::new(DatalinkDownloader::new(&server.url, test_config())?)
            .download(&["42".to_string()], &test_paths(&dir), false, false)
            .await?;
        assert_eq!(report.completed_sources, 1);
        assert!(started.elapsed() >= Duration::from_millis(990));
        assert!(report.final_adaptive_rps < report.configured_max_rps);
        Ok(())
    }

    #[tokio::test]
    async fn service_error_malformed_empty_disconnect_and_timeout_are_not_promoted() -> Result<()> {
        let cases = vec![
            MockResponse {
                status: 200,
                body: b"SERVICE ERROR\nContext: DataRetrieval".to_vec(),
                headers: Vec::new(),
                delay: Duration::ZERO,
                disconnect: false,
            },
            MockResponse {
                status: 200,
                body: b"not,a,gaia,csv\n".to_vec(),
                headers: Vec::new(),
                delay: Duration::ZERO,
                disconnect: false,
            },
            MockResponse {
                status: 200,
                body: Vec::new(),
                headers: Vec::new(),
                delay: Duration::ZERO,
                disconnect: true,
            },
            MockResponse {
                status: 200,
                body: b"late".to_vec(),
                headers: Vec::new(),
                delay: Duration::from_millis(200),
                disconnect: false,
            },
        ];
        for response in cases {
            let server =
                MockServer::start(vec![response.clone(), response], MockResponse::status(500))
                    .await?;
            let dir = tempfile::tempdir()?;
            let mut config = test_config();
            config.timeout = Duration::from_millis(50);
            let paths = test_paths(&dir);
            let report = Arc::new(DatalinkDownloader::new(&server.url, config)?)
                .download(&["42".to_string()], &paths, false, false)
                .await?;
            assert_eq!(report.completed_sources, 0);
            assert_eq!(report.failed_sources, 1);
            assert!(!raw_path(&paths.raw_dir, "42").exists());
            assert!(!part_path(&paths.raw_dir, "42").exists());
            assert!(!report.representative_error_files.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn non_transient_http_error_is_not_retried() -> Result<()> {
        let server =
            MockServer::start(vec![MockResponse::status(400)], MockResponse::valid()).await?;
        let dir = tempfile::tempdir()?;
        let report = Arc::new(DatalinkDownloader::new(&server.url, test_config())?)
            .download(&["42".to_string()], &test_paths(&dir), false, false)
            .await?;
        assert_eq!(report.failed_sources, 1);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn resume_valid_raw_and_valid_part_without_network_and_repairs_corrupt_raw() -> Result<()>
    {
        let server = MockServer::start(Vec::new(), MockResponse::valid()).await?;
        let dir = tempfile::tempdir()?;
        let paths = test_paths(&dir);
        fs::create_dir_all(&paths.raw_dir)?;
        fs::write(
            raw_path(&paths.raw_dir, "1"),
            valid_csv("GET /?ID=Gaia+DR3+1 HTTP/1.1\r\n"),
        )?;
        fs::write(
            part_path(&paths.raw_dir, "2"),
            valid_csv("GET /?ID=Gaia+DR3+2 HTTP/1.1\r\n"),
        )?;
        fs::write(raw_path(&paths.raw_dir, "3"), "truncated")?;
        let ids = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let report = Arc::new(DatalinkDownloader::new(&server.url, test_config())?)
            .download(&ids, &paths, true, false)
            .await?;
        assert_eq!(report.completed_sources, 3);
        assert_eq!(report.resumed_sources, 2);
        assert_eq!(server.requests.load(Ordering::SeqCst), 1);
        assert!(raw_path(&paths.raw_dir, "2").exists());
        assert!(!part_path(&paths.raw_dir, "2").exists());
        assert!(fs::read_dir(&paths.error_dir)?.any(|entry| {
            entry
                .ok()
                .is_some_and(|entry| entry.file_name().to_string_lossy().contains("invalid-raw"))
        }));
        Ok(())
    }
}
