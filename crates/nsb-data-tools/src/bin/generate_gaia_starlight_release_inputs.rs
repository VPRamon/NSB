use anyhow::{bail, Context, Result};
use clap::Parser;
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

const DEFAULT_TAP_URL: &str = "https://gea.esac.esa.int/tap-server/tap/sync";
const EXTRACT_FILE: &str = "gaia_dr3_starlight_extract.csv";

/// Generate maintainer Gaia DR3 release inputs for the NSB starlight pipeline.
///
/// This tool is explicitly offline/release tooling. The runtime `nsb` crate must
/// never call it and must never download Gaia data.
#[derive(Debug, Parser)]
#[command(about = "Generate Gaia DR3 starlight release input files")]
struct Args {
    /// Output directory for all generated release files.
    #[arg(long)]
    out_dir: PathBuf,

    /// Gaia G-band magnitude cut used in the metadata ADQL query.
    #[arg(long, default_value_t = 20.0)]
    max_g_mag: f64,

    /// Optional row limit for smoke runs.
    #[arg(long)]
    limit: Option<usize>,

    /// Number of source_id values per XP download chunk.
    #[arg(long, default_value_t = 5000)]
    chunk_size: usize,

    /// Lower wavelength bound, nm, required in XP sampled spectra.
    #[arg(long, default_value_t = 330.0)]
    band_min_nm: f64,

    /// Upper wavelength bound, nm, required in XP sampled spectra.
    #[arg(long, default_value_t = 650.0)]
    band_max_nm: f64,

    /// Gaia TAP sync endpoint.
    #[arg(long, default_value = DEFAULT_TAP_URL)]
    tap_url: String,

    /// Optional DataLink/curl URL template used to fetch XP chunks.
    ///
    /// Supported placeholders:
    /// - {source_ids}: comma-separated source_id list for the current chunk
    /// - {chunk}: zero-based chunk index
    ///
    /// The downloaded chunk must be parseable as CSV or JSON containing
    /// source_id, xp_wavelength_nm, and xp_flux_w_m2_nm fields. If Gaia returns a
    /// different native DataLink layout, keep the raw chunks and adapt the parser
    /// before production.
    #[arg(long)]
    datalink_template: Option<String>,

    /// Existing XP chunk directory to merge instead of downloading XP chunks.
    #[arg(long)]
    xp_dir: Option<PathBuf>,

    /// Reviewed Gaia-derived product policy text.
    #[arg(long)]
    license_policy_file: Option<PathBuf>,

    /// Independent validation reference JSON.
    #[arg(long)]
    validation_reference: Option<PathBuf>,

    /// Reuse existing metadata/XP files where possible.
    #[arg(long)]
    resume: bool,

    /// Generate candidate release inputs; missing validation/policy is allowed.
    #[arg(long)]
    candidate: bool,

    /// Require production-grade policy and validation inputs.
    #[arg(long)]
    production: bool,

    /// Print planned files/queries and exit before network or filesystem writes.
    #[arg(long)]
    dry_run: bool,

    /// Skip Gaia TAP metadata download and use an existing metadata CSV in out-dir.
    #[arg(long)]
    skip_metadata_download: bool,

    /// Skip XP download. Requires --xp-dir or existing out-dir/gaia_dr3_xp_chunks.
    #[arg(long)]
    skip_xp_download: bool,
}

#[derive(Debug)]
struct Paths {
    out_dir: PathBuf,
    adql: PathBuf,
    metadata: PathBuf,
    xp_dir: PathBuf,
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
    xp_chunks_requested: usize,
    xp_chunks_completed: usize,
    xp_chunks_failed: usize,
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

    let mut diagnostics = Diagnostics {
        schema_version: 1,
        catalogue_name: "Gaia",
        catalogue_release: "DR3",
        adql_path: paths.adql.display().to_string(),
        metadata_rows: metadata.len(),
        band_min_nm: args.band_min_nm,
        band_max_nm: args.band_max_nm,
        max_g_mag: args.max_g_mag,
        runtime_downloads: false,
        production_mode: args.production,
        ..Diagnostics::default()
    };

    let xp_dir = args.xp_dir.clone().unwrap_or_else(|| paths.xp_dir.clone());
    if !args.skip_xp_download {
        match args.datalink_template.as_deref() {
            Some(template) => {
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
            }
            None if args.production => bail!(
                "--production requires --datalink-template or --skip-xp-download with --xp-dir/existing chunks"
            ),
            None => {
                write_xp_template_note(&paths.xp_dir)?;
                eprintln!(
                    "candidate mode: no --datalink-template supplied; using any existing XP chunks in {}",
                    xp_dir.display()
                );
            }
        }
    }

    let xp = read_xp_products(&xp_dir, &mut diagnostics)?;
    if args.production && xp.is_empty() {
        bail!("--production requires parsed XP products");
    }

    merge_extract(&metadata, &xp, &paths.extract, &args, &mut diagnostics)?;
    let checksum = checksum_file(&paths.extract)?;
    diagnostics.extract_sha256 = Some(checksum.clone());
    fs::write(
        &paths.checksum,
        format!("{checksum}  {EXTRACT_FILE}\n"),
    )?;
    fs::write(
        &paths.diagnostics,
        format!("{}\n", serde_json::to_string_pretty(&diagnostics)?),
    )?;
    write_env_file(&paths, &args, &checksum)?;

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

impl Paths {
    fn new(out_dir: &Path) -> Self {
        Self {
            out_dir: out_dir.to_path_buf(),
            adql: out_dir.join("gaia_dr3_starlight_extract.adql"),
            metadata: out_dir.join("gaia_dr3_metadata.csv"),
            xp_dir: out_dir.join("gaia_dr3_xp_chunks"),
            extract: out_dir.join(EXTRACT_FILE),
            diagnostics: out_dir.join("gaia_dr3_starlight_extract.diagnostics.json"),
            checksum: out_dir.join("gaia_dr3_starlight_extract.sha256"),
            policy: out_dir.join("gaia_derived_product_policy.txt"),
            env: out_dir.join("starlight_release_inputs.env"),
            validation_template: out_dir.join("starlight_independent_validation_reference.template.json"),
        }
    }
}

fn build_adql(max_g_mag: f64, limit: Option<usize>) -> String {
    let top = limit.map(|n| format!("TOP {n} ")).unwrap_or_default();
    format!(
        "SELECT {top}\n  source_id,\n  ra,\n  dec,\n  ref_epoch,\n  pmra,\n  pmdec,\n  parallax,\n  radial_velocity,\n  phot_g_mean_mag,\n  phot_bp_mean_mag,\n  phot_rp_mean_mag,\n  duplicated_source\nFROM gaiadr3.gaia_source\nWHERE duplicated_source = 'false'\n  AND phot_g_mean_mag IS NOT NULL\n  AND ra IS NOT NULL\n  AND dec IS NOT NULL\n  AND ref_epoch IS NOT NULL\n  AND phot_g_mean_mag <= {max_g_mag}\n"
    )
}

fn print_dry_run(args: &Args, paths: &Paths, adql: &str) {
    println!("out_dir: {}", paths.out_dir.display());
    println!("metadata: {}", paths.metadata.display());
    println!("xp_dir: {}", paths.xp_dir.display());
    println!("extract: {}", paths.extract.display());
    println!("diagnostics: {}", paths.diagnostics.display());
    println!("checksum: {}", paths.checksum.display());
    println!("env: {}", paths.env.display());
    println!("tap_url: {}", args.tap_url);
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
        for blocked in ["todo", "unknown", "pending", "review required", "unreviewed"] {
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

fn read_xp_products(dir: &Path, diagnostics: &mut Diagnostics) -> Result<BTreeMap<String, XpProduct>> {
    let mut products = BTreeMap::new();
    if !dir.exists() {
        return Ok(products);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension() == Some(OsStr::new("txt")) {
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
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(path)?;
    let headers = reader.headers()?.clone();
    let source_idx = require_header(&headers, "source_id")?;
    let wave_idx = require_header(&headers, "xp_wavelength_nm")?;
    let flux_idx = require_header(&headers, "xp_flux_w_m2_nm")?;
    let mut out = BTreeMap::new();
    for row in reader.records() {
        let row = row?;
        let source_id = row.get(source_idx).unwrap_or_default().trim().to_string();
        let wavelengths = row.get(wave_idx).unwrap_or_default().trim().to_string();
        let fluxes = row.get(flux_idx).unwrap_or_default().trim().to_string();
        if !source_id.is_empty() {
            out.insert(source_id, XpProduct { wavelengths, fluxes });
        }
    }
    Ok(out)
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
                out.insert(source_id, XpProduct { wavelengths, fluxes });
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
            *diagnostics.rejection_reasons.entry("missing source_id".into()).or_default() += 1;
            continue;
        };
        let Some(product) = xp.get(source_id) else {
            diagnostics.rejected_rows += 1;
            *diagnostics.rejection_reasons.entry("missing XP product".into()).or_default() += 1;
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
        bail!("production Gaia extract has no accepted rows");
    }
    Ok(())
}

fn validate_xp_product(product: &XpProduct, band_min_nm: f64, band_max_nm: f64) -> std::result::Result<(), String> {
    let wavelengths = parse_series(&product.wavelengths).map_err(|_| "invalid XP wavelength array".to_string())?;
    let fluxes = parse_series(&product.fluxes).map_err(|_| "invalid XP flux array".to_string())?;
    if wavelengths.len() != fluxes.len() {
        return Err("XP wavelength/flux length mismatch".to_string());
    }
    if wavelengths.is_empty() {
        return Err("empty XP arrays".to_string());
    }
    if fluxes.iter().any(|value| !value.is_finite() || *value < 0.0) {
        return Err("invalid XP flux value".to_string());
    }
    let min_wave = wavelengths.iter().copied().fold(f64::INFINITY, f64::min);
    let max_wave = wavelengths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
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
    let extract = paths.extract.canonicalize().unwrap_or_else(|_| paths.extract.clone());
    let mut file = fs::File::create(&paths.env)?;
    writeln!(
        file,
        "export GAIA_DR3_STARLIGHT_EXTRACT=\"{}\"",
        shell_escape(&extract.display().to_string())
    )?;
    writeln!(file, "export GAIA_DR3_STARLIGHT_EXTRACT_SHA256=\"{checksum}\"")?;
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
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn write_xp_template_note(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("README.txt"),
        "Place Gaia XP chunk CSV/JSON files here. Each parsed product must expose source_id, xp_wavelength_nm, and xp_flux_w_m2_nm. Alternatively rerun with --datalink-template.\n",
    )?;
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
    fn shell_escape_escapes_quotes_and_newlines() {
        assert_eq!(shell_escape("a\"b\nc"), "a\\\"b\\nc");
    }
}
