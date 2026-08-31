//! Reproducible Starlight HEALPix anomaly diagnostics for issue #116.
//!
//! The legacy candidate exhibited six large NSIDE=2-aligned flux patches when
//! equatorial `source_id` pixels were mislabelled as Galactic. This module
//! quantifies parent-cell discontinuities on any candidate map using the same
//! Galactic nested semantics as production.

use crate::starlight::healpix::nested_parent_at_coarser_nside;
use crate::starlight::map::accumulator::{merge_shards, PartitionShard};
use crate::starlight::validation::candidate_map::{self, CandidateMap, CandidatePixel};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

fn merged_candidate_map(shard: &PartitionShard) -> Result<CandidateMap> {
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
    use crate::starlight::pack::CANONICAL_CANDIDATE_SHA256;

    #[test]
    fn legacy_candidate_exhibits_six_nside2_anomalies() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidate = root.join("crates/nsb/data/starlight_nside128.csv");
        let report = analyse_candidate_path(&candidate, 128, Some(CANONICAL_CANDIDATE_SHA256))?;
        assert_eq!(report.pixel_count, 196_608);
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
}
