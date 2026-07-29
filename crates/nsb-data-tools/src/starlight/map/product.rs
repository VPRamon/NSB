//! Deterministic canonical-map emission and production validation.

use super::accumulator::{merge_shards, PartitionShard};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::StarlightProductBand;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const REPORT_SCHEMA_VERSION: u32 = 6;
const DETERMINISTIC_MERGE_ALGORITHM: &str = "complete-partition-shard-v1";
const MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v4";
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
const CORRECTED_SPECTRAL_POLICY_ID: &str = "gaia-xp-continuous-uv-corrected-300-650-v1";

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
    pub ultraviolet_applicability: BTreeMap<crate::starlight::uv::ApplicabilityStatus, u64>,
    pub science_policy: SciencePolicyReport,
    pub band_diagnostics: BandDiagnosticsReport,
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

/// Separately accumulated band and uncertainty diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandDiagnosticsReport {
    pub corrected_300_336_label: String,
    pub measured_336_650_label: String,
    pub combined_300_650_label: String,
    pub total_flux_300_336_ph_m2_s: f64,
    pub total_flux_336_650_ph_m2_s: f64,
    pub total_flux_300_650_ph_m2_s: f64,
    pub statistical_uncertainty_300_336_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_300_650_ph_m2_s: f64,
    pub systematic_uncertainty_300_336_ph_m2_s: f64,
    pub systematic_uncertainty_300_650_ph_m2_s: f64,
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
    pub corrected_band_nm: Option<[u16; 2]>,
    pub combined_band_nm: Option<[u16; 2]>,
    pub ultraviolet_correction_applied: bool,
    pub correction_model_id: Option<String>,
    pub correction_artifact_sha256: Option<String>,
    pub calibration_status: Option<crate::starlight::uv::CalibrationStatus>,
    pub model_response: Option<crate::starlight::uv::ModelResponse>,
    pub measured_conditional_residual_statistical_correlation: Option<f64>,
    pub systematic_correlation: Option<crate::starlight::uv::SystematicCorrelation>,
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
    statistical_uncertainty: f64,
    systematic_uncertainty: f64,
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
    expected_product_band: StarlightProductBand,
    expected_uv_artifact_sha256: Option<&str>,
) -> Result<Vec<Artifact>> {
    if (expected_product_band == StarlightProductBand::Combined300To650)
        != expected_uv_artifact_sha256.is_some()
    {
        bail!("configured Starlight band and UV artifact identity are inconsistent");
    }
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
        if shard.product_band != expected_product_band {
            bail!(
                "Starlight shard {} uses product band {:?}, expected current configured band {:?}",
                shard.partition_id,
                shard.product_band,
                expected_product_band
            );
        }
        let shard_uv_sha256 = shard
            .ultraviolet_correction
            .as_ref()
            .map(|metadata| metadata.artifact_sha256.as_str());
        if shard_uv_sha256 != expected_uv_artifact_sha256 {
            bail!(
                "Starlight shard {} uses UV artifact {:?}, expected current configured artifact {:?}",
                shard.partition_id,
                shard_uv_sha256,
                expected_uv_artifact_sha256
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
    let science_policy = science_policy_report(&merged);
    write_map(
        &map_path,
        canonical_nside,
        map_pixels(&merged),
        &science_policy.spectral_coverage,
    )?;
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
        ultraviolet_applicability: merged.ultraviolet_applicability.clone(),
        science_policy,
        band_diagnostics: band_diagnostics(&merged)?,
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
    validate_map_spectral_headers(map_path, &report.science_policy.spectral_coverage)?;
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
    let diagnostics = &report.band_diagnostics;
    let diagnostic_values = [
        diagnostics.total_flux_300_336_ph_m2_s,
        diagnostics.total_flux_336_650_ph_m2_s,
        diagnostics.total_flux_300_650_ph_m2_s,
        diagnostics.statistical_uncertainty_300_336_ph_m2_s,
        diagnostics.statistical_uncertainty_336_650_ph_m2_s,
        diagnostics.statistical_uncertainty_300_650_ph_m2_s,
        diagnostics.systematic_uncertainty_300_336_ph_m2_s,
        diagnostics.systematic_uncertainty_300_650_ph_m2_s,
    ];
    let selected_diagnostic = if report
        .science_policy
        .spectral_coverage
        .ultraviolet_correction_applied
    {
        diagnostics.total_flux_300_650_ph_m2_s
    } else {
        diagnostics.total_flux_336_650_ph_m2_s
    };
    let ultraviolet_count = report
        .ultraviolet_applicability
        .values()
        .try_fold(0_u64, |total, count| total.checked_add(*count))
        .context("report UV applicability count overflow")?;
    let ultraviolet_applied = report
        .science_policy
        .spectral_coverage
        .ultraviolet_correction_applied;
    if diagnostics.corrected_300_336_label != "300–336 nm corrected"
        || diagnostics.measured_336_650_label != "336–650 nm measured"
        || diagnostics.combined_300_650_label != "300–650 nm combined"
        || diagnostic_values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        || selected_diagnostic.to_bits() != total_flux.to_bits()
        || (ultraviolet_applied && ultraviolet_count != report.admitted_sources)
        || (!ultraviolet_applied
            && (diagnostics.total_flux_300_336_ph_m2_s != 0.0
                || diagnostics.statistical_uncertainty_300_336_ph_m2_s != 0.0
                || diagnostics.systematic_uncertainty_300_336_ph_m2_s != 0.0
                || !report.ultraviolet_applicability.is_empty()
                || diagnostics.total_flux_300_650_ph_m2_s.to_bits()
                    != diagnostics.total_flux_336_650_ph_m2_s.to_bits()))
    {
        bail!("merge report band diagnostics are invalid or inconsistent");
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
    let base_headers = observed_headers
        .iter()
        .filter(|(key, _)| expected_headers.contains_key(*key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let spectral_keys = [
        "product_band",
        "corrected_component",
        "measured_component",
        "combined_component",
        "uv_correction_model_id",
        "uv_correction_sha256",
        "uv_calibration_status",
        "uv_model_response",
        "uv_measured_conditional_residual_statistical_correlation",
        "uv_systematic_correlation",
    ];
    if base_headers != expected_headers
        || observed_headers.len() != expected_headers.len() + spectral_keys.len()
        || spectral_keys
            .iter()
            .any(|key| !observed_headers.contains_key(*key))
    {
        bail!(
            "{} has unknown or incompatible map headers: expected {:?}, found {:?}",
            path.display(),
            expected_headers,
            observed_headers
        );
    }
    let measured_contract = observed_headers.get("product_band").map(String::as_str)
        == Some("336-650-measured")
        && observed_headers
            .get("corrected_component")
            .map(String::as_str)
            == Some("not-applied")
        && observed_headers
            .get("measured_component")
            .map(String::as_str)
            == Some("336-650-measured")
        && observed_headers
            .get("combined_component")
            .map(String::as_str)
            == Some("not-produced")
        && [
            "uv_correction_model_id",
            "uv_correction_sha256",
            "uv_calibration_status",
            "uv_model_response",
            "uv_measured_conditional_residual_statistical_correlation",
            "uv_systematic_correlation",
        ]
        .iter()
        .all(|key| observed_headers.get(*key).map(String::as_str) == Some("none"));
    let corrected_contract = observed_headers.get("product_band").map(String::as_str)
        == Some("300-650-combined")
        && observed_headers
            .get("corrected_component")
            .map(String::as_str)
            == Some("300-336-corrected")
        && observed_headers
            .get("measured_component")
            .map(String::as_str)
            == Some("336-650-measured")
        && observed_headers
            .get("combined_component")
            .map(String::as_str)
            == Some("300-650-combined")
        && observed_headers
            .get("uv_correction_model_id")
            .is_some_and(|value| !value.trim().is_empty() && value != "none")
        && observed_headers
            .get("uv_correction_sha256")
            .is_some_and(|value| is_sha256(value))
        && observed_headers
            .get("uv_calibration_status")
            .map(String::as_str)
            == Some("validated")
        && matches!(
            observed_headers
                .get("uv_model_response")
                .map(String::as_str),
            Some("absolute-uv-photon-flux" | "natural-log-uv-to-measured-flux-ratio-336-650")
        )
        && observed_headers
            .get("uv_measured_conditional_residual_statistical_correlation")
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
        && matches!(
            observed_headers
                .get("uv_systematic_correlation")
                .map(String::as_str),
            Some("independent-between-sources" | "fully-correlated-between-sources")
        );
    if !measured_contract && !corrected_contract {
        bail!("{} has inconsistent spectral metadata", path.display());
    }

    let mut data_lines = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'));
    if data_lines.next()
        != Some(
            "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,admitted_sources,excluded_sources",
        )
    {
        bail!("{} has an incompatible map column schema", path.display());
    }
    let mut pixels = BTreeMap::new();
    let mut previous_pixel = None;
    for line in data_lines {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 6 {
            bail!("{} contains a malformed map row", path.display());
        }
        let pixel = fields[0].parse::<u32>()?;
        let flux = fields[1].parse::<f64>()?;
        let statistical_uncertainty = fields[2].parse::<f64>()?;
        let systematic_uncertainty = fields[3].parse::<f64>()?;
        let admitted = fields[4].parse::<u64>()?;
        let excluded = fields[5].parse::<u64>()?;
        if u64::from(pixel) >= 12 * u64::from(expected_nside).pow(2)
            || !flux.is_finite()
            || flux < 0.0
            || !statistical_uncertainty.is_finite()
            || statistical_uncertainty < 0.0
            || !systematic_uncertainty.is_finite()
            || systematic_uncertainty < 0.0
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
                statistical_uncertainty,
                systematic_uncertainty,
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
                    statistical_uncertainty: value.statistical_variance.value().sqrt(),
                    systematic_uncertainty: value.selected_systematic_uncertainty(),
                    admitted: value.admitted_sources,
                    excluded: value.excluded_sources,
                    ..MapPixel::default()
                },
            )
        })
        .collect()
}

fn science_policy_report(merged: &PartitionShard) -> SciencePolicyReport {
    let ultraviolet = merged.ultraviolet_correction.as_ref();
    let corrected = ultraviolet.is_some();
    SciencePolicyReport {
        schema_version: 2,
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
            policy_id: if corrected {
                CORRECTED_SPECTRAL_POLICY_ID
            } else {
                SPECTRAL_POLICY_ID
            }
            .to_string(),
            target_band_nm: [300, 650],
            directly_integrated_band_nm: [336, 650],
            corrected_band_nm: corrected.then_some([300, 336]),
            combined_band_nm: corrected.then_some([300, 650]),
            ultraviolet_correction_applied: corrected,
            correction_model_id: ultraviolet.map(|metadata| metadata.model_id.clone()),
            correction_artifact_sha256: ultraviolet
                .map(|metadata| metadata.artifact_sha256.clone()),
            calibration_status: ultraviolet.map(|metadata| metadata.calibration_status),
            model_response: ultraviolet.map(|metadata| metadata.response.clone()),
            measured_conditional_residual_statistical_correlation: ultraviolet.map(|metadata| {
                f64::from_bits(metadata.measured_conditional_residual_statistical_correlation_bits)
            }),
            systematic_correlation: ultraviolet
                .map(|metadata| metadata.systematic_correlation),
            limitation: if corrected {
                "The 300-336 nm contribution is model-corrected; the 336-650 nm Gaia XP integral remains unchanged and is retained separately.".to_string()
            } else {
                "The frozen GaiaXPy design begins at 336 nm; no independently calibrated 300-336 nm correction is applied.".to_string()
            },
        },
    }
}

fn band_diagnostics(merged: &PartitionShard) -> Result<BandDiagnosticsReport> {
    let mut flux_uv = 0.0;
    let mut flux_measured = 0.0;
    let mut flux_combined = 0.0;
    let mut statistical_uv_variance = 0.0;
    let mut statistical_measured_variance = 0.0;
    let mut statistical_combined_variance = 0.0;
    let mut systematic_independent_variance = 0.0;
    let mut systematic_correlated = 0.0;
    for pixel in merged.pixels.values() {
        flux_uv += pixel.flux_300_336_ph_m2_s.value();
        flux_measured += pixel.flux_336_650_ph_m2_s.value();
        flux_combined += pixel.flux_300_650_ph_m2_s.value();
        statistical_uv_variance += pixel.statistical_variance_300_336.value();
        statistical_measured_variance += pixel.statistical_variance_336_650.value();
        statistical_combined_variance += pixel.statistical_variance_300_650.value();
        systematic_independent_variance += pixel.systematic_variance_300_336_independent.value();
        systematic_correlated += pixel.systematic_uncertainty_300_336_correlated.value();
    }
    let values = [
        flux_uv,
        flux_measured,
        flux_combined,
        statistical_uv_variance,
        statistical_measured_variance,
        statistical_combined_variance,
        systematic_independent_variance,
        systematic_correlated,
    ];
    if values
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        bail!("Starlight band diagnostics contain invalid totals");
    }
    let systematic = systematic_independent_variance
        .sqrt()
        .hypot(systematic_correlated);
    Ok(BandDiagnosticsReport {
        corrected_300_336_label: "300–336 nm corrected".to_string(),
        measured_336_650_label: "336–650 nm measured".to_string(),
        combined_300_650_label: "300–650 nm combined".to_string(),
        total_flux_300_336_ph_m2_s: flux_uv,
        total_flux_336_650_ph_m2_s: flux_measured,
        total_flux_300_650_ph_m2_s: flux_combined,
        statistical_uncertainty_300_336_ph_m2_s: statistical_uv_variance.sqrt(),
        statistical_uncertainty_336_650_ph_m2_s: statistical_measured_variance.sqrt(),
        statistical_uncertainty_300_650_ph_m2_s: statistical_combined_variance.sqrt(),
        systematic_uncertainty_300_336_ph_m2_s: systematic,
        systematic_uncertainty_300_650_ph_m2_s: systematic,
    })
}

fn science_policy_is_declared(policy: &SciencePolicyReport) -> bool {
    let spectral = &policy.spectral_coverage;
    let spectral_valid = if spectral.ultraviolet_correction_applied {
        spectral.policy_id == CORRECTED_SPECTRAL_POLICY_ID
            && spectral.corrected_band_nm == Some([300, 336])
            && spectral.combined_band_nm == Some([300, 650])
            && spectral
                .correction_model_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            && spectral
                .correction_artifact_sha256
                .as_deref()
                .is_some_and(is_sha256)
            && spectral.calibration_status
                == Some(crate::starlight::uv::CalibrationStatus::Validated)
            && spectral.model_response.is_some()
            && spectral
                .measured_conditional_residual_statistical_correlation
                .is_some_and(|value| value.is_finite() && (-1.0..=1.0).contains(&value))
            && spectral.systematic_correlation.is_some()
    } else {
        spectral.policy_id == SPECTRAL_POLICY_ID
            && spectral.corrected_band_nm.is_none()
            && spectral.combined_band_nm.is_none()
            && spectral.correction_model_id.is_none()
            && spectral.correction_artifact_sha256.is_none()
            && spectral.calibration_status.is_none()
            && spectral.model_response.is_none()
            && spectral
                .measured_conditional_residual_statistical_correlation
                .is_none()
            && spectral.systematic_correlation.is_none()
    };
    policy.schema_version == 2
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
        && spectral.target_band_nm == [300, 650]
        && spectral.directly_integrated_band_nm == [336, 650]
        && spectral_valid
}

fn write_map(
    path: &Path,
    nside: u32,
    pixels: BTreeMap<u32, MapPixel>,
    spectral: &SpectralCoverageReport,
) -> Result<()> {
    let (
        product_band,
        corrected_component,
        combined_component,
        model_id,
        artifact_sha256,
        status,
        model_response,
        statistical_correlation,
        systematic_correlation,
    ) = if spectral.ultraviolet_correction_applied {
        (
            "300-650-combined",
            "300-336-corrected",
            "300-650-combined",
            spectral
                .correction_model_id
                .as_deref()
                .context("corrected map has no model ID")?,
            spectral
                .correction_artifact_sha256
                .as_deref()
                .context("corrected map has no artifact checksum")?,
            "validated",
            response_label(
                spectral
                    .model_response
                    .as_ref()
                    .context("corrected map has no model response")?,
            ),
            spectral
                .measured_conditional_residual_statistical_correlation
                .context("corrected map has no measured/conditional-residual statistical correlation")?
                .to_string(),
            match spectral
                .systematic_correlation
                .context("corrected map has no systematic correlation")?
            {
                crate::starlight::uv::SystematicCorrelation::IndependentBetweenSources => {
                    "independent-between-sources"
                }
                crate::starlight::uv::SystematicCorrelation::FullyCorrelatedBetweenSources => {
                    "fully-correlated-between-sources"
                }
            }
            .to_string(),
        )
    } else {
        (
            "336-650-measured",
            "not-applied",
            "not-produced",
            "none",
            "none",
            "none",
            "none",
            "none".to_string(),
            "none".to_string(),
        )
    };
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
         # product_band={product_band}\n\
         # corrected_component={corrected_component}\n\
         # measured_component=336-650-measured\n\
         # combined_component={combined_component}\n\
         # uv_correction_model_id={model_id}\n\
         # uv_correction_sha256={artifact_sha256}\n\
         # uv_calibration_status={status}\n\
         # uv_model_response={model_response}\n\
         # uv_measured_conditional_residual_statistical_correlation={statistical_correlation}\n\
         # uv_systematic_correlation={systematic_correlation}\n\
         pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n"
    );
    for (pixel, value) in pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{:.17e},{:.17e},{},{}\n",
            value.flux,
            value.statistical_uncertainty,
            value.systematic_uncertainty,
            value.admitted,
            value.excluded
        ));
    }
    artifact_store::atomic_write(path, text.as_bytes())
}

fn response_label(response: &crate::starlight::uv::ModelResponse) -> &'static str {
    match response {
        crate::starlight::uv::ModelResponse::AbsoluteUvPhotonFlux => "absolute-uv-photon-flux",
        crate::starlight::uv::ModelResponse::NaturalLogUvToMeasuredFluxRatio { .. } => {
            "natural-log-uv-to-measured-flux-ratio-336-650"
        }
    }
}

fn validate_map_spectral_headers(path: &Path, spectral: &SpectralCoverageReport) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let expected = if spectral.ultraviolet_correction_applied {
        vec![
            "# product_band=300-650-combined".to_string(),
            "# corrected_component=300-336-corrected".to_string(),
            "# measured_component=336-650-measured".to_string(),
            "# combined_component=300-650-combined".to_string(),
            format!(
                "# uv_correction_model_id={}",
                spectral
                    .correction_model_id
                    .as_deref()
                    .context("corrected report has no model ID")?
            ),
            format!(
                "# uv_correction_sha256={}",
                spectral
                    .correction_artifact_sha256
                    .as_deref()
                    .context("corrected report has no artifact checksum")?
            ),
            "# uv_calibration_status=validated".to_string(),
            format!(
                "# uv_model_response={}",
                response_label(
                    spectral
                        .model_response
                        .as_ref()
                        .context("corrected report has no model response")?
                )
            ),
            format!(
                "# uv_measured_conditional_residual_statistical_correlation={}",
                spectral
                    .measured_conditional_residual_statistical_correlation
                    .context("corrected report has no measured/conditional-residual statistical correlation")?
            ),
            format!(
                "# uv_systematic_correlation={}",
                match spectral
                    .systematic_correlation
                    .context("corrected report has no systematic correlation")?
                {
                    crate::starlight::uv::SystematicCorrelation::IndependentBetweenSources => {
                        "independent-between-sources"
                    }
                    crate::starlight::uv::SystematicCorrelation::FullyCorrelatedBetweenSources => {
                        "fully-correlated-between-sources"
                    }
                }
            ),
        ]
    } else {
        vec![
            "# product_band=336-650-measured".to_string(),
            "# corrected_component=not-applied".to_string(),
            "# measured_component=336-650-measured".to_string(),
            "# combined_component=not-produced".to_string(),
            "# uv_correction_model_id=none".to_string(),
            "# uv_correction_sha256=none".to_string(),
            "# uv_calibration_status=none".to_string(),
            "# uv_model_response=none".to_string(),
            "# uv_measured_conditional_residual_statistical_correlation=none".to_string(),
            "# uv_systematic_correlation=none".to_string(),
        ]
    };
    if expected
        .iter()
        .any(|header| !text.lines().any(|line| line == header))
    {
        bail!("canonical map spectral metadata does not match merge report");
    }
    Ok(())
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
                if left.flux_ph_m2_s != right.flux_ph_m2_s
                    || left.flux_300_336_ph_m2_s != right.flux_300_336_ph_m2_s
                    || left.flux_336_650_ph_m2_s != right.flux_336_650_ph_m2_s
                    || left.flux_300_650_ph_m2_s != right.flux_300_650_ph_m2_s
                {
                    flux_mismatches += 1;
                    record_first_mismatch(
                        &mut first_mismatch,
                        format!("pixel {pixel} flux accumulator differs"),
                    );
                }
                if left.statistical_variance != right.statistical_variance
                    || left.systematic_variance != right.systematic_variance
                    || left.systematic_correlated_uncertainty
                        != right.systematic_correlated_uncertainty
                    || left.statistical_variance_300_336 != right.statistical_variance_300_336
                    || left.statistical_variance_336_650 != right.statistical_variance_336_650
                    || left.statistical_variance_300_650 != right.statistical_variance_300_650
                    || left.systematic_variance_300_336_independent
                        != right.systematic_variance_300_336_independent
                    || left.systematic_uncertainty_300_336_correlated
                        != right.systematic_uncertainty_300_336_correlated
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
    bytes.extend_from_slice(b"nsb-starlight-complete-merge-v2\0");
    bytes.extend_from_slice(&shard.nside.to_be_bytes());
    bytes.push(match shard.product_band {
        crate::starlight::config::StarlightProductBand::Measured336To650 => 0,
        crate::starlight::config::StarlightProductBand::Combined300To650 => 1,
    });
    if let Some(metadata) = &shard.ultraviolet_correction {
        bytes.push(1);
        append_string(&mut bytes, &metadata.model_id)?;
        append_string(&mut bytes, &metadata.artifact_sha256)?;
        bytes.extend_from_slice(
            &metadata
                .measured_conditional_residual_statistical_correlation_bits
                .to_be_bytes(),
        );
        match metadata.response {
            crate::starlight::uv::ModelResponse::AbsoluteUvPhotonFlux => bytes.push(0),
            crate::starlight::uv::ModelResponse::NaturalLogUvToMeasuredFluxRatio {
                denominator_band_nm,
            } => {
                bytes.push(1);
                bytes.extend_from_slice(&denominator_band_nm[0].to_be_bytes());
                bytes.extend_from_slice(&denominator_band_nm[1].to_be_bytes());
            }
        }
        bytes.push(match metadata.systematic_correlation {
            crate::starlight::uv::SystematicCorrelation::IndependentBetweenSources => 0,
            crate::starlight::uv::SystematicCorrelation::FullyCorrelatedBetweenSources => 1,
        });
    } else {
        bytes.push(0);
    }
    let pixel_count = u64::try_from(shard.pixels.len()).context("pixel count exceeds u64")?;
    bytes.extend_from_slice(&pixel_count.to_be_bytes());
    for (pixel, accumulator) in &shard.pixels {
        bytes.extend_from_slice(&pixel.to_be_bytes());
        for sum in [
            &accumulator.flux_ph_m2_s,
            &accumulator.statistical_variance,
            &accumulator.systematic_variance,
            &accumulator.systematic_correlated_uncertainty,
            &accumulator.flux_300_336_ph_m2_s,
            &accumulator.flux_336_650_ph_m2_s,
            &accumulator.flux_300_650_ph_m2_s,
            &accumulator.statistical_variance_300_336,
            &accumulator.statistical_variance_336_650,
            &accumulator.statistical_variance_300_650,
            &accumulator.systematic_variance_300_336_independent,
            &accumulator.systematic_uncertainty_300_336_correlated,
        ] {
            sum.append_canonical_bytes(&mut bytes)?;
        }
        bytes.extend_from_slice(&accumulator.observed_sources.to_be_bytes());
        bytes.extend_from_slice(&accumulator.admitted_sources.to_be_bytes());
        bytes.extend_from_slice(&accumulator.excluded_sources.to_be_bytes());
    }
    let reason_count =
        u64::try_from(shard.exclusion_reasons.len()).context("reason count exceeds u64")?;
    bytes.extend_from_slice(&reason_count.to_be_bytes());
    for (reason, count) in &shard.exclusion_reasons {
        append_string(&mut bytes, reason)?;
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    let applicability_count = u64::try_from(shard.ultraviolet_applicability.len())
        .context("UV applicability count exceeds u64")?;
    bytes.extend_from_slice(&applicability_count.to_be_bytes());
    for (status, count) in &shard.ultraviolet_applicability {
        bytes.push(match status {
            crate::starlight::uv::ApplicabilityStatus::InDomain => 0,
            crate::starlight::uv::ApplicabilityStatus::Boundary => 1,
            crate::starlight::uv::ApplicabilityStatus::OutOfDomain => 2,
        });
        bytes.extend_from_slice(&count.to_be_bytes());
    }
    Ok(bytes)
}

fn append_string(bytes: &mut Vec<u8>, value: &str) -> Result<()> {
    let value = value.as_bytes();
    let length = u32::try_from(value.len()).context("canonical string length exceeds u32")?;
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
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
        let artifacts = emit_maps(
            temp.path(),
            &["fixture".to_string()],
            nside,
            StarlightProductBand::Measured336To650,
            None,
        )
        .unwrap();
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
        let map = fs::read_to_string(temp.path().join("outputs/starlight_nside128.csv")).unwrap();
        assert!(map.contains("# product_band=336-650-measured"));
        assert!(map.contains("# corrected_component=not-applied"));
        assert!(map.contains("# combined_component=not-produced"));
        assert!(map.contains("# uv_correction_model_id=none"));
        assert!(map.contains("# uv_correction_sha256=none"));
        assert!(map.contains("# uv_calibration_status=none"));
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
    fn validation_rejects_pre_uv_report_schema() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.schema_version = 5;
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
            .position(|line| {
                line
                    == "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,admitted_sources,excluded_sources"
            })
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
        let shard = fixture_shard(128);
        let policy = science_policy_report(&shard);
        assert_eq!(policy.schema_version, 2);
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
        assert!(policy.spectral_coverage.corrected_band_nm.is_none());
        assert!(policy.spectral_coverage.combined_band_nm.is_none());
        assert!(policy.spectral_coverage.correction_model_id.is_none());
        assert!(policy
            .spectral_coverage
            .correction_artifact_sha256
            .is_none());
        assert!(policy.spectral_coverage.calibration_status.is_none());
        assert!(policy.spectral_coverage.model_response.is_none());
        assert!(science_policy_is_declared(&policy));
    }

    #[test]
    fn corrected_map_metadata_and_uncertainty_budget_are_explicit() {
        let temp = TempDir::new().unwrap();
        let metadata = crate::starlight::map::accumulator::UvCorrectionShardMetadata {
            model_id: "SYNTHETIC-NON-PRODUCTION-MAP-TEST".to_string(),
            artifact_sha256: "a".repeat(64),
            calibration_status: crate::starlight::uv::CalibrationStatus::Validated,
            response: crate::starlight::uv::ModelResponse::AbsoluteUvPhotonFlux,
            measured_conditional_residual_statistical_correlation_bits: 0.25_f64.to_bits(),
            systematic_correlation:
                crate::starlight::uv::SystematicCorrelation::FullyCorrelatedBetweenSources,
        };
        let mut shard = PartitionShard::new_with_policy(
            "corrected-fixture",
            128,
            crate::starlight::config::StarlightProductBand::Combined300To650,
            Some(metadata),
        )
        .unwrap();
        let source = |uv, measured, systematic| crate::starlight::uv::CombinedBandFlux {
            flux_300_336_ph_m2_s: uv,
            flux_336_650_ph_m2_s: measured,
            flux_300_650_ph_m2_s: uv + measured,
            statistical_uncertainty_300_336_ph_m2_s: 1.0,
            statistical_uncertainty_336_650_ph_m2_s: 2.0,
            statistical_uncertainty_300_650_ph_m2_s: 2.5,
            systematic_uncertainty_300_336_ph_m2_s: systematic,
            systematic_uncertainty_300_650_ph_m2_s: systematic,
            applicability_status: crate::starlight::uv::ApplicabilityStatus::InDomain,
            decision: crate::starlight::uv::EvaluationDecision::Applied,
            model_id: "SYNTHETIC-NON-PRODUCTION-MAP-TEST".to_string(),
            artifact_sha256: "a".repeat(64),
            systematic_correlation:
                crate::starlight::uv::SystematicCorrelation::FullyCorrelatedBetweenSources,
        };
        shard.admit_corrected(0, &source(10.0, 100.0, 3.0)).unwrap();
        shard.admit_corrected(1, &source(20.0, 200.0, 4.0)).unwrap();
        let path = temp.path().join("outputs/shards/corrected-fixture.json");
        shard.write(&path).unwrap();

        emit_maps(
            temp.path(),
            &["corrected-fixture".to_string()],
            128,
            StarlightProductBand::Combined300To650,
            Some(&"a".repeat(64)),
        )
        .unwrap();
        let report: MergeReport = serde_json::from_slice(
            &fs::read(temp.path().join("outputs/merge_report.json")).unwrap(),
        )
        .unwrap();
        let spectral = &report.science_policy.spectral_coverage;
        assert!(spectral.ultraviolet_correction_applied);
        assert_eq!(spectral.corrected_band_nm, Some([300, 336]));
        assert_eq!(spectral.combined_band_nm, Some([300, 650]));
        assert_eq!(
            spectral.correction_model_id.as_deref(),
            Some("SYNTHETIC-NON-PRODUCTION-MAP-TEST")
        );
        assert_eq!(
            spectral.model_response,
            Some(crate::starlight::uv::ModelResponse::AbsoluteUvPhotonFlux)
        );
        assert_eq!(report.band_diagnostics.total_flux_300_336_ph_m2_s, 30.0);
        assert_eq!(report.band_diagnostics.total_flux_336_650_ph_m2_s, 300.0);
        assert_eq!(report.band_diagnostics.total_flux_300_650_ph_m2_s, 330.0);
        assert_eq!(
            report
                .band_diagnostics
                .systematic_uncertainty_300_336_ph_m2_s,
            7.0
        );
        assert_eq!(
            report
                .ultraviolet_applicability
                .get(&crate::starlight::uv::ApplicabilityStatus::InDomain),
            Some(&2)
        );
        let map = fs::read_to_string(temp.path().join("outputs/starlight_nside128.csv")).unwrap();
        assert!(map.contains("# corrected_component=300-336-corrected"));
        assert!(map.contains("# measured_component=336-650-measured"));
        assert!(map.contains("# combined_component=300-650-combined"));
        assert!(map.contains("# uv_model_response=absolute-uv-photon-flux"));
        assert!(map.contains(
            "# uv_measured_conditional_residual_statistical_correlation=0.25"
        ));
        assert!(map.contains("# uv_systematic_correlation=fully-correlated-between-sources"));
        validate_report(&temp.path().join("outputs/merge_report.json")).unwrap();
    }

    #[test]
    fn finalization_rejects_shards_from_a_stale_uv_configuration() {
        let temp = TempDir::new().unwrap();
        let metadata = crate::starlight::map::accumulator::UvCorrectionShardMetadata {
            model_id: "stale-model".to_string(),
            artifact_sha256: "a".repeat(64),
            calibration_status: crate::starlight::uv::CalibrationStatus::Validated,
            response: crate::starlight::uv::ModelResponse::AbsoluteUvPhotonFlux,
            measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
            systematic_correlation:
                crate::starlight::uv::SystematicCorrelation::IndependentBetweenSources,
        };
        let shard = PartitionShard::new_with_policy(
            "stale-fixture",
            128,
            StarlightProductBand::Combined300To650,
            Some(metadata),
        )
        .unwrap();
        shard
            .write(&temp.path().join("outputs/shards/stale-fixture.json"))
            .unwrap();

        let error = emit_maps(
            temp.path(),
            &["stale-fixture".to_string()],
            128,
            StarlightProductBand::Combined300To650,
            Some(&"b".repeat(64)),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("uses UV artifact"));
        assert!(error.contains("expected current configured artifact"));
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
