//! Per-partition reconciliation scaffold for Gaia DR3 XP continuous bulk production.
//!
//! Writes partition manifests and a rolling ledger under the USB reconciliation
//! directory. Population close (184.7M) is deferred; this module tracks per-file
//! source accounting and HEALPix accumulator totals that can scale to full bulk.

use crate::gaia_storage_preflight::XP_CONTINUOUS_ONLY_POPULATION;
use crate::gaia_xp_continuous_healpix::XpContinuousHealpixAccumulator;
use crate::gaia_xp_continuous_pilot_io::atomic_write_json;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PARTITION_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const LEDGER_FILENAME: &str = "bulk_reconciliation_ledger.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PartitionSourceCounts {
    pub rows_scanned: u64,
    pub rows_valid: u64,
    pub rows_excluded: u64,
    pub rows_failed: u64,
    pub processed_unique: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PartitionAccumulatorTotals {
    pub healpix_checksum: String,
    pub nside: u32,
    pub occupied_pixels: u64,
    pub source_count: u64,
    pub valid_source_count: u64,
    pub excluded_source_count: u64,
    pub sum_flux: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionReconciliationManifest {
    pub schema_version: u32,
    pub partition_filename: String,
    pub bulk_checksum: String,
    pub cache_uuid: String,
    pub processed_at_utc: String,
    pub source_counts: PartitionSourceCounts,
    pub accumulator: PartitionAccumulatorTotals,
    pub reconciliation_ok: bool,
    pub population_target_sources: u64,
    pub population_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BulkReconciliationLedger {
    pub schema_version: u32,
    pub cache_uuid: String,
    pub updated_at_utc: String,
    pub population_target_sources: u64,
    pub population_status: String,
    pub partitions_complete: u32,
    pub partitions_pending_estimate: u32,
    pub cumulative: PartitionSourceCounts,
    pub cumulative_accumulator: PartitionAccumulatorTotals,
    pub partition_manifests: Vec<String>,
}

pub fn partition_manifest_path(reconciliation_dir: &Path, filename: &str) -> PathBuf {
    let stem = filename.trim_end_matches(".csv.gz");
    reconciliation_dir.join(format!("{stem}.reconciliation.json"))
}

pub fn accumulator_totals_from_healpix(
    accumulator: &XpContinuousHealpixAccumulator,
) -> PartitionAccumulatorTotals {
    let totals = accumulator.totals();
    let occupied_pixels = accumulator
        .pixels
        .iter()
        .filter(|pixel| pixel.source_count > 0)
        .count() as u64;
    PartitionAccumulatorTotals {
        healpix_checksum: accumulator.checksum(),
        nside: accumulator.nside,
        occupied_pixels,
        source_count: totals.source_count,
        valid_source_count: totals.valid_source_count,
        excluded_source_count: totals.excluded_source_count,
        sum_flux: totals.sum_flux,
    }
}

pub fn build_partition_manifest(
    partition_filename: &str,
    bulk_checksum: &str,
    cache_uuid: &str,
    source_counts: PartitionSourceCounts,
    accumulator: PartitionAccumulatorTotals,
    reconciliation_ok: bool,
) -> PartitionReconciliationManifest {
    PartitionReconciliationManifest {
        schema_version: PARTITION_MANIFEST_SCHEMA_VERSION,
        partition_filename: partition_filename.to_string(),
        bulk_checksum: bulk_checksum.to_string(),
        cache_uuid: cache_uuid.to_string(),
        processed_at_utc: crate::gaia_usb_cache::utc_now_rfc3339(),
        source_counts,
        accumulator,
        reconciliation_ok,
        population_target_sources: XP_CONTINUOUS_ONLY_POPULATION,
        population_status: "partition_complete_population_todo".to_string(),
    }
}

pub fn write_partition_manifest(
    reconciliation_dir: &Path,
    manifest: &PartitionReconciliationManifest,
) -> Result<PathBuf> {
    fs::create_dir_all(reconciliation_dir)?;
    let path = partition_manifest_path(reconciliation_dir, &manifest.partition_filename);
    atomic_write_json(&path, &(serde_json::to_string_pretty(manifest)? + "\n"))?;
    Ok(path)
}

pub fn load_or_init_ledger(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partitions_pending_estimate: u32,
) -> Result<BulkReconciliationLedger> {
    fs::create_dir_all(reconciliation_dir)?;
    let path = reconciliation_dir.join(LEDGER_FILENAME);
    if path.is_file() {
        let ledger: BulkReconciliationLedger = serde_json::from_str(&fs::read_to_string(&path)?)
            .with_context(|| {
                format!(
                    "failed to parse reconciliation ledger at {}",
                    path.display()
                )
            })?;
        return Ok(ledger);
    }
    Ok(BulkReconciliationLedger {
        schema_version: 1,
        cache_uuid: cache_uuid.to_string(),
        updated_at_utc: crate::gaia_usb_cache::utc_now_rfc3339(),
        population_target_sources: XP_CONTINUOUS_ONLY_POPULATION,
        population_status: "accumulating_partitions".to_string(),
        partitions_complete: 0,
        partitions_pending_estimate,
        cumulative: PartitionSourceCounts::default(),
        cumulative_accumulator: PartitionAccumulatorTotals::default(),
        partition_manifests: Vec::new(),
    })
}

pub fn merge_partition_into_ledger(
    ledger: &mut BulkReconciliationLedger,
    manifest: &PartitionReconciliationManifest,
    manifest_path: &Path,
) {
    ledger.cache_uuid = manifest.cache_uuid.clone();
    ledger.updated_at_utc = manifest.processed_at_utc.clone();
    ledger.partitions_complete += 1;
    ledger.cumulative.rows_scanned += manifest.source_counts.rows_scanned;
    ledger.cumulative.rows_valid += manifest.source_counts.rows_valid;
    ledger.cumulative.rows_excluded += manifest.source_counts.rows_excluded;
    ledger.cumulative.rows_failed += manifest.source_counts.rows_failed;
    ledger.cumulative.processed_unique += manifest.source_counts.processed_unique;
    ledger.cumulative_accumulator.healpix_checksum = manifest.accumulator.healpix_checksum.clone();
    ledger.cumulative_accumulator.nside = manifest.accumulator.nside;
    ledger.cumulative_accumulator.occupied_pixels += manifest.accumulator.occupied_pixels;
    ledger.cumulative_accumulator.source_count += manifest.accumulator.source_count;
    ledger.cumulative_accumulator.valid_source_count += manifest.accumulator.valid_source_count;
    ledger.cumulative_accumulator.excluded_source_count +=
        manifest.accumulator.excluded_source_count;
    ledger.cumulative_accumulator.sum_flux += manifest.accumulator.sum_flux;
    let manifest_ref = manifest_path.display().to_string();
    if !ledger.partition_manifests.contains(&manifest_ref) {
        ledger.partition_manifests.push(manifest_ref);
    }
}

pub fn write_ledger(
    reconciliation_dir: &Path,
    ledger: &BulkReconciliationLedger,
) -> Result<PathBuf> {
    fs::create_dir_all(reconciliation_dir)?;
    let path = reconciliation_dir.join(LEDGER_FILENAME);
    atomic_write_json(&path, &(serde_json::to_string_pretty(ledger)? + "\n"))?;
    Ok(path)
}

pub fn build_partition_from_processing_output(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partition_filename: &str,
    output_dir: &Path,
    partitions_pending_estimate: u32,
) -> Result<(PartitionReconciliationManifest, PathBuf, PathBuf)> {
    let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_metrics.json"),
    )?)?;
    let reconciliation: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_mini_pilot_reconciliation.json"),
    )?)?;
    let accumulator: XpContinuousHealpixAccumulator = serde_json::from_str(&fs::read_to_string(
        output_dir.join("phase5b_healpix_accumulator.json"),
    )?)?;

    let source_counts = PartitionSourceCounts {
        rows_scanned: metrics["rows_scanned"].as_u64().unwrap_or(0),
        rows_valid: metrics["rows_valid"].as_u64().unwrap_or(0),
        rows_excluded: metrics["rows_excluded"].as_u64().unwrap_or(0),
        rows_failed: metrics["rows_failed"].as_u64().unwrap_or(0),
        processed_unique: reconciliation["processed_unique"].as_u64().unwrap_or(0),
    };
    let accumulator_totals = accumulator_totals_from_healpix(&accumulator);
    let reconciliation_ok = reconciliation["reconciliation_ok"]
        .as_bool()
        .unwrap_or(false);
    let manifest = build_partition_manifest(
        partition_filename,
        metrics["bulk_checksum"].as_str().unwrap_or_default(),
        cache_uuid,
        source_counts,
        accumulator_totals,
        reconciliation_ok,
    );
    let manifest_path = write_partition_manifest(reconciliation_dir, &manifest)?;
    let mut ledger =
        load_or_init_ledger(reconciliation_dir, cache_uuid, partitions_pending_estimate)?;
    merge_partition_into_ledger(&mut ledger, &manifest, &manifest_path);
    let ledger_path = write_ledger(reconciliation_dir, &ledger)?;
    Ok((manifest, manifest_path, ledger_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_accumulates_partition_counts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let manifest = build_partition_manifest(
            "XpContinuousMeanSpectrum_000000-003111.csv.gz",
            "abc123",
            "cache-uuid",
            PartitionSourceCounts {
                rows_scanned: 100,
                rows_valid: 90,
                rows_excluded: 8,
                rows_failed: 2,
                processed_unique: 100,
            },
            PartitionAccumulatorTotals {
                healpix_checksum: "deadbeef".to_string(),
                nside: 64,
                occupied_pixels: 10,
                source_count: 100,
                valid_source_count: 90,
                excluded_source_count: 8,
                sum_flux: 123.4,
            },
            true,
        );
        let manifest_path = write_partition_manifest(dir.path(), &manifest)?;
        let mut ledger = load_or_init_ledger(dir.path(), "cache-uuid", 3385)?;
        merge_partition_into_ledger(&mut ledger, &manifest, &manifest_path);
        write_ledger(dir.path(), &ledger)?;
        assert_eq!(ledger.partitions_complete, 1);
        assert_eq!(ledger.cumulative.rows_valid, 90);
        assert_eq!(ledger.cumulative_accumulator.valid_source_count, 90);
        Ok(())
    }
}
