use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Parser};
use csv::{ReaderBuilder, StringRecord, Writer, WriterBuilder};
use flate2::read::GzDecoder;
use nsb_data_tools::gaia_xp::{
    self, PhotonFluxIntegral, XpProduct, BAND_MAX_NM, BAND_MIN_NM, NORMALIZED_FLUX_COLUMN,
    NORMALIZED_FLUX_ERROR_COLUMN, NORMALIZED_WAVELENGTH_COLUMN, PHOTOMETRY_MODEL,
    PHOTON_FLUX_COLUMN,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use siderust::catalogs::gaia::{GaiaDr3QualityFlags, GaiaDr3RawSourceRow, GaiaDr3Source};
use siderust::checksum::to_hex;
use std::collections::{BTreeMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const GAIA_DR3_REFERENCE_EPOCH_JYR: f64 = 2016.0;
const NON_POSITIVE_INTEGRAL_REASON: &str = "non-positive integrated photon flux";
const UNCERTAINTY_MODEL: &str = "independent_sample_trapezoid_propagation_v1";
const UNCERTAINTY_MODEL_CAVEAT: &str = "Gaia XP flux_error samples are propagated as independent through the trapezoidal weights. Wavelength-sample covariance and inter-source calibration systematics are not available in this input, so these values are diagnostics rather than a complete uncertainty budget.";

/// Prepare canonical Gaia DR3 passband-integrated starlight source rows.
///
/// The preferred input is the official Gaia XP sampled bulk distribution. A
/// normalized, one-row-per-source CSV remains available as the resumable Gaia
/// DataLink fallback. Raw XP spectra never enter the NSB runtime product.
#[derive(Debug, Parser)]
#[command(group(
    ArgGroup::new("gaia_input")
        .required(true)
        .multiple(false)
        .args(["input", "bulk_dir"])
))]
struct Args {
    /// Normalized DataLink fallback CSV, one source per row.
    #[arg(long)]
    input: Option<PathBuf>,
    /// Directory containing official Gaia XP sampled `*.csv.gz` bulk files.
    #[arg(long)]
    bulk_dir: Option<PathBuf>,
    /// Canonical derived source CSV, written atomically.
    #[arg(long)]
    output: PathBuf,
    /// Optional JSON diagnostics report (required with --production).
    #[arg(long)]
    diagnostics_output: Option<PathBuf>,
    /// Source catalogue name, normally "Gaia".
    #[arg(long)]
    catalog_name: String,
    /// Source catalogue release, normally "DR3".
    #[arg(long)]
    catalog_release: String,
    /// Reviewed catalogue license or derived-product redistribution policy.
    #[arg(long)]
    catalog_license: String,
    /// Expected SHA-256 of the file, or canonical sorted bulk byte stream.
    #[arg(long)]
    source_checksum: Option<String>,
    /// Fixed photometry model identifier.
    #[arg(long, default_value = PHOTOMETRY_MODEL)]
    photometry_model: String,
    /// Fixed lower wavelength bound, nm.
    #[arg(long, default_value_t = BAND_MIN_NM)]
    band_min_nm: f64,
    /// Fixed upper wavelength bound, nm.
    #[arg(long, default_value_t = BAND_MAX_NM)]
    band_max_nm: f64,
    /// Enable release gates and require a diagnostics report.
    #[arg(long)]
    production: bool,
}

#[derive(Debug, Clone, Copy)]
struct NormalizedColumns {
    source_id: usize,
    ra: usize,
    dec: usize,
    ref_epoch: usize,
    pmra: Option<usize>,
    pmdec: Option<usize>,
    parallax: Option<usize>,
    radial_velocity: Option<usize>,
    phot_g_mean_mag: Option<usize>,
    phot_bp_mean_mag: Option<usize>,
    phot_rp_mean_mag: Option<usize>,
    xp_wavelength_nm: usize,
    xp_flux_w_m2_nm: usize,
    xp_flux_error_w_m2_nm: usize,
    quality_ok: Option<usize>,
    duplicated_source: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct BulkColumns {
    source_id: usize,
    ra: usize,
    dec: usize,
    wavelength: usize,
    flux: usize,
    flux_error: usize,
}

enum InputSelection {
    Normalized(PathBuf),
    Bulk { files: Vec<PathBuf> },
}

impl InputSelection {
    fn mode(&self) -> &'static str {
        match self {
            Self::Normalized(_) => "normalized_datalink_fallback",
            Self::Bulk { .. } => "official_bulk_xp_sampled",
        }
    }

    fn checksum_algorithm(&self) -> &'static str {
        match self {
            Self::Normalized(_) => "sha256(file_bytes)",
            Self::Bulk { .. } => {
                "sha256(NSB_GAIA_XP_BULK_V1 + sorted filename,length,file-bytes tuples)"
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct Diagnostics<'a> {
    schema_version: u32,
    production_mode: bool,
    strict_gate_passed: bool,
    all_sources_accounted: bool,
    input_mode: &'static str,
    xp_product_type: &'static str,
    catalogue_name: &'a str,
    catalogue_release: &'a str,
    catalogue_license: &'a str,
    input_checksum: String,
    input_checksum_algorithm: &'static str,
    photometry_model: &'a str,
    band_min_nm: f64,
    band_max_nm: f64,
    bulk_reference_epoch_jyr: Option<f64>,
    astrometry_epoch_provenance: &'static str,
    input_records_read: usize,
    rows_read: usize,
    unique_source_ids_read: usize,
    rows_used: usize,
    unique_sources_represented: usize,
    rows_scientifically_excluded: usize,
    non_positive_integrated_flux_sources: usize,
    rows_unexpectedly_rejected: usize,
    rows_rejected: usize,
    rejection_reasons: BTreeMap<String, usize>,
    scientific_exclusion_reasons: BTreeMap<String, usize>,
    unexpected_rejection_reasons: BTreeMap<String, usize>,
    integrated_spectra: usize,
    sources_with_negative_flux_samples: usize,
    flux_samples_in_band: usize,
    negative_flux_samples: usize,
    negative_flux_sample_fraction: f64,
    integrated_positive_contribution_ph_m2_s: f64,
    integrated_negative_contribution_ph_m2_s: f64,
    integrated_negative_contribution_ratio: Option<f64>,
    integrated_uncertainty_sources: usize,
    integrated_uncertainty_missing_sources: usize,
    integrated_uncertainty_min_ph_m2_s: Option<f64>,
    integrated_uncertainty_max_ph_m2_s: Option<f64>,
    integrated_uncertainty_mean_ph_m2_s: Option<f64>,
    integrated_uncertainty_quadrature_sum_ph_m2_s: Option<f64>,
    uncertainty_model: &'static str,
    uncertainty_model_caveat: &'static str,
}

#[derive(Debug, Default)]
struct Counters {
    input_records_read: usize,
    sources_read: usize,
    unique_source_ids_read: usize,
    rows_used: usize,
    scientifically_excluded: usize,
    unexpectedly_rejected: usize,
    rejection_reasons: BTreeMap<String, usize>,
    scientific_exclusion_reasons: BTreeMap<String, usize>,
    unexpected_rejection_reasons: BTreeMap<String, usize>,
    science: ScienceStats,
}

#[derive(Debug, Default)]
struct ScienceStats {
    integrated_spectra: usize,
    sources_with_negative_flux_samples: usize,
    band_samples: usize,
    negative_samples: usize,
    positive_ph_m2_s: f64,
    negative_ph_m2_s: f64,
    uncertainty_sources: usize,
    uncertainty_missing_sources: usize,
    uncertainty_sum_ph_m2_s: f64,
    uncertainty_squared_sum: f64,
    uncertainty_min_ph_m2_s: Option<f64>,
    uncertainty_max_ph_m2_s: Option<f64>,
}

enum Conversion {
    Accepted {
        output: [String; 7],
        integral: PhotonFluxIntegral,
    },
    ScientificallyExcluded {
        reason: &'static str,
        integral: PhotonFluxIntegral,
    },
}

struct BulkSourceBuilder {
    source_id: u64,
    ra_deg: Option<f64>,
    dec_deg: Option<f64>,
    wavelengths_nm: Vec<f64>,
    flux_w_m2_nm: Vec<f64>,
    flux_error_w_m2_nm: Vec<f64>,
    structural_error: Option<String>,
}

struct PendingOutput {
    temporary_path: PathBuf,
    final_path: PathBuf,
    committed: bool,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    let input = select_input(&args)?;
    let input_checksum = checksum_input(&input, args.source_checksum.as_deref())?;
    let checksum_algorithm = input.checksum_algorithm();
    let input_mode = input.mode();
    let (mut writer, pending_output) = transactional_output_writer(&args.output)?;
    write_output_header(&mut writer)?;

    let mut counters = Counters::default();
    match &input {
        InputSelection::Normalized(path) => {
            process_normalized(path, &args, &mut writer, &mut counters)?;
        }
        InputSelection::Bulk { files } => {
            process_bulk(files, &args, &mut writer, &mut counters)?;
        }
    }
    writer.flush()?;
    drop(writer);

    let all_sources_accounted = counters.sources_read
        == counters.rows_used + counters.scientifically_excluded + counters.unexpectedly_rejected;
    let strict_gate_passed =
        all_sources_accounted && counters.unexpectedly_rejected == 0 && counters.rows_used > 0;
    let diagnostics = build_diagnostics(
        &args,
        &input,
        input_mode,
        input_checksum,
        checksum_algorithm,
        all_sources_accounted,
        strict_gate_passed,
        &counters,
    );
    if let Some(path) = &args.diagnostics_output {
        let raw = serde_json::to_string_pretty(&diagnostics)?;
        write_atomic(path, format!("{raw}\n").as_bytes())?;
    }

    if !all_sources_accounted {
        bail!(
            "strict Gaia preparation gate failed: {} selected sources were not accounted",
            counters.sources_read
        );
    }
    if counters.unexpectedly_rejected != 0 {
        bail!(
            "strict Gaia preparation gate failed: {} unexpected source rejections; see diagnostics",
            counters.unexpectedly_rejected
        );
    }
    if counters.rows_used == 0 {
        bail!("Gaia preparation produced no represented starlight sources");
    }

    pending_output.commit()?;
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    for (name, value) in [
        ("--catalog-name", &args.catalog_name),
        ("--catalog-release", &args.catalog_release),
        ("--catalog-license", &args.catalog_license),
        ("--photometry-model", &args.photometry_model),
    ] {
        if value.trim().is_empty() {
            bail!("{name} must not be empty");
        }
    }
    if args.input.is_some() == args.bulk_dir.is_some() {
        bail!("exactly one of --input or --bulk-dir is required");
    }
    if args.photometry_model != PHOTOMETRY_MODEL {
        bail!(
            "unsupported Gaia production photometry model {}",
            args.photometry_model
        );
    }
    if args.band_min_nm != BAND_MIN_NM || args.band_max_nm != BAND_MAX_NM {
        bail!("Gaia XP product band is fixed at {BAND_MIN_NM}-{BAND_MAX_NM} nm");
    }
    if args.production && args.diagnostics_output.is_none() {
        bail!("--production requires --diagnostics-output");
    }
    if args.output.as_os_str() == "-" {
        bail!("--output must be a file so the strict result can be committed atomically");
    }
    if args
        .diagnostics_output
        .as_ref()
        .is_some_and(|path| path == &args.output)
    {
        bail!("--output and --diagnostics-output must be different files");
    }
    Ok(())
}

fn select_input(args: &Args) -> Result<InputSelection> {
    if let Some(path) = &args.input {
        if !path.is_file() {
            bail!("normalized Gaia input is not a file: {}", path.display());
        }
        return Ok(InputSelection::Normalized(path.clone()));
    }
    let directory = args
        .bulk_dir
        .as_ref()
        .context("missing --bulk-dir after argument validation")?;
    Ok(InputSelection::Bulk {
        files: discover_bulk_files(directory)?,
    })
}

fn discover_bulk_files(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.is_dir() {
        bail!(
            "Gaia XP bulk input is not a directory: {}",
            directory.display()
        );
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory).with_context(|| {
        format!(
            "failed to list Gaia XP bulk directory {}",
            directory.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect Gaia XP bulk directory {}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".partial") || name.ends_with(".part") || name.ends_with(".tmp") {
            bail!("partial Gaia XP bulk file present: {}", path.display());
        }
        if entry.file_type()?.is_file() && name.ends_with(".csv.gz") {
            files.push(path);
        }
    }
    files.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    if files.is_empty() {
        bail!(
            "Gaia XP bulk directory contains no complete *.csv.gz files: {}",
            directory.display()
        );
    }
    Ok(files)
}

fn checksum_input(input: &InputSelection, expected: Option<&str>) -> Result<String> {
    let digest = match input {
        InputSelection::Normalized(path) => checksum_file(path)?,
        InputSelection::Bulk { files } => checksum_bulk(files)?,
    };
    if let Some(expected) = expected {
        let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
        if expected != digest {
            bail!("source checksum mismatch: expected sha256:{expected}, actual sha256:{digest}");
        }
    }
    Ok(format!("sha256:{digest}"))
}

fn checksum_file(path: &Path) -> Result<String> {
    let mut file = BufReader::new(
        File::open(path).with_context(|| format!("failed to checksum {}", path.display()))?,
    );
    let mut hasher = Sha256::new();
    copy_into_hasher(&mut file, &mut hasher)?;
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(to_hex(&digest))
}

fn checksum_bulk(files: &[PathBuf]) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"NSB_GAIA_XP_BULK_V1\0");
    for path in files {
        let name = path
            .file_name()
            .context("bulk path has no filename")?
            .to_string_lossy();
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("failed to stat bulk file {}", path.display()))?;
        hasher.update((name.len() as u64).to_be_bytes());
        hasher.update(name.as_bytes());
        hasher.update(metadata.len().to_be_bytes());
        let mut file = BufReader::new(
            File::open(path)
                .with_context(|| format!("failed to checksum bulk file {}", path.display()))?,
        );
        copy_into_hasher(&mut file, &mut hasher)?;
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(to_hex(&digest))
}

fn copy_into_hasher(reader: &mut impl Read, hasher: &mut Sha256) -> Result<()> {
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn process_normalized(
    path: &Path,
    args: &Args,
    writer: &mut Writer<Box<dyn Write>>,
    counters: &mut Counters,
) -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_path(path)
        .with_context(|| format!("failed to open Gaia extract {}", path.display()))?;
    let headers = reader
        .headers()
        .context("failed to read Gaia CSV header")?
        .clone();
    let columns = NormalizedColumns::from_headers(&headers)?;
    let mut source_ids = HashSet::new();

    for row in reader.records() {
        let row = row.context("failed to read Gaia CSV record")?;
        counters.input_records_read += 1;
        counters.sources_read += 1;
        let source_id = match parse_u64(&row, columns.source_id, "source_id") {
            Ok(source_id) => source_id,
            Err(error) => {
                counters.reject_unexpected(error.to_string());
                continue;
            }
        };
        if !source_ids.insert(source_id) {
            counters.reject_unexpected("duplicate source_id");
            continue;
        }
        counters.unique_source_ids_read += 1;
        match convert_normalized_row(&headers, &row, columns, args) {
            Ok(conversion) => counters.handle_conversion(conversion, writer)?,
            Err(error) => counters.reject_unexpected(error.to_string()),
        }
    }
    Ok(())
}

fn process_bulk(
    files: &[PathBuf],
    args: &Args,
    writer: &mut Writer<Box<dyn Write>>,
    counters: &mut Counters,
) -> Result<()> {
    let mut last_source_id = None;
    for path in files {
        let file = File::open(path)
            .with_context(|| format!("failed to open Gaia XP bulk file {}", path.display()))?;
        let decoder = GzDecoder::new(BufReader::new(file));
        let mut reader = ReaderBuilder::new()
            .comment(Some(b'#'))
            .trim(csv::Trim::All)
            .from_reader(decoder);
        let headers = reader
            .headers()
            .with_context(|| format!("failed to read bulk CSV header in {}", path.display()))?
            .clone();
        let columns = BulkColumns::from_headers(&headers)?;
        let mut pending: Option<BulkSourceBuilder> = None;

        for row in reader.records() {
            let row = row.with_context(|| {
                format!(
                    "failed to read Gaia XP bulk CSV record in {}",
                    path.display()
                )
            })?;
            counters.input_records_read += 1;
            let source_id = parse_u64(&row, columns.source_id, "source_id")
                .with_context(|| format!("invalid source_id in {}", path.display()))?;
            if pending
                .as_ref()
                .is_some_and(|source| source.source_id != source_id)
            {
                process_bulk_source(
                    pending.take().expect("pending source exists"),
                    args,
                    writer,
                    counters,
                    &mut last_source_id,
                )?;
            }
            let source = pending.get_or_insert_with(|| BulkSourceBuilder::new(source_id));
            source.push(&row, columns);
        }
        if let Some(source) = pending.take() {
            process_bulk_source(source, args, writer, counters, &mut last_source_id)?;
        }
    }
    Ok(())
}

fn process_bulk_source(
    source: BulkSourceBuilder,
    args: &Args,
    writer: &mut Writer<Box<dyn Write>>,
    counters: &mut Counters,
    last_source_id: &mut Option<u64>,
) -> Result<()> {
    counters.sources_read += 1;
    if let Some(previous) = *last_source_id {
        if source.source_id == previous {
            counters.reject_unexpected("duplicate source_id");
            return Ok(());
        }
        if source.source_id < previous {
            counters.reject_unexpected("bulk source_id order is not strictly increasing");
            return Ok(());
        }
    }
    *last_source_id = Some(source.source_id);
    counters.unique_source_ids_read += 1;
    match source.into_conversion(args) {
        Ok(conversion) => counters.handle_conversion(conversion, writer),
        Err(error) => {
            counters.reject_unexpected(error.to_string());
            Ok(())
        }
    }
}

fn convert_normalized_row(
    headers: &StringRecord,
    row: &StringRecord,
    columns: NormalizedColumns,
    args: &Args,
) -> Result<Conversion> {
    if normalized_xp_is_absent(row, columns) {
        bail!("missing passband photometry");
    }
    let raw = GaiaDr3RawSourceRow {
        source_id: parse_u64(row, columns.source_id, "source_id")?,
        ra_deg: parse_required_f64(row, columns.ra, "ra")?,
        dec_deg: parse_required_f64(row, columns.dec, "dec")?,
        ref_epoch_jyr: parse_required_f64(row, columns.ref_epoch, "ref_epoch")?,
        pmra_mas_per_yr: parse_optional_f64(row, columns.pmra, "pmra")?,
        pmdec_mas_per_yr: parse_optional_f64(row, columns.pmdec, "pmdec")?,
        parallax_mas: parse_optional_f64(row, columns.parallax, "parallax")?,
        radial_velocity_km_s: parse_optional_f64(row, columns.radial_velocity, "radial_velocity")?,
        phot_g_mean_mag: parse_optional_f64(row, columns.phot_g_mean_mag, "phot_g_mean_mag")?,
        phot_bp_mean_mag: parse_optional_f64(row, columns.phot_bp_mean_mag, "phot_bp_mean_mag")?,
        phot_rp_mean_mag: parse_optional_f64(row, columns.phot_rp_mean_mag, "phot_rp_mean_mag")?,
        quality: GaiaDr3QualityFlags {
            quality_ok: parse_optional_bool(row, columns.quality_ok).unwrap_or(true),
            duplicated_source: parse_optional_bool(row, columns.duplicated_source),
        },
    };
    let product = gaia_xp::parse_normalized_record(headers, row)?;
    convert_source(raw, product, args)
}

fn convert_source(raw: GaiaDr3RawSourceRow, product: XpProduct, args: &Args) -> Result<Conversion> {
    let product_source_id = product
        .source_id
        .parse::<u64>()
        .context("invalid Gaia XP product source_id")?;
    if product_source_id != raw.source_id {
        bail!(
            "Gaia source/product source_id mismatch: {} != {}",
            raw.source_id,
            product.source_id
        );
    }
    let icrs_ra_rad = raw.ra_deg.to_radians();
    let icrs_dec_rad = raw.dec_deg.to_radians();
    let source =
        GaiaDr3Source::try_from(raw).map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let integral = gaia_xp::integrate_photon_flux(&product)?;
    validate_integral(integral)?;
    if integral.total_ph_m2_s <= 0.0 {
        return Ok(Conversion::ScientificallyExcluded {
            reason: NON_POSITIVE_INTEGRAL_REASON,
            integral,
        });
    }

    Ok(Conversion::Accepted {
        output: [
            source.source_id.value().to_string(),
            format!("{icrs_ra_rad:.16}"),
            format!("{icrs_dec_rad:.16}"),
            format!("{:.6}", source.astrometry.epoch.value()),
            format!("{:.16e}", integral.total_ph_m2_s),
            args.photometry_model.clone(),
            "1.0000000000".to_string(),
        ],
        integral,
    })
}

fn validate_integral(integral: PhotonFluxIntegral) -> Result<()> {
    for (name, value) in [
        ("total", integral.total_ph_m2_s),
        ("positive contribution", integral.positive_ph_m2_s),
        ("negative contribution", integral.negative_ph_m2_s),
    ] {
        if !value.is_finite() {
            bail!("Gaia XP integrated photon flux {name} is not finite");
        }
    }
    if integral.positive_ph_m2_s < 0.0 || integral.negative_ph_m2_s > 0.0 {
        bail!("Gaia XP signed integrated contributions are structurally invalid");
    }
    if integral
        .uncertainty_ph_m2_s
        .is_some_and(|uncertainty| !uncertainty.is_finite() || uncertainty < 0.0)
    {
        bail!("Gaia XP integrated photon-flux uncertainty is not finite and non-negative");
    }
    Ok(())
}

fn normalized_xp_is_absent(row: &StringRecord, columns: NormalizedColumns) -> bool {
    [
        columns.xp_wavelength_nm,
        columns.xp_flux_w_m2_nm,
        columns.xp_flux_error_w_m2_nm,
    ]
    .into_iter()
    .all(|index| row.get(index).is_none_or(|value| value.trim().is_empty()))
}

impl NormalizedColumns {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            source_id: required_header(headers, "source_id")?,
            ra: required_header(headers, "ra")?,
            dec: required_header(headers, "dec")?,
            ref_epoch: required_header(headers, "ref_epoch")?,
            pmra: optional_header(headers, "pmra"),
            pmdec: optional_header(headers, "pmdec"),
            parallax: optional_header(headers, "parallax"),
            radial_velocity: optional_header(headers, "radial_velocity"),
            phot_g_mean_mag: optional_header(headers, "phot_g_mean_mag"),
            phot_bp_mean_mag: optional_header(headers, "phot_bp_mean_mag"),
            phot_rp_mean_mag: optional_header(headers, "phot_rp_mean_mag"),
            xp_wavelength_nm: required_header(headers, NORMALIZED_WAVELENGTH_COLUMN)?,
            xp_flux_w_m2_nm: required_header(headers, NORMALIZED_FLUX_COLUMN)?,
            xp_flux_error_w_m2_nm: required_header(headers, NORMALIZED_FLUX_ERROR_COLUMN)?,
            quality_ok: optional_header(headers, "quality_ok"),
            duplicated_source: optional_header(headers, "duplicated_source"),
        })
    }
}

impl BulkColumns {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        Ok(Self {
            source_id: required_header(headers, "source_id")?,
            ra: required_header(headers, "ra")?,
            dec: required_header(headers, "dec")?,
            wavelength: required_header(headers, "wavelength")?,
            flux: required_header(headers, "flux")?,
            flux_error: required_header(headers, "flux_error")?,
        })
    }
}

impl BulkSourceBuilder {
    fn new(source_id: u64) -> Self {
        Self {
            source_id,
            ra_deg: None,
            dec_deg: None,
            wavelengths_nm: Vec::with_capacity(343),
            flux_w_m2_nm: Vec::with_capacity(343),
            flux_error_w_m2_nm: Vec::with_capacity(343),
            structural_error: None,
        }
    }

    fn push(&mut self, row: &StringRecord, columns: BulkColumns) {
        if let Err(error) = self.try_push(row, columns) {
            if self.structural_error.is_none() {
                self.structural_error = Some(error.to_string());
            }
        }
    }

    fn try_push(&mut self, row: &StringRecord, columns: BulkColumns) -> Result<()> {
        let ra_deg = parse_required_f64(row, columns.ra, "ra")?;
        let dec_deg = parse_required_f64(row, columns.dec, "dec")?;
        if self.ra_deg.is_some_and(|value| value != ra_deg)
            || self.dec_deg.is_some_and(|value| value != dec_deg)
        {
            bail!("inconsistent astrometry within Gaia XP bulk source");
        }
        self.ra_deg.get_or_insert(ra_deg);
        self.dec_deg.get_or_insert(dec_deg);
        self.wavelengths_nm
            .push(parse_required_f64(row, columns.wavelength, "wavelength")?);
        self.flux_w_m2_nm
            .push(parse_required_f64(row, columns.flux, "flux")?);
        self.flux_error_w_m2_nm
            .push(parse_required_f64(row, columns.flux_error, "flux_error")?);
        Ok(())
    }

    fn into_conversion(self, args: &Args) -> Result<Conversion> {
        if let Some(error) = self.structural_error {
            bail!("malformed Gaia XP bulk source: {error}");
        }
        let raw = GaiaDr3RawSourceRow {
            source_id: self.source_id,
            ra_deg: self
                .ra_deg
                .context("empty Gaia XP bulk source astrometry")?,
            dec_deg: self
                .dec_deg
                .context("empty Gaia XP bulk source astrometry")?,
            ref_epoch_jyr: GAIA_DR3_REFERENCE_EPOCH_JYR,
            pmra_mas_per_yr: None,
            pmdec_mas_per_yr: None,
            parallax_mas: None,
            radial_velocity_km_s: None,
            phot_g_mean_mag: None,
            phot_bp_mean_mag: None,
            phot_rp_mean_mag: None,
            quality: GaiaDr3QualityFlags {
                quality_ok: true,
                duplicated_source: None,
            },
        };
        let product = XpProduct {
            source_id: self.source_id.to_string(),
            wavelengths_nm: self.wavelengths_nm,
            flux_w_m2_nm: self.flux_w_m2_nm,
            flux_error_w_m2_nm: Some(self.flux_error_w_m2_nm),
        };
        convert_source(raw, product, args)
    }
}

impl Counters {
    fn handle_conversion(
        &mut self,
        conversion: Conversion,
        writer: &mut Writer<Box<dyn Write>>,
    ) -> Result<()> {
        match conversion {
            Conversion::Accepted { output, integral } => {
                self.science.observe(integral);
                writer.write_record(output)?;
                self.rows_used += 1;
            }
            Conversion::ScientificallyExcluded { reason, integral } => {
                self.science.observe(integral);
                self.scientifically_excluded += 1;
                *self
                    .scientific_exclusion_reasons
                    .entry(reason.to_string())
                    .or_default() += 1;
                *self
                    .rejection_reasons
                    .entry(reason.to_string())
                    .or_default() += 1;
            }
        }
        Ok(())
    }

    fn reject_unexpected(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.unexpectedly_rejected += 1;
        *self
            .unexpected_rejection_reasons
            .entry(reason.clone())
            .or_default() += 1;
        *self.rejection_reasons.entry(reason).or_default() += 1;
    }
}

impl ScienceStats {
    fn observe(&mut self, integral: PhotonFluxIntegral) {
        self.integrated_spectra += 1;
        self.band_samples += integral.band_samples;
        self.negative_samples += integral.negative_samples;
        self.sources_with_negative_flux_samples += usize::from(integral.negative_samples != 0);
        self.positive_ph_m2_s += integral.positive_ph_m2_s;
        self.negative_ph_m2_s += integral.negative_ph_m2_s;
        if let Some(uncertainty) = integral.uncertainty_ph_m2_s {
            self.uncertainty_sources += 1;
            self.uncertainty_sum_ph_m2_s += uncertainty;
            self.uncertainty_squared_sum += uncertainty * uncertainty;
            self.uncertainty_min_ph_m2_s = Some(
                self.uncertainty_min_ph_m2_s
                    .map_or(uncertainty, |current| current.min(uncertainty)),
            );
            self.uncertainty_max_ph_m2_s = Some(
                self.uncertainty_max_ph_m2_s
                    .map_or(uncertainty, |current| current.max(uncertainty)),
            );
        } else {
            self.uncertainty_missing_sources += 1;
        }
    }

    fn negative_sample_fraction(&self) -> f64 {
        if self.band_samples == 0 {
            0.0
        } else {
            self.negative_samples as f64 / self.band_samples as f64
        }
    }

    fn negative_contribution_ratio(&self) -> Option<f64> {
        (self.positive_ph_m2_s > 0.0).then_some(-self.negative_ph_m2_s / self.positive_ph_m2_s)
    }

    fn uncertainty_mean(&self) -> Option<f64> {
        (self.uncertainty_sources != 0)
            .then_some(self.uncertainty_sum_ph_m2_s / self.uncertainty_sources as f64)
    }

    fn uncertainty_quadrature_sum(&self) -> Option<f64> {
        (self.uncertainty_sources != 0).then_some(self.uncertainty_squared_sum.sqrt())
    }
}

#[allow(clippy::too_many_arguments)]
fn build_diagnostics<'a>(
    args: &'a Args,
    input: &InputSelection,
    input_mode: &'static str,
    input_checksum: String,
    input_checksum_algorithm: &'static str,
    all_sources_accounted: bool,
    strict_gate_passed: bool,
    counters: &Counters,
) -> Diagnostics<'a> {
    let (bulk_reference_epoch_jyr, astrometry_epoch_provenance) = match input {
        InputSelection::Normalized(_) => (None, "per-source ref_epoch from normalized input"),
        InputSelection::Bulk { .. } => (
            Some(GAIA_DR3_REFERENCE_EPOCH_JYR),
            "Gaia DR3 reference epoch 2016.0 Julian years; official XP sampled bulk files omit ref_epoch",
        ),
    };
    Diagnostics {
        schema_version: 2,
        production_mode: args.production,
        strict_gate_passed,
        all_sources_accounted,
        input_mode,
        xp_product_type: "XP_SAMPLED",
        catalogue_name: &args.catalog_name,
        catalogue_release: &args.catalog_release,
        catalogue_license: &args.catalog_license,
        input_checksum,
        input_checksum_algorithm,
        photometry_model: &args.photometry_model,
        band_min_nm: BAND_MIN_NM,
        band_max_nm: BAND_MAX_NM,
        bulk_reference_epoch_jyr,
        astrometry_epoch_provenance,
        input_records_read: counters.input_records_read,
        rows_read: counters.sources_read,
        unique_source_ids_read: counters.unique_source_ids_read,
        rows_used: counters.rows_used,
        unique_sources_represented: counters.rows_used,
        rows_scientifically_excluded: counters.scientifically_excluded,
        non_positive_integrated_flux_sources: counters
            .scientific_exclusion_reasons
            .get(NON_POSITIVE_INTEGRAL_REASON)
            .copied()
            .unwrap_or(0),
        rows_unexpectedly_rejected: counters.unexpectedly_rejected,
        rows_rejected: counters.scientifically_excluded + counters.unexpectedly_rejected,
        rejection_reasons: counters.rejection_reasons.clone(),
        scientific_exclusion_reasons: counters.scientific_exclusion_reasons.clone(),
        unexpected_rejection_reasons: counters.unexpected_rejection_reasons.clone(),
        integrated_spectra: counters.science.integrated_spectra,
        sources_with_negative_flux_samples: counters.science.sources_with_negative_flux_samples,
        flux_samples_in_band: counters.science.band_samples,
        negative_flux_samples: counters.science.negative_samples,
        negative_flux_sample_fraction: counters.science.negative_sample_fraction(),
        integrated_positive_contribution_ph_m2_s: counters.science.positive_ph_m2_s,
        integrated_negative_contribution_ph_m2_s: counters.science.negative_ph_m2_s,
        integrated_negative_contribution_ratio: counters.science.negative_contribution_ratio(),
        integrated_uncertainty_sources: counters.science.uncertainty_sources,
        integrated_uncertainty_missing_sources: counters.science.uncertainty_missing_sources,
        integrated_uncertainty_min_ph_m2_s: counters.science.uncertainty_min_ph_m2_s,
        integrated_uncertainty_max_ph_m2_s: counters.science.uncertainty_max_ph_m2_s,
        integrated_uncertainty_mean_ph_m2_s: counters.science.uncertainty_mean(),
        integrated_uncertainty_quadrature_sum_ph_m2_s: counters
            .science
            .uncertainty_quadrature_sum(),
        uncertainty_model: UNCERTAINTY_MODEL,
        uncertainty_model_caveat: UNCERTAINTY_MODEL_CAVEAT,
    }
}

fn write_output_header(writer: &mut Writer<Box<dyn Write>>) -> Result<()> {
    writer.write_record([
        "source_id",
        "icrs_ra_rad",
        "icrs_dec_rad",
        "epoch_jyr",
        PHOTON_FLUX_COLUMN,
        "photometry_model",
        "weight",
    ])?;
    Ok(())
}

fn transactional_output_writer(path: &Path) -> Result<(Writer<Box<dyn Write>>, PendingOutput)> {
    let temporary_path = temporary_path(path)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .with_context(|| {
            format!(
                "failed to create temporary output catalogue {}",
                temporary_path.display()
            )
        })?;
    let writer = WriterBuilder::new().from_writer(Box::new(BufWriter::new(file)) as Box<dyn Write>);
    Ok((
        writer,
        PendingOutput {
            temporary_path,
            final_path: path.to_path_buf(),
            committed: false,
        },
    ))
}

impl PendingOutput {
    fn commit(mut self) -> Result<()> {
        std::fs::rename(&self.temporary_path, &self.final_path).with_context(|| {
            format!(
                "failed to commit output catalogue {}",
                self.final_path.display()
            )
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.temporary_path);
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary_path = temporary_path(path)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .with_context(|| {
                format!(
                    "failed to create temporary diagnostics {}",
                    temporary_path.display()
                )
            })?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&temporary_path, path)
            .with_context(|| format!("failed to commit diagnostics {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

fn temporary_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .context("output path must have a filename")?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.tmp.{}", std::process::id())))
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    optional_header(headers, name)
        .ok_or_else(|| anyhow::anyhow!("missing required column {name:?}"))
}

fn optional_header(headers: &StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|header| header.trim() == name)
}

fn parse_u64(row: &StringRecord, index: usize, name: &str) -> Result<u64> {
    row.get(index)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid integer field {name:?}"))
}

fn parse_required_f64(row: &StringRecord, index: usize, name: &str) -> Result<f64> {
    row.get(index)
        .ok_or_else(|| anyhow::anyhow!("missing field {name:?}"))?
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn parse_optional_f64(row: &StringRecord, index: Option<usize>, name: &str) -> Result<Option<f64>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let Some(raw) = row
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    raw.parse::<f64>()
        .map(Some)
        .with_context(|| format!("invalid numeric field {name:?}"))
}

fn parse_optional_bool(row: &StringRecord, index: Option<usize>) -> Option<bool> {
    index
        .and_then(|index| row.get(index))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "t" | "yes" | "ok"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    const HEADER: &str =
        "source_id,ra,dec,ref_epoch,xp_wavelength_nm,xp_flux_w_m2_nm,xp_flux_error_w_m2_nm\n";

    fn args(input: PathBuf, output: PathBuf, diagnostics: PathBuf) -> Args {
        Args {
            input: Some(input),
            bulk_dir: None,
            output,
            diagnostics_output: Some(diagnostics),
            catalog_name: "Gaia".to_string(),
            catalog_release: "DR3".to_string(),
            catalog_license: "CC-BY-4.0-derived-policy-reviewed".to_string(),
            source_checksum: None,
            photometry_model: PHOTOMETRY_MODEL.to_string(),
            band_min_nm: BAND_MIN_NM,
            band_max_nm: BAND_MAX_NM,
            production: false,
        }
    }

    fn write_normalized(path: &Path, rows: &str) -> Result<()> {
        std::fs::write(path, format!("{HEADER}{rows}"))?;
        Ok(())
    }

    fn report(path: &Path) -> Result<serde_json::Value> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }

    #[test]
    fn tiny_gaia_fixture_prepares_canonical_sources() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        write_normalized(
            &input,
            "42,120.0,-30.0,2016.0,336;500;650,1e-12;1e-12;1e-12,1e-14;1e-14;1e-14\n",
        )?;

        run(args(input, output.clone(), diagnostics.clone()))?;

        let canonical = std::fs::read_to_string(output)?;
        assert!(canonical.starts_with(&format!(
            "source_id,icrs_ra_rad,icrs_dec_rad,epoch_jyr,{PHOTON_FLUX_COLUMN},photometry_model,weight"
        )));
        assert!(canonical.contains(PHOTOMETRY_MODEL));
        let diagnostics = report(&diagnostics)?;
        assert_eq!(diagnostics["rows_read"], 1);
        assert_eq!(diagnostics["rows_used"], 1);
        assert_eq!(diagnostics["unique_sources_represented"], 1);
        assert_eq!(diagnostics["photometry_model"], PHOTOMETRY_MODEL);
        assert_eq!(diagnostics["band_min_nm"], BAND_MIN_NM);
        assert_eq!(diagnostics["band_max_nm"], BAND_MAX_NM);
        assert_eq!(diagnostics["integrated_uncertainty_sources"], 1);
        assert!(diagnostics["uncertainty_model_caveat"]
            .as_str()
            .is_some_and(|value| value.contains("covariance")));
        Ok(())
    }

    #[test]
    fn analytic_constant_energy_flux_matches_closed_form() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let flux = 2.5e-12;
        write_normalized(
            &input,
            &format!("42,0,0,2016,336;650,{flux};{flux},1e-14;1e-14\n"),
        )?;
        run(args(input, output.clone(), diagnostics))?;

        let mut reader = csv::Reader::from_path(output)?;
        let row = reader.records().next().context("missing output row")??;
        let actual: f64 = row[4].parse()?;
        let expected = flux * 0.5 * (BAND_MAX_NM.powi(2) - BAND_MIN_NM.powi(2)) * 1.0e-9
            / (6.626_070_15e-34 * 299_792_458.0);
        let relative_error = (actual - expected).abs() / expected;
        assert!(relative_error < 1.0e-14, "relative error {relative_error}");
        Ok(())
    }

    #[test]
    fn finite_negative_sample_is_retained_when_total_is_positive() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        write_normalized(
            &input,
            "42,10,0,2016,336;400;650,-1e-14;1e-12;1e-12,1e-14;1e-14;1e-14\n",
        )?;
        run(args(input, output.clone(), diagnostics.clone()))?;

        let output = std::fs::read_to_string(output)?;
        assert_eq!(output.lines().count(), 2);
        let diagnostics = report(&diagnostics)?;
        assert_eq!(diagnostics["negative_flux_samples"], 1);
        assert_eq!(diagnostics["sources_with_negative_flux_samples"], 1);
        assert!(
            (diagnostics["negative_flux_sample_fraction"]
                .as_f64()
                .context("missing negative fraction")?
                - 1.0 / 3.0)
                .abs()
                < 1.0e-15
        );
        assert!(
            diagnostics["integrated_positive_contribution_ph_m2_s"]
                .as_f64()
                .context("missing positive contribution")?
                > 0.0
        );
        assert!(
            diagnostics["integrated_negative_contribution_ph_m2_s"]
                .as_f64()
                .context("missing negative contribution")?
                < 0.0
        );
        assert!(
            diagnostics["integrated_negative_contribution_ratio"]
                .as_f64()
                .context("missing negative ratio")?
                > 0.0
        );
        Ok(())
    }

    #[test]
    fn malformed_and_nonfinite_spectra_fail_strict_gate_without_partial_output() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        write_normalized(
            &input,
            concat!(
                "1,0,0,2016,336;650,1e-12;1e-12,1e-14;1e-14\n",
                "2,0,0,2016,336;500;650,1e-12;1e-12,1e-14;1e-14;1e-14\n",
                "3,0,0,2016,336;500;400;650,1e-12;1e-12;1e-12;1e-12,1e-14;1e-14;1e-14;1e-14\n",
                "4,0,0,2016,336;650,NaN;1e-12,1e-14;1e-14\n",
                "5,0,0,2016,336;650,1e-12;1e-12,inf;1e-14\n",
                "6,0,0,2016,338;650,1e-12;1e-12,1e-14;1e-14\n",
                "7,0,0,2016,336;650,1e-12;1e-12,1e308;1e308\n",
                "8,360,0,2016,336;650,1e-12;1e-12,1e-14;1e-14\n",
            ),
        )?;

        let error = run(args(input, output.clone(), diagnostics.clone()))
            .expect_err("unexpected rejections must fail the strict gate");
        assert!(error.to_string().contains("unexpected source rejections"));
        assert!(
            !output.exists(),
            "strict failure published a partial output"
        );
        let diagnostics = report(&diagnostics)?;
        assert_eq!(diagnostics["rows_used"], 1);
        assert_eq!(diagnostics["rows_unexpectedly_rejected"], 7);
        assert_eq!(diagnostics["strict_gate_passed"], false);
        Ok(())
    }

    #[test]
    fn non_positive_integrals_are_explicit_scientific_exclusions() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        write_normalized(
            &input,
            concat!(
                "1,0,0,2016,336;500;650,1e-12;1e-12;1e-12,1e-14;1e-14;1e-14\n",
                "2,0,0,2016,336;500;650,0;0;0,1e-14;1e-14;1e-14\n",
                "3,0,0,2016,336;500;650,-1e-12;-1e-12;-1e-12,1e-14;1e-14;1e-14\n",
            ),
        )?;
        run(args(input, output.clone(), diagnostics.clone()))?;

        assert_eq!(std::fs::read_to_string(output)?.lines().count(), 2);
        let diagnostics = report(&diagnostics)?;
        assert_eq!(diagnostics["rows_used"], 1);
        assert_eq!(diagnostics["rows_scientifically_excluded"], 2);
        assert_eq!(diagnostics["non_positive_integrated_flux_sources"], 2);
        assert_eq!(
            diagnostics["scientific_exclusion_reasons"][NON_POSITIVE_INTEGRAL_REASON],
            2
        );
        assert_eq!(diagnostics["rows_unexpectedly_rejected"], 0);
        assert_eq!(diagnostics["strict_gate_passed"], true);
        assert_eq!(diagnostics["negative_flux_samples"], 3);
        Ok(())
    }

    #[test]
    fn official_bulk_gzip_is_streamed_source_by_source() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let bulk = dir.path().join("bulk");
        std::fs::create_dir(&bulk)?;
        write_gzip(
            &bulk.join("part-000.csv.gz"),
            concat!(
                "source_id,ra,dec,wavelength,flux,flux_error\n",
                "1,10,-20,336,1e-12,1e-14\n",
                "1,10,-20,650,1e-12,1e-14\n",
                "2,20,30,336,2e-12,2e-14\n",
                "2,20,30,650,2e-12,2e-14\n",
            ),
        )?;
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let mut args = args(
            dir.path().join("unused"),
            output.clone(),
            diagnostics.clone(),
        );
        args.input = None;
        args.bulk_dir = Some(bulk);
        run(args)?;

        assert_eq!(std::fs::read_to_string(output)?.lines().count(), 3);
        let diagnostics = report(&diagnostics)?;
        assert_eq!(diagnostics["input_mode"], "official_bulk_xp_sampled");
        assert_eq!(diagnostics["input_records_read"], 4);
        assert_eq!(diagnostics["rows_read"], 2);
        assert_eq!(diagnostics["unique_sources_represented"], 2);
        assert_eq!(
            diagnostics["bulk_reference_epoch_jyr"],
            GAIA_DR3_REFERENCE_EPOCH_JYR
        );
        Ok(())
    }

    #[test]
    fn duplicate_bulk_source_fails_without_publishing_output() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let bulk = dir.path().join("bulk");
        std::fs::create_dir(&bulk)?;
        let body = concat!(
            "source_id,ra,dec,wavelength,flux,flux_error\n",
            "1,10,-20,336,1e-12,1e-14\n",
            "1,10,-20,650,1e-12,1e-14\n",
        );
        write_gzip(&bulk.join("part-000.csv.gz"), body)?;
        write_gzip(&bulk.join("part-001.csv.gz"), body)?;
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let mut args = args(
            dir.path().join("unused"),
            output.clone(),
            diagnostics.clone(),
        );
        args.input = None;
        args.bulk_dir = Some(bulk);

        let error = run(args).expect_err("duplicate source must fail strict gate");
        assert!(error.to_string().contains("unexpected source rejections"));
        assert!(!output.exists());
        let diagnostics = report(&diagnostics)?;
        assert_eq!(
            diagnostics["unexpected_rejection_reasons"]["duplicate source_id"],
            1
        );
        Ok(())
    }

    #[test]
    fn partial_bulk_file_fails_before_processing() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let bulk = dir.path().join("bulk");
        std::fs::create_dir(&bulk)?;
        write_gzip(
            &bulk.join("part-000.csv.gz"),
            concat!(
                "source_id,ra,dec,wavelength,flux,flux_error\n",
                "1,10,-20,336,1e-12,1e-14\n",
                "1,10,-20,650,1e-12,1e-14\n",
            ),
        )?;
        std::fs::write(bulk.join("part-001.csv.gz.part"), b"incomplete")?;
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let mut args = args(dir.path().join("unused"), output.clone(), diagnostics);
        args.input = None;
        args.bulk_dir = Some(bulk);

        let error = run(args).expect_err("partial bulk input must fail closed");
        assert!(error.to_string().contains("partial Gaia XP bulk file"));
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn checksum_mismatch_is_rejected() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        write_normalized(&input, "42,10,0,2016,336;650,1e-12;1e-12,1e-14;1e-14\n")?;
        let mut args = args(input, output.clone(), diagnostics);
        args.source_checksum = Some(format!("sha256:{}", "0".repeat(64)));

        let error = run(args).expect_err("checksum mismatch");
        assert!(error.to_string().contains("source checksum mismatch"));
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn production_requires_diagnostics() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("gaia.csv");
        let output = dir.path().join("canonical.csv");
        write_normalized(&input, "42,10,0,2016,336;650,1e-12;1e-12,1e-14;1e-14\n")?;
        let mut args = args(input, output, dir.path().join("diagnostics.json"));
        args.production = true;
        args.diagnostics_output = None;
        let error = run(args).expect_err("production diagnostics are mandatory");
        assert!(error.to_string().contains("requires --diagnostics-output"));
        Ok(())
    }

    fn write_gzip(path: &Path, contents: &str) -> Result<()> {
        let file = File::create(path)?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(contents.as_bytes())?;
        encoder.finish()?;
        Ok(())
    }
}
