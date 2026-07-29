//! Deterministic canonical-map emission and production validation.

use super::accumulator::{merge_shards, PartitionShard};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const REPORT_SCHEMA_VERSION: u32 = 5;
const DETERMINISTIC_MERGE_ALGORITHM: &str = "complete-partition-shard-v1";
const MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v3";
const MAP_ORDERING: &str = "nested";
const MAP_REPRESENTATION: &str = "sparse";
const MAP_OMITTED_PIXEL_SEMANTICS: &str = "zero_flux_and_source_counts";
const MAP_FLUX_QUANTITY: &str = "integrated_per_pixel";
const MAP_FLUX_UNIT: &str = "ph_m-2_s-1";
const MAP_DERIVATION: &str = "canonical_gaia_source_accumulation";
const MAP_SOURCE_COUNT_SEMANTICS: &str = "exact_source_membership";
const ADMISSION_POLICY_ID: &str = "gaia-dr3-xp-continuous-join-v1";
const POPULATION_POLICY_ID: &str = "selection-function-identity-stub-v1";
const SPECTRAL_POLICY_ID: &str = "gaia-xp-continuous-336-650-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeReport {
    pub schema_version: u32,
    pub shard_count: usize,
    pub partition_ids: Vec<String>,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub exclusion_reasons: BTreeMap<String, u64>,
    pub science_policy: SciencePolicyReport,
    pub canonical_map: CanonicalMapReport,
    pub deterministic_merge: DeterministicMergeReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalMapReport {
    pub path: String,
    pub schema: String,
    pub nside: u32,
    pub ordering: String,
    pub flux_quantity: String,
    pub flux_unit: String,
    pub derivation: String,
    pub representation: String,
    pub omitted_pixel_semantics: String,
    pub pixel_domain_size: u64,
    pub occupied_pixels: u64,
    pub total_flux_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub sha256: String,
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
pub struct DeterministicMergeReport {
    pub algorithm: String,
    pub canonical_sha256: String,
    pub independent_partial_merge_sha256: String,
    pub compared_pixels: u64,
    pub pixel_key_mismatches: u64,
    pub flux_mismatches: u64,
    pub uncertainty_mismatches: u64,
    pub source_counter_mismatches: u64,
    pub exclusion_reason_mismatches: u64,
    pub first_mismatch: Option<String>,
    pub stable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct MapPixel {
    flux: f64,
    compensation: f64,
    admitted: u64,
    excluded: u64,
}

impl MapPixel {
    fn add(&mut self, flux: f64, admitted: u64, excluded: u64) -> Result<()> {
        if !flux.is_finite() {
            bail!("cannot sum non-finite map flux");
        }
        let adjusted = flux - self.compensation;
        let next = self.flux + adjusted;
        self.compensation = (next - self.flux) - adjusted;
        self.flux = next;
        if !self.flux.is_finite() || !self.compensation.is_finite() {
            bail!("numeric overflow in canonical map total");
        }
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

pub(crate) fn emit_maps(
    workspace: &Path,
    expected_partitions: &[String],
    canonical_nside: u32,
) -> Result<Vec<Artifact>> {
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
        if shard.nside != canonical_nside {
            bail!(
                "Starlight shard {} uses nside={}, expected configured canonical nside={canonical_nside}",
                shard.partition_id,
                shard.nside
            );
        }
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
    if merged.nside != canonical_nside {
        bail!("merged Starlight shard resolution does not match configuration");
    }
    let independent = independently_merge_partials(&shards)?;
    let deterministic_merge = require_complete_deterministic_merge(&merged, &independent)?;

    let output_root = workspace.join("outputs");
    let map_name = canonical_map_name(canonical_nside);
    let map_path = output_root.join(&map_name);
    write_map(&map_path, canonical_nside, map_pixels(&merged))?;
    let emitted_pixels = read_map(&map_path, canonical_nside)?;
    let (map_flux, map_admitted, map_excluded) = map_totals(&emitted_pixels)?;
    let map_sha256 = checksum_io::sha256_file(&map_path)?;
    let (observed_sources, admitted_sources, excluded_sources) = population_totals(&merged)?;
    if admitted_sources != map_admitted || excluded_sources != map_excluded {
        bail!("canonical map totals do not match merged shard population totals");
    }

    let report = MergeReport {
        schema_version: REPORT_SCHEMA_VERSION,
        shard_count: shards.len(),
        partition_ids,
        observed_sources,
        admitted_sources,
        excluded_sources,
        exclusion_reasons: merged.exclusion_reasons.clone(),
        science_policy: science_policy_report(),
        canonical_map: CanonicalMapReport {
            path: map_name.clone(),
            schema: MAP_SCHEMA.to_string(),
            nside: canonical_nside,
            ordering: MAP_ORDERING.to_string(),
            flux_quantity: MAP_FLUX_QUANTITY.to_string(),
            flux_unit: MAP_FLUX_UNIT.to_string(),
            derivation: MAP_DERIVATION.to_string(),
            representation: MAP_REPRESENTATION.to_string(),
            omitted_pixel_semantics: MAP_OMITTED_PIXEL_SEMANTICS.to_string(),
            pixel_domain_size: pixel_domain_size(canonical_nside)?,
            occupied_pixels: u64::try_from(emitted_pixels.len())
                .context("occupied pixel count exceeds u64")?,
            total_flux_ph_m2_s: map_flux,
            admitted_sources: map_admitted,
            excluded_sources: map_excluded,
            sha256: map_sha256,
        },
        deterministic_merge,
    };
    validate_report_fields(&report, &map_path, &emitted_pixels)?;

    let report_path = output_root.join("merge_report.json");
    artifact_store::atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;
    let mut artifacts = vec![
        Artifact {
            name: map_name,
            sha256: checksum_io::sha256_file(&map_path)?,
            bytes: map_path.metadata()?.len(),
            path: map_path,
        },
        Artifact {
            name: "merge_report.json".to_string(),
            sha256: checksum_io::sha256_file(&report_path)?,
            bytes: report_path.metadata()?.len(),
            path: report_path,
        },
    ];
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    artifact_store::atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub(crate) fn scientific_gates(
    workspace: &Path,
    canonical_nside: u32,
) -> Result<Vec<ValidationGate>> {
    let report_path = workspace.join("outputs/merge_report.json");
    let report: MergeReport = serde_json::from_slice(&fs::read(&report_path)?)
        .with_context(|| format!("parse {}", report_path.display()))?;
    let integrity = validate_report(&report_path);
    let accounting_passed = report
        .admitted_sources
        .checked_add(report.excluded_sources)
        .is_some_and(|total| {
            total == report.observed_sources
                && report.admitted_sources == report.canonical_map.admitted_sources
                && report.excluded_sources == report.canonical_map.excluded_sources
        });
    let map_path = workspace
        .join("outputs")
        .join(canonical_map_name(canonical_nside));
    let coverage = galactic_plane_coverage(&map_path, canonical_nside)?;
    let declared_policy = science_policy_is_declared(&report.science_policy);
    let flux_passed = report.canonical_map.total_flux_ph_m2_s.is_finite()
        && report.canonical_map.total_flux_ph_m2_s >= 0.0;
    let expected_pixel_domain = pixel_domain_size(canonical_nside)?;
    let cardinality_passed = report.canonical_map.representation == MAP_REPRESENTATION
        && report.canonical_map.omitted_pixel_semantics == MAP_OMITTED_PIXEL_SEMANTICS
        && report.canonical_map.pixel_domain_size == expected_pixel_domain
        && report.canonical_map.occupied_pixels > 0
        && report.canonical_map.occupied_pixels <= expected_pixel_domain;

    Ok(vec![
        ValidationGate {
            name: "canonical-map-integrity".to_string(),
            passed: integrity.is_ok(),
            detail: integrity.err().map_or_else(
                || report.canonical_map.sha256.clone(),
                |error| error.to_string(),
            ),
        },
        ValidationGate {
            name: "canonical-map-cardinality".to_string(),
            passed: cardinality_passed,
            detail: format!(
                "{} occupied of {} pixels; representation={}; omitted={}",
                report.canonical_map.occupied_pixels,
                report.canonical_map.pixel_domain_size,
                report.canonical_map.representation,
                report.canonical_map.omitted_pixel_semantics
            ),
        },
        ValidationGate {
            name: "canonical-map-flux".to_string(),
            passed: flux_passed,
            detail: format!(
                "{:.17e} {}",
                report.canonical_map.total_flux_ph_m2_s, report.canonical_map.flux_unit
            ),
        },
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
            passed: report.deterministic_merge.stable
                && report.deterministic_merge.canonical_sha256
                    == report.deterministic_merge.independent_partial_merge_sha256,
            detail: format!(
                "algorithm={} pixels={} canonical={} partial={} key_mismatches={} flux_mismatches={} uncertainty_mismatches={} source_counter_mismatches={} exclusion_reason_mismatches={}",
                report.deterministic_merge.algorithm,
                report.deterministic_merge.compared_pixels,
                report.deterministic_merge.canonical_sha256,
                report.deterministic_merge.independent_partial_merge_sha256,
                report.deterministic_merge.pixel_key_mismatches,
                report.deterministic_merge.flux_mismatches,
                report.deterministic_merge.uncertainty_mismatches,
                report.deterministic_merge.source_counter_mismatches,
                report.deterministic_merge.exclusion_reason_mismatches,
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
        || !science_policy_is_declared(&report.science_policy)
    {
        bail!("unsupported Starlight merge report");
    }
    let parent = path.parent().context("merge report has no parent")?;
    let map_path = parent.join(&report.canonical_map.path);
    let pixels = read_map(&map_path, report.canonical_map.nside)?;
    validate_report_fields(&report, &map_path, &pixels)
}

fn validate_report_fields(
    report: &MergeReport,
    map_path: &Path,
    pixels: &BTreeMap<u32, MapPixel>,
) -> Result<()> {
    let expected_path = canonical_map_name(report.canonical_map.nside);
    if report.canonical_map.path != expected_path
        || report.canonical_map.schema != MAP_SCHEMA
        || report.canonical_map.ordering != MAP_ORDERING
        || report.canonical_map.flux_quantity != MAP_FLUX_QUANTITY
        || report.canonical_map.flux_unit != MAP_FLUX_UNIT
        || report.canonical_map.derivation != MAP_DERIVATION
        || report.canonical_map.representation != MAP_REPRESENTATION
        || report.canonical_map.omitted_pixel_semantics != MAP_OMITTED_PIXEL_SEMANTICS
    {
        bail!("canonical map report contains an incompatible contract");
    }
    let actual_sha256 = checksum_io::sha256_file(map_path)?;
    if report.canonical_map.sha256 != actual_sha256 {
        bail!("canonical map checksum does not match merge report");
    }
    let (total_flux, admitted, excluded) = map_totals(pixels)?;
    let occupied_pixels =
        u64::try_from(pixels.len()).context("occupied pixel count exceeds u64")?;
    let expected_pixel_domain = pixel_domain_size(report.canonical_map.nside)?;
    if report.canonical_map.pixel_domain_size != expected_pixel_domain
        || occupied_pixels > expected_pixel_domain
        || report.canonical_map.occupied_pixels != occupied_pixels
        || report.canonical_map.total_flux_ph_m2_s.to_bits() != total_flux.to_bits()
        || report.canonical_map.admitted_sources != admitted
        || report.canonical_map.excluded_sources != excluded
        || report.admitted_sources != admitted
        || report.excluded_sources != excluded
    {
        bail!("canonical map totals do not match merge report");
    }
    let observed = admitted
        .checked_add(excluded)
        .context("canonical source accounting overflow")?;
    if report.observed_sources != observed {
        bail!("global observed-source total does not match canonical map");
    }
    let deterministic = &report.deterministic_merge;
    if deterministic.algorithm != DETERMINISTIC_MERGE_ALGORITHM
        || !deterministic.stable
        || deterministic.canonical_sha256 != deterministic.independent_partial_merge_sha256
        || !is_sha256(&deterministic.canonical_sha256)
        || !is_sha256(&deterministic.independent_partial_merge_sha256)
        || deterministic.compared_pixels != occupied_pixels
        || deterministic.pixel_key_mismatches != 0
        || deterministic.flux_mismatches != 0
        || deterministic.uncertainty_mismatches != 0
        || deterministic.source_counter_mismatches != 0
        || deterministic.exclusion_reason_mismatches != 0
        || deterministic.first_mismatch.is_some()
    {
        bail!("merge report does not contain complete deterministic evidence");
    }
    Ok(())
}

fn read_map(path: &Path, expected_nside: u32) -> Result<BTreeMap<u32, MapPixel>> {
    let text = fs::read_to_string(path)?;
    let expected_headers = BTreeMap::from([
        ("schema".to_string(), MAP_SCHEMA.to_string()),
        ("map_type".to_string(), "healpix".to_string()),
        ("coordinate_frame".to_string(), "galactic".to_string()),
        ("ordering".to_string(), MAP_ORDERING.to_string()),
        ("representation".to_string(), MAP_REPRESENTATION.to_string()),
        (
            "omitted_pixel_semantics".to_string(),
            MAP_OMITTED_PIXEL_SEMANTICS.to_string(),
        ),
        ("nside".to_string(), expected_nside.to_string()),
        ("flux_quantity".to_string(), MAP_FLUX_QUANTITY.to_string()),
        ("flux_unit".to_string(), MAP_FLUX_UNIT.to_string()),
        ("derivation".to_string(), MAP_DERIVATION.to_string()),
        (
            "source_count_semantics".to_string(),
            MAP_SOURCE_COUNT_SEMANTICS.to_string(),
        ),
    ]);
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
    let mut previous_pixel = None;
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
        if pixels.contains_key(&pixel) {
            bail!("{} contains duplicate pixel {pixel}", path.display());
        }
        if let Some(previous) = previous_pixel {
            if pixel < previous {
                bail!(
                    "{} contains non-canonical pixel ordering: {pixel} follows {previous}",
                    path.display()
                );
            }
        }
        previous_pixel = Some(pixel);
        pixels.insert(
            pixel,
            MapPixel {
                flux,
                admitted,
                excluded,
                ..MapPixel::default()
            },
        );
    }
    if pixels.is_empty() {
        bail!("{} contains no occupied map pixels", path.display());
    }
    Ok(pixels)
}

fn pixel_domain_size(nside: u32) -> Result<u64> {
    u64::from(nside)
        .checked_mul(u64::from(nside))
        .and_then(|pixels_per_face| pixels_per_face.checked_mul(12))
        .context("HEALPix pixel-domain size overflow")
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

fn map_pixels(merged: &PartitionShard) -> BTreeMap<u32, MapPixel> {
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
    let mut text = format!(
        "# schema={MAP_SCHEMA}\n\
         # map_type=healpix\n\
         # coordinate_frame=galactic\n\
         # ordering={MAP_ORDERING}\n\
         # representation={MAP_REPRESENTATION}\n\
         # omitted_pixel_semantics={MAP_OMITTED_PIXEL_SEMANTICS}\n\
         # nside={nside}\n\
         # flux_quantity={MAP_FLUX_QUANTITY}\n\
         # flux_unit={MAP_FLUX_UNIT}\n\
         # derivation={MAP_DERIVATION}\n\
         # source_count_semantics={MAP_SOURCE_COUNT_SEMANTICS}\n\
         pixel,flux_ph_m2_s,admitted_sources,excluded_sources\n"
    );
    for (pixel, value) in pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{},{}\n",
            value.flux, value.admitted, value.excluded
        ));
    }
    artifact_store::atomic_write(path, text.as_bytes())
}

fn canonical_map_name(nside: u32) -> String {
    format!("starlight_nside{nside}.csv")
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

fn require_complete_deterministic_merge(
    canonical: &PartitionShard,
    independent: &PartitionShard,
) -> Result<DeterministicMergeReport> {
    let report = complete_deterministic_merge_report(canonical, independent)?;
    if !report.stable {
        bail!(
            "complete deterministic merge mismatch: first={}; key_mismatches={}; flux_mismatches={}; uncertainty_mismatches={}; source_counter_mismatches={}; exclusion_reason_mismatches={}; canonical={}; independent={}",
            report.first_mismatch.as_deref().unwrap_or("digest mismatch"),
            report.pixel_key_mismatches,
            report.flux_mismatches,
            report.uncertainty_mismatches,
            report.source_counter_mismatches,
            report.exclusion_reason_mismatches,
            report.canonical_sha256,
            report.independent_partial_merge_sha256,
        );
    }
    Ok(report)
}

fn complete_deterministic_merge_report(
    canonical: &PartitionShard,
    independent: &PartitionShard,
) -> Result<DeterministicMergeReport> {
    canonical.validate()?;
    independent.validate()?;
    if canonical.nside != independent.nside {
        bail!(
            "cannot compare deterministic Starlight merges with nside={} and nside={}",
            canonical.nside,
            independent.nside
        );
    }

    let pixel_keys = canonical
        .pixels
        .keys()
        .chain(independent.pixels.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let compared_pixels =
        u64::try_from(pixel_keys.len()).context("compared pixel count exceeds u64")?;
    let mut pixel_key_mismatches = 0_u64;
    let mut flux_mismatches = 0_u64;
    let mut uncertainty_mismatches = 0_u64;
    let mut source_counter_mismatches = 0_u64;
    let mut exclusion_reason_mismatches = 0_u64;
    let mut first_mismatch = None;

    for pixel in pixel_keys {
        match (canonical.pixels.get(&pixel), independent.pixels.get(&pixel)) {
            (Some(left), Some(right)) => {
                if left.flux_ph_m2_s != right.flux_ph_m2_s {
                    flux_mismatches += 1;
                    record_first_mismatch(
                        &mut first_mismatch,
                        format!("pixel {pixel} flux accumulator differs"),
                    );
                }
                if left.statistical_variance != right.statistical_variance
                    || left.systematic_variance != right.systematic_variance
                {
                    uncertainty_mismatches += 1;
                    record_first_mismatch(
                        &mut first_mismatch,
                        format!("pixel {pixel} uncertainty accumulator differs"),
                    );
                }
                if left.observed_sources != right.observed_sources
                    || left.admitted_sources != right.admitted_sources
                    || left.excluded_sources != right.excluded_sources
                {
                    source_counter_mismatches += 1;
                    record_first_mismatch(
                        &mut first_mismatch,
                        format!("pixel {pixel} source counters differ"),
                    );
                }
            }
            _ => {
                pixel_key_mismatches += 1;
                record_first_mismatch(
                    &mut first_mismatch,
                    format!("pixel {pixel} exists in only one merge"),
                );
            }
        }
    }

    let reason_keys = canonical
        .exclusion_reasons
        .keys()
        .chain(independent.exclusion_reasons.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for reason in reason_keys {
        if canonical.exclusion_reasons.get(&reason) != independent.exclusion_reasons.get(&reason) {
            exclusion_reason_mismatches += 1;
            record_first_mismatch(
                &mut first_mismatch,
                format!("exclusion reason {reason:?} differs"),
            );
        }
    }

    let canonical_sha256 = checksum_io::sha256_bytes(&canonical_merge_bytes(canonical)?);
    let independent_partial_merge_sha256 =
        checksum_io::sha256_bytes(&canonical_merge_bytes(independent)?);
    let stable = pixel_key_mismatches == 0
        && flux_mismatches == 0
        && uncertainty_mismatches == 0
        && source_counter_mismatches == 0
        && exclusion_reason_mismatches == 0
        && canonical_sha256 == independent_partial_merge_sha256;

    Ok(DeterministicMergeReport {
        algorithm: DETERMINISTIC_MERGE_ALGORITHM.to_string(),
        canonical_sha256,
        independent_partial_merge_sha256,
        compared_pixels,
        pixel_key_mismatches,
        flux_mismatches,
        uncertainty_mismatches,
        source_counter_mismatches,
        exclusion_reason_mismatches,
        first_mismatch,
        stable,
    })
}

fn canonical_merge_bytes(shard: &PartitionShard) -> Result<Vec<u8>> {
    shard.validate()?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"nsb-starlight-complete-merge-v1 ");
    bytes.extend_from_slice(&shard.nside.to_be_bytes());
    let pixel_count = u64::try_from(shard.pixels.len()).context("pixel count exceeds u64")?;
    bytes.extend_from_slice(&pixel_count.to_be_bytes());
    for (pixel, accumulator) in &shard.pixels {
        bytes.extend_from_slice(&pixel.to_be_bytes());
        accumulator
            .flux_ph_m2_s
            .append_canonical_bytes(&mut bytes)?;
        accumulator
            .statistical_variance
            .append_canonical_bytes(&mut bytes)?;
        accumulator
            .systematic_variance
            .append_canonical_bytes(&mut bytes)?;
        bytes.extend_from_slice(&accumulator.observed_sources.to_be_bytes());
        bytes.extend_from_slice(&accumulator.admitted_sources.to_be_bytes());
        bytes.extend_from_slice(&accumulator.excluded_sources.to_be_bytes());
    }
    let reason_count =
        u64::try_from(shard.exclusion_reasons.len()).context("reason count exceeds u64")?;
    bytes.extend_from_slice(&reason_count.to_be_bytes());
    for (reason, count) in &shard.exclusion_reasons {
        let reason_bytes = reason.as_bytes();
        let reason_len = u32::try_from(reason_bytes.len()).context("reason length exceeds u32")?;
        bytes.extend_from_slice(&reason_len.to_be_bytes());
        bytes.extend_from_slice(reason_bytes);
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    Ok(bytes)
}

fn record_first_mismatch(first: &mut Option<String>, message: String) {
    if first.is_none() {
        *first = Some(message);
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn galactic_plane_coverage(path: &Path, nside: u32) -> Result<f64> {
    let pixels = read_map(path, nside)?;
    let occupied = pixels
        .into_iter()
        .filter_map(|(pixel, value)| (value.admitted > 0).then_some(pixel))
        .collect::<BTreeSet<_>>();
    let mut plane_pixels = 0_u64;
    let mut covered = 0_u64;
    let plane_sin_latitude_limit = 20_f64.to_radians().sin();
    for pixel in 0..12_u32 * nside * nside {
        if nested_pixel_center_sin_latitude(nside, pixel).abs() < plane_sin_latitude_limit {
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

    fn fixture_shard(nside: u32) -> PartitionShard {
        let mut shard = PartitionShard::new("fixture", nside).unwrap();
        shard.admit(0, 1.0e-6, 0.0, 0.0).unwrap();
        shard.admit(1_u64 << 47, 42.25, 0.0, 0.0).unwrap();
        shard.admit(2_u64 << 47, 1.0e12, 0.0, 0.0).unwrap();
        shard.exclude(3_u64 << 47, "fixture_exclusion").unwrap();
        shard
    }

    fn emit_fixture(temp: &TempDir, nside: u32) -> MergeReport {
        let shard = fixture_shard(nside);
        let shard_path = temp.path().join("outputs/shards/fixture.json");
        shard.write(&shard_path).unwrap();
        let artifacts = emit_maps(temp.path(), &["fixture".to_string()], nside).unwrap();
        assert_eq!(
            artifacts
                .iter()
                .map(|artifact| artifact.name.clone())
                .collect::<Vec<_>>(),
            ["merge_report.json".to_string(), canonical_map_name(nside)]
        );
        serde_json::from_slice(&fs::read(temp.path().join("outputs/merge_report.json")).unwrap())
            .unwrap()
    }

    fn rewrite_report(temp: &TempDir, report: &MergeReport) {
        artifact_store::atomic_write(
            &temp.path().join("outputs/merge_report.json"),
            &serde_json::to_vec_pretty(report).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn production_emits_only_canonical_map_and_report() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let outputs = fs::read_dir(temp.path().join("outputs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>();
        assert!(outputs.contains("starlight_nside128.csv"));
        assert!(outputs.contains("merge_report.json"));
        assert!(!outputs.iter().any(|name| {
            [
                "starlight_nside64.csv",
                "starlight_nside256.csv",
                "starlight_nside512.csv",
            ]
            .contains(&name.as_str())
        }));
    }

    #[test]
    fn configured_nside_reaches_emitted_map() {
        let temp = TempDir::new().unwrap();
        let report = emit_fixture(&temp, 256);
        assert_eq!(report.canonical_map.nside, 256);
        validate_map(&temp.path().join("outputs/starlight_nside256.csv"), 256).unwrap();
    }

    #[test]
    fn canonical_report_matches_map_flux() {
        let temp = TempDir::new().unwrap();
        let report = emit_fixture(&temp, 128);
        let pixels = read_map(&temp.path().join("outputs/starlight_nside128.csv"), 128).unwrap();
        assert_eq!(
            report.canonical_map.total_flux_ph_m2_s.to_bits(),
            map_totals(&pixels).unwrap().0.to_bits()
        );
    }

    #[test]
    fn canonical_report_matches_global_source_totals() {
        let temp = TempDir::new().unwrap();
        let report = emit_fixture(&temp, 128);
        assert_eq!(
            report.admitted_sources,
            report.canonical_map.admitted_sources
        );
        assert_eq!(
            report.excluded_sources,
            report.canonical_map.excluded_sources
        );
        assert_eq!(
            report.observed_sources,
            report.admitted_sources + report.excluded_sources
        );
    }

    #[test]
    fn validation_rejects_corrupted_global_admitted_sources() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.admitted_sources += 1;
        report.observed_sources += 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_corrupted_global_excluded_sources() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.excluded_sources += 1;
        report.observed_sources += 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_corrupted_observed_sources() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.observed_sources += 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_wrong_canonical_checksum() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.sha256 = "0".repeat(64);
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_wrong_canonical_nside() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.nside = 256;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_duplicate_map_pixel() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("0,1.0e0,1,0\n");
        artifact_store::atomic_write(&path, text.as_bytes()).unwrap();
        assert!(validate_map(&path, 128).is_err());
    }

    #[test]
    fn validation_rejects_unknown_map_header() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path).unwrap().replacen(
            "# map_type=healpix",
            "# unexpected=value\n# map_type=healpix",
            1,
        );
        artifact_store::atomic_write(&path, text.as_bytes()).unwrap();
        assert!(validate_map(&path, 128).is_err());
    }

    #[test]
    fn validation_rejects_out_of_order_map_pixels() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path).unwrap();
        let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        let first_data = lines
            .iter()
            .position(|line| line == "pixel,flux_ph_m2_s,admitted_sources,excluded_sources")
            .unwrap()
            + 1;
        lines.swap(first_data, first_data + 1);
        artifact_store::atomic_write(&path, format!("{}\n", lines.join("\n")).as_bytes()).unwrap();
        let error = validate_map(&path, 128).unwrap_err().to_string();
        assert!(error.contains("non-canonical pixel ordering"));
    }

    #[test]
    fn validation_rejects_missing_sparse_representation_header() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("# representation=sparse\n", "");
        artifact_store::atomic_write(&path, text.as_bytes()).unwrap();
        assert!(validate_map(&path, 128).is_err());
    }

    #[test]
    fn validation_rejects_incompatible_report_representation() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.representation = "full-sky".to_string();
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_report_cardinality_mismatch() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.occupied_pixels += 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_wrong_pixel_domain_size() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.pixel_domain_size -= 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
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

    #[test]
    fn independent_partial_merge_has_a_complete_stable_digest() {
        let mut first = PartitionShard::new("first", 128).unwrap();
        first.admit(0, 1.0e16, 0.1, 0.0).unwrap();
        let mut second = PartitionShard::new("second", 128).unwrap();
        second.admit(1, 1.0, 0.2, 0.0).unwrap();
        let mut third = PartitionShard::new("third", 128).unwrap();
        third.admit(2, 1.0, 0.3, 0.0).unwrap();
        let shards = vec![first, second, third];
        let canonical = merge_shards(shards.clone()).unwrap();
        let independent = independently_merge_partials(&shards).unwrap();
        let report = require_complete_deterministic_merge(&canonical, &independent).unwrap();
        assert!(report.stable);
        assert_eq!(report.compared_pixels, 1);
        assert_eq!(
            report.canonical_sha256,
            report.independent_partial_merge_sha256
        );
    }

    #[test]
    fn complete_merge_detects_a_later_pixel_difference() {
        let mut first = PartitionShard::new("first", 128).unwrap();
        first.admit(0, 1.0, 0.1, 0.0).unwrap();
        let mut second = PartitionShard::new("second", 128).unwrap();
        second.admit(1_u64 << 45, 2.0, 0.2, 0.0).unwrap();
        let canonical = merge_shards([first, second]).unwrap();
        let mut independent = canonical.clone();
        let mut replacement = PartitionShard::new("replacement", 128).unwrap();
        replacement.admit(1_u64 << 45, 3.0, 0.2, 0.0).unwrap();
        independent
            .pixels
            .insert(1, replacement.pixels.remove(&1).expect("replacement pixel"));

        let report = complete_deterministic_merge_report(&canonical, &independent).unwrap();
        assert!(!report.stable);
        assert_eq!(report.flux_mismatches, 1);
        assert!(report.first_mismatch.unwrap().contains("pixel 1"));
    }

    #[test]
    fn complete_merge_detects_an_additional_pixel_key() {
        let mut canonical = PartitionShard::new("canonical", 128).unwrap();
        canonical.admit(0, 1.0, 0.1, 0.0).unwrap();
        let mut independent = canonical.clone();
        let mut extra = PartitionShard::new("extra", 128).unwrap();
        extra.admit(1_u64 << 45, 2.0, 0.2, 0.0).unwrap();
        independent
            .pixels
            .insert(1, extra.pixels.remove(&1).expect("extra pixel"));

        let report = complete_deterministic_merge_report(&canonical, &independent).unwrap();
        assert!(!report.stable);
        assert_eq!(report.pixel_key_mismatches, 1);
    }

    #[test]
    fn complete_merge_detects_source_counters_when_flux_matches() {
        let mut canonical = PartitionShard::new("canonical", 128).unwrap();
        canonical.admit(0, 1.0, 0.0, 0.0).unwrap();
        let mut independent = PartitionShard::new("independent", 128).unwrap();
        independent.admit(0, 0.5, 0.0, 0.0).unwrap();
        independent.admit(1, 0.5, 0.0, 0.0).unwrap();

        let report = complete_deterministic_merge_report(&canonical, &independent).unwrap();
        assert!(!report.stable);
        assert_eq!(report.flux_mismatches, 0);
        assert_eq!(report.source_counter_mismatches, 1);
    }

    #[test]
    fn complete_merge_detects_uncertainty_differences() {
        let mut canonical = PartitionShard::new("canonical", 128).unwrap();
        canonical.admit(0, 1.0, 0.1, 0.0).unwrap();
        let mut independent = PartitionShard::new("independent", 128).unwrap();
        independent.admit(0, 1.0, 0.2, 0.0).unwrap();

        let report = complete_deterministic_merge_report(&canonical, &independent).unwrap();
        assert!(!report.stable);
        assert_eq!(report.flux_mismatches, 0);
        assert_eq!(report.uncertainty_mismatches, 1);
    }

    #[test]
    fn complete_merge_detects_exclusion_reason_differences() {
        let mut canonical = PartitionShard::new("canonical", 128).unwrap();
        canonical.exclude(0, "invalid_flux").unwrap();
        let mut independent = canonical.clone();
        independent.exclusion_reasons.clear();
        independent
            .exclusion_reasons
            .insert("calibration_failed".to_string(), 1);

        let report = complete_deterministic_merge_report(&canonical, &independent).unwrap();
        assert!(!report.stable);
        assert_eq!(report.exclusion_reason_mismatches, 2);
    }

    #[test]
    fn validation_rejects_corrupted_complete_merge_digest() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.deterministic_merge.independent_partial_merge_sha256 = "0".repeat(64);
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn internal_nested_latitudes_cover_expected_pixels() {
        let north = 2.0 / 3.0;
        for pixel in 0..4 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), north);
        }
        let plane_sin_latitude_limit = 20_f64.to_radians().sin();
        let plane_pixels = (0..12_u32 * 128 * 128)
            .filter(|pixel| {
                nested_pixel_center_sin_latitude(128, *pixel).abs() < plane_sin_latitude_limit
            })
            .count();
        assert_eq!(plane_pixels, 67_072);
    }
}
