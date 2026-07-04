use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

const DEFAULT_TAP_URL: &str = "https://gea.esac.esa.int/tap-server/tap/sync";
const DEFAULT_DATALINK_URL: &str = "https://gea.esac.esa.int/data-server/data";
const EXTRACT_FILE: &str = "gaia_dr3_starlight_extract.csv";

#[derive(Debug, Parser)]
#[command(about = "Generate Gaia DR3 starlight release input files")]
struct Args {
    #[arg(long)]
    out_dir: PathBuf,
    #[arg(long, default_value_t = 20.0)]
    max_g_mag: f64,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long, default_value_t = 5000)]
    chunk_size: usize,
    #[arg(long, default_value_t = 330.0)]
    band_min_nm: f64,
    #[arg(long, default_value_t = 650.0)]
    band_max_nm: f64,
    #[arg(long, default_value = DEFAULT_TAP_URL)]
    tap_url: String,
    #[arg(long, default_value = DEFAULT_DATALINK_URL)]
    datalink_url: String,
    #[arg(long, value_enum)]
    xp_retrieval: Option<XpRetrievalMode>,
    #[arg(long)]
    datalink_template: Option<String>,
    #[arg(long)]
    xp_dir: Option<PathBuf>,
    #[arg(long)]
    license_policy_file: Option<PathBuf>,
    #[arg(long)]
    validation_reference: Option<PathBuf>,
    #[arg(long)]
    resume: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum XpRetrievalMode {
    None,
    NormalizedChunks,
    GaiaDatalink,
}

impl XpRetrievalMode {
    fn as_str(self) -> &'static str {
        match self {
            XpRetrievalMode::None => "none",
            XpRetrievalMode::NormalizedChunks => "normalized-chunks",
            XpRetrievalMode::GaiaDatalink => "gaia-datalink",
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
    extract: PathBuf,
    diagnostics: PathBuf,
    checksum: PathBuf,
    policy: PathBuf,
    env: PathBuf,
    validation_template: PathBuf,
}

#[derive(Debug, Default, Serialize)]
struct Diagnostics {
    schema_version: u32,
    catalogue_name: &'static str,
    catalogue_release: &'static str,
    adql_path: String,
    metadata_rows: usize,
    xp_retrieval_mode: String,
    xp_chunks_requested: usize,
    xp_chunks_completed: usize,
    xp_chunks_failed: usize,
    xp_raw_responses_written: usize,
    xp_normalized_chunks_written: usize,
    xp_products_seen: usize,
    xp_products_parsed: usize,
    merged_rows: usize,
    accepted_rows: usize,
    rejected_rows: usize,
    rejection_reasons: BTreeMap<String, usize>,
    band_min_nm: f64,
    band_max_nm: f64,
    max_g_mag: f64,
    extract_sha256: Option<String>,
    runtime_downloads: bool,
    production_mode: bool,
}

#[derive(Debug, Clone)]
struct MetadataRow {
    fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct XpProduct {
    wavelengths: String,
    fluxes: String,
}

#[derive(Debug, Deserialize)]
struct ValidationReference {
    #[serde(default)]
    production_use: bool,
    #[serde(default)]
    regions: Vec<Value>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    let paths = Paths::new(&args.out_dir);
    let adql = build_adql(args.max_g_mag, args.limit);

    if args.dry_run {
        print_dry_run(&args, &paths, &adql);
        return Ok(());
    }

    fs::create_dir_all(&paths.out_dir)?;
    fs::create_dir_all(&paths.xp_dir)?;
    fs::create_dir_all(&paths.xp_raw_dir)?;
    fs::write(&paths.adql, &adql)?;

    validate_policy_and_reference(&args, &paths)?;

    if args.skip_metadata_download {
        ensure_file(&paths.metadata, "metadata CSV")?;
    } else if args.resume && paths.metadata.exists() {
        eprintln!("resume: using existing {}", paths.metadata.display());
    } else {
        download_metadata(&args.tap_url, &adql, &paths.metadata)?;
    }

    let metadata = read_metadata(&paths.metadata)?;
    let source_ids: Vec<String> = metadata
        .iter()
        .filter_map(|row| row.fields.get("source_id").cloned())
        .collect();

    let xp_dir = args.xp_dir.clone().unwrap_or_else(|| paths.xp_dir.clone());
    let xp_mode = select_xp_retrieval_mode(&args, &xp_dir)?;
    let mut diagnostics = Diagnostics {
        schema_version: 1,
        catalogue_name: "Gaia",
        catalogue_release: "DR3",
        adql_path: paths.adql.display().to_string(),
        metadata_rows: metadata.len(),
        xp_retrieval_mode: xp_mode.as_str().to_string(),
        band_min_nm: args.band_min_nm,
        band_max_nm: args.band_max_nm,
        max_g_mag: args.max_g_mag,
        runtime_downloads: false,
        production_mode: args.production,
        ..Diagnostics::default()
    };

    if args.skip_xp_download {
        eprintln!(
            "skip-xp-download: using existing XP chunks in {}",
            xp_dir.display()
        );
    } else {
        match xp_mode {
            XpRetrievalMode::None => {
                write_xp_template_note(&paths.xp_dir)?;
                eprintln!(
                    "XP retrieval disabled; using any existing chunks in {}",
                    xp_dir.display()
                );
            }
            XpRetrievalMode::NormalizedChunks => {
                eprintln!(
                    "using existing normalized XP chunks in {}",
                    xp_dir.display()
                );
            }
            XpRetrievalMode::GaiaDatalink => {
                if let Some(template) = args.datalink_template.as_deref() {
                    let xp_counts = download_xp_chunks(
                        template,
                        &source_ids,
                        args.chunk_size,
                        &paths.xp_dir,
                        args.resume,
                    )?;
                    diagnostics.xp_chunks_requested = xp_counts.requested;
                    diagnostics.xp_chunks_completed = xp_counts.completed;
                    diagnostics.xp_chunks_failed = xp_counts.failed;
                } else {
                    let xp_counts = download_gaia_datalink_xp(
                        &args.datalink_url,
                        &source_ids,
                        args.chunk_size,
                        &paths.xp_dir,
                        &paths.xp_raw_dir,
                        args.resume,
                    )?;
                    diagnostics.xp_chunks_requested = xp_counts.requested;
                    diagnostics.xp_chunks_completed = xp_counts.completed;
                    diagnostics.xp_chunks_failed = xp_counts.failed;
                    diagnostics.xp_raw_responses_written = xp_counts.raw_written;
                    diagnostics.xp_normalized_chunks_written = xp_counts.normalized_written;
                }
            }
        }
    }

    let xp = read_xp_products(&xp_dir, &mut diagnostics)?;

    merge_extract(&metadata, &xp, &paths.extract, &args, &mut diagnostics)?;
    let checksum = checksum_file(&paths.extract)?;
    diagnostics.extract_sha256 = Some(checksum.clone());
    let production_failure = production_failure(&args, &diagnostics);
    fs::write(&paths.checksum, format!("{checksum}  {EXTRACT_FILE}\n"))?;
    fs::write(
        &paths.diagnostics,
        format!("{}\n", serde_json::to_string_pretty(&diagnostics)?),
    )?;
    write_env_file(&paths, &args, &checksum)?;

    if let Some(message) = production_failure {
        bail!("{message}");
    }

    println!("generated {}", paths.extract.display());
    println!("generated {}", paths.diagnostics.display());
    println!("generated {}", paths.checksum.display());
    println!("generated {}", paths.env.display());
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    if args.candidate && args.production {
        bail!("choose either --candidate or --production, not both");
    }
    if !args.max_g_mag.is_finite() || args.max_g_mag <= 0.0 {
        bail!("--max-g-mag must be finite and positive");
    }
    if args.chunk_size == 0 {
        bail!("--chunk-size must be positive");
    }
    if !args.band_min_nm.is_finite()
        || !args.band_max_nm.is_finite()
        || args.band_min_nm >= args.band_max_nm
    {
        bail!("band bounds must be finite and satisfy min < max");
    }
    Ok(())
}

fn select_xp_retrieval_mode(args: &Args, xp_dir: &Path) -> Result<XpRetrievalMode> {
    if let Some(mode) = args.xp_retrieval {
        return Ok(mode);
    }
    if args.production {
        return Ok(XpRetrievalMode::GaiaDatalink);
    }
    if has_normalized_chunks(xp_dir)? {
        Ok(XpRetrievalMode::NormalizedChunks)
    } else {
        Ok(XpRetrievalMode::None)
    }
}

fn has_normalized_chunks(dir: &Path) -> Result<bool> {
    if !dir.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension() == Some(OsStr::new("csv")) {
            return Ok(true);
        }
    }
    Ok(false)
}

impl Paths {
    fn new(out_dir: &Path) -> Self {
        let xp_dir = out_dir.join("gaia_dr3_xp_chunks");
        Self {
            out_dir: out_dir.to_path_buf(),
            adql: out_dir.join("gaia_dr3_starlight_extract.adql"),
            metadata: out_dir.join("gaia_dr3_metadata.csv"),
            xp_raw_dir: xp_dir.join("raw"),
            xp_dir,
            extract: out_dir.join(EXTRACT_FILE),
            diagnostics: out_dir.join("gaia_dr3_starlight_extract.diagnostics.json"),
            checksum: out_dir.join("gaia_dr3_starlight_extract.sha256"),
            policy: out_dir.join("gaia_derived_product_policy.txt"),
            env: out_dir.join("starlight_release_inputs.env"),
            validation_template: out_dir
                .join("starlight_independent_validation_reference.template.json"),
        }
    }
}

fn build_adql(max_g_mag: f64, limit: Option<usize>) -> String {
    let top = limit.map(|n| format!("TOP {n} ")).unwrap_or_default();
    format!("SELECT {top}\n  source_id,\n  ra,\n  dec,\n  ref_epoch,\n  pmra,\n  pmdec,\n  parallax,\n  radial_velocity,\n  phot_g_mean_mag,\n  phot_bp_mean_mag,\n  phot_rp_mean_mag,\n  duplicated_source,\n  has_xp_sampled\nFROM gaiadr3.gaia_source\nWHERE duplicated_source = 'false'\n  AND has_xp_sampled = 'true'\n  AND phot_g_mean_mag IS NOT NULL\n  AND ra IS NOT NULL\n  AND dec IS NOT NULL\n  AND ref_epoch IS NOT NULL\n  AND phot_g_mean_mag <= {max_g_mag}\n")
}

fn print_dry_run(args: &Args, paths: &Paths, adql: &str) {
    println!("out_dir: {}", paths.out_dir.display());
    println!("metadata: {}", paths.metadata.display());
    println!("xp_dir: {}", paths.xp_dir.display());
    println!("xp_raw_dir: {}", paths.xp_raw_dir.display());
    println!("extract: {}", paths.extract.display());
    println!("diagnostics: {}", paths.diagnostics.display());
    println!("checksum: {}", paths.checksum.display());
    println!("env: {}", paths.env.display());
    println!("tap_url: {}", args.tap_url);
    println!("datalink_url: {}", args.datalink_url);
    println!("xp_retrieval: {:?}", args.xp_retrieval);
    println!("production: {}", args.production);
    println!("ADQL:\n{adql}");
}

fn validate_policy_and_reference(args: &Args, paths: &Paths) -> Result<()> {
    match args.license_policy_file.as_ref() {
        Some(path) => {
            let policy = fs::read_to_string(path)
                .with_context(|| format!("failed to read policy {}", path.display()))?;
            validate_text_field("license policy", &policy, args.production)?;
            fs::write(&paths.policy, policy)?;
        }
        None if args.production => bail!("--production requires --license-policy-file"),
        None => fs::write(
            &paths.policy,
            "TODO: reviewed Gaia-derived product redistribution and attribution policy.\n",
        )?,
    }

    match args.validation_reference.as_ref() {
        Some(path) => validate_validation_reference(path, args.production)?,
        None if args.production => bail!("--production requires --validation-reference"),
        None => fs::write(&paths.validation_template, validation_reference_template())?,
    }
    Ok(())
}

fn validate_text_field(name: &str, value: &str, production: bool) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{name} must not be empty");
    }
    if production {
        let lower = value.to_ascii_lowercase();
        for blocked in [
            "todo",
            "unknown",
            "pending",
            "review required",
            "unreviewed",
        ] {
            if lower.contains(blocked) {
                bail!("{name} contains production placeholder {blocked:?}");
            }
        }
    }
    Ok(())
}

fn validate_validation_reference(path: &Path, production: bool) -> Result<()> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read validation reference {}", path.display()))?;
    if production {
        validate_text_field("validation reference", &raw, true)?;
    }
    let parsed: ValidationReference = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse validation reference {}", path.display()))?;
    if production {
        if !parsed.production_use {
            bail!("production validation reference must set production_use=true");
        }
        if parsed.regions.len() < 4 {
            bail!("production validation reference must contain at least four regions");
        }
        let text = raw.to_ascii_lowercase();
        for required in ["pole", "plane", "center", "seam"] {
            if !text.contains(required) {
                bail!("production validation reference is missing a {required} region");
            }
        }
        if text.contains("null") {
            bail!("production validation reference must not contain null expected ranges");
        }
    }
    Ok(())
}

fn download_metadata(tap_url: &str, adql: &str, output: &Path) -> Result<()> {
    eprintln!("downloading Gaia metadata to {}", output.display());
    let status = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("-X")
        .arg("POST")
        .arg("--data-urlencode")
        .arg("REQUEST=doQuery")
        .arg("--data-urlencode")
        .arg("LANG=ADQL")
        .arg("--data-urlencode")
        .arg("FORMAT=csv")
        .arg("--data-urlencode")
        .arg(format!("QUERY={adql}"))
        .arg(tap_url)
        .arg("-o")
        .arg(output)
        .status()
        .context("failed to execute curl for Gaia TAP metadata download")?;
    if !status.success() {
        bail!("Gaia TAP metadata download failed with status {status}");
    }
    Ok(())
}

#[derive(Debug, Default)]
struct XpDownloadCounts {
    requested: usize,
    completed: usize,
    failed: usize,
    raw_written: usize,
    normalized_written: usize,
}

fn download_xp_chunks(
    template: &str,
    source_ids: &[String],
    chunk_size: usize,
    xp_dir: &Path,
    resume: bool,
) -> Result<XpDownloadCounts> {
    fs::create_dir_all(xp_dir)?;
    let mut counts = XpDownloadCounts::default();
    for (chunk_idx, chunk) in source_ids.chunks(chunk_size).enumerate() {
        counts.requested += 1;
        let path = xp_dir.join(format!("xp_chunk_{chunk_idx:06}.raw"));
        if resume && path.exists() {
            counts.completed += 1;
            counts.raw_written += 1;
            continue;
        }
        let joined = chunk.join(",");
        let url = template
            .replace("{source_ids}", &joined)
            .replace("{chunk}", &chunk_idx.to_string());
        let status = Command::new("curl")
            .arg("--fail")
            .arg("--location")
            .arg("--silent")
            .arg("--show-error")
            .arg(&url)
            .arg("-o")
            .arg(&path)
            .status()
            .with_context(|| format!("failed to execute curl for XP chunk {chunk_idx}"))?;
        if status.success() {
            counts.completed += 1;
            counts.raw_written += 1;
        } else {
            counts.failed += 1;
            fs::write(
                xp_dir.join(format!("xp_chunk_{chunk_idx:06}.error.txt")),
                format!("curl failed with status {status}\nurl={url}\n"),
            )?;
        }
    }
    Ok(counts)
}

fn download_gaia_datalink_xp(
    datalink_url: &str,
    source_ids: &[String],
    chunk_size: usize,
    xp_dir: &Path,
    raw_dir: &Path,
    resume: bool,
) -> Result<XpDownloadCounts> {
    fs::create_dir_all(xp_dir)?;
    fs::create_dir_all(raw_dir)?;
    let mut counts = XpDownloadCounts::default();
    for (chunk_idx, chunk) in source_ids.chunks(chunk_size).enumerate() {
        counts.requested += 1;
        let normalized = xp_dir.join(format!("xp_chunk_{chunk_idx:06}.csv"));
        if resume && normalized.exists() {
            counts.completed += 1;
            counts.normalized_written += 1;
            continue;
        }
        let mut chunk_products = BTreeMap::new();
        let mut chunk_failed = false;
        for source_id in chunk {
            let raw = raw_dir.join(format!("xp_source_{source_id}.csv"));
            if !(resume && raw.exists()) {
                if let Err(err) = download_one_gaia_datalink_source(datalink_url, source_id, &raw) {
                    chunk_failed = true;
                    fs::write(
                        raw_dir.join(format!("xp_source_{source_id}.error.txt")),
                        format!("{err:#}\n"),
                    )?;
                    continue;
                }
                counts.raw_written += 1;
                sleep(Duration::from_millis(100));
            }
            match read_xp_csv_with_fallback(&raw, Some(source_id)) {
                Ok(found) => chunk_products.extend(found),
                Err(err) => {
                    chunk_failed = true;
                    let preview = preview_file(&raw).unwrap_or_default();
                    fs::write(
                        raw_dir.join(format!("xp_source_{source_id}.parse-error.txt")),
                        format!("{err:#}\n\nfirst_bytes:\n{preview}\n"),
                    )?;
                }
            }
        }
        if chunk_products.is_empty() {
            counts.failed += 1;
            fs::write(
                xp_dir.join(format!("xp_chunk_{chunk_idx:06}.error.txt")),
                "no parseable XP products in chunk\n",
            )?;
            continue;
        }
        write_normalized_xp_chunk(&normalized, &chunk_products)?;
        counts.normalized_written += 1;
        if chunk_failed {
            counts.failed += 1;
        } else {
            counts.completed += 1;
        }
    }
    Ok(counts)
}

fn download_one_gaia_datalink_source(
    datalink_url: &str,
    source_id: &str,
    output: &Path,
) -> Result<()> {
    let status = Command::new("curl")
        .arg("--fail")
        .arg("--location")
        .arg("--silent")
        .arg("--show-error")
        .arg("-G")
        .arg(datalink_url)
        .arg("--data-urlencode")
        .arg(format!("ID=Gaia DR3 {source_id}"))
        .arg("--data-urlencode")
        .arg("RETRIEVAL_TYPE=XP_SAMPLED")
        .arg("--data-urlencode")
        .arg("DATA_STRUCTURE=INDIVIDUAL")
        .arg("--data-urlencode")
        .arg("FORMAT=csv")
        .arg("-o")
        .arg(output)
        .status()
        .with_context(|| format!("failed to execute curl for Gaia XP source {source_id}"))?;
    if !status.success() {
        bail!("Gaia DataLink XP download failed for source {source_id} with status {status}");
    }
    Ok(())
}

fn preview_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let len = bytes.len().min(2048);
    Ok(String::from_utf8_lossy(&bytes[..len]).to_string())
}

fn write_normalized_xp_chunk(path: &Path, products: &BTreeMap<String, XpProduct>) -> Result<()> {
    let mut writer = WriterBuilder::new().from_path(path)?;
    writer.write_record(["source_id", "xp_wavelength_nm", "xp_flux_w_m2_nm"])?;
    for (source_id, product) in products {
        writer.write_record([
            source_id.as_str(),
            product.wavelengths.as_str(),
            product.fluxes.as_str(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn read_metadata(path: &Path) -> Result<Vec<MetadataRow>> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open metadata CSV {}", path.display()))?;
    let headers = reader.headers()?.clone();
    for required in ["source_id", "ra", "dec", "ref_epoch"] {
        require_header(&headers, required)?;
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let fields = headers
            .iter()
            .zip(record.iter())
            .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            .collect();
        rows.push(MetadataRow { fields });
    }
    if rows.is_empty() {
        bail!("metadata CSV contains no rows");
    }
    Ok(rows)
}

fn read_xp_products(
    dir: &Path,
    diagnostics: &mut Diagnostics,
) -> Result<BTreeMap<String, XpProduct>> {
    let mut products = BTreeMap::new();
    if !dir.exists() {
        return Ok(products);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file()
            || path.extension() == Some(OsStr::new("txt"))
            || path.file_name() == Some(OsStr::new("README.txt"))
        {
            continue;
        }
        diagnostics.xp_products_seen += 1;
        let parsed = match path.extension().and_then(OsStr::to_str) {
            Some("csv") => read_xp_csv(&path),
            Some("json") => read_xp_json(&path),
            _ => read_xp_csv(&path).or_else(|_| read_xp_json(&path)),
        };
        match parsed {
            Ok(found) => {
                diagnostics.xp_products_parsed += found.len();
                products.extend(found);
            }
            Err(err) => {
                *diagnostics
                    .rejection_reasons
                    .entry(format!("unparsed XP chunk: {err}"))
                    .or_default() += 1;
            }
        }
    }
    Ok(products)
}

fn read_xp_csv(path: &Path) -> Result<BTreeMap<String, XpProduct>> {
    read_xp_csv_with_fallback(path, None)
}

fn read_xp_csv_with_fallback(
    path: &Path,
    fallback_source_id: Option<&str>,
) -> Result<BTreeMap<String, XpProduct>> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(path)?;
    let headers = reader.headers()?.clone();

    if let (Ok(source_idx), Ok(wave_idx), Ok(flux_idx)) = (
        require_header(&headers, "source_id"),
        require_header(&headers, "xp_wavelength_nm"),
        require_header(&headers, "xp_flux_w_m2_nm"),
    ) {
        let mut out = BTreeMap::new();
        for row in reader.records() {
            let row = row?;
            let source_id = row.get(source_idx).unwrap_or_default().trim().to_string();
            let wavelengths = row.get(wave_idx).unwrap_or_default().trim().to_string();
            let fluxes = row.get(flux_idx).unwrap_or_default().trim().to_string();
            if !source_id.is_empty() {
                out.insert(
                    source_id,
                    XpProduct {
                        wavelengths,
                        fluxes,
                    },
                );
            }
        }
        return Ok(out);
    }

    let wave_idx = find_any_header(
        &headers,
        &["wavelength", "wavelength_nm", "lambda", "lambda_nm", "wl"],
    )
    .ok_or_else(|| anyhow::anyhow!("missing XP wavelength column in {}", path.display()))?;
    let flux_idx = find_any_header(
        &headers,
        &["flux", "flux_w_m2_nm", "flux_density", "flux_lambda"],
    )
    .ok_or_else(|| anyhow::anyhow!("missing XP flux column in {}", path.display()))?;
    let source_idx = find_any_header(&headers, &["source_id", "sourceid", "source"]);
    let mut grouped: BTreeMap<String, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
    for row in reader.records() {
        let row = row?;
        let source_id = if let Some(source_idx) = source_idx {
            row.get(source_idx).unwrap_or_default().trim().to_string()
        } else if let Some(source_id) = fallback_source_id {
            source_id.to_string()
        } else {
            extract_source_id_from_path(path).unwrap_or_default()
        };
        if source_id.is_empty() {
            continue;
        }
        let waves = parse_maybe_series(row.get(wave_idx).unwrap_or_default())?;
        let fluxes = parse_maybe_series(row.get(flux_idx).unwrap_or_default())?;
        let entry = grouped.entry(source_id).or_default();
        entry.0.extend(waves);
        entry.1.extend(fluxes);
    }
    let mut out = BTreeMap::new();
    for (source_id, (waves, fluxes)) in grouped {
        if waves.len() == fluxes.len() && !waves.is_empty() {
            out.insert(
                source_id,
                XpProduct {
                    wavelengths: waves
                        .iter()
                        .map(|v| format!("{v:.8}"))
                        .collect::<Vec<_>>()
                        .join(";"),
                    fluxes: fluxes
                        .iter()
                        .map(|v| format!("{v:.8e}"))
                        .collect::<Vec<_>>()
                        .join(";"),
                },
            );
        }
    }
    if out.is_empty() {
        bail!("no parseable XP rows in {}", path.display());
    }
    Ok(out)
}

fn find_any_header(headers: &StringRecord, names: &[&str]) -> Option<usize> {
    headers.iter().position(|header| {
        let header = header.trim().to_ascii_lowercase();
        names
            .iter()
            .any(|candidate| header == *candidate || header.contains(candidate))
    })
}

fn extract_source_id_from_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    let mut current = String::new();
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if current.len() >= 10 {
            return Some(current);
        } else {
            current.clear();
        }
    }
    if current.len() >= 10 {
        Some(current)
    } else {
        None
    }
}

fn parse_maybe_series(raw: &str) -> Result<Vec<f64>> {
    let cleaned = raw
        .trim()
        .trim_matches('[')
        .trim_matches(']')
        .trim_matches('"')
        .replace([',', ' '], ";");
    cleaned
        .split(';')
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().parse::<f64>().map_err(Into::into))
        .collect()
}

fn read_xp_json(path: &Path) -> Result<BTreeMap<String, XpProduct>> {
    let raw = fs::read_to_string(path)?;
    let value: Value = serde_json::from_str(&raw)?;
    let mut out = BTreeMap::new();
    collect_xp_json(&value, &mut out);
    if out.is_empty() {
        bail!("no source_id/xp_wavelength_nm/xp_flux_w_m2_nm objects found in JSON");
    }
    Ok(out)
}

fn collect_xp_json(value: &Value, out: &mut BTreeMap<String, XpProduct>) {
    match value {
        Value::Object(map) => {
            if let (Some(source_id), Some(wavelengths), Some(fluxes)) = (
                json_string(map.get("source_id")),
                json_series(map.get("xp_wavelength_nm")),
                json_series(map.get("xp_flux_w_m2_nm")),
            ) {
                out.insert(
                    source_id,
                    XpProduct {
                        wavelengths,
                        fluxes,
                    },
                );
            }
            for child in map.values() {
                collect_xp_json(child, out);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_xp_json(child, out);
            }
        }
        _ => {}
    }
}

fn json_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_series(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Array(values) => {
            let mut out = Vec::new();
            for value in values {
                match value {
                    Value::Number(n) => out.push(n.to_string()),
                    Value::String(s) => out.push(s.clone()),
                    _ => return None,
                }
            }
            Some(out.join(";"))
        }
        _ => None,
    }
}

fn merge_extract(
    metadata: &[MetadataRow],
    xp: &BTreeMap<String, XpProduct>,
    output: &Path,
    args: &Args,
    diagnostics: &mut Diagnostics,
) -> Result<()> {
    let mut writer = WriterBuilder::new().from_path(output)?;
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
        "xp_wavelength_nm",
        "xp_flux_w_m2_nm",
    ])?;

    for row in metadata {
        let Some(source_id) = row.fields.get("source_id") else {
            diagnostics.rejected_rows += 1;
            *diagnostics
                .rejection_reasons
                .entry("missing source_id".into())
                .or_default() += 1;
            continue;
        };
        let Some(product) = xp.get(source_id) else {
            diagnostics.rejected_rows += 1;
            *diagnostics
                .rejection_reasons
                .entry("missing XP product".into())
                .or_default() += 1;
            continue;
        };
        match validate_xp_product(product, args.band_min_nm, args.band_max_nm) {
            Ok(()) => {}
            Err(reason) => {
                diagnostics.rejected_rows += 1;
                *diagnostics.rejection_reasons.entry(reason).or_default() += 1;
                continue;
            }
        }
        diagnostics.accepted_rows += 1;
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
            product.wavelengths.as_str(),
            product.fluxes.as_str(),
        ])?;
    }
    writer.flush()?;
    diagnostics.merged_rows = diagnostics.accepted_rows;
    if args.production && diagnostics.accepted_rows == 0 {
        *diagnostics
            .rejection_reasons
            .entry("zero accepted production rows".into())
            .or_default() += 1;
    }
    Ok(())
}

fn production_failure(args: &Args, diagnostics: &Diagnostics) -> Option<String> {
    if !args.production {
        return None;
    }
    if diagnostics.xp_chunks_failed > 0 {
        return Some(
            "--production requires all XP products to download and parse successfully".to_string(),
        );
    }
    if diagnostics.accepted_rows == 0 {
        return Some("--production requires at least one accepted Gaia XP source".to_string());
    }
    if diagnostics.rejected_rows > 0 {
        return Some(format!(
            "--production rejected {} selected Gaia rows; all selected sources require valid XP products",
            diagnostics.rejected_rows
        ));
    }
    if diagnostics
        .rejection_reasons
        .keys()
        .any(|reason| reason.starts_with("unparsed XP chunk"))
    {
        return Some("--production rejects malformed or unparsable XP products".to_string());
    }
    None
}

fn validate_xp_product(
    product: &XpProduct,
    band_min_nm: f64,
    band_max_nm: f64,
) -> std::result::Result<(), String> {
    let wavelengths = parse_series(&product.wavelengths)
        .map_err(|_| "invalid XP wavelength array".to_string())?;
    let fluxes = parse_series(&product.fluxes).map_err(|_| "invalid XP flux array".to_string())?;
    if wavelengths.len() != fluxes.len() {
        return Err("XP wavelength/flux length mismatch".to_string());
    }
    if wavelengths.is_empty() {
        return Err("empty XP arrays".to_string());
    }
    if fluxes
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("invalid XP flux value".to_string());
    }
    let min_wave = wavelengths.iter().copied().fold(f64::INFINITY, f64::min);
    let max_wave = wavelengths
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if min_wave > band_min_nm || max_wave < band_max_nm {
        return Err("XP spectrum does not cover requested band".to_string());
    }
    Ok(())
}

fn parse_series(raw: &str) -> std::result::Result<Vec<f64>, std::num::ParseFloatError> {
    raw.split(';')
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().parse::<f64>())
        .collect()
}

fn field<'a>(row: &'a MetadataRow, key: &str) -> &'a str {
    row.fields.get(key).map(String::as_str).unwrap_or("")
}

fn require_header(headers: &StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|header| header.trim() == name)
        .ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn checksum_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("sha256:{}", to_hex(&sha256(&bytes))))
}

fn ensure_file(path: &Path, description: &str) -> Result<()> {
    if !path.is_file() {
        bail!("missing {description}: {}", path.display());
    }
    Ok(())
}

fn write_env_file(paths: &Paths, args: &Args, checksum: &str) -> Result<()> {
    let policy = fs::read_to_string(&paths.policy).unwrap_or_default();
    let validation = args
        .validation_reference
        .as_ref()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .unwrap_or_else(|| paths.validation_template.clone());
    let extract = paths
        .extract
        .canonicalize()
        .unwrap_or_else(|_| paths.extract.clone());
    let mut file = fs::File::create(&paths.env)?;
    writeln!(
        file,
        "export GAIA_DR3_STARLIGHT_EXTRACT=\"{}\"",
        shell_escape(&extract.display().to_string())
    )?;
    writeln!(
        file,
        "export GAIA_DR3_STARLIGHT_EXTRACT_SHA256=\"{checksum}\""
    )?;
    writeln!(
        file,
        "export GAIA_DERIVED_PRODUCT_LICENSE_POLICY=\"{}\"",
        shell_escape(policy.trim())
    )?;
    writeln!(
        file,
        "export STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE=\"{}\"",
        shell_escape(&validation.display().to_string())
    )?;
    Ok(())
}

fn shell_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn write_xp_template_note(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("README.txt"), "Place Gaia XP chunk CSV/JSON files here. Each parsed product must expose source_id, xp_wavelength_nm, and xp_flux_w_m2_nm. Alternatively rerun with --xp-retrieval gaia-datalink.\n")?;
    Ok(())
}

fn validation_reference_template() -> &'static str {
    r#"{
  "schema_version": 1,
  "name": "NSB starlight independent validation reference template",
  "production_use": false,
  "band_nm": [330.0, 650.0],
  "units": "ph cm-2 ns-1 sr-1",
  "regions": [
    {
      "name": "north_galactic_pole",
      "frame": "galactic",
      "l_deg": 0.0,
      "b_deg": 90.0,
      "aperture_deg": 10.0,
      "expected_min": null,
      "expected_max": null,
      "source": "TODO: independent reference"
    },
    {
      "name": "galactic_plane_center",
      "frame": "galactic",
      "l_deg": 0.0,
      "b_deg": 0.0,
      "aperture_deg": 5.0,
      "expected_min": null,
      "expected_max": null,
      "source": "TODO: independent reference"
    },
    {
      "name": "galactic_center_region",
      "frame": "galactic",
      "l_deg": 0.0,
      "b_deg": 0.0,
      "aperture_deg": 10.0,
      "expected_min": null,
      "expected_max": null,
      "source": "TODO: independent reference"
    },
    {
      "name": "longitude_seam_plane",
      "frame": "galactic",
      "l_deg": 359.5,
      "b_deg": 0.0,
      "aperture_deg": 2.0,
      "expected_min": null,
      "expected_max": null,
      "source": "TODO: independent reference"
    }
  ]
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adql_contains_limit_and_magnitude_cut() {
        let adql = build_adql(12.5, Some(42));
        assert!(adql.contains("SELECT TOP 42"));
        assert!(adql.contains("phot_g_mean_mag <= 12.5"));
        assert!(adql.contains("has_xp_sampled = 'true'"));
    }

    #[test]
    fn xp_validation_rejects_mismatched_arrays() {
        let product = XpProduct {
            wavelengths: "330;400;650".into(),
            fluxes: "1;2".into(),
        };
        assert!(validate_xp_product(&product, 330.0, 650.0).is_err());
    }

    #[test]
    fn xp_validation_requires_band_coverage() {
        let product = XpProduct {
            wavelengths: "400;500;600".into(),
            fluxes: "1;2;3".into(),
        };
        assert_eq!(
            validate_xp_product(&product, 330.0, 650.0).unwrap_err(),
            "XP spectrum does not cover requested band"
        );
    }

    #[test]
    fn production_failure_rejects_missing_xp_products() {
        let args = test_args(true);
        let diagnostics = Diagnostics {
            production_mode: true,
            metadata_rows: 2,
            accepted_rows: 1,
            rejected_rows: 1,
            rejection_reasons: BTreeMap::from([("missing XP product".to_string(), 1)]),
            ..Diagnostics::default()
        };
        let message = production_failure(&args, &diagnostics).unwrap();
        assert!(message.contains("all selected sources require valid XP products"));
    }

    #[test]
    fn production_failure_rejects_malformed_xp_products() {
        let args = test_args(true);
        let diagnostics = Diagnostics {
            production_mode: true,
            metadata_rows: 1,
            accepted_rows: 0,
            rejected_rows: 1,
            rejection_reasons: BTreeMap::from([("invalid XP flux value".to_string(), 1)]),
            ..Diagnostics::default()
        };
        let message = production_failure(&args, &diagnostics).unwrap();
        assert!(message.contains("at least one accepted Gaia XP source"));
        assert_eq!(
            diagnostics.rejection_reasons.get("invalid XP flux value"),
            Some(&1)
        );
    }

    #[test]
    fn candidate_mode_keeps_non_production_diagnostics_non_blocking() {
        let args = test_args(false);
        let diagnostics = Diagnostics {
            production_mode: false,
            metadata_rows: 2,
            accepted_rows: 1,
            rejected_rows: 1,
            rejection_reasons: BTreeMap::from([("missing XP product".to_string(), 1)]),
            ..Diagnostics::default()
        };
        assert!(production_failure(&args, &diagnostics).is_none());
        assert_eq!(diagnostics.accepted_rows, 1);
        assert_eq!(diagnostics.rejected_rows, 1);
    }

    #[test]
    fn shell_escape_escapes_quotes_and_newlines() {
        assert_eq!(shell_escape("a\"b\nc"), "a\\\"b\\nc");
    }

    #[test]
    fn parses_series_with_commas_and_spaces() {
        assert_eq!(
            parse_maybe_series("[330, 400 650]").unwrap(),
            vec![330.0, 400.0, 650.0]
        );
    }

    fn test_args(production: bool) -> Args {
        Args {
            out_dir: PathBuf::from("unused"),
            max_g_mag: 20.0,
            limit: None,
            chunk_size: 5000,
            band_min_nm: 330.0,
            band_max_nm: 650.0,
            tap_url: DEFAULT_TAP_URL.to_string(),
            datalink_url: DEFAULT_DATALINK_URL.to_string(),
            xp_retrieval: Some(XpRetrievalMode::NormalizedChunks),
            datalink_template: None,
            xp_dir: None,
            license_policy_file: None,
            validation_reference: None,
            resume: false,
            candidate: !production,
            production,
            dry_run: false,
            skip_metadata_download: false,
            skip_xp_download: false,
        }
    }
}
