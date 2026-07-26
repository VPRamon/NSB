//! Submit or resume an auditable Gaia TAP query.

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use nsb_data_tools::gaia::acquisition::tap::{
    GaiaTapClient, TapClientConfig, TapFormat, TapManifest, TapMode, TapRequest,
    DEFAULT_GAIA_TAP_ENDPOINT,
};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModeArg {
    Sync,
    Async,
}

impl From<ModeArg> for TapMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Sync => Self::Sync,
            ModeArg::Async => Self::Async,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum FormatArg {
    Csv,
    Votable,
}

impl From<FormatArg> for TapFormat {
    fn from(value: FormatArg) -> Self {
        match value {
            FormatArg::Csv => Self::Csv,
            FormatArg::Votable => Self::Votable,
        }
    }
}

/// Run Gaia TAP without losing the submitted query or server diagnostics.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// TAP service root. For --resume-manifest, its saved endpoint is used unless overridden.
    #[arg(long)]
    endpoint: Option<String>,

    /// Literal ADQL text. Mutually exclusive with --query-file.
    #[arg(long, conflicts_with = "query_file")]
    query: Option<String>,

    /// UTF-8 ADQL file read without trimming or normalization.
    #[arg(long, conflicts_with = "query")]
    query_file: Option<PathBuf>,

    /// TAP mode for a new query or --resume-job-url.
    #[arg(long, value_enum, default_value_t = ModeArg::Async)]
    mode: ModeArg,

    /// Result format requested from and validated against TAP.
    #[arg(long, value_enum, default_value_t = FormatArg::Csv)]
    format: FormatArg,

    /// TAP MAXREC. A CSV response reaching this count fails as potentially truncated.
    #[arg(long)]
    maxrec: Option<u64>,

    /// Require exactly this many parsed data rows.
    #[arg(long)]
    expected_rows: Option<u64>,

    /// Final validated result. Required except with --resume-manifest.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Audit/checkpoint directory. Defaults to <output>.tap-artifacts.
    #[arg(long)]
    artifacts_dir: Option<PathBuf>,

    /// Replace an existing result for a newly submitted query.
    #[arg(long)]
    overwrite: bool,

    /// Poll an existing asynchronous UWS job instead of submitting a new one.
    #[arg(long, conflicts_with = "resume_manifest")]
    resume_job_url: Option<String>,

    /// Resume from manifest.json, or locally reuse its checksum-matching completed result.
    #[arg(long, conflicts_with = "resume_job_url")]
    resume_manifest: Option<PathBuf>,

    /// Timeout for each HTTP operation.
    #[arg(long, default_value_t = 120)]
    request_timeout_seconds: u64,

    /// Total wall-clock limit including UWS polling and result download.
    #[arg(long, default_value_t = 21_600)]
    timeout_seconds: u64,

    /// UWS polling interval.
    #[arg(long, default_value_t = 2)]
    poll_interval_seconds: u64,

    /// Maximum attempts for transient transport/HTTP failures, including the first.
    #[arg(long, default_value_t = 5)]
    max_attempts: u32,

    /// Initial exponential retry delay.
    #[arg(long, default_value_t = 500)]
    initial_backoff_millis: u64,

    /// Maximum exponential retry delay.
    #[arg(long, default_value_t = 30_000)]
    max_backoff_millis: u64,
}

/// Run the `query_gaia_tap` command using process arguments.
pub fn run_cli() -> Result<()> {
    let args = crate::parse_command_args();
    tokio::runtime::Runtime::new()?.block_on(run(args))
}

async fn run(args: Args) -> Result<()> {
    let config = TapClientConfig {
        request_timeout: Duration::from_secs(args.request_timeout_seconds),
        total_timeout: Duration::from_secs(args.timeout_seconds),
        poll_interval: Duration::from_secs(args.poll_interval_seconds),
        max_attempts: args.max_attempts,
        initial_backoff: Duration::from_millis(args.initial_backoff_millis),
        max_backoff: Duration::from_millis(args.max_backoff_millis),
    };

    if let Some(manifest_path) = args.resume_manifest.as_deref() {
        if args.query.is_some()
            || args.query_file.is_some()
            || args.output.is_some()
            || args.artifacts_dir.is_some()
            || args.maxrec.is_some()
            || args.expected_rows.is_some()
            || args.overwrite
        {
            bail!(
                "--resume-manifest uses the saved query, output, MAXREC, and population contract"
            );
        }
        let manifest = TapManifest::read(manifest_path)?;
        let endpoint = args.endpoint.as_deref().unwrap_or(&manifest.endpoint);
        let client = GaiaTapClient::new(endpoint, config)?;
        let outcome = client.resume_manifest(manifest_path).await?;
        print_outcome(&outcome)?;
        return Ok(());
    }

    let adql = read_query(args.query, args.query_file.as_deref())?;
    let output = args
        .output
        .context("--output is required for a new query or --resume-job-url")?;
    let artifact_dir = args
        .artifacts_dir
        .unwrap_or_else(|| default_artifact_dir(&output));
    let mode: TapMode = args.mode.into();
    if args.resume_job_url.is_some() && mode != TapMode::Async {
        bail!("--resume-job-url requires --mode async");
    }
    let request = TapRequest {
        adql,
        mode,
        format: args.format.into(),
        maxrec: args.maxrec,
        expected_rows: args.expected_rows,
        output_path: output,
        artifact_dir,
        overwrite: args.overwrite,
    };
    let endpoint = args
        .endpoint
        .as_deref()
        .unwrap_or(DEFAULT_GAIA_TAP_ENDPOINT);
    let client = GaiaTapClient::new(endpoint, config)?;
    let outcome = match args.resume_job_url {
        Some(job_url) => client.resume_job(&job_url, &request).await?,
        None => client.execute(&request).await?,
    };
    print_outcome(&outcome)
}

fn read_query(literal: Option<String>, path: Option<&Path>) -> Result<String> {
    match (literal, path) {
        (Some(query), None) => Ok(query),
        (None, Some(path)) => fs::read_to_string(path)
            .with_context(|| format!("failed to read ADQL file {}", path.display())),
        (None, None) => bail!("one of --query or --query-file is required"),
        (Some(_), Some(_)) => unreachable!("clap rejects conflicting query inputs"),
    }
}

fn default_artifact_dir(output: &Path) -> PathBuf {
    let mut value: OsString = output.as_os_str().to_owned();
    value.push(".tap-artifacts");
    PathBuf::from(value)
}

fn print_outcome(outcome: &impl serde::Serialize) -> Result<()> {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    serde_json::to_writer_pretty(&mut lock, outcome)?;
    use std::io::Write;
    writeln!(lock)?;
    Ok(())
}
