//! Machine-readable baseline reports for issue #116 smoke reproducibility.

use super::{
    analyse_workspace_shards, boundary_discontinuity_report, merged_candidate_map,
    HealpixAnomalyReport, BoundaryDiscontinuityReport,
};
use crate::dataset::RunConfig;
use crate::platform::checksum_io;
use crate::starlight::map::accumulator::{merge_shards, PartitionShard};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const SMOKE_PARTITIONS_PATH: &str =
    "docs/nsb_components/starlight/diagnostics/smoke-partitions-48.txt";

/// Frozen baseline metrics for a smoke workspace before further scientific changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineReport {
    pub phase: String,
    pub commit: String,
    pub config_path: String,
    pub config_sha256: String,
    pub workspace: String,
    pub partition_count: usize,
    pub partitions: Vec<String>,
    pub selection_artifact_sha256: Option<String>,
    pub photometric_artifact_sha256: Option<String>,
    pub uv_artifact_sha256: Option<String>,
    pub full_sky_anomalous_parents_reference: Vec<u32>,
    pub anomaly_report: HealpixAnomalyReport,
    pub boundary_report: BoundaryDiscontinuityReport,
    pub global_exclusion_reasons: BTreeMap<String, u64>,
    pub flux_metrics: FluxMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluxMetrics {
    pub total_flux_ph_m2_s: f64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub observed_sources: u64,
    pub global_median_flux_per_admitted: f64,
}

pub fn load_smoke_partitions(repo_root: &Path) -> Result<Vec<String>> {
    let path = repo_root.join(SMOKE_PARTITIONS_PATH);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read smoke partition list {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

pub fn write_baseline_report(
    repo_root: &Path,
    config_path: &Path,
    workspace: &Path,
    commit: &str,
    output_path: &Path,
    full_sky_anomalous_parents: &[u32],
) -> Result<BaselineReport> {
    let config_bytes = std::fs::read(config_path)?;
    let config: RunConfig = toml::from_slice(&config_bytes)
        .with_context(|| format!("parse config {}", config_path.display()))?;
    let starlight = config
        .starlight
        .as_ref()
        .context("config is not a Starlight run")?;
    let partitions = load_smoke_partitions(repo_root)?;
    let shards = load_workspace_shards(workspace, &partitions)?;
    let merged = merge_shards(shards)?;
    let candidate = merged_candidate_map(&merged)?;
    let anomaly_report = analyse_workspace_shards(workspace)?;
    let boundary_report = boundary_discontinuity_report(&candidate, 2)?;
    let mut global_exclusion_reasons = BTreeMap::new();
    let mut total_flux = 0.0;
    let mut admitted = 0_u64;
    let mut excluded = 0_u64;
    let mut observed = 0_u64;
    for (_, pixel) in &merged.pixels {
        total_flux += pixel.flux_ph_m2_s.value();
        admitted += pixel.admitted_sources;
        excluded += pixel.excluded_sources;
        observed += pixel.observed_sources;
    }
    for entry in std::fs::read_dir(workspace.join("workers"))? {
        let shard_path = entry?.path().join("shard.json");
        if !shard_path.is_file() {
            continue;
        }
        let shard: PartitionShard = serde_json::from_slice(&std::fs::read(&shard_path)?)?;
        for (reason, count) in shard.exclusion_reasons {
            *global_exclusion_reasons.entry(reason).or_default() += count;
        }
    }
    let global_median = anomaly_report.global_median_flux_per_admitted_source;
    let report = BaselineReport {
        phase: "phase0_baseline".to_string(),
        commit: commit.to_string(),
        config_path: config_path.display().to_string(),
        config_sha256: checksum_io::sha256_bytes(&config_bytes),
        workspace: workspace.display().to_string(),
        partition_count: partitions.len(),
        partitions,
        selection_artifact_sha256: starlight
            .selection_function
            .as_ref()
            .map(|pin| pin.sha256.clone()),
        photometric_artifact_sha256: starlight
            .photometric_inference
            .as_ref()
            .map(|pin| pin.sha256.clone()),
        uv_artifact_sha256: starlight
            .ultraviolet_correction
            .as_ref()
            .map(|pin| pin.sha256.clone()),
        full_sky_anomalous_parents_reference: full_sky_anomalous_parents.to_vec(),
        anomaly_report,
        boundary_report,
        global_exclusion_reasons,
        flux_metrics: FluxMetrics {
            total_flux_ph_m2_s: total_flux,
            admitted_sources: admitted,
            excluded_sources: excluded,
            observed_sources: observed,
            global_median_flux_per_admitted: global_median,
        },
    };
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

fn load_workspace_shards(workspace: &Path, partitions: &[String]) -> Result<Vec<PartitionShard>> {
    let mut shards = Vec::new();
    for partition in partitions {
        let shard_path = workspace.join("workers").join(partition).join("shard.json");
        let bytes = std::fs::read(&shard_path).with_context(|| {
            format!(
                "read shard {} for baseline",
                shard_path.display()
            )
        })?;
        shards.push(serde_json::from_slice(&bytes)?);
    }
    Ok(shards)
}

pub(crate) fn merged_candidate_map_from_shard(
    shard: &PartitionShard,
) -> Result<crate::starlight::validation::candidate_map::CandidateMap> {
    merged_candidate_map(shard)
}
