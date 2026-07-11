use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord, Trim};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

const MEAN_FILE: &str = "starlight_mean.release.csv";
const UNCERTAINTY_FILE: &str = "starlight_uncertainty.release.csv";
const COMPLETENESS_FILE: &str = "starlight_completeness.release.csv";
const DIAGNOSTICS_FILE: &str = "starlight_source_contributions.diagnostics.json";
const MANIFEST_FILE: &str = "starlight.production.manifest.toml";
const OUTPUT_FILES: [&str; 5] = [
    MEAN_FILE,
    UNCERTAINTY_FILE,
    COMPLETENESS_FILE,
    DIAGNOSTICS_FILE,
    MANIFEST_FILE,
];

const PRODUCT_SCHEMA_VERSION: u32 = 2;
const INPUT_MANIFEST_SCHEMA_VERSION: u32 = 1;
const BAND_MIN_NM: u16 = 300;
const BAND_MAX_NM: u16 = 650;
const INPUT_UNIT: &str = "ph m^-2 s^-1 per represented member";
const OUTPUT_UNIT: &str = "ph cm^-2 ns^-1 sr^-1";
const FLUX_UNIT_CONVERSION: f64 = 1.0e-13;
const FLUX_CONSERVATION_RELATIVE_TOLERANCE: f64 = 1.0e-12;
const MAX_EXACT_F64_INTEGER: u64 = 1_u64 << 53;

/// Aggregate checksum-pinned, normalized source/bin contributions into the
/// fail-closed integrated Starlight release candidate and its scientific
/// sidecars.
#[derive(Debug, Parser)]
#[command(name = "build_integrated_starlight_product")]
#[command(about = "Build a deterministic full-sky 300-650 nm Starlight product candidate")]
struct Args {
    /// TOML or JSON manifest containing checksum-pinned normalized input CSVs.
    #[arg(long)]
    inputs_manifest: PathBuf,

    /// HEALPix nside. It must be a non-zero power of two.
    #[arg(long)]
    nside: u32,

    /// Stable release identifier recorded verbatim in every artifact.
    #[arg(long)]
    release_id: String,

    /// SHA-256 of the calibrated inference/completeness model.
    #[arg(long)]
    model_checksum: String,

    /// Directory that receives exactly the five named release artifacts.
    #[arg(long)]
    output_dir: PathBuf,

    /// Required acknowledgement that this builder only emits a candidate.
    /// Promotion is exclusively the responsibility of the approval-aware packer.
    #[arg(long)]
    candidate_only: bool,
}

#[derive(Debug, Deserialize)]
struct InputManifest {
    schema_version: u32,
    #[serde(default)]
    release_id: Option<String>,
    #[serde(default, alias = "model_sha256")]
    model_checksum: Option<String>,
    #[serde(alias = "files")]
    inputs: Vec<InputSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct InputSpec {
    path: String,
    #[serde(alias = "checksum")]
    sha256: String,
    #[serde(default)]
    branch: Option<String>,
}

#[derive(Debug)]
struct PreparedInput {
    display_path: String,
    resolved_path: PathBuf,
    sha256: String,
    branch: Option<String>,
}

#[derive(Debug)]
struct ContributionRow {
    source_or_bin_id: String,
    healpix_index: usize,
    multiplicity: u64,
    measured: f64,
    inferred: f64,
    completeness: f64,
    statistical_uncertainty: f64,
    systematic_uncertainty: f64,
    extrapolation: bool,
    crowding: bool,
    branch: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct StableSum {
    sum: f64,
    compensation: f64,
}

impl StableSum {
    fn add(&mut self, value: f64, quantity: &str) -> Result<()> {
        let adjusted = value - self.compensation;
        let next = self.sum + adjusted;
        self.compensation = (next - self.sum) - adjusted;
        self.sum = next;
        if !self.sum.is_finite() || !self.compensation.is_finite() {
            bail!("numeric overflow while accumulating {quantity}");
        }
        Ok(())
    }

    fn value(self) -> f64 {
        if self.sum == 0.0 {
            0.0
        } else {
            self.sum
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Accumulator {
    measured: StableSum,
    inferred: StableSum,
    completeness: StableSum,
    statistical_variance: StableSum,
    systematic_correlated: StableSum,
    contribution_rows: u64,
    represented_multiplicity: u64,
    extrapolation: bool,
    crowding: bool,
    extrapolation_rows: u64,
    crowding_rows: u64,
    extrapolation_multiplicity: u64,
    crowding_multiplicity: u64,
}

impl Accumulator {
    fn add(&mut self, row: &ContributionRow) -> Result<()> {
        let multiplicity = row.multiplicity as f64;
        let measured = checked_product(multiplicity, row.measured, "measured contribution")?;
        let inferred = checked_product(multiplicity, row.inferred, "inferred contribution")?;
        let completeness = checked_product(
            multiplicity,
            row.completeness,
            "completeness-correction contribution",
        )?;
        let statistical_variance = checked_product(
            multiplicity,
            checked_product(
                row.statistical_uncertainty,
                row.statistical_uncertainty,
                "statistical uncertainty variance",
            )?,
            "multiplicity-weighted statistical uncertainty variance",
        )?;
        // The systematic term is deliberately treated as fully correlated
        // within a pixel (and in the all-sky accounting), so standard
        // deviations add linearly rather than in quadrature.
        let systematic_correlated = checked_product(
            multiplicity,
            row.systematic_uncertainty,
            "correlated systematic uncertainty",
        )?;

        self.measured.add(measured, "measured flux")?;
        self.inferred.add(inferred, "inferred flux")?;
        self.completeness
            .add(completeness, "completeness-correction flux")?;
        self.statistical_variance
            .add(statistical_variance, "statistical variance")?;
        self.systematic_correlated
            .add(systematic_correlated, "correlated systematic uncertainty")?;
        self.contribution_rows = self
            .contribution_rows
            .checked_add(1)
            .context("contribution-row count overflow")?;
        self.represented_multiplicity = self
            .represented_multiplicity
            .checked_add(row.multiplicity)
            .context("represented multiplicity overflow")?;
        if row.extrapolation {
            self.extrapolation = true;
            self.extrapolation_rows = self
                .extrapolation_rows
                .checked_add(1)
                .context("extrapolation-row count overflow")?;
            self.extrapolation_multiplicity = self
                .extrapolation_multiplicity
                .checked_add(row.multiplicity)
                .context("extrapolation multiplicity overflow")?;
        }
        if row.crowding {
            self.crowding = true;
            self.crowding_rows = self
                .crowding_rows
                .checked_add(1)
                .context("crowding-row count overflow")?;
            self.crowding_multiplicity = self
                .crowding_multiplicity
                .checked_add(row.multiplicity)
                .context("crowding multiplicity overflow")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct PixelValues {
    measured_radiance: f64,
    inferred_radiance: f64,
    completeness_radiance: f64,
    mean_radiance: f64,
    statistical_radiance: f64,
    systematic_radiance: f64,
    total_uncertainty_radiance: f64,
    inferred_fraction: f64,
}

impl PixelValues {
    fn from_accumulator(accumulator: &Accumulator, flux_to_radiance: f64) -> Result<Self> {
        let measured_flux = accumulator.measured.value();
        let inferred_flux = accumulator.inferred.value();
        let completeness_flux = accumulator.completeness.value();
        let total_flux = checked_sum3(
            measured_flux,
            inferred_flux,
            completeness_flux,
            "total flux",
        )?;
        let statistical_flux = accumulator.statistical_variance.value().sqrt();
        let systematic_flux = accumulator.systematic_correlated.value();
        let measured_radiance =
            checked_product(measured_flux, flux_to_radiance, "measured photon radiance")?;
        let inferred_radiance =
            checked_product(inferred_flux, flux_to_radiance, "inferred photon radiance")?;
        let completeness_radiance = checked_product(
            completeness_flux,
            flux_to_radiance,
            "completeness photon radiance",
        )?;
        let mean_radiance = checked_product(total_flux, flux_to_radiance, "mean photon radiance")?;
        let statistical_radiance = checked_product(
            statistical_flux,
            flux_to_radiance,
            "statistical photon-radiance uncertainty",
        )?;
        let systematic_radiance = checked_product(
            systematic_flux,
            flux_to_radiance,
            "systematic photon-radiance uncertainty",
        )?;
        let total_uncertainty_radiance = statistical_radiance.hypot(systematic_radiance);
        if !total_uncertainty_radiance.is_finite() {
            bail!("numeric overflow while combining total uncertainty");
        }
        let inferred_fraction = if total_flux > 0.0 {
            (inferred_flux + completeness_flux) / total_flux
        } else {
            0.0
        };
        if !inferred_fraction.is_finite() || !(0.0..=1.0).contains(&inferred_fraction) {
            bail!("invalid inferred fraction produced during aggregation");
        }
        Ok(Self {
            measured_radiance,
            inferred_radiance,
            completeness_radiance,
            mean_radiance,
            statistical_radiance,
            systematic_radiance,
            total_uncertainty_radiance,
            inferred_fraction,
        })
    }
}

#[derive(Debug, Serialize)]
struct InputDiagnostics {
    path: String,
    sha256: String,
    branch_constraint: Option<String>,
    contribution_rows: u64,
}

#[derive(Debug, Serialize)]
struct BranchDiagnostics {
    contribution_rows: u64,
    represented_multiplicity: u64,
    measured_300_650_ph_m2_s: f64,
    inferred_300_650_ph_m2_s: f64,
    completeness_correction_300_650_ph_m2_s: f64,
    total_300_650_ph_m2_s: f64,
    statistical_uncertainty_300_650_ph_m2_s: f64,
    systematic_uncertainty_300_650_ph_m2_s: f64,
    extrapolation_rows: u64,
    crowding_rows: u64,
}

#[derive(Debug, Serialize)]
struct FluxAccounting {
    input_measured_300_650_ph_m2_s: f64,
    input_inferred_300_650_ph_m2_s: f64,
    input_completeness_correction_300_650_ph_m2_s: f64,
    input_total_300_650_ph_m2_s: f64,
    output_solid_angle_integrated_300_650_ph_m2_s: f64,
    absolute_error_ph_m2_s: f64,
    relative_error: f64,
    relative_tolerance: f64,
    conservation_pass: bool,
}

#[derive(Debug, Serialize)]
struct UncertaintyAccounting {
    statistical_model: &'static str,
    systematic_model: &'static str,
    input_statistical_uncertainty_300_650_ph_m2_s: f64,
    output_statistical_uncertainty_300_650_ph_m2_s: f64,
    input_systematic_uncertainty_300_650_ph_m2_s: f64,
    output_systematic_uncertainty_300_650_ph_m2_s: f64,
    accounting_pass: bool,
}

#[derive(Debug, Serialize)]
struct CoverageDiagnostics {
    full_healpix_coverage: bool,
    ordering: &'static str,
    expected_pixels: usize,
    emitted_pixels: usize,
    empty_pixels: usize,
    first_pixel_emitted: bool,
    last_pixel_emitted: bool,
    seam_endpoint_pixels_distinct: bool,
}

#[derive(Debug, Serialize)]
struct FlagDiagnostics {
    extrapolation_contribution_rows: u64,
    extrapolation_represented_multiplicity: u64,
    extrapolation_pixels: usize,
    crowding_contribution_rows: u64,
    crowding_represented_multiplicity: u64,
    crowding_pixels: usize,
}

#[derive(Debug, Serialize)]
struct Diagnostics {
    schema_version: u32,
    product: &'static str,
    release_id: String,
    calibration_status: &'static str,
    production_ready: bool,
    candidate_only: bool,
    band_nm: [u16; 2],
    nside: u32,
    model_sha256: String,
    input_manifest_sha256: String,
    input_quantity: &'static str,
    input_unit: &'static str,
    output_quantity: &'static str,
    output_unit: &'static str,
    ph_m2_s_to_ph_cm2_ns: f64,
    pixel_area_sr: f64,
    inputs: Vec<InputDiagnostics>,
    unique_contribution_rows: u64,
    represented_multiplicity: u64,
    inferred_fraction: f64,
    flux_accounting: FluxAccounting,
    uncertainty_accounting: UncertaintyAccounting,
    coverage: CoverageDiagnostics,
    flags: FlagDiagnostics,
    branches: BTreeMap<String, BranchDiagnostics>,
    artifact_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct ManifestInput {
    path: String,
    sha256: String,
    branch_constraint: Option<String>,
    contribution_rows: u64,
}

#[derive(Debug, Serialize)]
struct UnitsManifest {
    input_quantity: &'static str,
    input_unit: &'static str,
    output_quantity: &'static str,
    output_unit: &'static str,
    ph_m2_s_to_ph_cm2_ns: f64,
    pixel_area_sr: f64,
    flux_to_radiance_factor: f64,
}

#[derive(Debug, Serialize)]
struct UncertaintyManifest {
    statistical: &'static str,
    systematic: &'static str,
    total: &'static str,
}

#[derive(Debug, Serialize)]
struct ProductManifest {
    schema_version: u32,
    product: &'static str,
    release_id: String,
    calibration_status: &'static str,
    production_ready: bool,
    candidate_only: bool,
    band_min_nm: u16,
    band_max_nm: u16,
    nside: u32,
    ordering: &'static str,
    expected_pixels: usize,
    model_sha256: String,
    input_manifest_sha256: String,
    flux_conservation_validated: bool,
    uncertainty_accounting_validated: bool,
    units: UnitsManifest,
    uncertainty_model: UncertaintyManifest,
    inputs: Vec<ManifestInput>,
    artifacts: BTreeMap<String, String>,
}

struct AggregatedProduct {
    pixels: Vec<Accumulator>,
    total: Accumulator,
    branches: BTreeMap<String, Accumulator>,
    inputs: Vec<InputDiagnostics>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    let model_sha256 = normalize_sha256(&args.model_checksum, "--model-checksum")?;
    let (manifest, input_manifest_sha256) = read_input_manifest(&args.inputs_manifest)?;
    validate_manifest_bindings(&manifest, &args, &model_sha256)?;
    let expected_pixels = expected_pixel_count(args.nside)?;
    let pixel_area_sr = 4.0 * std::f64::consts::PI / expected_pixels as f64;
    let flux_to_radiance = FLUX_UNIT_CONVERSION / pixel_area_sr;
    if !pixel_area_sr.is_finite() || pixel_area_sr <= 0.0 || !flux_to_radiance.is_finite() {
        bail!("nside produces invalid HEALPix unit conversion");
    }

    let prepared_inputs = prepare_inputs(&manifest, &args.inputs_manifest)?;
    let aggregated = aggregate_inputs(prepared_inputs, expected_pixels)?;
    let pixel_values = aggregated
        .pixels
        .iter()
        .map(|pixel| PixelValues::from_accumulator(pixel, flux_to_radiance))
        .collect::<Result<Vec<_>>>()?;

    let accounting = build_accounting(&aggregated, &pixel_values, pixel_area_sr)?;
    if !accounting.0.conservation_pass {
        bail!(
            "integrated flux conservation failed with relative error {} (tolerance {})",
            accounting.0.relative_error,
            accounting.0.relative_tolerance
        );
    }
    if !accounting.1.accounting_pass {
        bail!("uncertainty accounting failed");
    }

    fs::create_dir_all(&args.output_dir).with_context(|| {
        format!(
            "failed to create output directory {}",
            args.output_dir.display()
        )
    })?;
    if !args.output_dir.is_dir() {
        bail!(
            "output path {} is not a directory",
            args.output_dir.display()
        );
    }
    let staging = StagingDirectory::create(&args.output_dir)?;

    let mean_sha256 = write_hashed_file(&staging.path().join(MEAN_FILE), |writer| {
        write_mean_csv(
            writer,
            &args,
            expected_pixels,
            pixel_area_sr,
            flux_to_radiance,
            &model_sha256,
            &input_manifest_sha256,
            &aggregated,
            &pixel_values,
        )
    })?;
    let uncertainty_sha256 = write_hashed_file(&staging.path().join(UNCERTAINTY_FILE), |writer| {
        write_uncertainty_csv(
            writer,
            &args,
            expected_pixels,
            pixel_area_sr,
            flux_to_radiance,
            &model_sha256,
            &input_manifest_sha256,
            &pixel_values,
        )
    })?;
    let completeness_sha256 =
        write_hashed_file(&staging.path().join(COMPLETENESS_FILE), |writer| {
            write_completeness_csv(
                writer,
                &args,
                expected_pixels,
                pixel_area_sr,
                flux_to_radiance,
                &model_sha256,
                &input_manifest_sha256,
                &aggregated,
                &pixel_values,
            )
        })?;

    let mut csv_artifacts = BTreeMap::new();
    csv_artifacts.insert(MEAN_FILE.to_string(), mean_sha256);
    csv_artifacts.insert(UNCERTAINTY_FILE.to_string(), uncertainty_sha256);
    csv_artifacts.insert(COMPLETENESS_FILE.to_string(), completeness_sha256);
    let diagnostics = build_diagnostics(
        &args,
        expected_pixels,
        pixel_area_sr,
        &model_sha256,
        &input_manifest_sha256,
        &aggregated,
        accounting.0,
        accounting.1,
        csv_artifacts.clone(),
    )?;
    let mut diagnostics_bytes = serde_json::to_vec_pretty(&diagnostics)?;
    diagnostics_bytes.push(b'\n');
    let diagnostics_sha256 =
        write_hashed_bytes(&staging.path().join(DIAGNOSTICS_FILE), &diagnostics_bytes)?;

    let mut artifacts = csv_artifacts;
    artifacts.insert(DIAGNOSTICS_FILE.to_string(), diagnostics_sha256);
    let product_manifest = build_product_manifest(
        &args,
        expected_pixels,
        pixel_area_sr,
        flux_to_radiance,
        model_sha256,
        input_manifest_sha256,
        &aggregated,
        &diagnostics,
        artifacts,
    );
    let mut manifest_bytes = toml::to_string_pretty(&product_manifest)?.into_bytes();
    if !manifest_bytes.ends_with(b"\n") {
        manifest_bytes.push(b'\n');
    }
    write_hashed_bytes(&staging.path().join(MANIFEST_FILE), &manifest_bytes)?;

    staging.commit(&args.output_dir)?;
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    if !args.candidate_only {
        bail!(
            "--candidate-only is required: this builder is fail-closed and cannot emit production-ready artifacts"
        );
    }
    if args.nside == 0 || !args.nside.is_power_of_two() {
        bail!("--nside must be a non-zero power of two");
    }
    validate_metadata_value(&args.release_id, "--release-id")?;
    if args.output_dir.as_os_str().is_empty() {
        bail!("--output-dir must not be empty");
    }
    Ok(())
}

fn validate_metadata_value(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} must not be empty");
    }
    if value != value.trim() || value.chars().any(char::is_control) {
        bail!("{field} must be trimmed and contain no control characters");
    }
    Ok(())
}

fn expected_pixel_count(nside: u32) -> Result<usize> {
    let nside = u64::from(nside);
    let pixels = 12_u64
        .checked_mul(nside)
        .and_then(|value| value.checked_mul(nside))
        .context("HEALPix pixel-count overflow")?;
    usize::try_from(pixels).context("HEALPix map is too large for this platform")
}

fn read_input_manifest(path: &Path) -> Result<(InputManifest, String)> {
    let raw = fs::read(path)
        .with_context(|| format!("failed to read inputs manifest {}", path.display()))?;
    let checksum = sha256_bytes(&raw);
    let manifest = if raw.iter().copied().find(|byte| !byte.is_ascii_whitespace()) == Some(b'{') {
        serde_json::from_slice(&raw)
            .with_context(|| format!("failed to parse JSON inputs manifest {}", path.display()))?
    } else {
        let text = std::str::from_utf8(&raw)
            .with_context(|| format!("TOML inputs manifest {} is not UTF-8", path.display()))?;
        toml::from_str(text)
            .with_context(|| format!("failed to parse TOML inputs manifest {}", path.display()))?
    };
    Ok((manifest, checksum))
}

fn validate_manifest_bindings(
    manifest: &InputManifest,
    args: &Args,
    model_sha256: &str,
) -> Result<()> {
    if manifest.schema_version != INPUT_MANIFEST_SCHEMA_VERSION {
        bail!(
            "inputs manifest schema_version must be {INPUT_MANIFEST_SCHEMA_VERSION}, found {}",
            manifest.schema_version
        );
    }
    if manifest.inputs.is_empty() {
        bail!("inputs manifest must contain at least one checksummed input");
    }
    if let Some(release_id) = &manifest.release_id {
        if release_id != &args.release_id {
            bail!("inputs manifest release_id does not match --release-id");
        }
    }
    if let Some(checksum) = &manifest.model_checksum {
        let checksum = normalize_sha256(checksum, "inputs manifest model_checksum")?;
        if checksum != model_sha256 {
            bail!("inputs manifest model_checksum does not match --model-checksum");
        }
    }
    Ok(())
}

fn prepare_inputs(manifest: &InputManifest, manifest_path: &Path) -> Result<Vec<PreparedInput>> {
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen_paths = HashSet::new();
    let mut prepared = Vec::with_capacity(manifest.inputs.len());
    for input in &manifest.inputs {
        validate_metadata_value(&input.path, "inputs manifest path")?;
        if let Some(branch) = &input.branch {
            validate_identity(branch, "inputs manifest branch")?;
        }
        let sha256 = normalize_sha256(&input.sha256, "input sha256")?;
        let path = Path::new(&input.path);
        let resolved_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        };
        let canonical = resolved_path
            .canonicalize()
            .with_context(|| format!("failed to resolve input CSV {}", resolved_path.display()))?;
        if !canonical.is_file() {
            bail!("input {} is not a regular file", resolved_path.display());
        }
        if !seen_paths.insert(canonical.clone()) {
            bail!("duplicate input file in manifest: {}", input.path);
        }
        prepared.push(PreparedInput {
            display_path: input.path.clone(),
            resolved_path: canonical,
            sha256,
            branch: input.branch.clone(),
        });
    }
    prepared.sort_by(|left, right| {
        (&left.display_path, &left.branch, &left.sha256).cmp(&(
            &right.display_path,
            &right.branch,
            &right.sha256,
        ))
    });
    Ok(prepared)
}

fn aggregate_inputs(
    inputs: Vec<PreparedInput>,
    expected_pixels: usize,
) -> Result<AggregatedProduct> {
    let mut pixels = vec![Accumulator::default(); expected_pixels];
    let mut total = Accumulator::default();
    let mut branches: BTreeMap<String, Accumulator> = BTreeMap::new();
    let mut identities: HashSet<(String, String)> = HashSet::new();
    let mut input_diagnostics = Vec::with_capacity(inputs.len());

    for input in inputs {
        let rows = aggregate_input(
            &input,
            expected_pixels,
            &mut pixels,
            &mut total,
            &mut branches,
            &mut identities,
        )?;
        input_diagnostics.push(InputDiagnostics {
            path: input.display_path,
            sha256: input.sha256,
            branch_constraint: input.branch,
            contribution_rows: rows,
        });
    }
    Ok(AggregatedProduct {
        pixels,
        total,
        branches,
        inputs: input_diagnostics,
    })
}

fn aggregate_input(
    input: &PreparedInput,
    expected_pixels: usize,
    pixels: &mut [Accumulator],
    total: &mut Accumulator,
    branches: &mut BTreeMap<String, Accumulator>,
    identities: &mut HashSet<(String, String)>,
) -> Result<u64> {
    let file = File::open(&input.resolved_path)
        .with_context(|| format!("failed to open input CSV {}", input.resolved_path.display()))?;
    let hashing_reader = HashingReader::new(file);
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(Trim::All)
        .from_reader(hashing_reader);
    let headers = reader
        .headers()
        .with_context(|| format!("failed to read CSV header from {}", input.display_path))?
        .clone();
    let columns = ColumnIndices::from_headers(&headers)
        .with_context(|| format!("invalid CSV header in {}", input.display_path))?;
    let mut rows = 0_u64;
    for result in reader.records() {
        let record =
            result.with_context(|| format!("invalid CSV row in {}", input.display_path))?;
        let line = record.position().map_or(0, |position| position.line());
        let row = columns
            .parse(&record, expected_pixels)
            .with_context(|| format!("invalid contribution at {}:{line}", input.display_path))?;
        if let Some(expected_branch) = &input.branch {
            if &row.branch != expected_branch {
                bail!(
                    "input {} row {line} has branch {:?}, expected {:?}",
                    input.display_path,
                    row.branch,
                    expected_branch
                );
            }
        }
        let identity = (row.branch.clone(), row.source_or_bin_id.clone());
        if !identities.insert(identity) {
            bail!(
                "duplicate contribution identity branch={:?}, source_or_bin_id={:?}",
                row.branch,
                row.source_or_bin_id
            );
        }
        pixels[row.healpix_index]
            .add(&row)
            .with_context(|| format!("failed to aggregate pixel {}", row.healpix_index))?;
        total
            .add(&row)
            .context("failed to aggregate all-sky total")?;
        branches
            .entry(row.branch.clone())
            .or_default()
            .add(&row)
            .with_context(|| format!("failed to aggregate branch {:?}", row.branch))?;
        rows = rows.checked_add(1).context("input row-count overflow")?;
    }
    let hashing_reader = reader.into_inner();
    let actual_sha256 = hashing_reader.finish();
    if actual_sha256 != input.sha256 {
        bail!(
            "input checksum mismatch for {}: expected {}, actual {}",
            input.display_path,
            input.sha256,
            actual_sha256
        );
    }
    Ok(rows)
}

#[derive(Debug)]
struct ColumnIndices {
    source_or_bin_id: usize,
    healpix_index: usize,
    multiplicity: usize,
    measured: usize,
    inferred: usize,
    completeness: usize,
    statistical_uncertainty: usize,
    systematic_uncertainty: usize,
    extrapolation: usize,
    crowding: usize,
    branch: usize,
}

impl ColumnIndices {
    fn from_headers(headers: &StringRecord) -> Result<Self> {
        let mut names = Vec::with_capacity(headers.len());
        let mut seen = HashSet::new();
        for (index, raw) in headers.iter().enumerate() {
            let name = if index == 0 {
                raw.trim_start_matches('\u{feff}')
            } else {
                raw
            };
            if name.is_empty() {
                bail!("CSV contains an empty column name");
            }
            if !seen.insert(name.to_string()) {
                bail!("CSV contains duplicate column {name:?}");
            }
            names.push(name.to_string());
        }
        let required = |name: &str| -> Result<usize> {
            names
                .iter()
                .position(|candidate| candidate == name)
                .with_context(|| format!("missing required column {name:?}"))
        };
        let branch = names
            .iter()
            .position(|candidate| candidate == "branch" || candidate == "rama")
            .context("missing required column \"branch\"")?;
        Ok(Self {
            source_or_bin_id: required("source_or_bin_id")?,
            healpix_index: required("healpix_index")?,
            multiplicity: required("multiplicity")?,
            measured: required("measured_300_650")?,
            inferred: required("inferred_300_650")?,
            completeness: required("completeness_correction")?,
            statistical_uncertainty: required("statistical_uncertainty")?,
            systematic_uncertainty: required("systematic_uncertainty")?,
            extrapolation: required("flags_extrapolation")?,
            crowding: required("flags_crowding")?,
            branch,
        })
    }

    fn parse(&self, record: &StringRecord, expected_pixels: usize) -> Result<ContributionRow> {
        let field = |index: usize, name: &str| -> Result<&str> {
            record
                .get(index)
                .with_context(|| format!("missing field {name:?}"))
        };
        let source_or_bin_id = field(self.source_or_bin_id, "source_or_bin_id")?.to_string();
        validate_identity(&source_or_bin_id, "source_or_bin_id")?;
        let branch = field(self.branch, "branch")?.to_string();
        validate_identity(&branch, "branch")?;
        let healpix_index =
            parse_u64(field(self.healpix_index, "healpix_index")?, "healpix_index")?;
        let healpix_index = usize::try_from(healpix_index)
            .context("healpix_index is too large for this platform")?;
        if healpix_index >= expected_pixels {
            bail!(
                "healpix_index {healpix_index} is outside full-sky range 0..{}",
                expected_pixels - 1
            );
        }
        let multiplicity = parse_u64(field(self.multiplicity, "multiplicity")?, "multiplicity")?;
        if multiplicity == 0 {
            bail!("multiplicity must be greater than zero");
        }
        if multiplicity > MAX_EXACT_F64_INTEGER {
            bail!(
                "multiplicity exceeds {MAX_EXACT_F64_INTEGER}, the largest exactly representable f64 integer"
            );
        }
        Ok(ContributionRow {
            source_or_bin_id,
            healpix_index,
            multiplicity,
            measured: parse_nonnegative_f64(
                field(self.measured, "measured_300_650")?,
                "measured_300_650",
            )?,
            inferred: parse_nonnegative_f64(
                field(self.inferred, "inferred_300_650")?,
                "inferred_300_650",
            )?,
            completeness: parse_nonnegative_f64(
                field(self.completeness, "completeness_correction")?,
                "completeness_correction",
            )?,
            statistical_uncertainty: parse_nonnegative_f64(
                field(self.statistical_uncertainty, "statistical_uncertainty")?,
                "statistical_uncertainty",
            )?,
            systematic_uncertainty: parse_nonnegative_f64(
                field(self.systematic_uncertainty, "systematic_uncertainty")?,
                "systematic_uncertainty",
            )?,
            extrapolation: parse_bool(
                field(self.extrapolation, "flags_extrapolation")?,
                "flags_extrapolation",
            )?,
            crowding: parse_bool(field(self.crowding, "flags_crowding")?, "flags_crowding")?,
            branch,
        })
    }
}

fn validate_identity(value: &str, field: &str) -> Result<()> {
    if value.is_empty() || value != value.trim() {
        bail!("{field} must be non-empty and trimmed");
    }
    if value.len() > 512 || value.chars().any(char::is_control) {
        bail!("{field} is too long or contains control characters");
    }
    Ok(())
}

fn parse_u64(raw: &str, field: &str) -> Result<u64> {
    raw.parse::<u64>()
        .with_context(|| format!("{field} must be a non-negative integer"))
}

fn parse_nonnegative_f64(raw: &str, field: &str) -> Result<f64> {
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("{field} must be a finite non-negative number"))?;
    if !value.is_finite() || value < 0.0 {
        bail!("{field} must be finite and non-negative");
    }
    Ok(if value == 0.0 { 0.0 } else { value })
}

fn parse_bool(raw: &str, field: &str) -> Result<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => bail!("{field} must be true/false or 1/0"),
    }
}

fn checked_product(left: f64, right: f64, quantity: &str) -> Result<f64> {
    let result = left * right;
    if !result.is_finite() || result < 0.0 {
        bail!("invalid or overflowing {quantity}");
    }
    Ok(if result == 0.0 { 0.0 } else { result })
}

fn checked_sum3(first: f64, second: f64, third: f64, quantity: &str) -> Result<f64> {
    let result = first + second + third;
    if !result.is_finite() || result < 0.0 {
        bail!("invalid or overflowing {quantity}");
    }
    Ok(if result == 0.0 { 0.0 } else { result })
}

fn build_accounting(
    aggregated: &AggregatedProduct,
    pixels: &[PixelValues],
    pixel_area_sr: f64,
) -> Result<(FluxAccounting, UncertaintyAccounting)> {
    let input_measured = aggregated.total.measured.value();
    let input_inferred = aggregated.total.inferred.value();
    let input_completeness = aggregated.total.completeness.value();
    let input_total = checked_sum3(
        input_measured,
        input_inferred,
        input_completeness,
        "input total flux",
    )?;
    let radiance_to_flux = pixel_area_sr / FLUX_UNIT_CONVERSION;
    let mut output_total = StableSum::default();
    let mut output_statistical_variance = StableSum::default();
    let mut output_systematic = StableSum::default();
    for pixel in pixels {
        output_total.add(
            checked_product(
                pixel.mean_radiance,
                radiance_to_flux,
                "solid-angle-integrated output flux",
            )?,
            "solid-angle-integrated output flux",
        )?;
        let statistical_flux = checked_product(
            pixel.statistical_radiance,
            radiance_to_flux,
            "output statistical flux uncertainty",
        )?;
        output_statistical_variance.add(
            checked_product(
                statistical_flux,
                statistical_flux,
                "output statistical variance",
            )?,
            "output statistical variance",
        )?;
        output_systematic.add(
            checked_product(
                pixel.systematic_radiance,
                radiance_to_flux,
                "output systematic flux uncertainty",
            )?,
            "output systematic flux uncertainty",
        )?;
    }
    let output_total = output_total.value();
    let absolute_error = (output_total - input_total).abs();
    let relative_error = if input_total > 0.0 {
        absolute_error / input_total
    } else if output_total == 0.0 {
        0.0
    } else {
        f64::INFINITY
    };
    let conservation_pass = relative_error <= FLUX_CONSERVATION_RELATIVE_TOLERANCE;

    let input_statistical = aggregated.total.statistical_variance.value().sqrt();
    let output_statistical = output_statistical_variance.value().sqrt();
    let input_systematic = aggregated.total.systematic_correlated.value();
    let output_systematic = output_systematic.value();
    let statistical_error = relative_difference(input_statistical, output_statistical);
    let systematic_error = relative_difference(input_systematic, output_systematic);
    let accounting_pass = statistical_error <= FLUX_CONSERVATION_RELATIVE_TOLERANCE
        && systematic_error <= FLUX_CONSERVATION_RELATIVE_TOLERANCE;
    Ok((
        FluxAccounting {
            input_measured_300_650_ph_m2_s: input_measured,
            input_inferred_300_650_ph_m2_s: input_inferred,
            input_completeness_correction_300_650_ph_m2_s: input_completeness,
            input_total_300_650_ph_m2_s: input_total,
            output_solid_angle_integrated_300_650_ph_m2_s: output_total,
            absolute_error_ph_m2_s: absolute_error,
            relative_error,
            relative_tolerance: FLUX_CONSERVATION_RELATIVE_TOLERANCE,
            conservation_pass,
        },
        UncertaintyAccounting {
            statistical_model:
                "independent members and contributions: sqrt(sum(multiplicity * sigma^2))",
            systematic_model:
                "conservative fully correlated contributions: sum(multiplicity * sigma)",
            input_statistical_uncertainty_300_650_ph_m2_s: input_statistical,
            output_statistical_uncertainty_300_650_ph_m2_s: output_statistical,
            input_systematic_uncertainty_300_650_ph_m2_s: input_systematic,
            output_systematic_uncertainty_300_650_ph_m2_s: output_systematic,
            accounting_pass,
        },
    ))
}

fn relative_difference(expected: f64, actual: f64) -> f64 {
    if expected > 0.0 {
        (actual - expected).abs() / expected
    } else if actual == 0.0 {
        0.0
    } else {
        f64::INFINITY
    }
}

fn common_csv_header(
    schema: &str,
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    flux_to_radiance: f64,
    model_sha256: &str,
    input_manifest_sha256: &str,
) -> String {
    let mut output = String::new();
    writeln!(output, "# schema={schema}").expect("writing to String cannot fail");
    writeln!(output, "# schema_version={PRODUCT_SCHEMA_VERSION}")
        .expect("writing to String cannot fail");
    writeln!(output, "# release_id={}", args.release_id).expect("writing to String cannot fail");
    writeln!(output, "# calibration_status=candidate").expect("writing to String cannot fail");
    writeln!(output, "# production_ready=false").expect("writing to String cannot fail");
    writeln!(output, "# candidate_only=true").expect("writing to String cannot fail");
    writeln!(output, "# band_nm={BAND_MIN_NM}-{BAND_MAX_NM}")
        .expect("writing to String cannot fail");
    writeln!(output, "# nside={}", args.nside).expect("writing to String cannot fail");
    writeln!(output, "# ordering=ring").expect("writing to String cannot fail");
    writeln!(output, "# expected_pixels={expected_pixels}").expect("writing to String cannot fail");
    writeln!(output, "# input_quantity=per-member integrated photon flux")
        .expect("writing to String cannot fail");
    writeln!(output, "# input_unit={INPUT_UNIT}").expect("writing to String cannot fail");
    writeln!(
        output,
        "# output_quantity=per-pixel integrated photon radiance"
    )
    .expect("writing to String cannot fail");
    writeln!(output, "# output_unit={OUTPUT_UNIT}").expect("writing to String cannot fail");
    writeln!(output, "# ph_m2_s_to_ph_cm2_ns={FLUX_UNIT_CONVERSION:.17e}")
        .expect("writing to String cannot fail");
    writeln!(output, "# pixel_area_sr={pixel_area_sr:.17e}")
        .expect("writing to String cannot fail");
    writeln!(output, "# flux_to_radiance_factor={flux_to_radiance:.17e}")
        .expect("writing to String cannot fail");
    writeln!(output, "# model_sha256={model_sha256}").expect("writing to String cannot fail");
    writeln!(output, "# input_manifest_sha256={input_manifest_sha256}")
        .expect("writing to String cannot fail");
    output
}

#[allow(clippy::too_many_arguments)]
fn write_mean_csv<W: Write>(
    writer: &mut W,
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    flux_to_radiance: f64,
    model_sha256: &str,
    input_manifest_sha256: &str,
    aggregated: &AggregatedProduct,
    pixels: &[PixelValues],
) -> Result<()> {
    writer.write_all(
        common_csv_header(
            "nsb.starlight.mean",
            args,
            expected_pixels,
            pixel_area_sr,
            flux_to_radiance,
            model_sha256,
            input_manifest_sha256,
        )
        .as_bytes(),
    )?;
    writeln!(
        writer,
        "healpix_index,mean_radiance_300_650_ph_cm2_ns_sr,statistical_uncertainty_300_650_ph_cm2_ns_sr,systematic_uncertainty_300_650_ph_cm2_ns_sr,total_uncertainty_300_650_ph_cm2_ns_sr,inferred_fraction,flags_extrapolation,flags_crowding,contribution_rows,represented_multiplicity"
    )?;
    for (index, (pixel, accumulator)) in pixels.iter().zip(&aggregated.pixels).enumerate() {
        writeln!(
            writer,
            "{index},{:.17e},{:.17e},{:.17e},{:.17e},{:.17e},{},{},{},{}",
            pixel.mean_radiance,
            pixel.statistical_radiance,
            pixel.systematic_radiance,
            pixel.total_uncertainty_radiance,
            pixel.inferred_fraction,
            accumulator.extrapolation,
            accumulator.crowding,
            accumulator.contribution_rows,
            accumulator.represented_multiplicity,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_uncertainty_csv<W: Write>(
    writer: &mut W,
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    flux_to_radiance: f64,
    model_sha256: &str,
    input_manifest_sha256: &str,
    pixels: &[PixelValues],
) -> Result<()> {
    writer.write_all(
        common_csv_header(
            "nsb.starlight.uncertainty",
            args,
            expected_pixels,
            pixel_area_sr,
            flux_to_radiance,
            model_sha256,
            input_manifest_sha256,
        )
        .as_bytes(),
    )?;
    writeln!(
        writer,
        "# statistical_model=independent members and contributions: sqrt(sum(multiplicity * sigma^2))"
    )?;
    writeln!(
        writer,
        "# systematic_model=conservative fully correlated contributions: sum(multiplicity * sigma)"
    )?;
    writeln!(writer, "# total_model=sqrt(statistical^2 + systematic^2)")?;
    writeln!(
        writer,
        "healpix_index,statistical_uncertainty_300_650_ph_cm2_ns_sr,systematic_uncertainty_300_650_ph_cm2_ns_sr,total_uncertainty_300_650_ph_cm2_ns_sr"
    )?;
    for (index, pixel) in pixels.iter().enumerate() {
        writeln!(
            writer,
            "{index},{:.17e},{:.17e},{:.17e}",
            pixel.statistical_radiance, pixel.systematic_radiance, pixel.total_uncertainty_radiance,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_completeness_csv<W: Write>(
    writer: &mut W,
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    flux_to_radiance: f64,
    model_sha256: &str,
    input_manifest_sha256: &str,
    aggregated: &AggregatedProduct,
    pixels: &[PixelValues],
) -> Result<()> {
    writer.write_all(
        common_csv_header(
            "nsb.starlight.completeness",
            args,
            expected_pixels,
            pixel_area_sr,
            flux_to_radiance,
            model_sha256,
            input_manifest_sha256,
        )
        .as_bytes(),
    )?;
    writeln!(
        writer,
        "# inferred_fraction_definition=(inferred + completeness_correction) / mean"
    )?;
    writeln!(
        writer,
        "healpix_index,measured_radiance_300_650_ph_cm2_ns_sr,inferred_radiance_300_650_ph_cm2_ns_sr,completeness_correction_radiance_300_650_ph_cm2_ns_sr,inferred_fraction,flags_extrapolation,flags_crowding,contribution_rows,represented_multiplicity"
    )?;
    for (index, (pixel, accumulator)) in pixels.iter().zip(&aggregated.pixels).enumerate() {
        writeln!(
            writer,
            "{index},{:.17e},{:.17e},{:.17e},{:.17e},{},{},{},{}",
            pixel.measured_radiance,
            pixel.inferred_radiance,
            pixel.completeness_radiance,
            pixel.inferred_fraction,
            accumulator.extrapolation,
            accumulator.crowding,
            accumulator.contribution_rows,
            accumulator.represented_multiplicity,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_diagnostics(
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    model_sha256: &str,
    input_manifest_sha256: &str,
    aggregated: &AggregatedProduct,
    flux_accounting: FluxAccounting,
    uncertainty_accounting: UncertaintyAccounting,
    artifact_sha256: BTreeMap<String, String>,
) -> Result<Diagnostics> {
    let total_flux = checked_sum3(
        aggregated.total.measured.value(),
        aggregated.total.inferred.value(),
        aggregated.total.completeness.value(),
        "diagnostic total flux",
    )?;
    let inferred_flux = aggregated.total.inferred.value() + aggregated.total.completeness.value();
    let inferred_fraction = if total_flux > 0.0 {
        inferred_flux / total_flux
    } else {
        0.0
    };
    let empty_pixels = aggregated
        .pixels
        .iter()
        .filter(|pixel| pixel.contribution_rows == 0)
        .count();
    let extrapolation_pixels = aggregated
        .pixels
        .iter()
        .filter(|pixel| pixel.extrapolation)
        .count();
    let crowding_pixels = aggregated
        .pixels
        .iter()
        .filter(|pixel| pixel.crowding)
        .count();
    let branches = aggregated
        .branches
        .iter()
        .map(|(name, accumulator)| {
            let measured = accumulator.measured.value();
            let inferred = accumulator.inferred.value();
            let completeness = accumulator.completeness.value();
            let total = measured + inferred + completeness;
            (
                name.clone(),
                BranchDiagnostics {
                    contribution_rows: accumulator.contribution_rows,
                    represented_multiplicity: accumulator.represented_multiplicity,
                    measured_300_650_ph_m2_s: measured,
                    inferred_300_650_ph_m2_s: inferred,
                    completeness_correction_300_650_ph_m2_s: completeness,
                    total_300_650_ph_m2_s: total,
                    statistical_uncertainty_300_650_ph_m2_s: accumulator
                        .statistical_variance
                        .value()
                        .sqrt(),
                    systematic_uncertainty_300_650_ph_m2_s: accumulator
                        .systematic_correlated
                        .value(),
                    extrapolation_rows: accumulator.extrapolation_rows,
                    crowding_rows: accumulator.crowding_rows,
                },
            )
        })
        .collect();
    Ok(Diagnostics {
        schema_version: PRODUCT_SCHEMA_VERSION,
        product: "nsb.integrated_starlight_300_650nm",
        release_id: args.release_id.clone(),
        calibration_status: "candidate",
        production_ready: false,
        candidate_only: true,
        band_nm: [BAND_MIN_NM, BAND_MAX_NM],
        nside: args.nside,
        model_sha256: model_sha256.to_string(),
        input_manifest_sha256: input_manifest_sha256.to_string(),
        input_quantity: "per-member integrated photon flux",
        input_unit: INPUT_UNIT,
        output_quantity: "per-pixel integrated photon radiance",
        output_unit: OUTPUT_UNIT,
        ph_m2_s_to_ph_cm2_ns: FLUX_UNIT_CONVERSION,
        pixel_area_sr,
        inputs: aggregated
            .inputs
            .iter()
            .map(|input| InputDiagnostics {
                path: input.path.clone(),
                sha256: input.sha256.clone(),
                branch_constraint: input.branch_constraint.clone(),
                contribution_rows: input.contribution_rows,
            })
            .collect(),
        unique_contribution_rows: aggregated.total.contribution_rows,
        represented_multiplicity: aggregated.total.represented_multiplicity,
        inferred_fraction,
        flux_accounting,
        uncertainty_accounting,
        coverage: CoverageDiagnostics {
            full_healpix_coverage: true,
            ordering: "ring",
            expected_pixels,
            emitted_pixels: expected_pixels,
            empty_pixels,
            first_pixel_emitted: !aggregated.pixels.is_empty(),
            last_pixel_emitted: !aggregated.pixels.is_empty(),
            seam_endpoint_pixels_distinct: expected_pixels > 1,
        },
        flags: FlagDiagnostics {
            extrapolation_contribution_rows: aggregated.total.extrapolation_rows,
            extrapolation_represented_multiplicity: aggregated.total.extrapolation_multiplicity,
            extrapolation_pixels,
            crowding_contribution_rows: aggregated.total.crowding_rows,
            crowding_represented_multiplicity: aggregated.total.crowding_multiplicity,
            crowding_pixels,
        },
        branches,
        artifact_sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_product_manifest(
    args: &Args,
    expected_pixels: usize,
    pixel_area_sr: f64,
    flux_to_radiance: f64,
    model_sha256: String,
    input_manifest_sha256: String,
    aggregated: &AggregatedProduct,
    diagnostics: &Diagnostics,
    artifacts: BTreeMap<String, String>,
) -> ProductManifest {
    ProductManifest {
        schema_version: PRODUCT_SCHEMA_VERSION,
        product: "nsb.integrated_starlight_300_650nm",
        release_id: args.release_id.clone(),
        calibration_status: "candidate",
        production_ready: false,
        candidate_only: true,
        band_min_nm: BAND_MIN_NM,
        band_max_nm: BAND_MAX_NM,
        nside: args.nside,
        ordering: "ring",
        expected_pixels,
        model_sha256,
        input_manifest_sha256,
        flux_conservation_validated: diagnostics.flux_accounting.conservation_pass,
        uncertainty_accounting_validated: diagnostics.uncertainty_accounting.accounting_pass,
        units: UnitsManifest {
            input_quantity: "per-member integrated photon flux",
            input_unit: INPUT_UNIT,
            output_quantity: "per-pixel integrated photon radiance",
            output_unit: OUTPUT_UNIT,
            ph_m2_s_to_ph_cm2_ns: FLUX_UNIT_CONVERSION,
            pixel_area_sr,
            flux_to_radiance_factor: flux_to_radiance,
        },
        uncertainty_model: UncertaintyManifest {
            statistical: "independent members and contributions: sqrt(sum(multiplicity * sigma^2))",
            systematic: "conservative fully correlated contributions: sum(multiplicity * sigma)",
            total: "sqrt(statistical^2 + systematic^2)",
        },
        inputs: aggregated
            .inputs
            .iter()
            .map(|input| ManifestInput {
                path: input.path.clone(),
                sha256: input.sha256.clone(),
                branch_constraint: input.branch_constraint.clone(),
                contribution_rows: input.contribution_rows,
            })
            .collect(),
        artifacts,
    }
}

fn normalize_sha256(raw: &str, field: &str) -> Result<String> {
    let checksum = raw
        .strip_prefix("sha256:")
        .or_else(|| raw.strip_prefix("SHA256:"))
        .unwrap_or(raw);
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{field} must be a 64-digit SHA-256, optionally prefixed by sha256:");
    }
    Ok(format!("sha256:{}", checksum.to_ascii_lowercase()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format_sha256(&digest)
}

fn format_sha256(digest: &[u8]) -> String {
    let mut output = String::with_capacity("sha256:".len() + digest.len() * 2);
    output.push_str("sha256:");
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
}

impl<R> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    fn finish(self) -> String {
        let digest = self.hasher.finalize();
        format_sha256(&digest)
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.hasher.update(&buffer[..count]);
        Ok(count)
    }
}

struct HashingWriter {
    inner: BufWriter<File>,
    hasher: Sha256,
}

impl HashingWriter {
    fn create(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("failed to create staged artifact {}", path.display()))?;
        Ok(Self {
            inner: BufWriter::new(file),
            hasher: Sha256::new(),
        })
    }

    fn finish(mut self) -> Result<String> {
        self.flush().context("failed to flush staged artifact")?;
        self.inner
            .get_ref()
            .sync_all()
            .context("failed to sync staged artifact")?;
        let digest = self.hasher.finalize();
        Ok(format_sha256(&digest))
    }
}

impl Write for HashingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.hasher.update(&buffer[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn write_hashed_file<F>(path: &Path, write_content: F) -> Result<String>
where
    F: FnOnce(&mut HashingWriter) -> Result<()>,
{
    let mut writer = HashingWriter::create(path)?;
    write_content(&mut writer)
        .with_context(|| format!("failed to write staged artifact {}", path.display()))?;
    writer.finish()
}

fn write_hashed_bytes(path: &Path, bytes: &[u8]) -> Result<String> {
    write_hashed_file(path, |writer| {
        writer.write_all(bytes)?;
        Ok(())
    })
}

struct StagingDirectory {
    path: PathBuf,
    committed: bool,
}

impl StagingDirectory {
    fn create(output_dir: &Path) -> Result<Self> {
        for sequence in 0..1_000_u32 {
            let path = output_dir.join(format!(
                ".starlight-product.stage-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        committed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to create staging directory {}", path.display())
                    });
                }
            }
        }
        bail!("failed to allocate a unique staging directory");
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn commit(mut self, output_dir: &Path) -> Result<()> {
        sync_directory(&self.path)?;
        // The manifest is renamed last and therefore acts as the commit marker:
        // an interrupted replacement remains fail-closed because old manifest
        // checksums cannot validate a partially replaced artifact set.
        for name in OUTPUT_FILES {
            let staged = self.path.join(name);
            let final_path = output_dir.join(name);
            fs::rename(&staged, &final_path).with_context(|| {
                format!(
                    "failed atomic rename {} -> {}",
                    staged.display(),
                    final_path.display()
                )
            })?;
        }
        fs::remove_dir(&self.path).with_context(|| {
            format!("failed to remove staging directory {}", self.path.display())
        })?;
        sync_directory(output_dir)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open directory {} for sync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::TempDir;

    const HEADER: &str = "source_or_bin_id,healpix_index,multiplicity,measured_300_650,inferred_300_650,completeness_correction,statistical_uncertainty,systematic_uncertainty,flags_extrapolation,flags_crowding,branch\n";

    #[derive(Debug, Deserialize)]
    struct MeanRow {
        healpix_index: usize,
        mean_radiance_300_650_ph_cm2_ns_sr: f64,
        statistical_uncertainty_300_650_ph_cm2_ns_sr: f64,
        systematic_uncertainty_300_650_ph_cm2_ns_sr: f64,
        total_uncertainty_300_650_ph_cm2_ns_sr: f64,
        inferred_fraction: f64,
        flags_extrapolation: bool,
        flags_crowding: bool,
        contribution_rows: u64,
        represented_multiplicity: u64,
    }

    fn fixture(csv_body: &str) -> Result<(TempDir, PathBuf)> {
        let directory = tempfile::tempdir()?;
        let input = directory.path().join("contributions.csv");
        fs::write(&input, format!("{HEADER}{csv_body}"))?;
        let checksum = sha256_bytes(&fs::read(&input)?);
        let manifest = directory.path().join("inputs.toml");
        fs::write(
            &manifest,
            format!(
                "schema_version = 1\nrelease_id = \"fixture-v1\"\nmodel_checksum = \"{}\"\n\n[[inputs]]\npath = \"contributions.csv\"\nsha256 = \"{}\"\n",
                fixture_model_checksum(),
                checksum
            ),
        )?;
        Ok((directory, manifest))
    }

    fn fixture_model_checksum() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    fn fixture_args(manifest: PathBuf, output_dir: PathBuf) -> Args {
        Args {
            inputs_manifest: manifest,
            nside: 1,
            release_id: "fixture-v1".to_string(),
            model_checksum: fixture_model_checksum(),
            output_dir,
            candidate_only: true,
        }
    }

    fn read_mean(path: &Path) -> Result<Vec<MeanRow>> {
        let mut reader = ReaderBuilder::new().comment(Some(b'#')).from_path(path)?;
        reader
            .deserialize()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn assert_close(actual: f64, expected: f64) {
        let scale = expected.abs().max(1.0e-300);
        assert!(
            (actual - expected).abs() <= 1.0e-12 * scale,
            "actual={actual:.17e}, expected={expected:.17e}"
        );
    }

    #[test]
    fn deterministic_full_sky_product_conserves_flux_and_uncertainty() -> Result<()> {
        let body = concat!(
            "sample-a,0,2,10,1,0.5,2,1,true,false,xp_sampled\n",
            "sample-c,0,1,1,0,0,4,0.25,false,false,xp_sampled\n",
            "missing-b,11,1,5,0,1,3,0.5,false,true,no_xp\n",
        );
        let (directory, manifest) = fixture(body)?;
        let output_a = directory.path().join("output-a");
        let output_b = directory.path().join("output-b");
        run(fixture_args(manifest.clone(), output_a.clone()))?;
        run(fixture_args(manifest, output_b.clone()))?;

        for name in OUTPUT_FILES {
            assert_eq!(
                fs::read(output_a.join(name))?,
                fs::read(output_b.join(name))?,
                "artifact {name} is not deterministic"
            );
        }

        let rows = read_mean(&output_a.join(MEAN_FILE))?;
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0].healpix_index, 0);
        assert_eq!(rows[11].healpix_index, 11);
        let pixel_area = std::f64::consts::PI / 3.0;
        let factor = FLUX_UNIT_CONVERSION / pixel_area;
        assert_close(rows[0].mean_radiance_300_650_ph_cm2_ns_sr, 24.0 * factor);
        assert_close(
            rows[0].statistical_uncertainty_300_650_ph_cm2_ns_sr,
            24.0_f64.sqrt() * factor,
        );
        assert_close(
            rows[0].systematic_uncertainty_300_650_ph_cm2_ns_sr,
            2.25 * factor,
        );
        assert_close(
            rows[0].total_uncertainty_300_650_ph_cm2_ns_sr,
            (24.0_f64 + 2.25_f64.powi(2)).sqrt() * factor,
        );
        assert_close(rows[0].inferred_fraction, 3.0 / 24.0);
        assert!(rows[0].flags_extrapolation);
        assert!(!rows[0].flags_crowding);
        assert_eq!(rows[0].contribution_rows, 2);
        assert_eq!(rows[0].represented_multiplicity, 3);
        assert_close(rows[1].mean_radiance_300_650_ph_cm2_ns_sr, 0.0);
        assert_close(rows[11].mean_radiance_300_650_ph_cm2_ns_sr, 6.0 * factor);
        assert!(rows[11].flags_crowding);

        let diagnostics: serde_json::Value =
            serde_json::from_slice(&fs::read(output_a.join(DIAGNOSTICS_FILE))?)?;
        assert_eq!(diagnostics["calibration_status"], "candidate");
        assert_eq!(diagnostics["production_ready"], false);
        assert_eq!(diagnostics["unique_contribution_rows"], 3);
        assert_eq!(diagnostics["represented_multiplicity"], 4);
        assert_eq!(diagnostics["coverage"]["empty_pixels"], 10);
        assert_eq!(diagnostics["coverage"]["first_pixel_emitted"], true);
        assert_eq!(diagnostics["coverage"]["last_pixel_emitted"], true);
        assert_eq!(
            diagnostics["coverage"]["seam_endpoint_pixels_distinct"],
            true
        );
        assert_eq!(diagnostics["flux_accounting"]["conservation_pass"], true);
        assert_close(
            diagnostics["flux_accounting"]["input_total_300_650_ph_m2_s"]
                .as_f64()
                .context("missing input flux")?,
            30.0,
        );
        assert_close(
            diagnostics["uncertainty_accounting"]["input_statistical_uncertainty_300_650_ph_m2_s"]
                .as_f64()
                .context("missing statistical uncertainty")?,
            33.0_f64.sqrt(),
        );
        assert_close(
            diagnostics["uncertainty_accounting"]["input_systematic_uncertainty_300_650_ph_m2_s"]
                .as_f64()
                .context("missing systematic uncertainty")?,
            2.75,
        );

        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(output_a.join(MANIFEST_FILE))?)?;
        assert_eq!(manifest["calibration_status"].as_str(), Some("candidate"));
        assert_eq!(manifest["production_ready"].as_bool(), Some(false));
        assert_eq!(manifest["candidate_only"].as_bool(), Some(true));
        assert_eq!(manifest["schema_version"].as_integer(), Some(2));
        for artifact in [
            MEAN_FILE,
            UNCERTAINTY_FILE,
            COMPLETENESS_FILE,
            DIAGNOSTICS_FILE,
        ] {
            let recorded = manifest["artifacts"][artifact]
                .as_str()
                .context("missing artifact checksum")?;
            assert_eq!(recorded, sha256_bytes(&fs::read(output_a.join(artifact))?));
        }
        Ok(())
    }

    #[test]
    fn rejects_negative_contributions_before_writing_outputs() -> Result<()> {
        let (directory, manifest) = fixture("negative,0,1,-1,0,0,1,1,false,false,xp_sampled\n")?;
        let output = directory.path().join("output");
        let error = run(fixture_args(manifest, output.clone())).unwrap_err();
        assert!(format!("{error:#}").contains("measured_300_650"));
        assert!(!output.join(MEAN_FILE).exists());
        Ok(())
    }

    #[test]
    fn rejects_duplicate_branch_identity_without_double_counting() -> Result<()> {
        let (directory, manifest) = fixture(concat!(
            "duplicate,0,1,1,0,0,1,1,false,false,xp_sampled\n",
            "duplicate,1,1,2,0,0,1,1,false,false,xp_sampled\n",
        ))?;
        let error = run(fixture_args(manifest, directory.path().join("output"))).unwrap_err();
        assert!(format!("{error:#}").contains("duplicate contribution identity"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_multiplicity() -> Result<()> {
        for multiplicity in ["0", "-1", "1.5", "9007199254740993"] {
            let body =
                format!("invalid-multiplicity,0,{multiplicity},1,0,0,1,1,false,false,xp_sampled\n");
            let (directory, manifest) = fixture(&body)?;
            let error = run(fixture_args(manifest, directory.path().join("output"))).unwrap_err();
            assert!(
                format!("{error:#}").contains("multiplicity"),
                "unexpected error for {multiplicity}: {error:#}"
            );
        }
        Ok(())
    }

    #[test]
    fn candidate_only_is_mandatory_and_fail_closed() -> Result<()> {
        let (directory, manifest) = fixture("valid,0,1,1,0,0,1,1,false,false,xp_sampled\n")?;
        let mut args = fixture_args(manifest, directory.path().join("output"));
        args.candidate_only = false;
        let error = run(args).unwrap_err();
        assert!(format!("{error:#}").contains("--candidate-only is required"));
        Ok(())
    }
}
