//! Reproducible Starlight HEALPix anomaly diagnostics for issue #116.
//!
//! Quantifies parent-cell discontinuities and HEALPix boundary jumps on any
//! candidate map using the same Galactic nested semantics as production.

pub mod baseline;
pub mod flux_attribution;
pub mod processor;

pub use baseline::{write_baseline_report, BaselineReport, SMOKE_PARTITIONS_PATH};
pub use flux_attribution::{
    load_partition_list, run_flux_attribution, FluxAttributionReport, FluxAttributionRow,
};
pub use processor::{
    run_diagnostic_suite, DiagnosticSuiteReport, PhotometricArtifactOverride, TRACE_PARENTS_SMOKE,
};

use crate::dataset::RunManifest;
use crate::platform::artifact_store;
use crate::platform::checksum_io;
use crate::starlight::healpix::{nested_neighbours, nested_parent_at_coarser_nside};
use crate::starlight::map::accumulator::{merge_shards, PartitionShard};
use crate::starlight::map::product::{reconstruct_selected_band_canonical_map_sha256, MergeReport};
use crate::starlight::validation::candidate_map::{self, CandidateMap, CandidatePixel};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const NSIDE2_PARENT_NSIDE: u32 = 2;

/// Quantitative summary for one NSIDE=2 parent cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParentCellMetrics {
    pub parent: u32,
    pub pixel_count: u64,
    pub total_flux_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub median_flux_per_admitted_source: f64,
}

/// Full anomaly report for a candidate map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealpixAnomalyReport {
    pub nside: u32,
    pub parent_nside: u32,
    pub pixel_count: u64,
    pub global_median_flux_per_admitted_source: f64,
    pub parent_cells: Vec<ParentCellMetrics>,
    pub anomalous_parents: Vec<u32>,
}

/// Boundary discontinuity metrics for a scalar sky map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryDiscontinuityReport {
    pub parent_nside: u32,
    pub median_internal_log_jump: f64,
    pub median_cross_parent_log_jump: f64,
    pub cross_to_internal_ratio: f64,
}

impl HealpixAnomalyReport {
    /// Parents whose median flux/admitted exceeds `threshold_ratio` times the
    /// global median across all NSIDE=2 parents.
    pub fn detect_anomalous_parents(&mut self, threshold_ratio: f64) {
        self.anomalous_parents = self
            .parent_cells
            .iter()
            .filter(|cell| {
                cell.median_flux_per_admitted_source
                    > threshold_ratio * self.global_median_flux_per_admitted_source
            })
            .map(|cell| cell.parent)
            .collect();
    }
}

/// Analyse a candidate map and return NSIDE=2 parent metrics.
pub fn analyse_candidate_map(candidate: &CandidateMap) -> Result<HealpixAnomalyReport> {
    let mut parents: BTreeMap<u32, ParentCellMetrics> = BTreeMap::new();
    for (pixel, value) in &candidate.pixels {
        let parent = nested_parent_at_coarser_nside(*pixel, candidate.nside, NSIDE2_PARENT_NSIDE)?;
        let entry = parents.entry(parent).or_insert_with(|| ParentCellMetrics {
            parent,
            pixel_count: 0,
            total_flux_ph_m2_s: 0.0,
            admitted_sources: 0,
            excluded_sources: 0,
            median_flux_per_admitted_source: 0.0,
        });
        entry.pixel_count += 1;
        entry.total_flux_ph_m2_s += value.flux_ph_m2_s;
        entry.admitted_sources += value.admitted_sources;
        entry.excluded_sources += value.excluded_sources;
    }

    let mut per_source_ratios = BTreeMap::<u32, Vec<f64>>::new();
    for (pixel, value) in &candidate.pixels {
        if value.admitted_sources == 0 {
            continue;
        }
        let parent = nested_parent_at_coarser_nside(*pixel, candidate.nside, NSIDE2_PARENT_NSIDE)?;
        per_source_ratios
            .entry(parent)
            .or_default()
            .push(value.flux_ph_m2_s / value.admitted_sources as f64);
    }

    let mut parent_cells = Vec::new();
    for (parent, mut metrics) in parents {
        if let Some(ratios) = per_source_ratios.get(&parent) {
            let mut sorted = ratios.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mid = sorted.len() / 2;
            metrics.median_flux_per_admitted_source = if sorted.len() % 2 == 0 {
                (sorted[mid - 1] + sorted[mid]) / 2.0
            } else {
                sorted[mid]
            };
        }
        parent_cells.push(metrics);
    }

    let mut medians: Vec<f64> = parent_cells
        .iter()
        .map(|cell| cell.median_flux_per_admitted_source)
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect();
    medians.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let global_median = if medians.is_empty() {
        0.0
    } else {
        medians[medians.len() / 2]
    };

    let mut report = HealpixAnomalyReport {
        nside: candidate.nside,
        parent_nside: NSIDE2_PARENT_NSIDE,
        pixel_count: candidate.pixels.len() as u64,
        global_median_flux_per_admitted_source: global_median,
        parent_cells,
        anomalous_parents: Vec::new(),
    };
    report.detect_anomalous_parents(5.0);
    Ok(report)
}

/// Merge all `workers/*/shard.json` under a workspace and analyse NSIDE=2 parents.
pub fn analyse_workspace_shards(workspace: &Path) -> Result<HealpixAnomalyReport> {
    let workers = workspace.join("workers");
    let mut shards = Vec::new();
    for entry in std::fs::read_dir(&workers)? {
        let entry = entry?;
        let shard_path = entry.path().join("shard.json");
        if shard_path.is_file() {
            let bytes = std::fs::read(&shard_path)?;
            shards.push(
                serde_json::from_slice::<PartitionShard>(&bytes)
                    .with_context(|| format!("parse shard {}", shard_path.display()))?,
            );
        }
    }
    anyhow::ensure!(!shards.is_empty(), "no shards under {}", workers.display());
    let merged = merge_shards(shards)?;
    analyse_candidate_map(&merged_candidate_map(&merged)?)
}

const COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT: &str = "combined-candidate-measured-subcomponent";

/// Export the measured-band subcomponent of a frozen *combined* candidate as a
/// diagnostic sparse CSV.
///
/// This is **not** a true Measured336To650 production product: admission
/// inherits the combined-product UV gate from the parent shards. Parent
/// identity is fail-closed against `outputs/merge_report.json` (partition
/// inventory + reconstructed selected-band canonical checksum).
/// Caller-supplied parent SHA / source commit are assertions only.
pub fn export_measured_336_650_from_shards(
    workspace: &Path,
    output: &Path,
    provenance_output: Option<&Path>,
    parent_combined_map_sha256: Option<&str>,
    source_commit: Option<&str>,
) -> Result<MeasuredBandExportReport> {
    let merge_report = load_frozen_merge_report(workspace)?;
    let verified_source_commit = resolve_verified_source_commit(workspace, source_commit)?;

    let mut shards = load_production_shards(workspace)?;
    shards.sort_by(|left, right| left.partition_id.cmp(&right.partition_id));
    let partition_ids: Vec<String> = shards
        .iter()
        .map(|shard| shard.partition_id.clone())
        .collect();
    verify_partition_inventory_against_merge_report(&partition_ids, &merge_report)?;

    let merged = merge_shards(shards)?;
    if merged.nside != merge_report.canonical_map.nside {
        bail!(
            "merged shard nside={} does not match frozen merge report nside={}",
            merged.nside,
            merge_report.canonical_map.nside
        );
    }
    let reconstructed_sha = reconstruct_selected_band_canonical_map_sha256(&merged)
        .context("reconstruct selected-band canonical map from frozen shards")?;
    if reconstructed_sha != merge_report.canonical_map.sha256 {
        bail!(
            "reconstructed selected-band canonical map sha256 {reconstructed_sha} does not match frozen merge report parent checksum {}",
            merge_report.canonical_map.sha256
        );
    }
    let verified_parent_sha = merge_report.canonical_map.sha256.clone();
    if let Some(expected) = parent_combined_map_sha256 {
        if expected != verified_parent_sha {
            bail!(
                "caller-supplied parent_combined_map_sha256={expected} does not match verified frozen parent checksum {verified_parent_sha}"
            );
        }
    }

    let nside = merged.nside;
    let shard_count = partition_ids.len();
    let mut pixels = BTreeMap::new();
    let mut total_flux = 0.0_f64;
    let mut admitted = 0_u64;
    let mut excluded = 0_u64;
    for (pixel, accumulator) in &merged.pixels {
        let flux = accumulator.flux_336_650_ph_m2_s.value();
        let statistical = accumulator.statistical_variance_336_650.value().sqrt();
        // Measured-band systematic on the combined product path is filed into the
        // selected-band buckets; for this diagnostic subcomponent export we report
        // statistical uncertainty from the measured sub-band variance and leave
        // systematic at 0 (explicitly documented in provenance).
        let systematic = 0.0_f64;
        total_flux += flux;
        admitted = admitted
            .checked_add(accumulator.admitted_sources)
            .context("admitted overflow")?;
        excluded = excluded
            .checked_add(accumulator.excluded_sources)
            .context("excluded overflow")?;
        pixels.insert(
            *pixel,
            CandidatePixel {
                flux_ph_m2_s: flux,
                statistical_uncertainty_ph_m2_s: statistical,
                systematic_uncertainty_ph_m2_s: systematic,
                total_uncertainty_ph_m2_s: statistical.hypot(systematic),
                admitted_sources: accumulator.admitted_sources,
                excluded_sources: accumulator.excluded_sources,
            },
        );
    }

    let mut text = format!(
        "# schema={}\n\
         # map_type=healpix\n\
         # coordinate_frame=galactic\n\
         # ordering=nested\n\
         # representation=sparse\n\
         # omitted_pixel_semantics=zero_flux_and_source_counts\n\
         # nside={nside}\n\
         # flux_quantity=integrated_per_pixel\n\
         # flux_unit=ph_m-2_s-1\n\
         # derivation={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}\n\
         # artifact_class={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}\n\
         # provisional=true\n\
         # source_count_semantics=exact_source_membership\n\
         # product_band=336-650-measured\n\
         # corrected_component=not-applied\n\
         # measured_component=336-650-from-combined-candidate\n\
         # combined_component=parent-combined-not-relabelled\n\
         # wavelength_min_nm=336\n\
         # wavelength_max_nm=650\n\
         # physical_quantity=photon_radiance_336_650_nm\n\
         # uv_correction_model_id=none\n\
         # uv_correction_sha256=none\n\
         # uv_calibration_status=none\n\
         # uv_model_response=none\n\
         # uv_measured_conditional_residual_statistical_correlation=none\n\
         # uv_systematic_correlation=none\n\
         pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
        candidate_map::EXPECTED_MAP_SCHEMA,
    );
    for (pixel, value) in &pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{:.17e},{:.17e},{:.17e},{},{}\n",
            value.flux_ph_m2_s,
            value.statistical_uncertainty_ph_m2_s,
            value.systematic_uncertainty_ph_m2_s,
            value.total_uncertainty_ph_m2_s,
            value.admitted_sources,
            value.excluded_sources
        ));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    artifact_store::atomic_write(output, text.as_bytes()).with_context(|| {
        format!(
            "write combined-candidate measured-subcomponent map {}",
            output.display()
        )
    })?;
    let sha256 = checksum_io::sha256_file(output)?;

    let report = MeasuredBandExportReport {
        schema_version: 2,
        experiment: "A_combined_candidate_measured_subcomponent_export".to_string(),
        issue: "182".to_string(),
        artifact_class: COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT.to_string(),
        provisional: true,
        workspace: workspace.display().to_string(),
        output_path: output.display().to_string(),
        output_sha256: sha256,
        nside,
        shard_count,
        partition_ids,
        occupied_pixels: pixels.len(),
        total_flux_336_650_ph_m2_s: total_flux,
        admitted_sources: admitted,
        excluded_sources: excluded,
        // Physical bandpass/quantity contract (loader identity). Admission
        // semantics remain those of the parent combined candidate — see
        // artifact_class / provisional / notes.
        product_band: "336-650-measured".to_string(),
        physical_quantity: "photon_radiance_336_650_nm".to_string(),
        wavelength_min_nm: 336,
        wavelength_max_nm: 650,
        parent_combined_map_sha256: Some(verified_parent_sha),
        source_commit: verified_source_commit,
        notes: vec![
            "Diagnostic combined-candidate-measured-subcomponent export; NOT a true Measured336To650 production candidate.".to_string(),
            "Admission inherits the parent combined-product UV gate from frozen shards; sources excluded for invalid_uv_predictors are absent here.".to_string(),
            "Flux column is flux_336_650_ph_m2_s from combined-product partition shards; systematic uncertainty is 0.0 on this diagnostic export.".to_string(),
            "Parent identity is the verified frozen merge_report canonical_map.sha256 after partition inventory and selected-band reconstruction checks.".to_string(),
        ],
    };

    if let Some(path) = provenance_output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(&report)?;
        std::fs::write(path, payload)
            .with_context(|| format!("write provenance {}", path.display()))?;
    }
    Ok(report)
}

/// Provenance for a diagnostic combined-candidate measured-subcomponent export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeasuredBandExportReport {
    pub schema_version: u32,
    pub experiment: String,
    pub issue: String,
    pub artifact_class: String,
    pub provisional: bool,
    pub workspace: String,
    pub output_path: String,
    pub output_sha256: String,
    pub nside: u32,
    pub shard_count: usize,
    pub partition_ids: Vec<String>,
    pub occupied_pixels: usize,
    pub total_flux_336_650_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub product_band: String,
    pub physical_quantity: String,
    pub wavelength_min_nm: u16,
    pub wavelength_max_nm: u16,
    /// Verified parent combined canonical map SHA-256 from the frozen merge report.
    pub parent_combined_map_sha256: Option<String>,
    /// Verified software commit from workspace RunManifest evidence, when present.
    pub source_commit: Option<String>,
    pub notes: Vec<String>,
}

fn load_frozen_merge_report(workspace: &Path) -> Result<MergeReport> {
    let report_path = workspace.join("outputs/merge_report.json");
    if !report_path.is_file() {
        bail!(
            "frozen merge report missing at {}; cannot verify combined-candidate provenance",
            report_path.display()
        );
    }
    let bytes = std::fs::read(&report_path)
        .with_context(|| format!("read frozen merge report {}", report_path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parse frozen merge report {}", report_path.display()))
}

fn verify_partition_inventory_against_merge_report(
    shard_partition_ids: &[String],
    merge_report: &MergeReport,
) -> Result<()> {
    if shard_partition_ids.len() != merge_report.shard_count
        || merge_report.partition_ids.len() != merge_report.shard_count
    {
        bail!(
            "shard inventory count mismatch: found {} shards, merge report shard_count={}, merge report partition_ids={}",
            shard_partition_ids.len(),
            merge_report.shard_count,
            merge_report.partition_ids.len()
        );
    }
    if shard_partition_ids != merge_report.partition_ids.as_slice() {
        let found: BTreeSet<_> = shard_partition_ids.iter().cloned().collect();
        let expected: BTreeSet<_> = merge_report.partition_ids.iter().cloned().collect();
        let missing: Vec<_> = expected.difference(&found).cloned().collect();
        let extra: Vec<_> = found.difference(&expected).cloned().collect();
        bail!(
            "shard partition inventory does not match frozen merge report (missing={missing:?}, extra={extra:?})"
        );
    }
    Ok(())
}

/// Discover software_commit from workspace RunManifest files under `runs/`.
///
/// Caller-supplied `expected` values are assertions against that evidence and
/// are never copied as provenance. Missing RunManifest evidence leaves
/// `source_commit` as `None` only when the caller did not assert one; the
/// frozen merge report remains the required parent-map identity.
fn resolve_verified_source_commit(
    workspace: &Path,
    expected: Option<&str>,
) -> Result<Option<String>> {
    let discovered = discover_workspace_software_commits(workspace)?;
    match (discovered.as_slice(), expected) {
        ([], None) => Ok(None),
        ([], Some(expected)) => bail!(
            "caller-supplied source_commit={expected} but workspace has no RunManifest software_commit evidence under runs/"
        ),
        ([commit], None) => Ok(Some(commit.clone())),
        ([commit], Some(expected)) => {
            if commit != expected {
                bail!(
                    "caller-supplied source_commit={expected} does not match verified workspace software_commit={commit}"
                );
            }
            Ok(Some(commit.clone()))
        }
        (commits, _) => bail!(
            "workspace has conflicting RunManifest software_commit values: {commits:?}"
        ),
    }
}

fn discover_workspace_software_commits(workspace: &Path) -> Result<Vec<String>> {
    let runs_root = workspace.join("runs");
    if !runs_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut commits = BTreeSet::new();
    let mut stack = vec![runs_root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) != Some("run.json") {
                continue;
            }
            let bytes = std::fs::read(&path)
                .with_context(|| format!("read run manifest {}", path.display()))?;
            let manifest: RunManifest = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse run manifest {}", path.display()))?;
            if !manifest.software_commit.is_empty() && manifest.software_commit != "unknown" {
                commits.insert(manifest.software_commit);
            }
        }
    }
    Ok(commits.into_iter().collect())
}

fn load_production_shards(workspace: &Path) -> Result<Vec<PartitionShard>> {
    let shard_root = workspace.join("outputs/shards");
    if shard_root.is_dir() {
        let mut paths: Vec<_> = std::fs::read_dir(&shard_root)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        anyhow::ensure!(
            !paths.is_empty(),
            "no shards under {}",
            shard_root.display()
        );
        let mut shards = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = std::fs::read(&path)?;
            shards.push(
                serde_json::from_slice::<PartitionShard>(&bytes)
                    .with_context(|| format!("parse shard {}", path.display()))?,
            );
        }
        return Ok(shards);
    }

    let workers = workspace.join("workers");
    anyhow::ensure!(
        workers.is_dir(),
        "workspace has neither outputs/shards nor workers/: {}",
        workspace.display()
    );
    let mut shards = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&workers)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let shard_path = entry.path().join("shard.json");
        if shard_path.is_file() {
            let bytes = std::fs::read(&shard_path)?;
            shards.push(
                serde_json::from_slice::<PartitionShard>(&bytes)
                    .with_context(|| format!("parse shard {}", shard_path.display()))?,
            );
        }
    }
    anyhow::ensure!(!shards.is_empty(), "no shards under {}", workers.display());
    Ok(shards)
}

/// Merge every `workers/*/shard.json` under `workspace` and write a sparse
/// candidate-v5 CSV suitable for diagnostic heatmaps.
pub fn export_workspace_candidate_map(workspace: &Path, output: &Path) -> Result<String> {
    let workers = workspace.join("workers");
    let mut shards = Vec::new();
    for entry in std::fs::read_dir(&workers)? {
        let entry = entry?;
        let shard_path = entry.path().join("shard.json");
        if shard_path.is_file() {
            let bytes = std::fs::read(&shard_path)?;
            shards.push(
                serde_json::from_slice::<PartitionShard>(&bytes)
                    .with_context(|| format!("parse shard {}", shard_path.display()))?,
            );
        }
    }
    anyhow::ensure!(!shards.is_empty(), "no shards under {}", workers.display());
    let merged = merge_shards(shards)?;
    let candidate = merged_candidate_map(&merged)?;
    write_candidate_map_csv(&candidate, output)?;
    checksum_io::sha256_file(output)
}

fn write_candidate_map_csv(candidate: &CandidateMap, path: &Path) -> Result<()> {
    let mut text = format!(
        "# schema={}\n\
         # map_type=healpix\n\
         # coordinate_frame=galactic\n\
         # ordering=nested\n\
         # representation=sparse\n\
         # omitted_pixel_semantics=zero_flux_and_source_counts\n\
         # nside={}\n\
         # flux_quantity=integrated_per_pixel\n\
         # flux_unit={}\n\
         pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
        candidate_map::EXPECTED_MAP_SCHEMA,
        candidate.nside,
        candidate.flux_unit
    );
    for (pixel, value) in &candidate.pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{:.17e},{:.17e},{:.17e},{},{}\n",
            value.flux_ph_m2_s,
            value.statistical_uncertainty_ph_m2_s,
            value.systematic_uncertainty_ph_m2_s,
            value.total_uncertainty_ph_m2_s,
            value.admitted_sources,
            value.excluded_sources
        ));
    }
    artifact_store::atomic_write(path, text.as_bytes())
        .with_context(|| format!("write candidate map {}", path.display()))
}

pub(crate) fn merged_candidate_map(shard: &PartitionShard) -> Result<CandidateMap> {
    let mut pixels = std::collections::BTreeMap::new();
    for (pixel, accumulator) in &shard.pixels {
        let statistical = accumulator.statistical_variance.value().sqrt();
        let systematic = accumulator
            .systematic_variance
            .value()
            .sqrt()
            .hypot(accumulator.systematic_correlated_uncertainty.value());
        pixels.insert(
            *pixel,
            CandidatePixel {
                flux_ph_m2_s: accumulator.flux_ph_m2_s.value(),
                statistical_uncertainty_ph_m2_s: statistical,
                systematic_uncertainty_ph_m2_s: systematic,
                total_uncertainty_ph_m2_s: statistical.hypot(systematic),
                admitted_sources: accumulator.admitted_sources,
                excluded_sources: accumulator.excluded_sources,
            },
        );
    }
    Ok(CandidateMap {
        nside: shard.nside,
        schema: crate::starlight::validation::candidate_map::EXPECTED_MAP_SCHEMA.to_string(),
        flux_unit: crate::starlight::validation::candidate_map::EXPECTED_FLUX_UNIT.to_string(),
        sha256: String::new(),
        pixels,
    })
}

/// Measure median |Δ log10(value)| across NSIDE parent boundaries vs inside parents.
pub fn boundary_discontinuity_report(
    candidate: &CandidateMap,
    parent_nside: u32,
) -> Result<BoundaryDiscontinuityReport> {
    let mut internal_jumps = Vec::new();
    let mut cross_jumps = Vec::new();
    let values: BTreeMap<u32, f64> = candidate
        .pixels
        .iter()
        .filter_map(|(pixel, value)| {
            if value.admitted_sources == 0 {
                return None;
            }
            let ratio = value.flux_ph_m2_s / value.admitted_sources as f64;
            if ratio > 0.0 && ratio.is_finite() {
                Some((*pixel, ratio))
            } else {
                None
            }
        })
        .collect();

    for (pixel, value) in &values {
        let parent = nested_parent_at_coarser_nside(*pixel, candidate.nside, parent_nside)?;
        let neighbours = nested_neighbours(candidate.nside, *pixel)?;
        for neighbour in neighbours {
            let Some(other) = values.get(&neighbour) else {
                continue;
            };
            let jump = log10_jump(*value, *other);
            let other_parent =
                nested_parent_at_coarser_nside(neighbour, candidate.nside, parent_nside)?;
            if parent == other_parent {
                internal_jumps.push(jump);
            } else {
                cross_jumps.push(jump);
            }
        }
    }

    let median_internal = median(&internal_jumps);
    let median_cross = median(&cross_jumps);
    let ratio = if median_internal > 0.0 {
        median_cross / median_internal
    } else {
        0.0
    };
    Ok(BoundaryDiscontinuityReport {
        parent_nside,
        median_internal_log_jump: median_internal,
        median_cross_parent_log_jump: median_cross,
        cross_to_internal_ratio: ratio,
    })
}

fn log10_jump(left: f64, right: f64) -> f64 {
    (left.max(1.0e-30).log10() - right.max(1.0e-30).log10()).abs()
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted[sorted.len() / 2]
}

/// Load a pinned candidate map and analyse its NSIDE=2 parent discontinuities.
pub fn analyse_candidate_path(
    path: &Path,
    expected_nside: u32,
    expected_sha256: Option<&str>,
) -> Result<HealpixAnomalyReport> {
    let candidate =
        candidate_map::load(path, expected_nside, expected_sha256).with_context(|| {
            format!(
                "load candidate map {} for HEALPix anomaly diagnostics",
                path.display()
            )
        })?;
    analyse_candidate_map(&candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{DatasetName, Executor, Operation, RunManifest, RunStatus};
    use crate::starlight::config::StarlightProductBand;
    use crate::starlight::healpix::test_support::fixture_icrs_from_source_id;
    use crate::starlight::map::accumulator::{PartitionShard, UvCorrectionShardMetadata};
    use crate::starlight::map::product::emit_maps;
    use crate::starlight::pack::{
        CANONICAL_CANDIDATE_SHA256, LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_PATH,
        LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_SHA256,
    };
    use crate::starlight::uv::{
        ApplicabilityStatus, CalibrationStatus, CombinedBandFlux, EvaluationDecision,
        ModelResponse, SystematicCorrelation,
    };
    use crate::starlight::validation::candidate_map::{CandidateMap, CandidatePixel};
    use std::fs;
    use tempfile::TempDir;

    const FIXTURE_NSIDE: u32 = 1;
    const FIXTURE_UV_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FIXTURE_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn uv_metadata() -> UvCorrectionShardMetadata {
        UvCorrectionShardMetadata {
            model_id: "fixture-combined-measured-subcomponent".to_string(),
            artifact_sha256: FIXTURE_UV_SHA.to_string(),
            calibration_status: CalibrationStatus::Validated,
            response: ModelResponse::AbsoluteUvPhotonFlux,
            measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
            systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
        }
    }

    fn combined_source(uv: f64, measured: f64) -> CombinedBandFlux {
        CombinedBandFlux {
            flux_300_336_ph_m2_s: uv,
            flux_336_650_ph_m2_s: measured,
            flux_300_650_ph_m2_s: uv + measured,
            statistical_uncertainty_300_336_ph_m2_s: 0.1,
            statistical_uncertainty_336_650_ph_m2_s: 0.2,
            statistical_uncertainty_300_650_ph_m2_s: 0.3,
            systematic_uncertainty_300_336_ph_m2_s: 0.0,
            systematic_uncertainty_300_650_ph_m2_s: 0.0,
            applicability_status: ApplicabilityStatus::InDomain,
            decision: EvaluationDecision::Applied,
            model_id: "fixture-combined-measured-subcomponent".to_string(),
            artifact_sha256: FIXTURE_UV_SHA.to_string(),
            systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
        }
    }

    fn write_combined_shard(
        workspace: &Path,
        partition_id: &str,
        source_id: u64,
    ) -> PartitionShard {
        let mut shard = PartitionShard::new_with_policy(
            partition_id,
            FIXTURE_NSIDE,
            StarlightProductBand::Combined300To650,
            Some(uv_metadata()),
        )
        .unwrap();
        shard
            .admit_corrected(
                fixture_icrs_from_source_id(source_id),
                &combined_source(1.0, 10.0),
            )
            .unwrap();
        let path = workspace
            .join("outputs/shards")
            .join(format!("{partition_id}.json"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        shard.write(&path).unwrap();
        shard
    }

    fn frozen_combined_workspace() -> (TempDir, String) {
        let temp = TempDir::new().unwrap();
        write_combined_shard(temp.path(), "part-a", 0);
        write_combined_shard(temp.path(), "part-b", 1_u64 << 47);
        emit_maps(
            temp.path(),
            &["part-a".to_string(), "part-b".to_string()],
            FIXTURE_NSIDE,
            StarlightProductBand::Combined300To650,
            Some(FIXTURE_UV_SHA),
            None,
        )
        .unwrap();
        let merge: MergeReport = serde_json::from_slice(
            &fs::read(temp.path().join("outputs/merge_report.json")).unwrap(),
        )
        .unwrap();
        write_run_manifest(temp.path(), FIXTURE_COMMIT);
        (temp, merge.canonical_map.sha256)
    }

    fn write_run_manifest(workspace: &Path, software_commit: &str) {
        let path = workspace
            .join("runs")
            .join("starlight")
            .join("fixture-run")
            .join("run.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let manifest = RunManifest {
            schema_version: 2,
            run_id: "fixture-run".to_string(),
            dataset: DatasetName::Starlight,
            operation: Operation::Validate,
            executor: Executor::Local,
            status: RunStatus::Complete,
            config_path: workspace.join("config.toml"),
            config_sha256: "b".repeat(64),
            software_commit: software_commit.to_string(),
            resolved_workspace: workspace.to_path_buf(),
            partitions: vec!["part-a".to_string(), "part-b".to_string()],
            artifacts: Vec::new(),
            validation_report: None,
            slurm_job_id: None,
            error: None,
        };
        fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }

    #[test]
    fn measured_subcomponent_export_succeeds_for_matching_frozen_workspace() -> Result<()> {
        let (temp, parent_sha) = frozen_combined_workspace();
        let output = temp.path().join("measured-subcomponent.csv");
        let provenance = temp.path().join("provenance.json");
        let report = export_measured_336_650_from_shards(
            temp.path(),
            &output,
            Some(&provenance),
            Some(&parent_sha),
            Some(FIXTURE_COMMIT),
        )?;
        assert!(output.is_file());
        assert_eq!(
            report.artifact_class,
            COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT
        );
        assert!(report.provisional);
        assert_eq!(report.product_band, "336-650-measured");
        assert_eq!(
            report.parent_combined_map_sha256.as_deref(),
            Some(parent_sha.as_str())
        );
        assert_eq!(report.source_commit.as_deref(), Some(FIXTURE_COMMIT));
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("NOT a true Measured336To650")));
        let csv = fs::read_to_string(&output)?;
        assert!(csv.contains(&format!(
            "# derivation={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}"
        )));
        assert!(csv.contains(&format!(
            "# artifact_class={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}"
        )));
        assert!(csv.contains("# product_band=336-650-measured"));
        Ok(())
    }

    #[test]
    fn measured_subcomponent_export_rejects_wrong_parent_sha() -> Result<()> {
        let (temp, parent_sha) = frozen_combined_workspace();
        let output = temp.path().join("out.csv");
        let err = export_measured_336_650_from_shards(
            temp.path(),
            &output,
            None,
            Some("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"),
            Some(FIXTURE_COMMIT),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("caller-supplied parent_combined_map_sha256"),
            "unexpected error: {err}"
        );
        assert!(err.contains(&parent_sha) || err.contains("does not match verified"));
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn measured_subcomponent_export_rejects_missing_shard() -> Result<()> {
        let (temp, parent_sha) = frozen_combined_workspace();
        fs::remove_file(temp.path().join("outputs/shards/part-b.json"))?;
        let output = temp.path().join("out.csv");
        let err = export_measured_336_650_from_shards(
            temp.path(),
            &output,
            None,
            Some(&parent_sha),
            Some(FIXTURE_COMMIT),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("missing") || err.contains("inventory"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[test]
    fn measured_subcomponent_export_rejects_extra_shard() -> Result<()> {
        let (temp, parent_sha) = frozen_combined_workspace();
        write_combined_shard(temp.path(), "part-extra", 2_u64 << 47);
        let output = temp.path().join("out.csv");
        let err = export_measured_336_650_from_shards(
            temp.path(),
            &output,
            None,
            Some(&parent_sha),
            Some(FIXTURE_COMMIT),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("extra") || err.contains("inventory") || err.contains("count mismatch"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[test]
    fn measured_subcomponent_export_rejects_wrong_partition_set() -> Result<()> {
        let (temp, parent_sha) = frozen_combined_workspace();
        let report_path = temp.path().join("outputs/merge_report.json");
        let mut merge: MergeReport = serde_json::from_slice(&fs::read(&report_path)?)?;
        merge.partition_ids = vec!["part-a".to_string(), "part-other".to_string()];
        fs::write(&report_path, serde_json::to_vec_pretty(&merge)?)?;
        let output = temp.path().join("out.csv");
        let err = export_measured_336_650_from_shards(
            temp.path(),
            &output,
            None,
            Some(&parent_sha),
            Some(FIXTURE_COMMIT),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("partition") || err.contains("inventory") || err.contains("missing"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[test]
    fn legacy_candidate_exhibits_six_nside2_anomalies() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = root.join(LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_PATH);
        let report = analyse_candidate_path(
            &candidate,
            128,
            Some(LEGACY_HEALPIX_ANOMALY_REGRESSION_FIXTURE_SHA256),
        )?;
        assert_eq!(report.pixel_count, 48);
        assert!(
            report.anomalous_parents.len() >= 6,
            "expected at least six anomalous NSIDE=2 parents, got {:?}",
            report.anomalous_parents
        );
        for parent in [0_u32, 16, 18, 26, 27, 43] {
            assert!(
                report.anomalous_parents.contains(&parent),
                "parent {parent} should be anomalous in the legacy candidate"
            );
        }
        Ok(())
    }

    #[test]
    fn corrected_candidate_does_not_reproduce_legacy_six_parent_anomalies() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = root.join("crates/nsb/data/starlight_nside128.csv");
        let report = analyse_candidate_path(&candidate, 128, Some(CANONICAL_CANDIDATE_SHA256))?;
        let legacy_six = [0_u32, 16, 18, 26, 27, 43];
        let legacy_anomalous: Vec<_> = legacy_six
            .into_iter()
            .filter(|parent| report.anomalous_parents.contains(parent))
            .collect();
        assert!(
            legacy_anomalous.len() <= 1,
            "expected at most one legacy parent still anomalous after frame fix, got {legacy_anomalous:?} (all anomalous parents: {:?})",
            report.anomalous_parents
        );
        Ok(())
    }

    #[test]
    fn corrected_candidate_reports_boundary_discontinuity_metrics() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = root.join("crates/nsb/data/starlight_nside128.csv");
        let map = candidate_map::load(&candidate, 128, Some(CANONICAL_CANDIDATE_SHA256))?;
        let report = boundary_discontinuity_report(&map, NSIDE2_PARENT_NSIDE)?;
        assert!(report.median_internal_log_jump.is_finite());
        assert!(report.median_cross_parent_log_jump.is_finite());
        assert!(report.cross_to_internal_ratio.is_finite());
        Ok(())
    }

    #[test]
    fn smoke_workspace_anomaly_report_when_configured() -> Result<()> {
        let Some(workspace) = std::env::var_os("NSB_STARLIGHT_SMOKE_WORKSPACE") else {
            return Ok(());
        };
        let workspace = Path::new(&workspace);
        let report = analyse_workspace_shards(workspace)?;
        let merged = crate::starlight::map::accumulator::merge_shards(
            std::fs::read_dir(workspace.join("workers"))?
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let shard_path = entry.path().join("shard.json");
                    if !shard_path.is_file() {
                        return None;
                    }
                    let bytes = std::fs::read(&shard_path).ok()?;
                    serde_json::from_slice(&bytes).ok()
                })
                .collect::<Vec<PartitionShard>>(),
        )?;
        let mut pixels = std::collections::BTreeMap::new();
        for (pixel, accumulator) in &merged.pixels {
            if accumulator.admitted_sources == 0 {
                continue;
            }
            pixels.insert(
                *pixel,
                CandidatePixel {
                    flux_ph_m2_s: accumulator.flux_ph_m2_s.value(),
                    statistical_uncertainty_ph_m2_s: 0.0,
                    systematic_uncertainty_ph_m2_s: 0.0,
                    total_uncertainty_ph_m2_s: 0.0,
                    admitted_sources: accumulator.admitted_sources,
                    excluded_sources: accumulator.excluded_sources,
                },
            );
        }
        let map = CandidateMap {
            nside: merged.nside,
            schema: "smoke".to_string(),
            flux_unit: "ph/m2/s".to_string(),
            sha256: String::new(),
            pixels,
        };
        let boundary = boundary_discontinuity_report(&map, NSIDE2_PARENT_NSIDE)?;
        eprintln!(
            "smoke workspace {:?}: anomalous_parents={:?} boundary_ratio={:.4}",
            workspace, report.anomalous_parents, boundary.cross_to_internal_ratio
        );
        Ok(())
    }

    fn fixture_measured_shard(nside: u32) -> PartitionShard {
        use crate::starlight::healpix::test_support::fixture_icrs_from_source_id;
        let mut shard = PartitionShard::new("fixture-export", nside).unwrap();
        shard
            .admit(fixture_icrs_from_source_id(0), 1.5e-3, 1.0e-5, 0.0)
            .unwrap();
        shard
            .admit(fixture_icrs_from_source_id(1_u64 << 47), 2.25, 0.01, 0.0)
            .unwrap();
        shard
            .exclude(
                fixture_icrs_from_source_id(2_u64 << 47),
                "fixture_exclusion",
            )
            .unwrap();
        shard
    }

    #[test]
    fn export_measured_336_650_from_outputs_shards() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let shard = fixture_measured_shard(1);
        let shard_dir = workspace.join("outputs/shards");
        std::fs::create_dir_all(&shard_dir)?;
        shard.write(&shard_dir.join("fixture-export.json"))?;
        emit_maps(
            workspace,
            &["fixture-export".to_string()],
            1,
            StarlightProductBand::Measured336To650,
            None,
            None,
        )?;
        let merge: MergeReport =
            serde_json::from_slice(&std::fs::read(workspace.join("outputs/merge_report.json"))?)?;
        let output = workspace.join("measured-336-650.csv");
        let provenance = workspace.join("measured-336-650.provenance.json");
        let report = export_measured_336_650_from_shards(
            workspace,
            &output,
            Some(&provenance),
            Some(&merge.canonical_map.sha256),
            None,
        )?;
        assert!(output.is_file());
        assert!(provenance.is_file());
        assert_eq!(report.shard_count, 1);
        assert!(report.occupied_pixels >= 1);
        assert_eq!(report.admitted_sources, 2);
        assert_eq!(report.excluded_sources, 1);
        assert_eq!(report.product_band, "336-650-measured");
        assert_eq!(
            report.parent_combined_map_sha256.as_deref(),
            Some(merge.canonical_map.sha256.as_str())
        );
        let text = std::fs::read_to_string(&output)?;
        assert!(text.contains("product_band=336-650-measured"));
        assert!(text.contains(&format!(
            "artifact_class={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}"
        )));
        assert!(text.contains(&format!(
            "derivation={COMBINED_CANDIDATE_MEASURED_SUBCOMPONENT}"
        )));
        let loaded: MeasuredBandExportReport =
            serde_json::from_slice(&std::fs::read(&provenance)?)?;
        assert_eq!(loaded.output_sha256, report.output_sha256);
        Ok(())
    }

    #[test]
    fn export_measured_336_650_from_workers_fallback() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let shard = fixture_measured_shard(1);
        let shard_dir = workspace.join("outputs/shards");
        std::fs::create_dir_all(&shard_dir)?;
        shard.write(&shard_dir.join("fixture-export.json"))?;
        emit_maps(
            workspace,
            &["fixture-export".to_string()],
            1,
            StarlightProductBand::Measured336To650,
            None,
            None,
        )?;
        // Move shards to workers/ and drop outputs/shards so the loader falls back.
        let worker_dir = workspace.join("workers").join("fixture-export");
        std::fs::create_dir_all(&worker_dir)?;
        std::fs::rename(
            shard_dir.join("fixture-export.json"),
            worker_dir.join("shard.json"),
        )?;
        std::fs::remove_dir_all(&shard_dir)?;
        let merge: MergeReport =
            serde_json::from_slice(&std::fs::read(workspace.join("outputs/merge_report.json"))?)?;
        let output = workspace.join("out.csv");
        let report = export_measured_336_650_from_shards(
            workspace,
            &output,
            None,
            Some(&merge.canonical_map.sha256),
            None,
        )?;
        assert_eq!(report.shard_count, 1);
        assert!(report.total_flux_336_650_ph_m2_s > 0.0);
        Ok(())
    }

    #[test]
    fn export_measured_336_650_fails_closed_without_shards() {
        let temporary = tempfile::tempdir().unwrap();
        let err = export_measured_336_650_from_shards(
            temporary.path(),
            &temporary.path().join("out.csv"),
            None,
            None,
            None,
        )
        .expect_err("empty workspace must fail");
        let message = format!("{err:#}");
        assert!(
            message.contains("frozen merge report missing")
                || message.contains("neither outputs/shards nor workers")
                || message.contains("no shards"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn export_measured_336_650_fails_closed_on_empty_outputs_shards() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("outputs/shards")).unwrap();
        let err = export_measured_336_650_from_shards(
            temporary.path(),
            &temporary.path().join("out.csv"),
            None,
            None,
            None,
        )
        .expect_err("workspace without merge report must fail closed");
        assert!(
            format!("{err:#}").contains("frozen merge report missing")
                || format!("{err:#}").contains("no shards"),
            "unexpected error: {err:#}"
        );
    }
}
