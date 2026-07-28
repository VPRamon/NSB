//! Deterministic merge, map emission, and production validation.

use super::accumulator::{merge_shards, PartitionShard};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const REPORT_SCHEMA_VERSION: u32 = 3;
const MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v2";
const CROSS_RESOLUTION_RELATIVE_FLUX_TOLERANCE: f64 = 0.001;
const ADMISSION_POLICY_ID: &str = "gaia-dr3-xp-continuous-join-v1";
const POPULATION_POLICY_ID: &str = "selection-function-identity-stub-v1";
const SPECTRAL_POLICY_ID: &str = "gaia-xp-continuous-336-650-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeReport {
    pub schema_version: u32,
    pub nside: u32,
    pub shard_count: usize,
    pub partition_ids: Vec<String>,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub exclusion_reasons: BTreeMap<String, u64>,
    pub science_policy: SciencePolicyReport,
    pub resolution_summaries: Vec<ResolutionSummary>,
    pub map_sha256: BTreeMap<String, String>,
    pub deterministic_reference: DeterministicReference,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyMergeReportV2 {
    schema_version: u32,
    nside: u32,
    shard_count: usize,
    partition_ids: Vec<String>,
    observed_sources: u64,
    admitted_sources: u64,
    excluded_sources: u64,
    exclusion_reasons: BTreeMap<String, u64>,
    science_policy: SciencePolicyReport,
    map_sha256: BTreeMap<String, String>,
    deterministic_reference: DeterministicReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionRole {
    ConservativeDownsample,
    Canonical,
    DiagnosticConservativeUpsample,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapDerivation {
    ConservativeDownsampleFromNside128,
    CanonicalGaiaSourceAccumulation,
    UniformAreaConservativeUpsampleFromNside128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionSummary {
    pub nside: u32,
    pub role: ResolutionRole,
    pub derivation: MapDerivation,
    pub occupied_pixels: u64,
    pub total_flux_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub relative_flux_drift_from_nside128: f64,
}

/// Machine-readable declaration of what the current candidate does and does not model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SciencePolicyReport {
    pub schema_version: u32,
    pub admission_policy_id: String,
    pub admission_rules: Vec<String>,
    pub population_correction: PopulationCorrectionReport,
    pub spectral_coverage: SpectralCoverageReport,
}

/// Versioned no-op placeholder for the not-yet-calibrated Gaia selection model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationCorrectionReport {
    pub policy_id: String,
    pub applied: bool,
    pub minimum_weight: f64,
    pub maximum_weight: f64,
    pub residual_faint_tail_estimated: bool,
    pub limitation: String,
}

/// Explicit passband coverage declaration for the XP-only candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralCoverageReport {
    pub policy_id: String,
    pub target_band_nm: [u16; 2],
    pub directly_integrated_band_nm: [u16; 2],
    pub ultraviolet_correction_applied: bool,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterministicReference {
    pub pixel: u32,
    pub canonical_sha256: String,
    pub independent_partial_merge_sha256: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct MapPixel {
    flux: f64,
    compensation: f64,
    admitted: u64,
    excluded: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MapContract {
    nside: u32,
    role: ResolutionRole,
    derivation: MapDerivation,
    source_count_semantics: &'static str,
}

impl MapContract {
    fn for_nside(nside: u32) -> Result<Self> {
        match nside {
            64 => Ok(Self {
                nside,
                role: ResolutionRole::ConservativeDownsample,
                derivation: MapDerivation::ConservativeDownsampleFromNside128,
                source_count_semantics: "exact_parent_aggregation",
            }),
            128 => Ok(Self {
                nside,
                role: ResolutionRole::Canonical,
                derivation: MapDerivation::CanonicalGaiaSourceAccumulation,
                source_count_semantics: "exact_source_membership",
            }),
            256 | 512 => Ok(Self {
                nside,
                role: ResolutionRole::DiagnosticConservativeUpsample,
                derivation: MapDerivation::UniformAreaConservativeUpsampleFromNside128,
                source_count_semantics: "deterministic_parent_apportionment_not_localization",
            }),
            _ => bail!("unsupported Starlight map nside={nside}"),
        }
    }

    fn derivation_name(self) -> &'static str {
        match self.derivation {
            MapDerivation::ConservativeDownsampleFromNside128 => {
                "conservative_downsample_from_nside128"
            }
            MapDerivation::CanonicalGaiaSourceAccumulation => "canonical_gaia_source_accumulation",
            MapDerivation::UniformAreaConservativeUpsampleFromNside128 => {
                "uniform_area_conservative_upsample_from_nside128"
            }
        }
    }
}

impl MapPixel {
    fn add(&mut self, flux: f64, admitted: u64, excluded: u64) -> Result<()> {
        let adjusted = flux - self.compensation;
        let next = self.flux + adjusted;
        self.compensation = (next - self.flux) - adjusted;
        self.flux = next;
        self.admitted = self
            .admitted
            .checked_add(admitted)
            .context("map admitted count overflow")?;
        self.excluded = self
            .excluded
            .checked_add(excluded)
            .context("map excluded count overflow")?;
        Ok(())
    }
}

pub(crate) fn emit_maps(workspace: &Path, expected_partitions: &[String]) -> Result<Vec<Artifact>> {
    let shard_root = workspace.join("outputs/shards");
    let mut shard_paths = if shard_root.is_dir() {
        fs::read_dir(&shard_root)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    shard_paths.sort();
    if shard_paths.is_empty() {
        bail!("no reconciled Starlight shards; run build workers before validate");
    }
    let mut shards = Vec::with_capacity(shard_paths.len());
    for path in shard_paths {
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("parse Starlight shard {}", path.display()))?;
        shard.validate()?;
        shards.push(shard);
    }
    shards.sort_by(|left, right| left.partition_id.cmp(&right.partition_id));
    let partition_ids = shards
        .iter()
        .map(|shard| shard.partition_id.clone())
        .collect::<Vec<_>>();
    if partition_ids != expected_partitions {
        bail!(
            "reconciled Starlight shard set is incomplete: found {} of {} expected partitions",
            partition_ids.len(),
            expected_partitions.len()
        );
    }
    let merged = merge_shards(shards.clone())?;
    let independent = independently_merge_partials(&shards)?;
    let deterministic_reference = deterministic_reference(&merged, &independent)?;

    let output_root = workspace.join("outputs");
    let map128 = output_root.join("starlight_nside128.csv");
    let map64 = output_root.join("starlight_nside64.csv");
    let map256 = output_root.join("starlight_nside256.csv");
    let map512 = output_root.join("starlight_nside512.csv");
    write_map(&map128, 128, map_pixels_128(&merged))?;
    write_map(&map64, 64, downsample_64(&merged)?)?;
    write_map(&map256, 256, upsample_256(&merged)?)?;
    write_map(&map512, 512, upsample_512(&merged)?)?;

    let resolution_summaries = resolution_summaries_from_paths(&[
        (64, &map64),
        (128, &map128),
        (256, &map256),
        (512, &map512),
    ])?;
    let mut map_sha256 = BTreeMap::new();
    for (name, path) in [
        ("starlight_nside128.csv", &map128),
        ("starlight_nside64.csv", &map64),
        ("starlight_nside256.csv", &map256),
        ("starlight_nside512.csv", &map512),
    ] {
        map_sha256.insert(name.to_string(), checksum_io::sha256_file(path)?);
    }
    let (observed_sources, admitted_sources, excluded_sources) = population_totals(&merged)?;
    let report = MergeReport {
        schema_version: REPORT_SCHEMA_VERSION,
        nside: merged.nside,
        shard_count: shards.len(),
        partition_ids,
        observed_sources,
        admitted_sources,
        excluded_sources,
        exclusion_reasons: merged.exclusion_reasons.clone(),
        science_policy: science_policy_report(),
        resolution_summaries,
        map_sha256,
        deterministic_reference,
    };
    let report_path = output_root.join("merge_report.json");
    artifact_store::atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;

    let mut artifacts = Vec::new();
    for (name, path) in [
        ("merge_report.json", report_path),
        ("starlight_nside128.csv", map128),
        ("starlight_nside256.csv", map256),
        ("starlight_nside512.csv", map512),
        ("starlight_nside64.csv", map64),
    ] {
        artifacts.push(Artifact {
            name: name.to_string(),
            sha256: checksum_io::sha256_file(&path)?,
            bytes: path.metadata()?.len(),
            path,
        });
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    artifact_store::atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub(crate) fn scientific_gates(workspace: &Path) -> Result<Vec<ValidationGate>> {
    let report_path = workspace.join("outputs/merge_report.json");
    let report: MergeReport = serde_json::from_slice(&fs::read(&report_path)?)
        .with_context(|| format!("parse {}", report_path.display()))?;
    let accounting_passed = report
        .admitted_sources
        .checked_add(report.excluded_sources)
        .is_some_and(|total| total == report.observed_sources);
    let coverage = galactic_plane_coverage(&workspace.join("outputs/starlight_nside128.csv"))?;
    let declared_policy = science_policy_is_declared(&report.science_policy);
    let flux_sweep = validate_flux_gate(&workspace.join("outputs"), &report);
    let source_sweep = validate_source_gate(&workspace.join("outputs"), &report);
    let flux_sweep_detail = flux_sweep
        .as_ref()
        .map(|()| {
            format!(
                "nside 64/128/256/512 drift <= {:.3}%",
                CROSS_RESOLUTION_RELATIVE_FLUX_TOLERANCE * 100.0
            )
        })
        .unwrap_or_else(|error| error.to_string());
    let source_sweep_detail = source_sweep
        .as_ref()
        .map(|()| "nside 64/128/256/512 counts conserved exactly".to_string())
        .unwrap_or_else(|error| error.to_string());
    Ok(vec![
        ValidationGate {
            name: "pixel-coverage-galactic-plane".to_string(),
            passed: coverage >= 0.70,
            detail: format!("{coverage:.6} (required >= 0.70)"),
        },
        ValidationGate {
            name: "population-accounting".to_string(),
            passed: accounting_passed,
            detail: format!(
                "{} admitted + {} excluded = {} observed",
                report.admitted_sources, report.excluded_sources, report.observed_sources
            ),
        },
        ValidationGate {
            name: "cross-resolution-flux-conservation".to_string(),
            passed: flux_sweep.is_ok(),
            detail: flux_sweep_detail,
        },
        ValidationGate {
            name: "cross-resolution-source-accounting".to_string(),
            passed: source_sweep.is_ok(),
            detail: source_sweep_detail,
        },
        ValidationGate {
            name: "declared-science-policy".to_string(),
            passed: declared_policy,
            detail: format!(
                "admission={}; population={} applied={}; spectral={} ultraviolet_correction={}",
                report.science_policy.admission_policy_id,
                report.science_policy.population_correction.policy_id,
                report.science_policy.population_correction.applied,
                report.science_policy.spectral_coverage.policy_id,
                report
                    .science_policy
                    .spectral_coverage
                    .ultraviolet_correction_applied
            ),
        },
        ValidationGate {
            name: "deterministic-independent-partial-merge".to_string(),
            passed: report.deterministic_reference.stable,
            detail: format!(
                "pixel {} canonical={} partial={}",
                report.deterministic_reference.pixel,
                report.deterministic_reference.canonical_sha256,
                report
                    .deterministic_reference
                    .independent_partial_merge_sha256
            ),
        },
    ])
}

pub(crate) fn validate_map(path: &Path, expected_nside: u32) -> Result<()> {
    read_map(path, expected_nside).map(|_| ())
}

pub(crate) fn validate_report(path: &Path) -> Result<()> {
    let report: MergeReport = serde_json::from_slice(&fs::read(path)?)?;
    if report.schema_version != REPORT_SCHEMA_VERSION
        || report.nside != 128
        || !science_policy_is_declared(&report.science_policy)
    {
        bail!("unsupported Starlight merge report");
    }
    let expected_maps = BTreeSet::from([
        "starlight_nside64.csv",
        "starlight_nside128.csv",
        "starlight_nside256.csv",
        "starlight_nside512.csv",
    ]);
    if report
        .map_sha256
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_maps
    {
        bail!("Starlight merge report does not declare the complete resolution sweep");
    }
    for (name, expected) in &report.map_sha256 {
        let map_path = path
            .parent()
            .context("merge report has no parent")?
            .join(name);
        if checksum_io::sha256_file(&map_path)? != *expected {
            bail!("merge report checksum mismatch for {name}");
        }
    }
    validate_resolution_sweep(
        path.parent().context("merge report has no parent")?,
        &report,
    )?;
    Ok(())
}

/// Repair the published resolution sweep from its canonical nside=128 map.
///
/// This migration intentionally does not need the original Gaia partitions. The
/// nside=64 payload values are preserved while all four maps receive the v2
/// headers; nside=256 and nside=512 are re-derived conservatively.
pub fn repair_resolution_sweep(directory: &Path) -> Result<()> {
    let map128_path = directory.join("starlight_nside128.csv");
    let map64_path = directory.join("starlight_nside64.csv");
    let canonical = read_legacy_map_rows(&map128_path, 128)?;
    let map64 = read_legacy_map_rows(&map64_path, 64)?;
    let maps = BTreeMap::from([
        (64, map64),
        (128, canonical.clone()),
        (256, upsample_pixels(&canonical, 1)?),
        (512, upsample_pixels(&canonical, 2)?),
    ]);
    for (nside, pixels) in &maps {
        write_map(
            &directory.join(format!("starlight_nside{nside}.csv")),
            *nside,
            pixels.clone(),
        )?;
    }

    let mut map_sha256 = BTreeMap::new();
    for nside in [64, 128, 256, 512] {
        let name = format!("starlight_nside{nside}.csv");
        map_sha256.insert(
            name.clone(),
            checksum_io::sha256_file(&directory.join(name))?,
        );
    }
    let report_path = directory.join("merge_report.json");
    let report_bytes = fs::read(&report_path)?;
    let mut report = if let Ok(report) = serde_json::from_slice::<MergeReport>(&report_bytes) {
        report
    } else {
        let legacy: LegacyMergeReportV2 = serde_json::from_slice(&report_bytes)
            .context("merge report is neither strict schema v2 nor v3")?;
        if legacy.schema_version != 2 {
            bail!(
                "unsupported legacy Starlight merge report schema {}",
                legacy.schema_version
            );
        }
        MergeReport {
            schema_version: REPORT_SCHEMA_VERSION,
            nside: legacy.nside,
            shard_count: legacy.shard_count,
            partition_ids: legacy.partition_ids,
            observed_sources: legacy.observed_sources,
            admitted_sources: legacy.admitted_sources,
            excluded_sources: legacy.excluded_sources,
            exclusion_reasons: legacy.exclusion_reasons,
            science_policy: legacy.science_policy,
            resolution_summaries: Vec::new(),
            map_sha256: legacy.map_sha256,
            deterministic_reference: legacy.deterministic_reference,
        }
    };
    report.schema_version = REPORT_SCHEMA_VERSION;
    report.resolution_summaries = resolution_summaries(&maps)?;
    report.map_sha256 = map_sha256;
    artifact_store::atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;
    validate_report(&report_path)
}

fn read_legacy_map_rows(path: &Path, expected_nside: u32) -> Result<BTreeMap<u32, MapPixel>> {
    let text = fs::read_to_string(path)?;
    let mut data_lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    if data_lines.next() != Some("pixel,flux_ph_m2_s,admitted_sources,excluded_sources") {
        bail!("{} has an incompatible map column schema", path.display());
    }
    let mut pixels = BTreeMap::new();
    for line in data_lines {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 4 {
            bail!("{} contains a malformed map row", path.display());
        }
        let pixel = fields[0].parse::<u32>()?;
        let value = MapPixel {
            flux: fields[1].parse::<f64>()?,
            admitted: fields[2].parse::<u64>()?,
            excluded: fields[3].parse::<u64>()?,
            ..MapPixel::default()
        };
        if u64::from(pixel) >= 12 * u64::from(expected_nside).pow(2)
            || !value.flux.is_finite()
            || value.flux < 0.0
            || pixels.insert(pixel, value).is_some()
        {
            bail!(
                "{} contains an invalid or duplicate map row",
                path.display()
            );
        }
    }
    if pixels.is_empty() {
        bail!("{} contains no occupied map pixels", path.display());
    }
    Ok(pixels)
}

fn read_map(path: &Path, expected_nside: u32) -> Result<BTreeMap<u32, MapPixel>> {
    let text = fs::read_to_string(path)?;
    let contract = MapContract::for_nside(expected_nside)?;
    let expected_headers = BTreeMap::from([
        ("schema", MAP_SCHEMA.to_string()),
        ("map_type", "healpix".to_string()),
        ("coordinate_frame", "galactic".to_string()),
        ("ordering", "nested".to_string()),
        ("nside", expected_nside.to_string()),
        ("flux_quantity", "integrated_per_pixel".to_string()),
        ("flux_unit", "ph_m-2_s-1".to_string()),
        ("derivation", contract.derivation_name().to_string()),
        (
            "source_count_semantics",
            contract.source_count_semantics.to_string(),
        ),
    ])
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect::<BTreeMap<_, _>>();
    let mut observed_headers = BTreeMap::new();
    for line in text
        .lines()
        .take_while(|line| line.trim_start().starts_with('#'))
    {
        let (key, value) = line
            .trim_start()
            .strip_prefix('#')
            .context("invalid map header prefix")?
            .trim()
            .split_once('=')
            .with_context(|| format!("{} contains malformed map header", path.display()))?;
        if observed_headers
            .insert(key.to_string(), value.to_string())
            .is_some()
        {
            bail!("{} contains duplicate map header {key}", path.display());
        }
    }
    if observed_headers != expected_headers {
        bail!(
            "{} has unknown or incompatible map headers: expected {:?}, found {:?}",
            path.display(),
            expected_headers,
            observed_headers
        );
    }

    let mut data_lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    if data_lines.next() != Some("pixel,flux_ph_m2_s,admitted_sources,excluded_sources") {
        bail!("{} has an incompatible map column schema", path.display());
    }
    let mut pixels = BTreeMap::new();
    for line in data_lines {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 4 {
            bail!("{} contains a malformed map row", path.display());
        }
        let pixel = fields[0].parse::<u32>()?;
        let flux = fields[1].parse::<f64>()?;
        let admitted = fields[2].parse::<u64>()?;
        let excluded = fields[3].parse::<u64>()?;
        if u64::from(pixel) >= 12 * u64::from(expected_nside).pow(2)
            || !flux.is_finite()
            || flux < 0.0
        {
            bail!(
                "{} contains an invalid pixel or non-finite/negative flux",
                path.display()
            );
        }
        if pixels
            .insert(
                pixel,
                MapPixel {
                    flux,
                    admitted,
                    excluded,
                    ..MapPixel::default()
                },
            )
            .is_some()
        {
            bail!("{} contains duplicate pixel {pixel}", path.display());
        }
    }
    if pixels.is_empty() {
        bail!("{} contains no occupied map pixels", path.display());
    }
    Ok(pixels)
}

fn resolution_summaries_from_paths(paths: &[(u32, &Path)]) -> Result<Vec<ResolutionSummary>> {
    let maps = paths
        .iter()
        .map(|(nside, path)| Ok((*nside, read_map(path, *nside)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    resolution_summaries(&maps)
}

fn resolution_summaries(
    maps: &BTreeMap<u32, BTreeMap<u32, MapPixel>>,
) -> Result<Vec<ResolutionSummary>> {
    let reference = maps
        .get(&128)
        .context("resolution sweep is missing nside=128")?;
    let reference_flux = map_totals(reference)?.0;
    [64, 128, 256, 512]
        .into_iter()
        .map(|nside| {
            let pixels = maps
                .get(&nside)
                .with_context(|| format!("resolution sweep is missing nside={nside}"))?;
            let (total_flux, admitted_sources, excluded_sources) = map_totals(pixels)?;
            let contract = MapContract::for_nside(nside)?;
            Ok(ResolutionSummary {
                nside,
                role: contract.role,
                derivation: contract.derivation,
                occupied_pixels: u64::try_from(pixels.len())
                    .context("occupied pixel count exceeds u64")?,
                total_flux_ph_m2_s: total_flux,
                admitted_sources,
                excluded_sources,
                relative_flux_drift_from_nside128: relative_flux_drift(total_flux, reference_flux),
            })
        })
        .collect()
}

fn map_totals(pixels: &BTreeMap<u32, MapPixel>) -> Result<(f64, u64, u64)> {
    let mut total = MapPixel::default();
    for pixel in pixels.values() {
        total.add(pixel.flux, pixel.admitted, pixel.excluded)?;
    }
    if !total.flux.is_finite() || total.flux < 0.0 {
        bail!("map has a non-finite or negative total flux");
    }
    Ok((total.flux, total.admitted, total.excluded))
}

fn relative_flux_drift(total: f64, reference: f64) -> f64 {
    if reference == 0.0 {
        if total == 0.0 {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        (total - reference).abs() / reference.abs()
    }
}

fn validate_resolution_sweep(directory: &Path, report: &MergeReport) -> Result<()> {
    validate_flux_gate(directory, report)?;
    validate_source_gate(directory, report)
}

fn read_resolution_sweep(directory: &Path) -> Result<BTreeMap<u32, BTreeMap<u32, MapPixel>>> {
    [64, 128, 256, 512]
        .into_iter()
        .map(|nside| {
            let path = directory.join(format!("starlight_nside{nside}.csv"));
            Ok((nside, read_map(&path, nside)?))
        })
        .collect()
}

fn validate_flux_gate(directory: &Path, report: &MergeReport) -> Result<()> {
    let maps = read_resolution_sweep(directory)?;
    let actual_summaries = resolution_summaries(&maps)?;
    if report.resolution_summaries.len() != 4 {
        bail!("merge report must contain exactly four resolution summaries");
    }
    for (expected, actual) in report
        .resolution_summaries
        .iter()
        .zip(actual_summaries.iter())
    {
        if expected.nside != actual.nside
            || expected.role != actual.role
            || expected.derivation != actual.derivation
            || expected.occupied_pixels != actual.occupied_pixels
            || expected.total_flux_ph_m2_s.to_bits() != actual.total_flux_ph_m2_s.to_bits()
            || expected.relative_flux_drift_from_nside128.to_bits()
                != actual.relative_flux_drift_from_nside128.to_bits()
        {
            bail!(
                "merge report resolution summary for nside={} does not match emitted map",
                actual.nside
            );
        }
    }
    let canonical = maps.get(&128).context("missing canonical map")?;
    for summary in &actual_summaries {
        if !summary.total_flux_ph_m2_s.is_finite()
            || summary.total_flux_ph_m2_s < 0.0
            || !summary.relative_flux_drift_from_nside128.is_finite()
            || summary.relative_flux_drift_from_nside128 > CROSS_RESOLUTION_RELATIVE_FLUX_TOLERANCE
        {
            bail!(
                "nside={} violates cross-resolution flux conservation: drift={}",
                summary.nside,
                summary.relative_flux_drift_from_nside128
            );
        }
    }
    validate_downsample_relationship(
        canonical,
        maps.get(&64).context("missing nside=64 map")?,
        true,
    )?;
    validate_upsample_relationship(
        canonical,
        maps.get(&256).context("missing nside=256 map")?,
        1,
        true,
    )?;
    validate_upsample_relationship(
        canonical,
        maps.get(&512).context("missing nside=512 map")?,
        2,
        true,
    )
}

fn validate_source_gate(directory: &Path, report: &MergeReport) -> Result<()> {
    let maps = read_resolution_sweep(directory)?;
    let actual_summaries = resolution_summaries(&maps)?;
    if report.resolution_summaries.len() != 4 {
        bail!("merge report must contain exactly four resolution summaries");
    }
    for (expected, actual) in report
        .resolution_summaries
        .iter()
        .zip(actual_summaries.iter())
    {
        if expected.nside != actual.nside
            || expected.role != actual.role
            || expected.derivation != actual.derivation
            || expected.occupied_pixels != actual.occupied_pixels
            || expected.admitted_sources != actual.admitted_sources
            || expected.excluded_sources != actual.excluded_sources
        {
            bail!(
                "merge report source totals for nside={} do not match emitted map",
                actual.nside
            );
        }
    }
    let canonical = maps.get(&128).context("missing canonical map")?;
    let canonical_totals = map_totals(canonical)?;
    for summary in &actual_summaries {
        if summary.admitted_sources != canonical_totals.1
            || summary.excluded_sources != canonical_totals.2
        {
            bail!(
                "nside={} violates cross-resolution source accounting",
                summary.nside
            );
        }
    }
    validate_downsample_relationship(
        canonical,
        maps.get(&64).context("missing nside=64 map")?,
        false,
    )?;
    validate_upsample_relationship(
        canonical,
        maps.get(&256).context("missing nside=256 map")?,
        1,
        false,
    )?;
    validate_upsample_relationship(
        canonical,
        maps.get(&512).context("missing nside=512 map")?,
        2,
        false,
    )
}

fn validate_downsample_relationship(
    canonical: &BTreeMap<u32, MapPixel>,
    downsampled: &BTreeMap<u32, MapPixel>,
    flux: bool,
) -> Result<()> {
    let mut expected = BTreeMap::<u32, MapPixel>::new();
    for (pixel, value) in canonical {
        expected
            .entry(*pixel >> 2)
            .or_default()
            .add(value.flux, value.admitted, value.excluded)?;
    }
    compare_pixel_maps(&expected, downsampled, "nside=128 to nside=64", flux)
}

fn validate_upsample_relationship(
    canonical: &BTreeMap<u32, MapPixel>,
    upsampled: &BTreeMap<u32, MapPixel>,
    order_delta: u32,
    flux: bool,
) -> Result<()> {
    let child_count = 1_u64
        .checked_shl(2 * order_delta)
        .context("invalid HEALPix upsample order")?;
    let mut expected = BTreeMap::new();
    for (pixel, parent) in canonical {
        for child in 0..child_count {
            let child_pixel = u64::from(*pixel)
                .checked_shl(2 * order_delta)
                .context("HEALPix child pixel overflow")?
                | child;
            expected.insert(
                u32::try_from(child_pixel).context("HEALPix child pixel exceeds u32")?,
                derive_child(*parent, child_count, child)?,
            );
        }
    }
    compare_pixel_maps(&expected, upsampled, "HEALPix parent-child upsample", flux)
}

fn compare_pixel_maps(
    expected: &BTreeMap<u32, MapPixel>,
    actual: &BTreeMap<u32, MapPixel>,
    relationship: &str,
    flux: bool,
) -> Result<()> {
    if expected.keys().ne(actual.keys()) {
        bail!("{relationship} contains missing or unexpected pixels");
    }
    for (pixel, expected_pixel) in expected {
        let actual_pixel = actual
            .get(pixel)
            .context("pixel disappeared during compare")?;
        let flux_tolerance = expected_pixel.flux.abs().max(1.0) * 1.0e-12;
        let invalid = if flux {
            (actual_pixel.flux - expected_pixel.flux).abs() > flux_tolerance
        } else {
            actual_pixel.admitted != expected_pixel.admitted
                || actual_pixel.excluded != expected_pixel.excluded
        };
        if invalid {
            bail!("{relationship} is not conservative at pixel {pixel}");
        }
    }
    Ok(())
}

fn map_pixels_128(merged: &PartitionShard) -> BTreeMap<u32, MapPixel> {
    merged
        .pixels
        .iter()
        .map(|(pixel, value)| {
            (
                *pixel,
                MapPixel {
                    flux: value.flux_ph_m2_s.value(),
                    admitted: value.admitted_sources,
                    excluded: value.excluded_sources,
                    ..MapPixel::default()
                },
            )
        })
        .collect()
}

fn downsample_64(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    downsample_pixels(&map_pixels_128(merged))
}

fn downsample_pixels(parents: &BTreeMap<u32, MapPixel>) -> Result<BTreeMap<u32, MapPixel>> {
    let mut pixels = BTreeMap::<u32, MapPixel>::new();
    for (pixel, value) in parents {
        pixels
            .entry(*pixel >> 2)
            .or_default()
            .add(value.flux, value.admitted, value.excluded)?;
    }
    Ok(pixels)
}

fn upsample_256(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    upsample(merged, 1)
}

fn upsample_512(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    upsample(merged, 2)
}

fn upsample(merged: &PartitionShard, order_delta: u32) -> Result<BTreeMap<u32, MapPixel>> {
    upsample_pixels(&map_pixels_128(merged), order_delta)
}

fn upsample_pixels(
    parents: &BTreeMap<u32, MapPixel>,
    order_delta: u32,
) -> Result<BTreeMap<u32, MapPixel>> {
    let child_count = 1_u32
        .checked_shl(2 * order_delta)
        .context("invalid HEALPix upsample order")?;
    let mut pixels = BTreeMap::new();
    for (pixel, parent) in parents {
        for child in 0..child_count {
            let child_pixel = pixel
                .checked_shl(2 * order_delta)
                .context("HEALPix child pixel overflow")?
                | child;
            pixels.insert(
                child_pixel,
                derive_child(*parent, u64::from(child_count), u64::from(child))?,
            );
        }
    }
    Ok(pixels)
}

/// Uniformly divide integrated pixel flux and conservatively apportion integer counts.
///
/// The count apportionment is deterministic bookkeeping only. It does not recover
/// or claim sub-pixel source locations that were discarded by the nside=128 map.
fn derive_child(parent: MapPixel, child_count: u64, child_index: u64) -> Result<MapPixel> {
    if child_count == 0 || child_index >= child_count {
        bail!("invalid HEALPix child count or index");
    }
    let flux = parent.flux / child_count as f64;
    if !flux.is_finite() {
        bail!("non-finite child flux during HEALPix upsample");
    }
    Ok(MapPixel {
        flux,
        admitted: apportioned_count(parent.admitted, child_count, child_index)?,
        excluded: apportioned_count(parent.excluded, child_count, child_index)?,
        ..MapPixel::default()
    })
}

fn apportioned_count(total: u64, child_count: u64, child_index: u64) -> Result<u64> {
    if child_count == 0 || child_index >= child_count {
        bail!("invalid source-count apportionment");
    }
    let quotient = total / child_count;
    let remainder = total % child_count;
    quotient
        .checked_add(u64::from(child_index < remainder))
        .context("apportioned source count overflow")
}

fn science_policy_report() -> SciencePolicyReport {
    SciencePolicyReport {
        schema_version: 1,
        admission_policy_id: ADMISSION_POLICY_ID.to_string(),
        admission_rules: vec![
            "require_gaia_source_match".to_string(),
            "exclude_calibration_failed".to_string(),
            "exclude_non_positive_or_non_finite_flux".to_string(),
            "exclude_invalid_statistical_uncertainty".to_string(),
        ],
        population_correction: PopulationCorrectionReport {
            policy_id: POPULATION_POLICY_ID.to_string(),
            applied: false,
            minimum_weight: 1.0,
            maximum_weight: 1.0,
            residual_faint_tail_estimated: false,
            limitation: "No validated sky-, magnitude-, and colour-conditioned Gaia selection-function model is configured; admitted source weights remain exactly one.".to_string(),
        },
        spectral_coverage: SpectralCoverageReport {
            policy_id: SPECTRAL_POLICY_ID.to_string(),
            target_band_nm: [300, 650],
            directly_integrated_band_nm: [336, 650],
            ultraviolet_correction_applied: false,
            limitation: "The frozen GaiaXPy design begins at 336 nm; no independently calibrated 300-336 nm correction is applied.".to_string(),
        },
    }
}

fn science_policy_is_declared(policy: &SciencePolicyReport) -> bool {
    policy.schema_version == 1
        && policy.admission_policy_id == ADMISSION_POLICY_ID
        && policy.admission_rules
            == [
                "require_gaia_source_match",
                "exclude_calibration_failed",
                "exclude_non_positive_or_non_finite_flux",
                "exclude_invalid_statistical_uncertainty",
            ]
        && policy.population_correction.policy_id == POPULATION_POLICY_ID
        && !policy.population_correction.applied
        && policy.population_correction.minimum_weight == 1.0
        && policy.population_correction.maximum_weight == 1.0
        && !policy.population_correction.residual_faint_tail_estimated
        && policy.spectral_coverage.policy_id == SPECTRAL_POLICY_ID
        && policy.spectral_coverage.target_band_nm == [300, 650]
        && policy.spectral_coverage.directly_integrated_band_nm == [336, 650]
        && !policy.spectral_coverage.ultraviolet_correction_applied
}

fn write_map(path: &Path, nside: u32, pixels: BTreeMap<u32, MapPixel>) -> Result<()> {
    let contract = MapContract::for_nside(nside)?;
    let mut text = format!(
        "# schema={MAP_SCHEMA}\n\
         # map_type=healpix\n\
         # coordinate_frame=galactic\n\
         # ordering=nested\n\
         # nside={nside}\n\
         # flux_quantity=integrated_per_pixel\n\
         # flux_unit=ph_m-2_s-1\n\
         # derivation={}\n\
         # source_count_semantics={}\n\
         pixel,flux_ph_m2_s,admitted_sources,excluded_sources\n",
        contract.derivation_name(),
        contract.source_count_semantics,
    );
    for (pixel, value) in pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{},{}\n",
            value.flux, value.admitted, value.excluded
        ));
    }
    artifact_store::atomic_write(path, text.as_bytes())
}

fn independently_merge_partials(shards: &[PartitionShard]) -> Result<PartitionShard> {
    if shards.len() == 1 {
        return merge_shards(shards.to_vec());
    }
    let midpoint = shards.len().div_ceil(2);
    let mut left = merge_shards(shards[..midpoint].to_vec())?;
    let mut right = merge_shards(shards[midpoint..].iter().rev().cloned().collect::<Vec<_>>())?;
    left.partition_id = "partial-left".to_string();
    right.partition_id = "partial-right".to_string();
    merge_shards([right, left])
}

fn deterministic_reference(
    canonical: &PartitionShard,
    independent: &PartitionShard,
) -> Result<DeterministicReference> {
    for (pixel, value) in &canonical.pixels {
        let Some(other) = independent.pixels.get(pixel) else {
            continue;
        };
        let canonical_sha256 = checksum_io::sha256_bytes(&serde_json::to_vec(value)?);
        let independent_sha256 = checksum_io::sha256_bytes(&serde_json::to_vec(other)?);
        if canonical_sha256 == independent_sha256 {
            return Ok(DeterministicReference {
                pixel: *pixel,
                canonical_sha256,
                independent_partial_merge_sha256: independent_sha256,
                stable: true,
            });
        }
    }
    bail!("no pixel is stable across independent partial Starlight merges")
}

fn population_totals(merged: &PartitionShard) -> Result<(u64, u64, u64)> {
    let mut observed = 0_u64;
    let mut admitted = 0_u64;
    let mut excluded = 0_u64;
    for pixel in merged.pixels.values() {
        observed = observed
            .checked_add(pixel.observed_sources)
            .context("observed source total overflow")?;
        admitted = admitted
            .checked_add(pixel.admitted_sources)
            .context("admitted source total overflow")?;
        excluded = excluded
            .checked_add(pixel.excluded_sources)
            .context("excluded source total overflow")?;
    }
    Ok((observed, admitted, excluded))
}

fn galactic_plane_coverage(path: &Path) -> Result<f64> {
    let text = fs::read_to_string(path)?;
    let occupied = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split(',');
            let pixel = fields.next()?.parse::<u32>().ok()?;
            let _flux = fields.next()?;
            let admitted = fields.next()?.parse::<u64>().ok()?;
            (admitted > 0).then_some(pixel)
        })
        .collect::<BTreeSet<_>>();
    let mut plane_pixels = 0_u64;
    let mut covered = 0_u64;
    let plane_sin_latitude_limit = 20_f64.to_radians().sin();
    for pixel in 0..12_u32 * 128 * 128 {
        if nested_pixel_center_sin_latitude(128, pixel).abs() < plane_sin_latitude_limit {
            plane_pixels += 1;
            if occupied.contains(&pixel) {
                covered += 1;
            }
        }
    }
    if plane_pixels == 0 {
        bail!("HEALPix geometry produced no Galactic-plane pixels");
    }
    Ok(covered as f64 / plane_pixels as f64)
}

/// Return `sin(latitude)` for the centre of a NESTED HEALPix pixel.
///
/// The coverage gate only needs latitude, so decoding the face-local Morton
/// index and applying the standard HEALPix ring-coordinate equation avoids
/// pulling a full geometry crate (and its serialization dependency) into the
/// production binary.
fn nested_pixel_center_sin_latitude(nside: u32, pixel: u32) -> f64 {
    debug_assert!(nside.is_power_of_two());
    debug_assert!(pixel < 12 * nside * nside);

    const JRLL: [u32; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];

    let pixels_per_face = nside * nside;
    let face = pixel / pixels_per_face;
    let face_pixel = pixel % pixels_per_face;
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut source_bit = 0_u32;
    let mut coordinate_bit = 1_u32;
    while coordinate_bit < nside {
        x |= ((face_pixel >> source_bit) & 1) * coordinate_bit;
        y |= ((face_pixel >> (source_bit + 1)) & 1) * coordinate_bit;
        source_bit += 2;
        coordinate_bit <<= 1;
    }

    let ring = i64::from(JRLL[face as usize] * nside) - i64::from(x) - i64::from(y) - 1;
    let nside_i64 = i64::from(nside);
    let nside_f64 = f64::from(nside);
    if ring < nside_i64 {
        1.0 - (ring * ring) as f64 / (3.0 * nside_f64 * nside_f64)
    } else if ring > 3 * nside_i64 {
        let south_ring = 4 * nside_i64 - ring;
        -1.0 + (south_ring * south_ring) as f64 / (3.0 * nside_f64 * nside_f64)
    } else {
        (2 * nside_i64 - ring) as f64 * (2.0 / (3.0 * nside_f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn synthetic_canonical_map() -> BTreeMap<u32, MapPixel> {
        // Insert deliberately out of pixel order; BTreeMap canonicalizes emission.
        [
            (
                11,
                MapPixel {
                    flux: 1.0e12,
                    admitted: 17,
                    excluded: 3,
                    ..MapPixel::default()
                },
            ),
            (
                2,
                MapPixel {
                    flux: 1.0e-6,
                    admitted: 4,
                    excluded: 1,
                    ..MapPixel::default()
                },
            ),
            (
                7,
                MapPixel {
                    flux: 42.25,
                    admitted: 33,
                    excluded: 16,
                    ..MapPixel::default()
                },
            ),
        ]
        .into_iter()
        .collect()
    }

    fn assert_parent_totals(
        parents: &BTreeMap<u32, MapPixel>,
        children: &BTreeMap<u32, MapPixel>,
        order_delta: u32,
    ) {
        let child_count = 1_u32 << (2 * order_delta);
        for (parent_pixel, parent) in parents {
            let mut reconstructed = MapPixel::default();
            for child in 0..child_count {
                reconstructed
                    .add(
                        children[&((parent_pixel << (2 * order_delta)) | child)].flux,
                        children[&((parent_pixel << (2 * order_delta)) | child)].admitted,
                        children[&((parent_pixel << (2 * order_delta)) | child)].excluded,
                    )
                    .unwrap();
            }
            assert!((reconstructed.flux - parent.flux).abs() <= parent.flux.abs() * 1.0e-15);
            assert_eq!(reconstructed.admitted, parent.admitted);
            assert_eq!(reconstructed.excluded, parent.excluded);
        }
    }

    fn write_synthetic_sweep(temp: &TempDir) -> Result<MergeReport> {
        let canonical = synthetic_canonical_map();
        let maps = BTreeMap::from([
            (64, downsample_pixels(&canonical)?),
            (128, canonical.clone()),
            (256, upsample_pixels(&canonical, 1)?),
            (512, upsample_pixels(&canonical, 2)?),
        ]);
        let mut map_sha256 = BTreeMap::new();
        for (nside, pixels) in &maps {
            let name = format!("starlight_nside{nside}.csv");
            let path = temp.path().join(&name);
            write_map(&path, *nside, pixels.clone())?;
            map_sha256.insert(name, checksum_io::sha256_file(&path)?);
        }
        let report = MergeReport {
            schema_version: REPORT_SCHEMA_VERSION,
            nside: 128,
            shard_count: 1,
            partition_ids: vec!["fixture".to_string()],
            observed_sources: 74,
            admitted_sources: 54,
            excluded_sources: 20,
            exclusion_reasons: BTreeMap::from([("fixture_exclusion".to_string(), 20)]),
            science_policy: science_policy_report(),
            resolution_summaries: resolution_summaries(&maps)?,
            map_sha256,
            deterministic_reference: DeterministicReference {
                pixel: 2,
                canonical_sha256: "a".repeat(64),
                independent_partial_merge_sha256: "a".repeat(64),
                stable: true,
            },
        };
        artifact_store::atomic_write(
            &temp.path().join("merge_report.json"),
            &serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(report)
    }

    #[test]
    fn internal_nested_latitudes_cover_the_expected_galactic_plane_pixels() {
        let north = 2.0 / 3.0;
        for pixel in 0..4 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), north);
        }
        for pixel in 4..8 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), 0.0);
        }
        for pixel in 8..12 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), -north);
        }

        let plane_sin_latitude_limit = 20_f64.to_radians().sin();
        let mut plane_pixels = 0_usize;
        for pixel in 0..12_u32 * 128 * 128 {
            plane_pixels += usize::from(
                nested_pixel_center_sin_latitude(128, pixel).abs() < plane_sin_latitude_limit,
            );
        }
        assert_eq!(plane_pixels, 67_072);
    }

    #[test]
    fn nested_resolution_transforms_preserve_expected_pixel_relationships() {
        let mut shard = PartitionShard::new("fixture", 128).unwrap();
        shard.admit(42_u64 << 45, 10.0, 1.0, 0.0).unwrap();
        let pixel = *shard.pixels.keys().next().unwrap();
        let down = downsample_64(&shard).unwrap();
        assert!(down.contains_key(&(pixel >> 2)));
        let up = upsample_256(&shard).unwrap();
        assert!((0..4).all(|child| up.contains_key(&((pixel << 2) | child))));
        let up512 = upsample_512(&shard).unwrap();
        assert!((0..16).all(|child| up512.contains_key(&((pixel << 4) | child))));
    }

    #[test]
    fn upsample_256_preserves_parent_flux() {
        let parents = synthetic_canonical_map();
        let children = upsample_pixels(&parents, 1).unwrap();
        assert_parent_totals(&parents, &children, 1);
    }

    #[test]
    fn upsample_512_preserves_parent_flux() {
        let parents = synthetic_canonical_map();
        let children = upsample_pixels(&parents, 2).unwrap();
        assert_parent_totals(&parents, &children, 2);
    }

    #[test]
    fn upsample_256_preserves_parent_source_counts() {
        let parents = synthetic_canonical_map();
        let children = upsample_pixels(&parents, 1).unwrap();
        assert_parent_totals(&parents, &children, 1);
    }

    #[test]
    fn upsample_512_preserves_parent_source_counts() {
        let parents = synthetic_canonical_map();
        let children = upsample_pixels(&parents, 2).unwrap();
        assert_parent_totals(&parents, &children, 2);
    }

    #[test]
    fn upsample_handles_non_divisible_source_counts() {
        assert_eq!(
            (0..4)
                .map(|child| apportioned_count(7, 4, child).unwrap())
                .collect::<Vec<_>>(),
            [2, 2, 2, 1]
        );
        assert_eq!(
            (0..16)
                .map(|child| apportioned_count(3, 16, child).unwrap())
                .sum::<u64>(),
            3
        );
    }

    #[test]
    fn resolution_sweep_preserves_total_flux() {
        let canonical = synthetic_canonical_map();
        let maps = BTreeMap::from([
            (64, downsample_pixels(&canonical).unwrap()),
            (128, canonical.clone()),
            (256, upsample_pixels(&canonical, 1).unwrap()),
            (512, upsample_pixels(&canonical, 2).unwrap()),
        ]);
        for summary in resolution_summaries(&maps).unwrap() {
            assert!(summary.relative_flux_drift_from_nside128 <= 1.0e-15);
        }
    }

    #[test]
    fn resolution_sweep_preserves_global_source_counts() {
        let canonical = synthetic_canonical_map();
        let expected = map_totals(&canonical).unwrap();
        for pixels in [
            downsample_pixels(&canonical).unwrap(),
            canonical.clone(),
            upsample_pixels(&canonical, 1).unwrap(),
            upsample_pixels(&canonical, 2).unwrap(),
        ] {
            let totals = map_totals(&pixels).unwrap();
            assert_eq!((totals.1, totals.2), (expected.1, expected.2));
        }
    }

    #[test]
    fn resolution_summaries_record_flux_totals_and_drift() {
        let temp = TempDir::new().unwrap();
        let report = write_synthetic_sweep(&temp).unwrap();
        assert_eq!(report.resolution_summaries.len(), 4);
        assert_eq!(
            report
                .resolution_summaries
                .iter()
                .map(|summary| summary.nside)
                .collect::<Vec<_>>(),
            [64, 128, 256, 512]
        );
        assert!(report
            .resolution_summaries
            .iter()
            .all(|summary| summary.relative_flux_drift_from_nside128 <= 1.0e-15));
    }

    #[test]
    fn small_fixture_runs_from_generation_through_validation() {
        let temp = TempDir::new().unwrap();
        write_synthetic_sweep(&temp).unwrap();
        validate_report(&temp.path().join("merge_report.json")).unwrap();
    }

    #[test]
    fn validation_rejects_multiplied_child_flux() {
        let temp = TempDir::new().unwrap();
        let report = write_synthetic_sweep(&temp).unwrap();
        let path = temp.path().join("starlight_nside256.csv");
        let mut pixels = read_map(&path, 256).unwrap();
        pixels.values_mut().for_each(|pixel| pixel.flux *= 4.0);
        write_map(&path, 256, pixels).unwrap();
        assert!(validate_resolution_sweep(temp.path(), &report).is_err());
    }

    #[test]
    fn validation_rejects_multiplied_source_counts() {
        let temp = TempDir::new().unwrap();
        let report = write_synthetic_sweep(&temp).unwrap();
        let path = temp.path().join("starlight_nside512.csv");
        let mut pixels = read_map(&path, 512).unwrap();
        pixels.values_mut().for_each(|pixel| {
            pixel.admitted *= 16;
            pixel.excluded *= 16;
        });
        write_map(&path, 512, pixels).unwrap();
        assert!(validate_resolution_sweep(temp.path(), &report).is_err());
    }

    #[test]
    fn validation_rejects_missing_resolution_summary() {
        let temp = TempDir::new().unwrap();
        let mut report = write_synthetic_sweep(&temp).unwrap();
        report.resolution_summaries.pop();
        assert!(validate_resolution_sweep(temp.path(), &report).is_err());
    }

    #[test]
    fn validation_rejects_inconsistent_report_totals() {
        let temp = TempDir::new().unwrap();
        let mut report = write_synthetic_sweep(&temp).unwrap();
        report.resolution_summaries[2].admitted_sources += 1;
        assert!(validate_resolution_sweep(temp.path(), &report).is_err());
    }

    #[test]
    fn independent_partial_merge_has_a_stable_pixel_checksum() {
        let mut first = PartitionShard::new("first", 128).unwrap();
        first.admit(0, 1.0, 0.1, 0.0).unwrap();
        let mut second = PartitionShard::new("second", 128).unwrap();
        second.admit(1_u64 << 45, 2.0, 0.2, 0.0).unwrap();
        let mut third = PartitionShard::new("third", 128).unwrap();
        third.admit(2_u64 << 45, 3.0, 0.3, 0.0).unwrap();
        let shards = vec![first, second, third];
        let canonical = merge_shards(shards.clone()).unwrap();
        let independent = independently_merge_partials(&shards).unwrap();
        assert!(
            deterministic_reference(&canonical, &independent)
                .unwrap()
                .stable
        );
    }

    #[test]
    fn candidate_science_limitations_are_versioned_and_explicit() {
        let policy = science_policy_report();
        assert_eq!(policy.schema_version, 1);
        assert_eq!(policy.admission_policy_id, ADMISSION_POLICY_ID);
        assert_eq!(policy.population_correction.policy_id, POPULATION_POLICY_ID);
        assert!(!policy.population_correction.applied);
        assert_eq!(policy.population_correction.minimum_weight, 1.0);
        assert_eq!(policy.population_correction.maximum_weight, 1.0);
        assert_eq!(policy.spectral_coverage.policy_id, SPECTRAL_POLICY_ID);
        assert_eq!(
            policy.spectral_coverage.directly_integrated_band_nm,
            [336, 650]
        );
        assert!(!policy.spectral_coverage.ultraviolet_correction_applied);
        assert!(science_policy_is_declared(&policy));
    }
}
