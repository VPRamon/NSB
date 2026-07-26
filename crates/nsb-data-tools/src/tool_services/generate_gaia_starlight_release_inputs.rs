use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use nsb_data_tools::gaia_bulk::{BulkConfig, BulkDownloader, BulkPaths};
use nsb_data_tools::gaia_datalink::{
    rebuild_normalized_chunks, DatalinkConfig, DatalinkDownloader, DownloadPaths, DownloadReport,
    NormalizationReport,
};
use nsb_data_tools::gaia_xp::{
    integrate_photon_flux, parse_normalized_record, PhotonFluxIntegral, XpProduct, BAND_MAX_NM,
    BAND_MIN_NM, NORMALIZED_FLUX_COLUMN, NORMALIZED_FLUX_ERROR_COLUMN,
    NORMALIZED_WAVELENGTH_COLUMN,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_TAP_URL: &str = "https://gea.esac.esa.int/tap-server/tap/sync";
const DEFAULT_DATALINK_URL: &str = "https://gea.esac.esa.int/data-server/data";
const DEFAULT_BULK_URL: &str =
    "https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_sampled_mean_spectrum/";
const EXPECTED_SAMPLED_SOURCES: usize = 34_468_373;
const PUBLISHED_CONTINUOUS_SOURCES: usize = 219_197_643;
const EXTRACT_FILE: &str = "gaia_dr3_starlight_extract.csv";

#[derive(Debug, Parser)]
#[command(about = "Generate complete, restartable Gaia DR3 starlight release inputs")]
struct Args {
    #[arg(long)]
    out_dir: PathBuf,
    #[arg(long, default_value_t = 15.0)]
    max_g_mag: f64,
    /// Candidate-only DataLink source limit. Production uses the complete bulk inventory.
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long, default_value_t = EXPECTED_SAMPLED_SOURCES)]
    expected_source_count: usize,
    #[arg(long, default_value_t = 5000)]
    chunk_size: usize,
    #[arg(long, default_value_t = BAND_MIN_NM)]
    band_min_nm: f64,
    #[arg(long, default_value_t = BAND_MAX_NM)]
    band_max_nm: f64,
    #[arg(long, default_value = DEFAULT_TAP_URL)]
    tap_url: String,
    #[arg(long, default_value = DEFAULT_DATALINK_URL)]
    datalink_url: String,
    #[arg(long, default_value = DEFAULT_BULK_URL)]
    bulk_url: String,
    #[arg(long, value_enum)]
    xp_retrieval: Option<XpRetrievalMode>,
    #[arg(long)]
    xp_dir: Option<PathBuf>,
    #[arg(long)]
    license_policy_file: Option<PathBuf>,
    #[arg(long)]
    validation_reference: Option<PathBuf>,
    #[arg(long)]
    missing_flux_report: Option<PathBuf>,
    #[arg(long)]
    resume: bool,
    #[arg(long)]
    retry_failed_only: bool,
    #[arg(long)]
    allow_partial_candidate_xp: bool,
    #[arg(long)]
    candidate: bool,
    #[arg(long)]
    production: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    skip_metadata_download: bool,
    #[arg(long)]
    skip_xp_download: bool,

    #[arg(long, default_value_t = 8)]
    datalink_concurrency: usize,
    #[arg(long, default_value_t = 12.0)]
    datalink_max_rps: f64,
    #[arg(long, default_value_t = 60)]
    datalink_timeout_secs: u64,
    #[arg(long, default_value_t = 10)]
    datalink_connect_timeout_secs: u64,
    #[arg(long, default_value_t = 6)]
    datalink_max_attempts: u32,
    #[arg(long, default_value_t = 500)]
    datalink_initial_backoff_ms: u64,
    #[arg(long, default_value_t = 60)]
    datalink_max_backoff_secs: u64,
    #[arg(long, default_value_t = 30)]
    progress_interval_secs: u64,

    #[arg(long, default_value_t = 4)]
    bulk_concurrency: usize,
    #[arg(long, default_value_t = 900)]
    bulk_timeout_secs: u64,
    #[arg(long, default_value_t = 15)]
    bulk_connect_timeout_secs: u64,
    #[arg(long, default_value_t = 6)]
    bulk_max_attempts: u32,
    #[arg(long, default_value_t = 1000)]
    bulk_initial_backoff_ms: u64,
    #[arg(long, default_value_t = 120)]
    bulk_max_backoff_secs: u64,
    /// Candidate-only deterministic prefix of the official bulk inventory.
    #[arg(long)]
    bulk_file_limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
enum XpRetrievalMode {
    GaiaBulk,
    GaiaDatalink,
    NormalizedChunks,
}

impl XpRetrievalMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::GaiaBulk => "gaia-bulk",
            Self::GaiaDatalink => "gaia-datalink",
            Self::NormalizedChunks => "normalized-chunks",
        }
    }
}

#[derive(Debug)]
struct Paths {
    out_dir: PathBuf,
    adql: PathBuf,
    metadata: PathBuf,
    xp_dir: PathBuf,
    xp_raw_dir: PathBuf,
    xp_error_dir: PathBuf,
    xp_checkpoint: PathBuf,
    bulk_dir: PathBuf,
    bulk_error_dir: PathBuf,
    bulk_md5: PathBuf,
    bulk_manifest: PathBuf,
    extract: PathBuf,
    diagnostics: PathBuf,
    checksum: PathBuf,
    policy: PathBuf,
    env: PathBuf,
    http_error_dir: PathBuf,
}

#[derive(Debug, Default, Serialize)]
struct Diagnostics {
    schema_version: u32,
    catalogue_name: &'static str,
    catalogue_release: &'static str,
    source_population: String,
    selection_predicate: String,
    completeness_limitations: String,
    magnitude_limit: String,
    xp_product_type: String,
    estimated_missing_flux_contribution: String,
    release_completeness_gate: String,
    adql_path: String,
    metadata_rows: usize,
    selected_sources: usize,
    completed_sources: usize,
    retried_sources: usize,
    failed_sources: usize,
    parsed_sources: usize,
    accepted_sources: usize,
    scientifically_excluded_sources: usize,
    unexpected_rejected_sources: usize,
    rejection_reasons: BTreeMap<String, usize>,
    xp_retrieval_mode: String,
    normalization: Option<NormalizationReport>,
    datalink: Option<DownloadReport>,
    bulk: Option<Value>,
    bulk_error: Option<String>,
    requests_total: usize,
    retry_attempts_total: usize,
    http_status_counts: BTreeMap<String, usize>,
    throughput_overall_sources_per_second: f64,
    throughput_recent_sources_per_second: f64,
    elapsed_seconds: f64,
    eta_seconds: Option<f64>,
    bytes_downloaded: u64,
    negative_samples: usize,
    band_samples: usize,
    negative_sample_fraction: f64,
    integrated_positive_ph_m2_s: f64,
    integrated_negative_ph_m2_s: f64,
    integrated_total_ph_m2_s: f64,
    integrated_negative_contribution_ratio: f64,
    integrated_uncertainty_quadrature_ph_m2_s: Option<f64>,
    uncertainty_model: &'static str,
    band_min_nm: f64,
    band_max_nm: f64,
    max_g_mag: f64,
    extract_sha256: Option<String>,
    checkpoint_path: String,
    representative_error_files: Vec<String>,
    partial_files: usize,
    production_mode: bool,
}

#[derive(Debug, Clone)]
struct MetadataRow {
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PopulationSummary {
    selected_sources: usize,
    min_g: f64,
    max_g: f64,
}

#[derive(Debug, Deserialize)]
struct ValidationReference {
    production_use: bool,
    band_nm: [f64; 2],
    regions: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct MissingFluxReport {
    production_use: bool,
    band_nm: [f64; 2],
    sampled_sources: usize,
    continuous_sources: usize,
    estimated_missing_flux_fraction: f64,
    confidence_lower_fraction: f64,
    confidence_upper_fraction: f64,
    continuous_sample_size: usize,
    method: String,
    global_and_regional_pass: bool,
}

#[tokio::main]
/// Run the `generate_gaia_starlight_release_inputs` command using process arguments.
pub async fn run_cli() -> Result<()> {
    run(Args::parse()).await
}

async fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    let paths = Paths::new(&args.out_dir);
    let mode = select_xp_retrieval_mode(&args);
    let adql = build_adql(mode, args.max_g_mag, args.limit);
    if args.dry_run {
        print_dry_run(&args, &paths, mode, &adql);
        return Ok(());
    }

    create_directories(&paths)?;
    atomic_write(&paths.adql, adql.as_bytes())?;
    validate_policy_and_science_gates(&args, &paths)?;

    if args.skip_metadata_download {
        ensure_file(&paths.metadata, "metadata/preflight CSV")?;
    } else if args.resume && valid_metadata_file(&paths.metadata, mode).is_ok() {
        log::info!("resume: validated existing {}", paths.metadata.display());
    } else {
        download_tap_csv(&args, &paths, &adql).await?;
    }

    let (metadata, selected_sources) = match mode {
        XpRetrievalMode::GaiaBulk => {
            let summary = read_population_summary(&paths.metadata)?;
            validate_population_summary(&args, &summary)?;
            (Vec::new(), summary.selected_sources)
        }
        XpRetrievalMode::GaiaDatalink | XpRetrievalMode::NormalizedChunks => {
            let metadata = read_metadata(&paths.metadata)?;
            let count = metadata.len();
            (metadata, count)
        }
    };

    let mut diagnostics = base_diagnostics(&args, &paths, mode, selected_sources);
    let operation_result = match mode {
        XpRetrievalMode::GaiaBulk => run_bulk_mode(&args, &paths, &mut diagnostics).await,
        XpRetrievalMode::GaiaDatalink => {
            run_datalink_mode(&args, &paths, &metadata, &mut diagnostics).await
        }
        XpRetrievalMode::NormalizedChunks => {
            run_normalized_mode(&args, &paths, &metadata, &mut diagnostics)
        }
    };
    if let Err(err) = operation_result {
        *diagnostics
            .rejection_reasons
            .entry(format!("pipeline error: {err:#}"))
            .or_default() += 1;
        diagnostics.unexpected_rejected_sources = diagnostics
            .selected_sources
            .saturating_sub(diagnostics.accepted_sources);
        write_diagnostics(&paths.diagnostics, &diagnostics)?;
        return Err(err);
    }

    diagnostics.partial_files = count_partial_files(&paths.out_dir)?;
    finalize_derived_diagnostics(&mut diagnostics);
    let failure = strict_failure(&args, mode, &diagnostics);
    write_diagnostics(&paths.diagnostics, &diagnostics)?;
    write_env_file(&paths, mode, &diagnostics)?;
    if let Some(message) = failure {
        bail!("{message}");
    }

    println!("generated {}", paths.diagnostics.display());
    println!("generated {}", paths.env.display());
    if paths.extract.exists() {
        println!("generated {}", paths.extract.display());
    }
    if paths.bulk_manifest.exists() {
        println!("generated {}", paths.bulk_manifest.display());
    }
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    if args.candidate == args.production {
        bail!("choose exactly one of --candidate or --production");
    }
    if args.allow_partial_candidate_xp && !args.candidate {
        bail!("--allow-partial-candidate-xp is candidate-only");
    }
    if args.retry_failed_only && !args.resume {
        bail!("--retry-failed-only requires --resume");
    }
    if !args.max_g_mag.is_finite() || args.max_g_mag != 15.0 {
        bail!("Gaia DR3 XP sampled has the fixed effective magnitude limit --max-g-mag=15");
    }
    if args.expected_source_count == 0 || args.chunk_size == 0 {
        bail!("expected source count and chunk size must be positive");
    }
    if args.band_min_nm.to_bits() != BAND_MIN_NM.to_bits()
        || args.band_max_nm.to_bits() != BAND_MAX_NM.to_bits()
    {
        bail!("the only supported Gaia XP product contract is 336-650 nm");
    }
    let datalink = datalink_config(args);
    datalink.validate()?;
    if args.bulk_concurrency == 0
        || args.bulk_timeout_secs == 0
        || args.bulk_connect_timeout_secs == 0
        || args.bulk_max_attempts == 0
        || args.bulk_initial_backoff_ms == 0
        || args.bulk_max_backoff_secs == 0
    {
        bail!("bulk concurrency, timeouts, attempts, and backoff must be positive");
    }
    if args.production {
        if args
            .xp_retrieval
            .is_some_and(|mode| mode != XpRetrievalMode::GaiaBulk)
        {
            bail!("--production requires --xp-retrieval gaia-bulk");
        }
        if args.limit.is_some() || args.bulk_file_limit.is_some() {
            bail!("--production rejects source or bulk-file limits");
        }
        if args.expected_source_count != EXPECTED_SAMPLED_SOURCES {
            bail!(
                "--production expected source count must be {EXPECTED_SAMPLED_SOURCES} for Gaia DR3 XP sampled"
            );
        }
    }
    Ok(())
}

fn select_xp_retrieval_mode(args: &Args) -> XpRetrievalMode {
    args.xp_retrieval.unwrap_or(if args.production {
        XpRetrievalMode::GaiaBulk
    } else if args.limit.is_some() {
        XpRetrievalMode::GaiaDatalink
    } else {
        XpRetrievalMode::GaiaBulk
    })
}

impl Paths {
    fn new(out_dir: &Path) -> Self {
        let xp_dir = out_dir.join("gaia_dr3_xp_chunks");
        let bulk_dir = out_dir.join("gaia_dr3_xp_sampled_bulk");
        Self {
            out_dir: out_dir.to_path_buf(),
            adql: out_dir.join("gaia_dr3_starlight_extract.adql"),
            metadata: out_dir.join("gaia_dr3_metadata.csv"),
            xp_raw_dir: xp_dir.join("raw"),
            xp_error_dir: xp_dir.join("errors"),
            xp_checkpoint: xp_dir.join("source_checkpoint.jsonl"),
            xp_dir,
            bulk_error_dir: bulk_dir.join("errors"),
            bulk_md5: bulk_dir.join("official_md5sum.txt"),
            bulk_manifest: bulk_dir.join("bulk_manifest.json"),
            bulk_dir,
            extract: out_dir.join(EXTRACT_FILE),
            diagnostics: out_dir.join("gaia_dr3_starlight_extract.diagnostics.json"),
            checksum: out_dir.join("gaia_dr3_starlight_extract.sha256"),
            policy: out_dir.join("gaia_derived_product_policy.txt"),
            env: out_dir.join("starlight_release_inputs.env"),
            http_error_dir: out_dir.join("http_errors"),
        }
    }
}

fn create_directories(paths: &Paths) -> Result<()> {
    for path in [
        &paths.out_dir,
        &paths.xp_dir,
        &paths.xp_raw_dir,
        &paths.xp_error_dir,
        &paths.bulk_dir,
        &paths.bulk_error_dir,
        &paths.http_error_dir,
    ] {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn build_adql(mode: XpRetrievalMode, max_g_mag: f64, limit: Option<usize>) -> String {
    match mode {
        XpRetrievalMode::GaiaBulk => format!(
            "SELECT COUNT(*) AS selected_sources,\n       MIN(phot_g_mean_mag) AS min_g,\n       MAX(phot_g_mean_mag) AS max_g\nFROM gaiadr3.gaia_source\nWHERE has_xp_sampled = 'True'\n  AND phot_g_mean_mag IS NOT NULL\n  AND phot_g_mean_mag <= {max_g_mag}\n  AND source_id IS NOT NULL\n  AND ra IS NOT NULL\n  AND dec IS NOT NULL\n  AND ref_epoch IS NOT NULL\n"
        ),
        XpRetrievalMode::GaiaDatalink | XpRetrievalMode::NormalizedChunks => {
            let top = limit.map(|value| format!("TOP {value} ")).unwrap_or_default();
            format!(
                "SELECT {top}\n  source_id,\n  ra,\n  dec,\n  ref_epoch,\n  pmra,\n  pmdec,\n  parallax,\n  radial_velocity,\n  phot_g_mean_mag,\n  phot_bp_mean_mag,\n  phot_rp_mean_mag,\n  duplicated_source,\n  has_xp_sampled\nFROM gaiadr3.gaia_source\nWHERE has_xp_sampled = 'True'\n  AND phot_g_mean_mag IS NOT NULL\n  AND phot_g_mean_mag <= {max_g_mag}\n  AND source_id IS NOT NULL\n  AND ra IS NOT NULL\n  AND dec IS NOT NULL\n  AND ref_epoch IS NOT NULL\nORDER BY source_id\n"
            )
        }
    }
}

fn print_dry_run(args: &Args, paths: &Paths, mode: XpRetrievalMode, adql: &str) {
    println!("out_dir: {}", paths.out_dir.display());
    println!("metadata: {}", paths.metadata.display());
    println!("xp_retrieval: {}", mode.as_str());
    println!("bulk_dir: {}", paths.bulk_dir.display());
    println!("xp_raw_dir: {}", paths.xp_raw_dir.display());
    println!("checkpoint: {}", paths.xp_checkpoint.display());
    println!("production: {}", args.production);
    println!("ADQL:\n{adql}");
}

fn validate_policy_and_science_gates(args: &Args, paths: &Paths) -> Result<()> {
    match args.license_policy_file.as_ref() {
        Some(path) => {
            let policy = fs::read_to_string(path)
                .with_context(|| format!("failed to read policy {}", path.display()))?;
            validate_text("license policy", &policy, args.production)?;
            if args.production && !policy.lines().any(|line| {
                line.trim().eq_ignore_ascii_case("approved_for_production = true")
            }) {
                bail!("--production requires approved_for_production = true in the reviewed policy");
            }
            atomic_write(&paths.policy, policy.as_bytes())?;
        }
        None if args.production => bail!("--production requires --license-policy-file"),
        None => atomic_write(
            &paths.policy,
            b"approved_for_production = false\nreason = candidate run without a redistribution policy\n",
        )?,
    }

    match args.validation_reference.as_ref() {
        Some(path) => validate_validation_reference(path, args.production)?,
        None if args.production => bail!("--production requires --validation-reference"),
        None => {}
    }
    match args.missing_flux_report.as_ref() {
        Some(path) => validate_missing_flux_report(path, args.production)?,
        None if args.production => bail!("--production requires --missing-flux-report"),
        None => {}
    }
    Ok(())
}

fn validate_text(name: &str, value: &str, production: bool) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{name} must not be empty");
    }
    if production {
        let lower = value.to_ascii_lowercase();
        for blocked in ["todo", "placeholder", "unknown", "pending", "unreviewed"] {
            if lower.contains(blocked) {
                bail!("{name} contains production placeholder {blocked:?}");
            }
        }
    }
    Ok(())
}

fn validate_validation_reference(path: &Path, production: bool) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    validate_text("validation reference", &raw, production)?;
    let reference: ValidationReference = serde_json::from_str(&raw)?;
    if reference.band_nm.map(f64::to_bits) != [BAND_MIN_NM, BAND_MAX_NM].map(f64::to_bits) {
        bail!("independent validation reference must use the exact 336-650 nm band");
    }
    if production {
        if !reference.production_use || reference.regions.len() < 4 {
            bail!("production validation reference must be independently reviewed and contain at least four regions");
        }
        if raw.to_ascii_lowercase().contains("nsb maintainer gaia") {
            bail!("production validation reference must be independent of the generated NSB map");
        }
    }
    Ok(())
}

fn validate_missing_flux_report(path: &Path, production: bool) -> Result<()> {
    let raw = fs::read_to_string(path)?;
    validate_text("missing-flux report", &raw, production)?;
    let report: MissingFluxReport = serde_json::from_str(&raw)?;
    if report.band_nm.map(f64::to_bits) != [BAND_MIN_NM, BAND_MAX_NM].map(f64::to_bits)
        || report.sampled_sources != EXPECTED_SAMPLED_SOURCES
        || report.continuous_sources != PUBLISHED_CONTINUOUS_SOURCES
    {
        bail!("missing-flux report population or spectral contract does not match Gaia DR3");
    }
    for value in [
        report.estimated_missing_flux_fraction,
        report.confidence_lower_fraction,
        report.confidence_upper_fraction,
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            bail!("missing-flux fractions must be finite and in [0,1]");
        }
    }
    if report.confidence_lower_fraction > report.estimated_missing_flux_fraction
        || report.estimated_missing_flux_fraction > report.confidence_upper_fraction
        || report.continuous_sample_size == 0
    {
        bail!("missing-flux confidence interval or sample size is invalid");
    }
    validate_text("missing-flux method", &report.method, production)?;
    if production && (!report.production_use || !report.global_and_regional_pass) {
        bail!("production missing-flux report must pass global and regional review");
    }
    Ok(())
}

async fn download_tap_csv(args: &Args, paths: &Paths, adql: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(args.datalink_connect_timeout_secs))
        .timeout(Duration::from_secs(args.datalink_timeout_secs))
        .user_agent(concat!(
            "NSB/",
            env!("CARGO_PKG_VERSION"),
            " Gaia-DR3-release-tool"
        ))
        .build()?;
    let part = paths.metadata.with_extension("csv.part");
    let mut last_error = String::new();
    for attempt in 1..=args.datalink_max_attempts {
        let response = client
            .post(&args.tap_url)
            .form(&[
                ("REQUEST", "doQuery"),
                ("LANG", "ADQL"),
                ("FORMAT", "csv"),
                ("QUERY", adql),
            ])
            .send()
            .await;
        match response {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                let body = response.bytes().await.unwrap_or_default().to_vec();
                if status.is_success() && !nsb_data_tools::gaia_xp::contains_service_error(&body) {
                    atomic_write(&part, &body)?;
                    valid_metadata_file(&part, select_xp_retrieval_mode(args))?;
                    fs::rename(&part, &paths.metadata)?;
                    return Ok(());
                }
                last_error = format!("TAP attempt={attempt} status={status}");
                persist_http_error(&paths.http_error_dir, "tap", attempt, &headers, &body)?;
                if !matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504) {
                    break;
                }
            }
            Err(err) => {
                last_error = format!("TAP attempt={attempt} transport error: {err}");
                persist_http_error(
                    &paths.http_error_dir,
                    "tap",
                    attempt,
                    &reqwest::header::HeaderMap::new(),
                    last_error.as_bytes(),
                )?;
            }
        }
        if attempt < args.datalink_max_attempts {
            tokio::time::sleep(exponential_delay(
                args.datalink_initial_backoff_ms,
                args.datalink_max_backoff_secs,
                attempt,
            ))
            .await;
        }
    }
    if part.exists() {
        fs::remove_file(part)?;
    }
    bail!("Gaia TAP metadata/preflight download failed: {last_error}")
}

fn valid_metadata_file(path: &Path, mode: XpRetrievalMode) -> Result<()> {
    match mode {
        XpRetrievalMode::GaiaBulk => {
            read_population_summary(path)?;
        }
        XpRetrievalMode::GaiaDatalink | XpRetrievalMode::NormalizedChunks => {
            read_metadata(path)?;
        }
    }
    Ok(())
}

fn read_population_summary(path: &Path) -> Result<PopulationSummary> {
    let mut reader = ReaderBuilder::new().trim(csv::Trim::All).from_path(path)?;
    let headers = reader.headers()?.clone();
    let selected = required_header(&headers, "selected_sources")?;
    let min_g = required_header(&headers, "min_g")?;
    let max_g = required_header(&headers, "max_g")?;
    let rows = reader
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() != 1 {
        bail!("Gaia population preflight must contain exactly one row");
    }
    Ok(PopulationSummary {
        selected_sources: rows[0][selected].parse()?,
        min_g: rows[0][min_g].parse()?,
        max_g: rows[0][max_g].parse()?,
    })
}

fn validate_population_summary(args: &Args, summary: &PopulationSummary) -> Result<()> {
    if summary.selected_sources != args.expected_source_count {
        bail!(
            "Gaia population completeness gate failed: expected {}, TAP returned {} (the previous 592652-row file is incomplete)",
            args.expected_source_count,
            summary.selected_sources
        );
    }
    if !summary.min_g.is_finite()
        || !summary.max_g.is_finite()
        || summary.max_g > args.max_g_mag
        || summary.min_g > summary.max_g
    {
        bail!("Gaia population magnitude range is invalid");
    }
    Ok(())
}

fn read_metadata(path: &Path) -> Result<Vec<MetadataRow>> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(path)?;
    let headers = reader.headers()?.clone();
    for required in ["source_id", "ra", "dec", "ref_epoch"] {
        required_header(&headers, required)?;
    }
    let mut rows = Vec::new();
    let mut source_ids = BTreeSet::new();
    for row in reader.records() {
        let row = row?;
        let fields = headers
            .iter()
            .zip(row.iter())
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect::<BTreeMap<_, _>>();
        let source_id = fields
            .get("source_id")
            .context("metadata row missing source_id")?;
        if !source_ids.insert(source_id.clone()) {
            bail!("duplicate metadata source_id {source_id}");
        }
        rows.push(MetadataRow { fields });
    }
    if rows.is_empty() {
        bail!("metadata CSV contains no rows");
    }
    Ok(rows)
}

fn base_diagnostics(
    args: &Args,
    paths: &Paths,
    mode: XpRetrievalMode,
    selected_sources: usize,
) -> Diagnostics {
    Diagnostics {
        schema_version: 2,
        catalogue_name: "Gaia",
        catalogue_release: "DR3",
        source_population: "Gaia DR3 sources with published externally calibrated XP sampled mean spectra".to_string(),
        selection_predicate: "has_xp_sampled='True' AND phot_g_mean_mag<=15 AND valid source_id/ra/dec/ref_epoch; duplicated_source retained as a quality flag".to_string(),
        completeness_limitations: "Magnitude-limited sampled subset of 219197643 XP continuous sources; mission-mean spectra; blending, confusion, variability, and probabilistic non-stellar classification remain".to_string(),
        magnitude_limit: "effective Gaia DR3 XP sampled limit G<=15".to_string(),
        xp_product_type: "Gaia DR3 XP_SAMPLED; 343 samples on 336..1020 nm at 2 nm; flux and flux_error in W m^-2 nm^-1".to_string(),
        estimated_missing_flux_contribution: args
            .missing_flux_report
            .as_ref()
            .map(|path| format!("reviewed report {}", path.display()))
            .unwrap_or_else(|| "unknown; reviewed continuous-only stratified estimate not supplied".to_string()),
        release_completeness_gate: if args.missing_flux_report.is_some() {
            "review supplied; downstream validation required".to_string()
        } else {
            "failed: missing-flux report absent".to_string()
        },
        adql_path: paths.adql.display().to_string(),
        metadata_rows: selected_sources,
        selected_sources,
        xp_retrieval_mode: mode.as_str().to_string(),
        band_min_nm: BAND_MIN_NM,
        band_max_nm: BAND_MAX_NM,
        max_g_mag: args.max_g_mag,
        checkpoint_path: paths.xp_checkpoint.display().to_string(),
        uncertainty_model: "diagonal propagation of Gaia flux_error; sampled CSV provides no inter-sample covariance",
        production_mode: args.production,
        ..Diagnostics::default()
    }
}

async fn run_bulk_mode(args: &Args, paths: &Paths, diagnostics: &mut Diagnostics) -> Result<()> {
    if args.skip_xp_download {
        ensure_file(&paths.bulk_md5, "official bulk checksum inventory")?;
        ensure_file(&paths.bulk_manifest, "verified bulk manifest")?;
        let manifest: Value = serde_json::from_slice(&fs::read(&paths.bulk_manifest)?)?;
        if manifest.get("complete_inventory").and_then(Value::as_bool) != Some(true) {
            bail!("existing bulk manifest is not complete");
        }
        diagnostics.bulk = Some(manifest);
        diagnostics.completed_sources = diagnostics.selected_sources;
        return Ok(());
    }
    let config = BulkConfig {
        concurrency: args.bulk_concurrency,
        timeout: Duration::from_secs(args.bulk_timeout_secs),
        connect_timeout: Duration::from_secs(args.bulk_connect_timeout_secs),
        max_attempts: args.bulk_max_attempts,
        initial_backoff: Duration::from_millis(args.bulk_initial_backoff_ms),
        max_backoff: Duration::from_secs(args.bulk_max_backoff_secs),
        progress_interval: Duration::from_secs(args.progress_interval_secs),
        file_limit: args.bulk_file_limit,
        filename_allowlist: None,
    };
    let bulk_paths = BulkPaths {
        download_dir: paths.bulk_dir.clone(),
        error_dir: paths.bulk_error_dir.clone(),
        checksum_manifest_path: paths.bulk_md5.clone(),
        output_manifest_path: paths.bulk_manifest.clone(),
    };
    let downloader = BulkDownloader::new(&args.bulk_url, config)?;
    match downloader.download(&bulk_paths, args.resume).await {
        Ok(report) => {
            diagnostics.requests_total = report.requests_total;
            diagnostics.retry_attempts_total = report.retries_total;
            diagnostics.http_status_counts = report.http_status_counts.clone();
            diagnostics.bytes_downloaded = report.bytes_downloaded;
            diagnostics.elapsed_seconds = report.elapsed_seconds;
            diagnostics.representative_error_files = report.representative_errors.clone();
            diagnostics.failed_sources = report.failed_files;
            diagnostics.bulk = Some(serde_json::to_value(&report)?);
            if report.complete_inventory {
                diagnostics.completed_sources = diagnostics.selected_sources;
            }
            Ok(())
        }
        Err(err) => {
            diagnostics.bulk_error = Some(format!("{err:#}"));
            if paths.bulk_manifest.exists() {
                diagnostics.bulk = serde_json::from_slice(&fs::read(&paths.bulk_manifest)?).ok();
            }
            Err(err)
        }
    }
}

async fn run_datalink_mode(
    args: &Args,
    paths: &Paths,
    metadata: &[MetadataRow],
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    if args.limit.is_none() && !args.skip_metadata_download {
        bail!("DataLink mode requires --limit; complete production uses gaia-bulk");
    }
    let source_ids = metadata
        .iter()
        .map(|row| field(row, "source_id").to_string())
        .collect::<Vec<_>>();
    if !args.skip_xp_download {
        let downloader = Arc::new(DatalinkDownloader::new(
            &args.datalink_url,
            datalink_config(args),
        )?);
        let report = downloader
            .download(
                &source_ids,
                &DownloadPaths {
                    raw_dir: paths.xp_raw_dir.clone(),
                    error_dir: paths.xp_error_dir.clone(),
                    checkpoint: paths.xp_checkpoint.clone(),
                },
                args.resume,
                args.retry_failed_only,
            )
            .await?;
        copy_download_diagnostics(diagnostics, &report);
        diagnostics.datalink = Some(report);
    }
    let normalization = rebuild_normalized_chunks(
        &source_ids,
        args.chunk_size,
        &paths.xp_raw_dir,
        &paths.xp_dir,
    )?;
    diagnostics.parsed_sources = normalization.products_parsed;
    diagnostics.failed_sources += normalization.failures.len();
    diagnostics.normalization = Some(normalization);
    let products = load_normalized_products(&paths.xp_dir)?;
    merge_extract(metadata, &products, paths, diagnostics)
}

fn run_normalized_mode(
    args: &Args,
    paths: &Paths,
    metadata: &[MetadataRow],
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    let xp_dir = args.xp_dir.as_deref().unwrap_or(&paths.xp_dir);
    let products = load_normalized_products(xp_dir)?;
    diagnostics.parsed_sources = products.len();
    merge_extract(metadata, &products, paths, diagnostics)
}

fn datalink_config(args: &Args) -> DatalinkConfig {
    DatalinkConfig {
        concurrency: args.datalink_concurrency,
        max_rps: args.datalink_max_rps,
        timeout: Duration::from_secs(args.datalink_timeout_secs),
        connect_timeout: Duration::from_secs(args.datalink_connect_timeout_secs),
        max_attempts: args.datalink_max_attempts,
        initial_backoff: Duration::from_millis(args.datalink_initial_backoff_ms),
        max_backoff: Duration::from_secs(args.datalink_max_backoff_secs),
        progress_interval: Duration::from_secs(args.progress_interval_secs),
    }
}

fn copy_download_diagnostics(diagnostics: &mut Diagnostics, report: &DownloadReport) {
    diagnostics.completed_sources = report.completed_sources;
    diagnostics.retried_sources = report.retried_sources;
    diagnostics.failed_sources = report.failed_sources;
    diagnostics.requests_total = report.requests_total;
    diagnostics.retry_attempts_total = report.retry_attempts_total;
    diagnostics.http_status_counts = report.http_status_counts.clone();
    diagnostics.throughput_overall_sources_per_second =
        report.throughput_overall_sources_per_second;
    diagnostics.throughput_recent_sources_per_second = report.throughput_recent_sources_per_second;
    diagnostics.elapsed_seconds = report.elapsed_seconds;
    diagnostics.eta_seconds = (report.throughput_overall_sources_per_second > 0.0)
        .then(|| report.pending_sources as f64 / report.throughput_overall_sources_per_second);
    diagnostics.bytes_downloaded = report.bytes_downloaded;
    diagnostics.checkpoint_path = report.checkpoint_path.clone();
    diagnostics.representative_error_files = report.representative_error_files.clone();
}

fn load_normalized_products(dir: &Path) -> Result<BTreeMap<String, XpProduct>> {
    let mut paths = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("xp_chunk_") && name.ends_with(".csv"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        bail!("no normalized Gaia XP chunks found in {}", dir.display());
    }
    let mut products = BTreeMap::new();
    for path in paths {
        let mut reader = ReaderBuilder::new().trim(csv::Trim::All).from_path(&path)?;
        let headers = reader.headers()?.clone();
        for row in reader.records() {
            let product = parse_normalized_record(&headers, &row?)?;
            let source_id = product.source_id.clone();
            if products.insert(source_id.clone(), product).is_some() {
                bail!("duplicate normalized Gaia XP source_id {source_id}");
            }
        }
    }
    Ok(products)
}

fn merge_extract(
    metadata: &[MetadataRow],
    products: &BTreeMap<String, XpProduct>,
    paths: &Paths,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    let part = paths.extract.with_extension("csv.part");
    let file = File::create(&part)?;
    let mut writer = WriterBuilder::new().from_writer(file);
    writer.write_record([
        "source_id",
        "ra",
        "dec",
        "ref_epoch",
        "pmra",
        "pmdec",
        "parallax",
        "radial_velocity",
        "phot_g_mean_mag",
        "phot_bp_mean_mag",
        "phot_rp_mean_mag",
        "duplicated_source",
        NORMALIZED_WAVELENGTH_COLUMN,
        NORMALIZED_FLUX_COLUMN,
        NORMALIZED_FLUX_ERROR_COLUMN,
    ])?;

    let mut uncertainty_variance = 0.0;
    for row in metadata {
        let source_id = field(row, "source_id");
        let Some(product) = products.get(source_id) else {
            reject(diagnostics, "missing XP product", false);
            continue;
        };
        let integral = match integrate_photon_flux(product) {
            Ok(integral) => integral,
            Err(err) => {
                reject(diagnostics, &format!("invalid XP product: {err:#}"), false);
                continue;
            }
        };
        accumulate_integral(diagnostics, integral);
        if integral.total_ph_m2_s <= 0.0 {
            reject(
                diagnostics,
                "scientific exclusion: non-positive signed 336-650 nm integral",
                true,
            );
            continue;
        }
        if let Some(uncertainty) = integral.uncertainty_ph_m2_s {
            uncertainty_variance += uncertainty.powi(2);
        }
        let errors = product
            .flux_error_w_m2_nm
            .as_deref()
            .context("normalized Gaia XP product is missing flux_error")?;
        writer.write_record([
            field(row, "source_id"),
            field(row, "ra"),
            field(row, "dec"),
            field(row, "ref_epoch"),
            field(row, "pmra"),
            field(row, "pmdec"),
            field(row, "parallax"),
            field(row, "radial_velocity"),
            field(row, "phot_g_mean_mag"),
            field(row, "phot_bp_mean_mag"),
            field(row, "phot_rp_mean_mag"),
            field(row, "duplicated_source"),
            &nsb_data_tools::gaia_xp::format_series(&product.wavelengths_nm, false),
            &nsb_data_tools::gaia_xp::format_series(&product.flux_w_m2_nm, true),
            &nsb_data_tools::gaia_xp::format_series(errors, true),
        ])?;
        diagnostics.accepted_sources += 1;
    }
    writer.flush()?;
    writer.into_inner()?.sync_all()?;
    fs::rename(&part, &paths.extract)?;
    diagnostics.integrated_uncertainty_quadrature_ph_m2_s = Some(uncertainty_variance.sqrt());
    diagnostics.extract_sha256 = Some(checksum_file(&paths.extract)?);
    atomic_write(
        &paths.checksum,
        format!(
            "{}  {EXTRACT_FILE}\n",
            diagnostics.extract_sha256.as_deref().unwrap_or_default()
        )
        .as_bytes(),
    )?;
    Ok(())
}

fn reject(diagnostics: &mut Diagnostics, reason: &str, scientific: bool) {
    if scientific {
        diagnostics.scientifically_excluded_sources += 1;
    } else {
        diagnostics.unexpected_rejected_sources += 1;
    }
    *diagnostics
        .rejection_reasons
        .entry(reason.to_string())
        .or_default() += 1;
}

fn accumulate_integral(diagnostics: &mut Diagnostics, integral: PhotonFluxIntegral) {
    diagnostics.negative_samples += integral.negative_samples;
    diagnostics.band_samples += integral.band_samples;
    diagnostics.integrated_positive_ph_m2_s += integral.positive_ph_m2_s;
    diagnostics.integrated_negative_ph_m2_s += integral.negative_ph_m2_s;
    diagnostics.integrated_total_ph_m2_s += integral.total_ph_m2_s;
}

fn finalize_derived_diagnostics(diagnostics: &mut Diagnostics) {
    diagnostics.negative_sample_fraction = if diagnostics.band_samples == 0 {
        0.0
    } else {
        diagnostics.negative_samples as f64 / diagnostics.band_samples as f64
    };
    diagnostics.integrated_negative_contribution_ratio =
        if diagnostics.integrated_positive_ph_m2_s > 0.0 {
            -diagnostics.integrated_negative_ph_m2_s / diagnostics.integrated_positive_ph_m2_s
        } else {
            0.0
        };
    if diagnostics.completed_sources == 0 && diagnostics.accepted_sources > 0 {
        diagnostics.completed_sources = diagnostics.accepted_sources
            + diagnostics.scientifically_excluded_sources
            + diagnostics.unexpected_rejected_sources;
    }
}

fn strict_failure(args: &Args, mode: XpRetrievalMode, diagnostics: &Diagnostics) -> Option<String> {
    if diagnostics.partial_files > 0 {
        return Some(format!(
            "pipeline left {} partial files; resume before continuing",
            diagnostics.partial_files
        ));
    }
    if mode == XpRetrievalMode::GaiaBulk {
        let limited_candidate_complete = args.candidate
            && args.allow_partial_candidate_xp
            && args.bulk_file_limit.is_some()
            && diagnostics
                .bulk
                .as_ref()
                .and_then(|bulk| bulk.get("complete"))
                .and_then(Value::as_bool)
                == Some(true);
        if limited_candidate_complete
            && diagnostics.bulk_error.is_none()
            && diagnostics.failed_sources == 0
        {
            return None;
        }
        if diagnostics.bulk_error.is_some()
            || diagnostics.completed_sources != diagnostics.selected_sources
            || diagnostics.failed_sources > 0
        {
            return Some(
                "bulk XP sampled inventory is incomplete or failed validation".to_string(),
            );
        }
        if args.production && diagnostics.release_completeness_gate.starts_with("failed") {
            return Some("production missing-flux completeness gate did not pass".to_string());
        }
        return None;
    }
    let incomplete = diagnostics.completed_sources != diagnostics.selected_sources
        || diagnostics.failed_sources > 0
        || diagnostics.unexpected_rejected_sources > 0
        || diagnostics.accepted_sources + diagnostics.scientifically_excluded_sources
            != diagnostics.selected_sources;
    if incomplete && !(args.candidate && args.allow_partial_candidate_xp) {
        return Some(format!(
            "strict XP gate failed: selected={} completed={} accepted={} scientific_exclusions={} failures={} unexpected_rejections={}",
            diagnostics.selected_sources,
            diagnostics.completed_sources,
            diagnostics.accepted_sources,
            diagnostics.scientifically_excluded_sources,
            diagnostics.failed_sources,
            diagnostics.unexpected_rejected_sources
        ));
    }
    None
}

fn write_diagnostics(path: &Path, diagnostics: &Diagnostics) -> Result<()> {
    atomic_write(
        path,
        format!("{}\n", serde_json::to_string_pretty(diagnostics)?).as_bytes(),
    )
}

fn write_env_file(paths: &Paths, mode: XpRetrievalMode, diagnostics: &Diagnostics) -> Result<()> {
    let mut raw = String::new();
    raw.push_str(&format!(
        "export GAIA_DR3_STARLIGHT_DIAGNOSTICS=\"{}\"\n",
        shell_escape(&absolute(&paths.diagnostics).display().to_string())
    ));
    raw.push_str(&format!(
        "export GAIA_DR3_SELECTED_SOURCES=\"{}\"\n",
        diagnostics.selected_sources
    ));
    match mode {
        XpRetrievalMode::GaiaBulk => raw.push_str(&format!(
            "export GAIA_DR3_XP_BULK_DIR=\"{}\"\nexport GAIA_DR3_XP_BULK_MANIFEST=\"{}\"\n",
            shell_escape(&absolute(&paths.bulk_dir).display().to_string()),
            shell_escape(&absolute(&paths.bulk_manifest).display().to_string())
        )),
        XpRetrievalMode::GaiaDatalink | XpRetrievalMode::NormalizedChunks => {
            raw.push_str(&format!(
                "export GAIA_DR3_STARLIGHT_EXTRACT=\"{}\"\nexport GAIA_DR3_STARLIGHT_EXTRACT_SHA256=\"{}\"\n",
                shell_escape(&absolute(&paths.extract).display().to_string()),
                diagnostics.extract_sha256.as_deref().unwrap_or_default()
            ));
        }
    }
    atomic_write(&paths.env, raw.as_bytes())
}

fn checksum_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{hex}"))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    let part = path.with_extension(format!("{extension}.part"));
    let mut file = File::create(&part)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&part, path)?;
    Ok(())
}

fn persist_http_error(
    dir: &Path,
    stem: &str,
    attempt: u32,
    headers: &reqwest::header::HeaderMap,
    body: &[u8],
) -> Result<()> {
    fs::create_dir_all(dir)?;
    let mut header_text = String::new();
    for (name, value) in headers {
        header_text.push_str(name.as_str());
        header_text.push_str(": ");
        header_text.push_str(value.to_str().unwrap_or("<non-UTF8>"));
        header_text.push('\n');
    }
    atomic_write(
        &dir.join(format!("{stem}.attempt_{attempt:02}.headers.txt")),
        header_text.as_bytes(),
    )?;
    atomic_write(&dir.join(format!("{stem}.attempt_{attempt:02}.body")), body)
}

fn count_partial_files(dir: &Path) -> Result<usize> {
    let mut count = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(current)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "part")
            {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn exponential_delay(initial_ms: u64, max_secs: u64, attempt: u32) -> Duration {
    let multiplier = 1_u64 << attempt.saturating_sub(1).min(20);
    Duration::from_millis(initial_ms.saturating_mul(multiplier)).min(Duration::from_secs(max_secs))
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|header| header.trim() == name)
        .ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn field<'a>(row: &'a MetadataRow, key: &str) -> &'a str {
    row.fields.get(key).map(String::as_str).unwrap_or("")
}

fn ensure_file(path: &Path, description: &str) -> Result<()> {
    if !path.is_file() {
        bail!("missing {description}: {}", path.display());
    }
    Ok(())
}

fn absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn shell_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_query_is_complete_and_keeps_quality_flagged_sources() {
        let adql = build_adql(XpRetrievalMode::GaiaBulk, 15.0, None);
        assert!(adql.contains("COUNT(*) AS selected_sources"));
        assert!(adql.contains("has_xp_sampled = 'True'"));
        assert!(adql.contains("phot_g_mean_mag <= 15"));
        assert!(!adql.contains("duplicated_source ="));
    }

    #[test]
    fn datalink_query_is_deterministic_and_limited() {
        let adql = build_adql(XpRetrievalMode::GaiaDatalink, 15.0, Some(42));
        assert!(adql.contains("SELECT TOP 42"));
        assert!(adql.contains("ORDER BY source_id"));
    }

    #[test]
    fn production_rejects_the_old_population_size_and_non_bulk_mode() {
        let mut args = test_args();
        args.production = true;
        args.candidate = false;
        args.expected_source_count = 592_652;
        assert!(validate_args(&args).is_err());
        args.expected_source_count = EXPECTED_SAMPLED_SOURCES;
        args.xp_retrieval = Some(XpRetrievalMode::GaiaDatalink);
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn fixed_spectral_contract_has_no_tolerance_or_alias() {
        let mut args = test_args();
        args.band_min_nm = 335.999_999;
        assert!(validate_args(&args).is_err());
        args.band_min_nm = BAND_MIN_NM;
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn population_summary_must_match_the_release_count() {
        let args = test_args();
        assert!(validate_population_summary(
            &args,
            &PopulationSummary {
                selected_sources: 592_652,
                min_g: 2.0,
                max_g: 15.0,
            }
        )
        .is_err());
    }

    fn test_args() -> Args {
        Args {
            out_dir: PathBuf::from("unused"),
            max_g_mag: 15.0,
            limit: Some(10),
            expected_source_count: EXPECTED_SAMPLED_SOURCES,
            chunk_size: 100,
            band_min_nm: BAND_MIN_NM,
            band_max_nm: BAND_MAX_NM,
            tap_url: DEFAULT_TAP_URL.to_string(),
            datalink_url: DEFAULT_DATALINK_URL.to_string(),
            bulk_url: DEFAULT_BULK_URL.to_string(),
            xp_retrieval: Some(XpRetrievalMode::GaiaDatalink),
            xp_dir: None,
            license_policy_file: None,
            validation_reference: None,
            missing_flux_report: None,
            resume: true,
            retry_failed_only: false,
            allow_partial_candidate_xp: false,
            candidate: true,
            production: false,
            dry_run: false,
            skip_metadata_download: false,
            skip_xp_download: false,
            datalink_concurrency: 4,
            datalink_max_rps: 10.0,
            datalink_timeout_secs: 60,
            datalink_connect_timeout_secs: 10,
            datalink_max_attempts: 3,
            datalink_initial_backoff_ms: 100,
            datalink_max_backoff_secs: 10,
            progress_interval_secs: 30,
            bulk_concurrency: 2,
            bulk_timeout_secs: 900,
            bulk_connect_timeout_secs: 15,
            bulk_max_attempts: 3,
            bulk_initial_backoff_ms: 100,
            bulk_max_backoff_secs: 10,
            bulk_file_limit: None,
        }
    }
}
