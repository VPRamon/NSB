//! Restartable Gaia TAP queries with auditable on-disk artefacts.
//!
//! The client deliberately keeps query submission separate from validation.  The
//! exact ADQL text is sent as an `application/x-www-form-urlencoded` field and is
//! also persisted byte-for-byte before the first request.  Both synchronous TAP
//! and asynchronous UWS jobs use the same result validation and manifest format.

use crate::checksum_io::sha256_file;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, LOCATION, RETRY_AFTER};
use reqwest::{Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Public Gaia Archive TAP endpoint used by the command-line tool by default.
pub const DEFAULT_GAIA_TAP_ENDPOINT: &str = "https://gea.esac.esa.int/tap-server/tap";

/// TAP execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapMode {
    /// Submit the query to the TAP synchronous endpoint.
    Sync,
    /// Create and poll an asynchronous UWS job.
    Async,
}

/// Supported TAP result serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TapFormat {
    /// Comma-separated values with a header row.
    Csv,
    /// IVOA VOTable XML (TABLEDATA row counting is supported).
    Votable,
}

impl TapFormat {
    fn tap_value(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Votable => "votable",
        }
    }

    /// Conventional filename extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Votable => "vot",
        }
    }

    fn accept(self) -> &'static str {
        match self {
            Self::Csv => "text/csv, application/x-votable+xml;q=0.5",
            Self::Votable => "application/x-votable+xml, text/xml;q=0.9",
        }
    }
}

/// Network, retry, and polling limits for a TAP client.
#[derive(Debug, Clone)]
pub struct TapClientConfig {
    /// Timeout for an individual HTTP request.
    pub request_timeout: Duration,
    /// Wall-clock limit for submission, polling, and download.
    pub total_timeout: Duration,
    /// Delay between non-terminal UWS phase polls.
    pub poll_interval: Duration,
    /// Maximum attempts for a transient request failure, including the first.
    pub max_attempts: u32,
    /// First exponential retry delay.
    pub initial_backoff: Duration,
    /// Maximum exponential retry delay.
    pub max_backoff: Duration,
}

impl Default for TapClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(120),
            total_timeout: Duration::from_secs(6 * 60 * 60),
            poll_interval: Duration::from_secs(2),
            max_attempts: 5,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl TapClientConfig {
    /// Reject configurations that could disable bounds or create retry loops.
    pub fn validate(&self) -> Result<()> {
        if self.request_timeout.is_zero() || self.total_timeout.is_zero() {
            bail!("TAP request and total timeouts must be positive");
        }
        if self.poll_interval.is_zero() {
            bail!("TAP poll interval must be positive");
        }
        if self.max_attempts == 0 {
            bail!("TAP max attempts must be positive");
        }
        if self.initial_backoff.is_zero() || self.max_backoff < self.initial_backoff {
            bail!("TAP backoff must be positive and max >= initial");
        }
        Ok(())
    }
}

/// A single query and its reproducibility/validation contract.
#[derive(Debug, Clone)]
pub struct TapRequest {
    /// ADQL sent to TAP and saved without whitespace normalization.
    pub adql: String,
    /// Synchronous request or asynchronous UWS job.
    pub mode: TapMode,
    /// Requested and validated result format.
    pub format: TapFormat,
    /// Optional TAP `MAXREC` limit.  Reaching it in CSV is treated as truncation.
    pub maxrec: Option<u64>,
    /// Optional exact number of data rows required after parsing.
    pub expected_rows: Option<u64>,
    /// Final result path.  A sibling partial file is used until validation passes.
    pub output_path: PathBuf,
    /// Directory for query, manifest, status, headers, log, and error response.
    pub artifact_dir: PathBuf,
    /// Permit replacement of an existing final result for a new query.
    pub overwrite: bool,
}

impl TapRequest {
    /// Construct an asynchronous CSV request with conservative defaults.
    pub fn new(
        adql: impl Into<String>,
        output_path: impl Into<PathBuf>,
        artifact_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            adql: adql.into(),
            mode: TapMode::Async,
            format: TapFormat::Csv,
            maxrec: None,
            expected_rows: None,
            output_path: output_path.into(),
            artifact_dir: artifact_dir.into(),
            overwrite: false,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.adql.trim().is_empty() {
            bail!("ADQL query must not be empty");
        }
        if self.maxrec == Some(0) {
            bail!("MAXREC must be positive when supplied");
        }
        if self.output_path.as_os_str().is_empty() || self.artifact_dir.as_os_str().is_empty() {
            bail!("TAP output and artifact paths must not be empty");
        }
        Ok(())
    }
}

/// Stable filenames produced for every query.
#[derive(Debug, Clone)]
pub struct TapArtifactPaths {
    /// Artifact directory.
    pub directory: PathBuf,
    /// Exact submitted ADQL.
    pub query: PathBuf,
    /// Machine-readable restart manifest.
    pub manifest: PathBuf,
    /// JSON-lines event log.
    pub log: PathBuf,
    /// Chronological HTTP/UWS status record.
    pub status: PathBuf,
    /// Chronological response-header record.
    pub headers: PathBuf,
    /// Complete final or transient service-error bodies, when any exist.
    pub body_error: PathBuf,
}

impl TapArtifactPaths {
    /// Resolve standard artifact filenames below `directory`.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        Self {
            query: directory.join("query.adql"),
            manifest: directory.join("manifest.json"),
            log: directory.join("events.jsonl"),
            status: directory.join("status.txt"),
            headers: directory.join("headers.txt"),
            body_error: directory.join("body-error.bin"),
            directory,
        }
    }
}

/// On-disk state sufficient to audit or resume an asynchronous query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TapManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// TAP service root.
    pub endpoint: String,
    /// Selected TAP mode.
    pub mode: TapMode,
    /// Selected result format.
    pub format: TapFormat,
    /// Requested row limit.
    pub maxrec: Option<u64>,
    /// Required exact result population.
    pub expected_rows: Option<u64>,
    /// Absolute path to the exact ADQL file.
    pub query_path: String,
    /// Absolute final result path.
    pub output_path: String,
    /// Current local/UWS state.
    pub status: String,
    /// UWS job URL, recorded immediately after submission.
    pub job_url: Option<String>,
    /// Result URL used after job completion.
    pub result_url: Option<String>,
    /// Most recent HTTP status.
    pub http_status: Option<u16>,
    /// Validated result size in bytes.
    pub bytes: Option<u64>,
    /// Streaming SHA-256 of the validated final result.
    pub sha256: Option<String>,
    /// Parsed row count when the encoding makes it available.
    pub row_count: Option<u64>,
    /// Whether TAP explicitly or conservatively indicated truncation.
    pub truncated: bool,
    /// Whether a TAP service error was embedded in a successful HTTP response.
    pub service_error: bool,
    /// Last failure detail, if any.
    pub error: Option<String>,
    /// Creation timestamp as Unix milliseconds.
    pub created_unix_millis: u128,
    /// Last update timestamp as Unix milliseconds.
    pub updated_unix_millis: u128,
}

impl TapManifest {
    /// Read a manifest without making a network request.
    pub fn read(path: &Path) -> Result<Self> {
        let raw = fs::read(path)
            .with_context(|| format!("failed to read TAP manifest {}", path.display()))?;
        let manifest: Self = serde_json::from_slice(&raw)
            .with_context(|| format!("invalid TAP manifest {}", path.display()))?;
        if manifest.schema_version != 1 {
            bail!(
                "unsupported TAP manifest schema {} in {}",
                manifest.schema_version,
                path.display()
            );
        }
        Ok(manifest)
    }
}

/// Successful query result and its audit locations.
#[derive(Debug, Clone, Serialize)]
pub struct TapOutcome {
    /// Final validated result path.
    pub output_path: PathBuf,
    /// SHA-256 without an algorithm prefix.
    pub sha256: String,
    /// File size.
    pub bytes: u64,
    /// Parsed row count when available.
    pub row_count: Option<u64>,
    /// UWS job URL for asynchronous requests.
    pub job_url: Option<String>,
    /// Manifest containing all restart metadata.
    pub manifest_path: PathBuf,
    /// True when a completed manifest/result avoided a new download.
    pub resumed_existing_result: bool,
}

/// HTTP client for bounded, auditable Gaia TAP queries.
#[derive(Debug)]
pub struct GaiaTapClient {
    endpoint: Url,
    client: reqwest::Client,
    config: TapClientConfig,
}

impl GaiaTapClient {
    /// Build a TAP client. Redirects are handled explicitly so UWS `Location`
    /// headers and every intermediate response remain observable.
    pub fn new(endpoint: &str, config: TapClientConfig) -> Result<Self> {
        config.validate()?;
        let endpoint = Url::parse(&format!("{}/", endpoint.trim_end_matches('/')))
            .with_context(|| format!("invalid TAP endpoint {endpoint:?}"))?;
        if endpoint.scheme() != "http" && endpoint.scheme() != "https" {
            bail!("TAP endpoint must use HTTP or HTTPS");
        }
        let client = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!(
                "NSB/",
                env!("CARGO_PKG_VERSION"),
                " reproducible-Gaia-TAP"
            ))
            .build()
            .context("failed to build TAP HTTP client")?;
        Ok(Self {
            endpoint,
            client,
            config,
        })
    }

    /// Submit a new synchronous query or asynchronous UWS job.
    pub async fn execute(&self, request: &TapRequest) -> Result<TapOutcome> {
        request.validate()?;
        let request = absolutize_request(request)?;
        if request.output_path.exists() && !request.overwrite {
            bail!(
                "TAP output already exists at {}; use overwrite or resume its manifest",
                request.output_path.display()
            );
        }
        let paths = prepare_artifacts(&request, true)?;
        let mut manifest = self.new_manifest(&request, &paths);
        save_manifest(&paths.manifest, &mut manifest)?;
        log_event(&paths, "prepared", "query and restart manifest persisted")?;
        let deadline = Instant::now() + self.config.total_timeout;

        let result = match request.mode {
            TapMode::Sync => {
                self.run_sync(&request, &paths, &mut manifest, deadline)
                    .await
            }
            TapMode::Async => {
                self.submit_async(&request, &paths, &mut manifest, deadline)
                    .await
            }
        };
        self.finish_or_record_error(result, &paths, &mut manifest)
    }

    /// Resume polling and downloading an already-created UWS job URL.
    pub async fn resume_job(&self, job_url: &str, request: &TapRequest) -> Result<TapOutcome> {
        request.validate()?;
        if request.mode != TapMode::Async {
            bail!("resuming a UWS job requires async TAP mode");
        }
        let job_url = Url::parse(job_url).context("invalid UWS job URL")?;
        let request = absolutize_request(request)?;
        if request.output_path.exists() && !request.overwrite {
            bail!(
                "TAP output already exists at {}; resume the manifest instead",
                request.output_path.display()
            );
        }
        let paths = prepare_artifacts(&request, true)?;
        let mut manifest = self.new_manifest(&request, &paths);
        manifest.job_url = Some(job_url.to_string());
        manifest.status = "resuming".to_owned();
        save_manifest(&paths.manifest, &mut manifest)?;
        log_event(&paths, "resuming", job_url.as_str())?;
        let deadline = Instant::now() + self.config.total_timeout;
        let result = self
            .poll_async_job(&job_url, &request, &paths, &mut manifest, deadline)
            .await;
        self.finish_or_record_error(result, &paths, &mut manifest)
    }

    /// Resume from an on-disk manifest. A completed, checksum-matching output is
    /// validated locally and returned without contacting TAP.
    pub async fn resume_manifest(&self, manifest_path: &Path) -> Result<TapOutcome> {
        let manifest_path = absolute_path(manifest_path)?;
        let mut paths = TapArtifactPaths::new(
            manifest_path
                .parent()
                .ok_or_else(|| anyhow!("manifest has no parent directory"))?,
        );
        paths.manifest = manifest_path.clone();
        let mut manifest = TapManifest::read(&manifest_path)?;
        if manifest.mode != TapMode::Async {
            bail!("only asynchronous TAP manifests can be resumed");
        }
        if normalize_endpoint(&manifest.endpoint)? != self.endpoint {
            bail!(
                "manifest endpoint {} does not match client endpoint {}",
                manifest.endpoint,
                self.endpoint
            );
        }
        let query_path = PathBuf::from(&manifest.query_path);
        let adql = fs::read_to_string(&query_path)
            .with_context(|| format!("failed to read saved ADQL {}", query_path.display()))?;
        let request = TapRequest {
            adql,
            mode: TapMode::Async,
            format: manifest.format,
            maxrec: manifest.maxrec,
            expected_rows: manifest.expected_rows,
            output_path: PathBuf::from(&manifest.output_path),
            artifact_dir: paths.directory.clone(),
            overwrite: true,
        };
        request.validate()?;

        if manifest.status == "completed" && request.output_path.is_file() {
            let sha256 = sha256_file(&request.output_path)?;
            if manifest.sha256.as_deref() == Some(sha256.as_str()) {
                let validated = validate_result(
                    &request.output_path,
                    request.format,
                    request.maxrec,
                    request.expected_rows,
                )
                .map_err(|failure| anyhow!(failure.message))?;
                log_event(&paths, "resume-complete", "checksum and validation reused")?;
                return Ok(TapOutcome {
                    bytes: fs::metadata(&request.output_path)?.len(),
                    output_path: request.output_path,
                    sha256,
                    row_count: validated.row_count,
                    job_url: manifest.job_url,
                    manifest_path,
                    resumed_existing_result: true,
                });
            }
            log_event(
                &paths,
                "resume-redownload",
                "completed output is missing or its checksum changed",
            )?;
        }

        let job_url = manifest
            .job_url
            .as_deref()
            .ok_or_else(|| anyhow!("manifest has no UWS job URL to resume"))?;
        let job_url = Url::parse(job_url).context("invalid saved UWS job URL")?;
        manifest.status = "resuming".to_owned();
        manifest.error = None;
        save_manifest(&paths.manifest, &mut manifest)?;
        log_event(&paths, "resuming", job_url.as_str())?;
        let deadline = Instant::now() + self.config.total_timeout;
        let result = self
            .poll_async_job(&job_url, &request, &paths, &mut manifest, deadline)
            .await;
        self.finish_or_record_error(result, &paths, &mut manifest)
    }

    fn new_manifest(&self, request: &TapRequest, paths: &TapArtifactPaths) -> TapManifest {
        let now = unix_millis();
        TapManifest {
            schema_version: 1,
            endpoint: self.endpoint.as_str().trim_end_matches('/').to_owned(),
            mode: request.mode,
            format: request.format,
            maxrec: request.maxrec,
            expected_rows: request.expected_rows,
            query_path: paths.query.display().to_string(),
            output_path: request.output_path.display().to_string(),
            status: "prepared".to_owned(),
            job_url: None,
            result_url: None,
            http_status: None,
            bytes: None,
            sha256: None,
            row_count: None,
            truncated: false,
            service_error: false,
            error: None,
            created_unix_millis: now,
            updated_unix_millis: now,
        }
    }

    fn finish_or_record_error(
        &self,
        result: Result<TapOutcome>,
        paths: &TapArtifactPaths,
        manifest: &mut TapManifest,
    ) -> Result<TapOutcome> {
        match result {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                if is_timeout_error(&error) {
                    manifest.status = "timed_out".to_owned();
                } else if manifest.status != "timed_out"
                    && manifest.status != "service_error"
                    && manifest.status != "truncated"
                {
                    manifest.status = "failed".to_owned();
                }
                manifest.error = Some(format!("{error:#}"));
                let save_result = save_manifest(&paths.manifest, manifest);
                let log_result = log_event(paths, &manifest.status, &format!("{error:#}"));
                if let Err(save_error) = save_result {
                    return Err(
                        error.context(format!("also failed to save TAP manifest: {save_error:#}"))
                    );
                }
                if let Err(log_error) = log_result {
                    return Err(
                        error.context(format!("also failed to append TAP log: {log_error:#}"))
                    );
                }
                Err(error)
            }
        }
    }

    async fn run_sync(
        &self,
        request: &TapRequest,
        paths: &TapArtifactPaths,
        manifest: &mut TapManifest,
        deadline: Instant,
    ) -> Result<TapOutcome> {
        let url = self.endpoint.join("sync")?;
        let form = query_form(request, false);
        manifest.status = "submitting".to_owned();
        save_manifest(&paths.manifest, manifest)?;
        log_event(paths, "submitting", url.as_str())?;
        let response = self
            .send_with_retry(deadline, paths, "sync-submit", || {
                self.client
                    .post(url.clone())
                    .header(reqwest::header::ACCEPT, request.format.accept())
                    .form(&form)
            })
            .await?;
        manifest.http_status = Some(response.status().as_u16());
        if !response.status().is_success() {
            let status = response.status();
            append_response_body(response, &paths.body_error, "sync final error").await?;
            bail!("TAP sync returned non-retryable HTTP {status}");
        }
        self.consume_result(response, request, paths, manifest, None, deadline)
            .await
    }

    async fn submit_async(
        &self,
        request: &TapRequest,
        paths: &TapArtifactPaths,
        manifest: &mut TapManifest,
        deadline: Instant,
    ) -> Result<TapOutcome> {
        let url = self.endpoint.join("async")?;
        let form = query_form(request, true);
        manifest.status = "submitting".to_owned();
        save_manifest(&paths.manifest, manifest)?;
        log_event(paths, "submitting", url.as_str())?;
        let response = self
            .send_with_retry(deadline, paths, "async-submit", || {
                self.client
                    .post(url.clone())
                    .header(reqwest::header::ACCEPT, "application/xml, text/plain;q=0.5")
                    .form(&form)
            })
            .await?;
        let status = response.status();
        manifest.http_status = Some(status.as_u16());
        if !(status.is_success() || status.is_redirection()) {
            append_response_body(response, &paths.body_error, "async submission error").await?;
            bail!("TAP async submission returned non-retryable HTTP {status}");
        }
        let location = response
            .headers()
            .get(LOCATION)
            .ok_or_else(|| anyhow!("TAP async submission HTTP {status} omitted Location"))
            .and_then(|value| {
                value
                    .to_str()
                    .map(str::to_owned)
                    .context("TAP async Location is not valid text")
            });
        let location = match location {
            Ok(location) => location,
            Err(error) => {
                append_response_body(response, &paths.body_error, "async submission error").await?;
                return Err(error);
            }
        };
        let job_url = response
            .url()
            .join(&location)
            .with_context(|| format!("invalid UWS Location {location:?}"))?;
        manifest.job_url = Some(job_url.to_string());
        manifest.status = "submitted".to_owned();
        save_manifest(&paths.manifest, manifest)?;
        log_event(paths, "submitted", job_url.as_str())?;
        self.poll_async_job(&job_url, request, paths, manifest, deadline)
            .await
    }

    async fn poll_async_job(
        &self,
        job_url: &Url,
        request: &TapRequest,
        paths: &TapArtifactPaths,
        manifest: &mut TapManifest,
        deadline: Instant,
    ) -> Result<TapOutcome> {
        let phase_url = child_url(job_url, "phase")?;
        let mut started_pending_job = false;
        loop {
            ensure_time_remaining(deadline)?;
            let response = self
                .send_with_retry(deadline, paths, "uws-phase", || {
                    self.client
                        .get(phase_url.clone())
                        .header(reqwest::header::ACCEPT, "text/plain, application/xml;q=0.5")
                })
                .await
                .inspect_err(|error| {
                    if is_timeout_error(error) {
                        manifest.status = "timed_out".to_owned();
                    }
                })?;
            manifest.http_status = Some(response.status().as_u16());
            if !response.status().is_success() {
                let status = response.status();
                append_response_body(response, &paths.body_error, "UWS phase error").await?;
                bail!("UWS phase returned non-retryable HTTP {status}");
            }
            let phase = response
                .text()
                .await
                .context("failed to read UWS phase response")?;
            let phase = phase.trim().to_ascii_uppercase();
            append_status(paths, &format!("UWS PHASE {phase}"))?;
            log_event(paths, "uws-phase", &phase)?;
            manifest.status = phase.to_ascii_lowercase();
            save_manifest(&paths.manifest, manifest)?;

            match phase.as_str() {
                "COMPLETED" => break,
                "ERROR" => {
                    manifest.status = "service_error".to_owned();
                    manifest.service_error = true;
                    self.capture_uws_error(job_url, deadline, paths).await?;
                    bail!("UWS job entered ERROR phase");
                }
                "ABORTED" => bail!("UWS job was aborted"),
                "UNKNOWN" => bail!("UWS job entered UNKNOWN phase"),
                "HELD" | "SUSPENDED" => {
                    bail!("UWS job entered non-progressing {phase} phase")
                }
                "PENDING" if !started_pending_job => {
                    self.start_pending_job(job_url, deadline, paths).await?;
                    started_pending_job = true;
                }
                "PENDING" | "QUEUED" | "EXECUTING" => {}
                other => bail!("unrecognized UWS phase {other:?}"),
            }
            let remaining = ensure_time_remaining(deadline)?;
            tokio::time::sleep(self.config.poll_interval.min(remaining)).await;
        }

        let mut result_url = child_url(job_url, "results/result")?;
        manifest.result_url = Some(result_url.to_string());
        manifest.status = "downloading".to_owned();
        save_manifest(&paths.manifest, manifest)?;
        log_event(paths, "downloading", result_url.as_str())?;

        for redirects in 0..=5 {
            let response = self
                .send_with_retry(deadline, paths, "uws-result", || {
                    self.client
                        .get(result_url.clone())
                        .header(reqwest::header::ACCEPT, request.format.accept())
                })
                .await
                .inspect_err(|error| {
                    if is_timeout_error(error) {
                        manifest.status = "timed_out".to_owned();
                    }
                })?;
            manifest.http_status = Some(response.status().as_u16());
            if response.status().is_redirection() {
                if redirects == 5 {
                    bail!("too many redirects while retrieving UWS result");
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .ok_or_else(|| anyhow!("UWS result redirect omitted Location"))?
                    .to_str()
                    .context("UWS result Location is not valid text")?;
                result_url = response.url().join(location)?;
                manifest.result_url = Some(result_url.to_string());
                save_manifest(&paths.manifest, manifest)?;
                log_event(paths, "result-redirect", result_url.as_str())?;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                append_response_body(response, &paths.body_error, "UWS result error").await?;
                bail!("UWS result returned non-retryable HTTP {status}");
            }
            return self
                .consume_result(response, request, paths, manifest, Some(job_url), deadline)
                .await;
        }
        unreachable!("redirect loop always returns or errors")
    }

    async fn start_pending_job(
        &self,
        job_url: &Url,
        deadline: Instant,
        paths: &TapArtifactPaths,
    ) -> Result<()> {
        let phase_url = child_url(job_url, "phase")?;
        let form = [("PHASE", "RUN")];
        let response = self
            .send_with_retry(deadline, paths, "uws-start", || {
                self.client.post(phase_url.clone()).form(&form)
            })
            .await?;
        if !(response.status().is_success() || response.status().is_redirection()) {
            let status = response.status();
            append_response_body(response, &paths.body_error, "UWS start error").await?;
            bail!("starting pending UWS job returned HTTP {status}");
        }
        log_event(paths, "uws-started", job_url.as_str())?;
        Ok(())
    }

    async fn capture_uws_error(
        &self,
        job_url: &Url,
        deadline: Instant,
        paths: &TapArtifactPaths,
    ) -> Result<()> {
        let error_url = child_url(job_url, "error")?;
        let response = self
            .send_with_retry(deadline, paths, "uws-error", || {
                self.client.get(error_url.clone())
            })
            .await?;
        append_response_body(response, &paths.body_error, "UWS service error").await
    }

    async fn consume_result(
        &self,
        response: Response,
        request: &TapRequest,
        paths: &TapArtifactPaths,
        manifest: &mut TapManifest,
        job_url: Option<&Url>,
        deadline: Instant,
    ) -> Result<TapOutcome> {
        let part_path = partial_path(&request.output_path);
        if part_path.exists() {
            fs::remove_file(&part_path)
                .with_context(|| format!("failed to remove stale {}", part_path.display()))?;
        }
        let remaining = ensure_time_remaining(deadline)?;
        let bytes =
            match tokio::time::timeout(remaining, download_response(response, &part_path)).await {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(error)) => {
                    if part_path.exists() {
                        preserve_result_as_error(
                            &part_path,
                            &paths.body_error,
                            &format!("TAP result stream failed: {error:#}"),
                        )?;
                    }
                    return Err(error);
                }
                Err(_) => {
                    manifest.status = "timed_out".to_owned();
                    if part_path.exists() {
                        preserve_result_as_error(
                            &part_path,
                            &paths.body_error,
                            "TAP result download exceeded the total timeout",
                        )?;
                    }
                    bail!("TAP operation timed out while downloading the result");
                }
            };
        let sha256 = sha256_file(&part_path)?;
        manifest.bytes = Some(bytes);
        manifest.sha256 = Some(sha256.clone());
        manifest.status = "validating".to_owned();
        save_manifest(&paths.manifest, manifest)?;
        log_event(
            paths,
            "validating",
            &format!("{} bytes sha256:{sha256}", bytes),
        )?;

        let validation = validate_result(
            &part_path,
            request.format,
            request.maxrec,
            request.expected_rows,
        );
        let validated = match validation {
            Ok(validated) => validated,
            Err(failure) => {
                manifest.row_count = failure.row_count;
                match failure.kind {
                    ValidationFailureKind::Service => {
                        manifest.service_error = true;
                        manifest.status = "service_error".to_owned();
                        preserve_result_as_error(&part_path, &paths.body_error, &failure.message)?;
                    }
                    ValidationFailureKind::Truncated => {
                        manifest.truncated = true;
                        manifest.status = "truncated".to_owned();
                        preserve_result_as_error(&part_path, &paths.body_error, &failure.message)?;
                    }
                    ValidationFailureKind::Malformed => {
                        preserve_result_as_error(&part_path, &paths.body_error, &failure.message)?;
                    }
                    ValidationFailureKind::RowCount => {
                        replace_file(&part_path, &request.output_path)?;
                        append_error_text(&paths.body_error, &failure.message)?;
                    }
                }
                bail!(failure.message);
            }
        };

        if request.output_path.exists() && request.overwrite {
            fs::remove_file(&request.output_path).with_context(|| {
                format!(
                    "failed to replace existing TAP output {}",
                    request.output_path.display()
                )
            })?;
        }
        replace_file(&part_path, &request.output_path)?;
        manifest.row_count = validated.row_count;
        manifest.status = "completed".to_owned();
        manifest.error = None;
        save_manifest(&paths.manifest, manifest)?;
        log_event(
            paths,
            "completed",
            &format!(
                "{} rows, sha256:{sha256}",
                validated
                    .row_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unknown".to_owned())
            ),
        )?;
        Ok(TapOutcome {
            output_path: request.output_path.clone(),
            sha256,
            bytes,
            row_count: validated.row_count,
            job_url: job_url
                .map(ToString::to_string)
                .or_else(|| manifest.job_url.clone()),
            manifest_path: paths.manifest.clone(),
            resumed_existing_result: false,
        })
    }

    async fn send_with_retry<F>(
        &self,
        deadline: Instant,
        paths: &TapArtifactPaths,
        label: &str,
        build: F,
    ) -> Result<Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let mut backoff = self.config.initial_backoff;
        for attempt in 1..=self.config.max_attempts {
            let remaining = ensure_time_remaining(deadline)?;
            log_event(paths, "http-attempt", &format!("{label} attempt {attempt}"))?;
            let sent = tokio::time::timeout(remaining, build().send()).await;
            match sent {
                Ok(Ok(response)) => {
                    append_response_metadata(paths, label, attempt, &response)?;
                    if is_transient_status(response.status()) && attempt < self.config.max_attempts
                    {
                        let delay = retry_delay(response.headers(), backoff);
                        let status = response.status();
                        append_response_body(
                            response,
                            &paths.body_error,
                            &format!("transient {label} attempt {attempt}"),
                        )
                        .await?;
                        log_event(
                            paths,
                            "retry",
                            &format!("{label} HTTP {status}; delay {} ms", delay.as_millis()),
                        )?;
                        sleep_before_retry(deadline, delay).await?;
                        backoff = backoff.saturating_mul(2).min(self.config.max_backoff);
                        continue;
                    }
                    return Ok(response);
                }
                Ok(Err(error)) if is_transient_request_error(&error) => {
                    if attempt == self.config.max_attempts {
                        return Err(error).context(format!(
                            "transient {label} request failed after {attempt} attempts"
                        ));
                    }
                    log_event(
                        paths,
                        "retry",
                        &format!(
                            "{label} transport error: {error}; delay {} ms",
                            backoff.as_millis()
                        ),
                    )?;
                    sleep_before_retry(deadline, backoff).await?;
                    backoff = backoff.saturating_mul(2).min(self.config.max_backoff);
                }
                Ok(Err(error)) => {
                    return Err(error).context(format!("non-retryable {label} request failure"));
                }
                Err(_) => bail!("TAP operation timed out during {label}"),
            }
        }
        unreachable!("positive max_attempts always returns")
    }
}

fn query_form(request: &TapRequest, asynchronous: bool) -> Vec<(&'static str, String)> {
    let mut form = vec![
        ("REQUEST", "doQuery".to_owned()),
        ("LANG", "ADQL".to_owned()),
        ("FORMAT", request.format.tap_value().to_owned()),
        ("QUERY", request.adql.clone()),
    ];
    if let Some(maxrec) = request.maxrec {
        form.push(("MAXREC", maxrec.to_string()));
    }
    if asynchronous {
        form.push(("PHASE", "RUN".to_owned()));
    }
    form
}

fn absolutize_request(request: &TapRequest) -> Result<TapRequest> {
    let mut request = request.clone();
    request.output_path = absolute_path(&request.output_path)?;
    request.artifact_dir = absolute_path(&request.artifact_dir)?;
    Ok(request)
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("failed to determine current directory")?
            .join(path))
    }
}

fn prepare_artifacts(request: &TapRequest, reset: bool) -> Result<TapArtifactPaths> {
    fs::create_dir_all(&request.artifact_dir).with_context(|| {
        format!(
            "failed to create TAP artifact directory {}",
            request.artifact_dir.display()
        )
    })?;
    if let Some(parent) = request.output_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create TAP output directory {}", parent.display())
        })?;
    }
    let paths = TapArtifactPaths::new(request.artifact_dir.clone());
    if reset {
        for path in [
            &paths.query,
            &paths.manifest,
            &paths.log,
            &paths.status,
            &paths.headers,
            &paths.body_error,
        ] {
            if path.exists() {
                fs::remove_file(path)
                    .with_context(|| format!("failed to reset TAP artifact {}", path.display()))?;
            }
        }
    }
    fs::write(&paths.query, request.adql.as_bytes())
        .with_context(|| format!("failed to persist exact ADQL to {}", paths.query.display()))?;
    Ok(paths)
}

fn normalize_endpoint(endpoint: &str) -> Result<Url> {
    Url::parse(&format!("{}/", endpoint.trim_end_matches('/')))
        .with_context(|| format!("invalid TAP endpoint {endpoint:?}"))
}

fn child_url(base: &Url, child: &str) -> Result<Url> {
    let mut base = base.clone();
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(child)
        .with_context(|| format!("failed to append {child:?} to {base}"))
}

fn partial_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("tap-result"))
        .to_os_string();
    name.push(".tap.part");
    output.with_file_name(name)
}

async fn download_response(response: Response, path: &Path) -> Result<u64> {
    let mut file = File::create(path)
        .with_context(|| format!("failed to create TAP partial result {}", path.display()))?;
    let mut bytes = 0_u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed while streaming TAP result body")?;
        file.write_all(&chunk)
            .with_context(|| format!("failed to write TAP partial result {}", path.display()))?;
        bytes = bytes
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| anyhow!("TAP result byte count overflow"))?;
    }
    file.flush()?;
    file.sync_all()?;
    Ok(bytes)
}

async fn append_response_body(response: Response, path: &Path, label: &str) -> Result<()> {
    let status = response.status();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open TAP body-error {}", path.display()))?;
    writeln!(file, "\n--- {label}; HTTP {status} ---")?;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed while saving TAP error response")?;
        file.write_all(&chunk)?;
    }
    writeln!(file)?;
    file.flush()?;
    Ok(())
}

fn append_error_text(path: &Path, message: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "\n--- local validation error ---\n{message}")?;
    Ok(())
}

fn preserve_result_as_error(source: &Path, destination: &Path, message: &str) -> Result<()> {
    append_error_text(destination, message)?;
    let mut input =
        BufReader::new(File::open(source).with_context(|| {
            format!("failed to reopen invalid TAP result {}", source.display())
        })?);
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(destination)?;
    writeln!(output, "--- response body follows ---")?;
    std::io::copy(&mut input, &mut output)?;
    writeln!(output)?;
    fs::remove_file(source)?;
    Ok(())
}

fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    fs::rename(source, destination).with_context(|| {
        format!(
            "failed to atomically move {} to {}",
            source.display(),
            destination.display()
        )
    })
}

fn append_response_metadata(
    paths: &TapArtifactPaths,
    label: &str,
    attempt: u32,
    response: &Response,
) -> Result<()> {
    append_status(
        paths,
        &format!(
            "HTTP {label} attempt={attempt} status={}",
            response.status()
        ),
    )?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.headers)?;
    writeln!(
        file,
        "HTTP {label} attempt={attempt} status={} url={}",
        response.status(),
        response.url()
    )?;
    for (name, value) in response.headers() {
        writeln!(
            file,
            "{}: {}",
            name.as_str(),
            value.to_str().unwrap_or("<non-UTF-8 header>")
        )?;
    }
    writeln!(file)?;
    Ok(())
}

fn append_status(paths: &TapArtifactPaths, status: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.status)?;
    writeln!(file, "{} {status}", unix_millis())?;
    Ok(())
}

#[derive(Serialize)]
struct LogEvent<'a> {
    unix_millis: u128,
    event: &'a str,
    detail: &'a str,
}

fn log_event(paths: &TapArtifactPaths, event: &str, detail: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log)?;
    serde_json::to_writer(
        &mut file,
        &LogEvent {
            unix_millis: unix_millis(),
            event,
            detail,
        },
    )?;
    writeln!(file)?;
    Ok(())
}

fn save_manifest(path: &Path, manifest: &mut TapManifest) -> Result<()> {
    manifest.updated_unix_millis = unix_millis();
    let temporary = path.with_extension("json.part");
    let mut file = File::create(&temporary)
        .with_context(|| format!("failed to create TAP manifest {}", temporary.display()))?;
    serde_json::to_writer_pretty(&mut file, manifest)?;
    writeln!(file)?;
    file.flush()?;
    file.sync_all()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to publish TAP manifest {}", path.display()))?;
    Ok(())
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn is_transient_status(status: StatusCode) -> bool {
    matches!(status.as_u16(), 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

fn is_transient_request_error(error: &reqwest::Error) -> bool {
    error.status().is_none()
        && (error.is_timeout() || error.is_connect() || error.is_request() || error.is_body())
}

fn retry_delay(headers: &HeaderMap, fallback: Duration) -> Duration {
    let Some(value) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    else {
        return fallback;
    };
    if let Ok(seconds) = value.parse::<u64>() {
        return Duration::from_secs(seconds);
    }
    if let Ok(when) = httpdate::parse_http_date(value) {
        return when.duration_since(SystemTime::now()).unwrap_or_default();
    }
    fallback
}

async fn sleep_before_retry(deadline: Instant, delay: Duration) -> Result<()> {
    let remaining = ensure_time_remaining(deadline)?;
    if delay >= remaining {
        bail!("TAP operation timed out before the next retry");
    }
    tokio::time::sleep(delay).await;
    Ok(())
}

fn ensure_time_remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| anyhow!("TAP operation timed out"))
}

fn is_timeout_error(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("timed out")
}

#[derive(Debug)]
struct ValidatedResult {
    row_count: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
enum ValidationFailureKind {
    Service,
    Truncated,
    Malformed,
    RowCount,
}

#[derive(Debug)]
struct ValidationFailure {
    kind: ValidationFailureKind,
    message: String,
    row_count: Option<u64>,
}

fn validate_result(
    path: &Path,
    format: TapFormat,
    maxrec: Option<u64>,
    expected_rows: Option<u64>,
) -> std::result::Result<ValidatedResult, ValidationFailure> {
    let row_count = match format {
        TapFormat::Csv => validate_csv(path)?,
        TapFormat::Votable => validate_votable(path)?,
    };
    if let (Some(maxrec), Some(rows)) = (maxrec, row_count) {
        if rows >= maxrec {
            return Err(ValidationFailure {
                kind: ValidationFailureKind::Truncated,
                message: format!(
                    "TAP result has {rows} rows, reaching MAXREC={maxrec}; truncation cannot be excluded"
                ),
                row_count,
            });
        }
    }
    if let Some(expected) = expected_rows {
        match row_count {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return Err(ValidationFailure {
                    kind: ValidationFailureKind::RowCount,
                    message: format!(
                        "TAP population validation failed: expected {expected} rows, parsed {actual}"
                    ),
                    row_count,
                });
            }
            None => {
                return Err(ValidationFailure {
                    kind: ValidationFailureKind::RowCount,
                    message: format!(
                        "TAP population validation requires {expected} rows, but this VOTable encoding is not countable"
                    ),
                    row_count,
                });
            }
        }
    }
    Ok(ValidatedResult { row_count })
}

fn validate_csv(path: &Path) -> std::result::Result<Option<u64>, ValidationFailure> {
    let mut prefix_file = File::open(path).map_err(|error| malformed(path, error))?;
    let mut prefix = vec![0_u8; 128 * 1024];
    let read = prefix_file
        .read(&mut prefix)
        .map_err(|error| malformed(path, error))?;
    prefix.truncate(read);
    let prefix_text = String::from_utf8_lossy(&prefix);
    match csv_embedded_status(&prefix_text) {
        Some(EmbeddedTapStatus::Error) => {
            return Err(ValidationFailure {
                kind: ValidationFailureKind::Service,
                message: "TAP service error found in CSV response body".to_owned(),
                row_count: None,
            });
        }
        Some(EmbeddedTapStatus::Overflow) => {
            return Err(ValidationFailure {
                kind: ValidationFailureKind::Truncated,
                message: "TAP returned QUERY_STATUS=OVERFLOW instead of CSV".to_owned(),
                row_count: None,
            });
        }
        None => {}
    }
    let trimmed = prefix_text.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    if trimmed.starts_with('<') {
        return Err(ValidationFailure {
            kind: ValidationFailureKind::Malformed,
            message: "TAP returned XML when CSV was requested".to_owned(),
            row_count: None,
        });
    }
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_path(path)
        .map_err(|error| malformed(path, error))?;
    let headers = reader.headers().map_err(|error| malformed(path, error))?;
    if headers.is_empty() || (headers.len() == 1 && headers.get(0).unwrap_or("").trim().is_empty())
    {
        return Err(ValidationFailure {
            kind: ValidationFailureKind::Malformed,
            message: "TAP CSV has no non-empty header row".to_owned(),
            row_count: None,
        });
    }
    let mut rows = 0_u64;
    for record in reader.records() {
        record.map_err(|error| malformed(path, error))?;
        rows = rows.checked_add(1).ok_or_else(|| ValidationFailure {
            kind: ValidationFailureKind::Malformed,
            message: "TAP CSV row count overflow".to_owned(),
            row_count: None,
        })?;
    }
    Ok(Some(rows))
}

#[derive(Debug, Clone, Copy)]
enum EmbeddedTapStatus {
    Error,
    Overflow,
}

fn csv_embedded_status(prefix: &str) -> Option<EmbeddedTapStatus> {
    let lower = prefix.to_ascii_lowercase();
    let trimmed = lower.trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n']);
    if trimmed.starts_with("error:")
        || trimmed.starts_with("error ")
        || trimmed.starts_with("error\r")
        || trimmed.starts_with("error\n")
        || trimmed.starts_with("# error")
        || trimmed.starts_with("tap error")
    {
        return Some(EmbeddedTapStatus::Error);
    }
    let compact: String = lower
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let query_status =
        compact.contains("name=\"query_status\"") || compact.contains("name='query_status'");
    if query_status && (compact.contains("value=\"error\"") || compact.contains("value='error'")) {
        Some(EmbeddedTapStatus::Error)
    } else if query_status
        && (compact.contains("value=\"overflow\"") || compact.contains("value='overflow'"))
    {
        Some(EmbeddedTapStatus::Overflow)
    } else {
        None
    }
}

fn malformed(path: &Path, error: impl std::fmt::Display) -> ValidationFailure {
    ValidationFailure {
        kind: ValidationFailureKind::Malformed,
        message: format!("malformed TAP result {}: {error}", path.display()),
        row_count: None,
    }
}

#[derive(Default)]
struct VotableScan {
    saw_open: bool,
    saw_close: bool,
    in_tabledata: bool,
    saw_tabledata: bool,
    saw_binary: bool,
    rows: u64,
    service_error: bool,
    overflow: bool,
}

fn validate_votable(path: &Path) -> std::result::Result<Option<u64>, ValidationFailure> {
    let file = File::open(path).map_err(|error| malformed(path, error))?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut buffer = [0_u8; 64 * 1024];
    let mut tag = Vec::with_capacity(256);
    let mut in_tag = false;
    let mut scan = VotableScan::default();
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| malformed(path, error))?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            if !in_tag {
                if *byte == b'<' {
                    in_tag = true;
                    tag.clear();
                    tag.push(*byte);
                }
                continue;
            }
            if tag.len() >= 64 * 1024 {
                return Err(ValidationFailure {
                    kind: ValidationFailureKind::Malformed,
                    message: "VOTable contains an implausibly large XML tag".to_owned(),
                    row_count: None,
                });
            }
            tag.push(*byte);
            if *byte == b'>' {
                process_votable_tag(&tag, &mut scan);
                in_tag = false;
            }
        }
    }
    if in_tag || !scan.saw_open || !scan.saw_close {
        return Err(ValidationFailure {
            kind: ValidationFailureKind::Malformed,
            message: "truncated or non-VOTable XML response".to_owned(),
            row_count: None,
        });
    }
    let row_count = scan.saw_tabledata.then_some(scan.rows);
    if scan.service_error {
        return Err(ValidationFailure {
            kind: ValidationFailureKind::Service,
            message: "TAP VOTable contains QUERY_STATUS=ERROR".to_owned(),
            row_count,
        });
    }
    if scan.overflow {
        return Err(ValidationFailure {
            kind: ValidationFailureKind::Truncated,
            message: "TAP VOTable contains QUERY_STATUS=OVERFLOW".to_owned(),
            row_count,
        });
    }
    Ok(row_count)
}

fn process_votable_tag(tag: &[u8], scan: &mut VotableScan) {
    let lower = String::from_utf8_lossy(tag).to_ascii_lowercase();
    if tag_name_is(&lower, "votable", false) {
        scan.saw_open = true;
    } else if tag_name_is(&lower, "votable", true) {
        scan.saw_close = true;
    } else if tag_name_is(&lower, "tabledata", false) {
        scan.in_tabledata = true;
        scan.saw_tabledata = true;
    } else if tag_name_is(&lower, "tabledata", true) {
        scan.in_tabledata = false;
    } else if scan.in_tabledata && tag_name_is(&lower, "tr", false) {
        scan.rows = scan.rows.saturating_add(1);
    } else if tag_name_is(&lower, "binary", false) || tag_name_is(&lower, "binary2", false) {
        scan.saw_binary = true;
    }
    if tag_name_is(&lower, "info", false)
        && attribute_value(&lower, "name").as_deref() == Some("query_status")
    {
        match attribute_value(&lower, "value").as_deref() {
            Some("error") => scan.service_error = true,
            Some("overflow") => scan.overflow = true,
            _ => {}
        }
    }
}

fn tag_name_is(tag: &str, expected: &str, closing: bool) -> bool {
    let bytes = tag.as_bytes();
    let mut index = 1;
    if closing {
        if bytes.get(index) != Some(&b'/') {
            return false;
        }
        index += 1;
    } else if bytes.get(index) == Some(&b'/') {
        return false;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        index += 1;
    }
    let remainder = &tag[index..];
    let name_end = remainder
        .bytes()
        .position(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace())
        .unwrap_or(remainder.len());
    remainder[..name_end]
        .rsplit(':')
        .next()
        .is_some_and(|name| name == expected)
}

fn attribute_value(tag: &str, expected: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        while index < bytes.len()
            && (bytes[index].is_ascii_whitespace()
                || bytes[index] == b'<'
                || bytes[index] == b'/'
                || bytes[index] == b'>')
        {
            index += 1;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b':' | b'-'))
        {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let name = &tag[start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let quote = *bytes.get(index)?;
        if quote != b'\'' && quote != b'"' {
            continue;
        }
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != quote {
            index += 1;
        }
        if name == expected {
            return Some(tag[value_start..index].to_owned());
        }
        index = index.saturating_add(1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn test_config() -> TapClientConfig {
        TapClientConfig {
            request_timeout: Duration::from_secs(2),
            total_timeout: Duration::from_secs(3),
            poll_interval: Duration::from_millis(2),
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(4),
        }
    }

    fn http_response(status: &str, headers: &[(&str, &str)], body: &str) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }
        response.push_str("\r\n");
        response.push_str(body);
        response.into_bytes()
    }

    async fn fixture_server(
        responses: Vec<Vec<u8>>,
    ) -> Result<(
        String,
        Arc<Mutex<Vec<Vec<u8>>>>,
        tokio::task::JoinHandle<Result<()>>,
    )> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) = listener.accept().await?;
                let request = read_http_request(&mut socket).await?;
                captured
                    .lock()
                    .expect("fixture mutex poisoned")
                    .push(request);
                socket.write_all(&response).await?;
                socket.shutdown().await?;
            }
            Ok(())
        });
        Ok((format!("http://{address}/tap"), requests, task))
    }

    async fn read_http_request(socket: &mut tokio::net::TcpStream) -> Result<Vec<u8>> {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end;
        loop {
            let read = socket.read(&mut buffer).await?;
            if read == 0 {
                bail!("fixture client closed before request headers completed");
            }
            request.extend_from_slice(&buffer[..read]);
            if let Some(index) = find_bytes(&request, b"\r\n\r\n") {
                header_end = index + 4;
                break;
            }
        }
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let read = socket.read(&mut buffer).await?;
            if read == 0 {
                bail!("fixture client closed before request body completed");
            }
            request.extend_from_slice(&buffer[..read]);
        }
        Ok(request)
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn request_path(request: &[u8]) -> String {
        String::from_utf8_lossy(request)
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_owned()
    }

    fn decoded_form(request: &[u8]) -> BTreeMap<String, String> {
        let body_start = find_bytes(request, b"\r\n\r\n").expect("request headers") + 4;
        String::from_utf8_lossy(&request[body_start..])
            .split('&')
            .filter_map(|field| {
                let (name, value) = field.split_once('=')?;
                Some((percent_decode(name), percent_decode(value)))
            })
            .collect()
    }

    fn percent_decode(value: &str) -> String {
        let value = value.as_bytes();
        let mut output = Vec::with_capacity(value.len());
        let mut index = 0;
        while index < value.len() {
            match value[index] {
                b'+' => output.push(b' '),
                b'%' if index + 2 < value.len() => {
                    let high = hex(value[index + 1]).expect("percent high nibble");
                    let low = hex(value[index + 2]).expect("percent low nibble");
                    output.push((high << 4) | low);
                    index += 2;
                }
                byte => output.push(byte),
            }
            index += 1;
        }
        String::from_utf8(output).expect("UTF-8 fixture form")
    }

    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    fn request_in(dir: &tempfile::TempDir, adql: &str, mode: TapMode) -> TapRequest {
        TapRequest {
            adql: adql.to_owned(),
            mode,
            format: TapFormat::Csv,
            maxrec: Some(10),
            expected_rows: Some(2),
            output_path: dir.path().join("result.csv"),
            artifact_dir: dir.path().join("tap-run"),
            overwrite: false,
        }
    }

    #[tokio::test]
    async fn sync_preserves_literal_adql_maxrec_and_audit_artifacts() -> Result<()> {
        let response = http_response(
            "200 OK",
            &[("Content-Type", "text/csv"), ("X-Fixture", "sync")],
            "source_id,label\n1,alpha\n2,beta\n",
        );
        let (endpoint, requests, server) = fixture_server(vec![response]).await?;
        let dir = tempfile::tempdir()?;
        let adql = "SELECT TOP 2 source_id\nFROM gaiadr3.gaia_source\nWHERE note = 'a+b% c'";
        let mut request = request_in(&dir, adql, TapMode::Sync);
        request.maxrec = Some(10);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let outcome = client.execute(&request).await?;
        server.await??;

        let requests = requests.lock().expect("fixture mutex poisoned");
        assert_eq!(requests.len(), 1);
        assert_eq!(request_path(&requests[0]), "/tap/sync");
        let form = decoded_form(&requests[0]);
        assert_eq!(form.get("QUERY").map(String::as_str), Some(adql));
        assert_eq!(form.get("MAXREC").map(String::as_str), Some("10"));
        assert_eq!(form.get("REQUEST").map(String::as_str), Some("doQuery"));
        assert_eq!(
            fs::read_to_string(dir.path().join("tap-run/query.adql"))?,
            adql
        );
        assert_eq!(outcome.row_count, Some(2));
        assert_eq!(outcome.sha256, sha256_file(&outcome.output_path)?);
        assert!(
            fs::read_to_string(dir.path().join("tap-run/headers.txt"))?.contains("x-fixture: sync")
        );
        let manifest = TapManifest::read(&outcome.manifest_path)?;
        assert_eq!(manifest.status, "completed");
        Ok(())
    }

    #[tokio::test]
    async fn http_400_is_not_retried_and_body_is_preserved() -> Result<()> {
        let response = http_response(
            "400 Bad Request",
            &[("Content-Type", "application/xml"), ("X-Error", "fixture")],
            "<VOTABLE><INFO name=\"QUERY_STATUS\" value=\"ERROR\"/></VOTABLE>",
        );
        let (endpoint, requests, server) = fixture_server(vec![response]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT invalid_column FROM t", TapMode::Sync);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let error = client
            .execute(&request)
            .await
            .expect_err("HTTP 400 must fail");
        server.await??;

        assert!(error.to_string().contains("HTTP 400"));
        assert_eq!(requests.lock().expect("fixture mutex poisoned").len(), 1);
        let body = fs::read_to_string(dir.path().join("tap-run/body-error.bin"))?;
        assert!(body.contains("QUERY_STATUS"));
        assert!(
            fs::read_to_string(dir.path().join("tap-run/status.txt"))?.contains("400 Bad Request")
        );
        assert!(fs::read_to_string(dir.path().join("tap-run/headers.txt"))?
            .contains("x-error: fixture"));
        Ok(())
    }

    #[tokio::test]
    async fn retries_transient_status_then_validates_success() -> Result<()> {
        let busy = http_response(
            "503 Service Unavailable",
            &[("Retry-After", "0")],
            "temporarily busy",
        );
        let success = http_response(
            "200 OK",
            &[("Content-Type", "text/csv")],
            "source_id\n1\n2\n",
        );
        let (endpoint, requests, server) = fixture_server(vec![busy, success]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Sync);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let outcome = client.execute(&request).await?;
        server.await??;

        assert_eq!(outcome.row_count, Some(2));
        assert_eq!(requests.lock().expect("fixture mutex poisoned").len(), 2);
        assert!(
            fs::read_to_string(dir.path().join("tap-run/body-error.bin"))?
                .contains("temporarily busy")
        );
        Ok(())
    }

    #[tokio::test]
    async fn async_uws_polls_downloads_and_reuses_completed_manifest() -> Result<()> {
        let submitted = http_response("303 See Other", &[("Location", "/tap/async/42")], "");
        let executing = http_response("200 OK", &[("Content-Type", "text/plain")], "EXECUTING\n");
        let completed = http_response("200 OK", &[("Content-Type", "text/plain")], "COMPLETED\n");
        let result = http_response(
            "200 OK",
            &[("Content-Type", "text/csv")],
            "source_id\n10\n20\n",
        );
        let (endpoint, requests, server) =
            fixture_server(vec![submitted, executing, completed, result]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Async);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let outcome = client.execute(&request).await?;
        server.await??;

        {
            let captured = requests.lock().expect("fixture mutex poisoned");
            assert_eq!(request_path(&captured[0]), "/tap/async");
            assert_eq!(
                decoded_form(&captured[0]).get("PHASE").map(String::as_str),
                Some("RUN")
            );
            assert_eq!(request_path(&captured[1]), "/tap/async/42/phase");
            assert_eq!(request_path(&captured[2]), "/tap/async/42/phase");
            assert_eq!(request_path(&captured[3]), "/tap/async/42/results/result");
        }
        let expected_job_url = format!("{endpoint}/async/42");
        assert_eq!(outcome.job_url.as_deref(), Some(expected_job_url.as_str()));

        let resumed = client.resume_manifest(&outcome.manifest_path).await?;
        assert!(resumed.resumed_existing_result);
        assert_eq!(resumed.sha256, outcome.sha256);
        Ok(())
    }

    #[tokio::test]
    async fn existing_job_url_resumes_without_resubmission() -> Result<()> {
        let completed = http_response("200 OK", &[("Content-Type", "text/plain")], "COMPLETED\n");
        let result = http_response(
            "200 OK",
            &[("Content-Type", "text/csv")],
            "source_id\n10\n20\n",
        );
        let (endpoint, requests, server) = fixture_server(vec![completed, result]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Async);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let job_url = format!("{endpoint}/async/existing");
        let outcome = client.resume_job(&job_url, &request).await?;
        server.await??;

        let captured = requests.lock().expect("fixture mutex poisoned");
        assert_eq!(captured.len(), 2);
        assert_eq!(request_path(&captured[0]), "/tap/async/existing/phase");
        assert_eq!(
            request_path(&captured[1]),
            "/tap/async/existing/results/result"
        );
        assert_eq!(outcome.job_url.as_deref(), Some(job_url.as_str()));
        Ok(())
    }

    #[tokio::test]
    async fn votable_overflow_is_detected_and_preserved() -> Result<()> {
        let body = concat!(
            "<?xml version=\"1.0\"?><VOTABLE><RESOURCE><INFO name=\"QUERY_STATUS\" value=\"OK\"/>",
            "<TABLE><DATA><TABLEDATA><TR><TD>1</TD></TR></TABLEDATA></DATA></TABLE>",
            "<INFO value=\"OVERFLOW\" name=\"QUERY_STATUS\"/></RESOURCE></VOTABLE>"
        );
        let response = http_response(
            "200 OK",
            &[("Content-Type", "application/x-votable+xml")],
            body,
        );
        let (endpoint, _, server) = fixture_server(vec![response]).await?;
        let dir = tempfile::tempdir()?;
        let mut request = request_in(&dir, "SELECT source_id FROM t", TapMode::Sync);
        request.format = TapFormat::Votable;
        request.maxrec = None;
        request.expected_rows = None;
        request.output_path = dir.path().join("result.vot");
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let error = client
            .execute(&request)
            .await
            .expect_err("OVERFLOW must fail closed");
        server.await??;

        assert!(error.to_string().contains("OVERFLOW"));
        let manifest = TapManifest::read(&dir.path().join("tap-run/manifest.json"))?;
        assert!(manifest.truncated);
        assert_eq!(manifest.status, "truncated");
        assert!(
            fs::read_to_string(dir.path().join("tap-run/body-error.bin"))?.contains("QUERY_STATUS")
        );
        Ok(())
    }

    #[tokio::test]
    async fn http_429_is_retried_with_backoff() -> Result<()> {
        let busy = http_response(
            "429 Too Many Requests",
            &[("Retry-After", "0")],
            "rate limited",
        );
        let success = http_response(
            "200 OK",
            &[("Content-Type", "text/csv")],
            "source_id\n1\n2\n",
        );
        let (endpoint, requests, server) = fixture_server(vec![busy, success]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Sync);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let outcome = client.execute(&request).await?;
        server.await??;

        assert_eq!(outcome.row_count, Some(2));
        assert_eq!(requests.lock().expect("fixture mutex poisoned").len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn existing_output_is_not_overwritten_without_flag() -> Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("result.csv"), "source_id\n99\n")?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Sync);
        let client = GaiaTapClient::new("http://127.0.0.1:9/tap", test_config())?;
        let error = client
            .execute(&request)
            .await
            .expect_err("existing output must block");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(
            fs::read_to_string(dir.path().join("result.csv"))?,
            "source_id\n99\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn uws_error_phase_is_not_retried() -> Result<()> {
        let submitted = http_response("303 See Other", &[("Location", "/tap/async/99")], "");
        let error_phase = http_response("200 OK", &[("Content-Type", "text/plain")], "ERROR\n");
        let error_body = http_response(
            "200 OK",
            &[("Content-Type", "text/plain")],
            "ADQL syntax error",
        );
        let (endpoint, requests, server) =
            fixture_server(vec![submitted, error_phase, error_body]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT bad FROM t", TapMode::Async);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let error = client
            .execute(&request)
            .await
            .expect_err("UWS ERROR must fail closed");
        server.await??;

        assert!(error.to_string().contains("ERROR phase"));
        assert_eq!(requests.lock().expect("fixture mutex poisoned").len(), 3);
        assert!(
            fs::read_to_string(dir.path().join("tap-run/body-error.bin"))?
                .contains("ADQL syntax error")
        );
        Ok(())
    }

    #[tokio::test]
    async fn html_error_body_is_detected() -> Result<()> {
        let response = http_response(
            "200 OK",
            &[("Content-Type", "text/html")],
            "<html><body>TAP error</body></html>",
        );
        let (endpoint, _, server) = fixture_server(vec![response]).await?;
        let dir = tempfile::tempdir()?;
        let request = request_in(&dir, "SELECT source_id FROM t", TapMode::Sync);
        let client = GaiaTapClient::new(&endpoint, test_config())?;
        let error = client
            .execute(&request)
            .await
            .expect_err("HTML must fail validation");
        server.await??;

        assert!(error.to_string().contains("TAP") || error.to_string().contains("malformed"));
        Ok(())
    }

    #[test]
    fn csv_reaching_maxrec_and_wrong_population_fail_closed() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("result.csv");
        fs::write(&path, "id\n1\n2\n")?;
        let overflow = validate_result(&path, TapFormat::Csv, Some(2), None)
            .expect_err("MAXREC boundary must be suspicious");
        assert!(matches!(overflow.kind, ValidationFailureKind::Truncated));
        let population = validate_result(&path, TapFormat::Csv, None, Some(3))
            .expect_err("wrong population must fail");
        assert!(matches!(population.kind, ValidationFailureKind::RowCount));
        Ok(())
    }
}
