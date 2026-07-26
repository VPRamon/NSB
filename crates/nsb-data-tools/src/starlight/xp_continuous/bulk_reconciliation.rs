//! Per-partition reconciliation scaffold for Gaia DR3 XP continuous bulk production.
//!
//! Writes partition manifests and a rolling ledger under the USB reconciliation
//! directory. Population close (184.7M) is deferred; this module tracks per-file
//! source accounting and HEALPix accumulator totals that can scale to full bulk.

use crate::gaia::acquisition::storage_preflight::XP_CONTINUOUS_ONLY_POPULATION;
use crate::gaia::xp::healpix::XpContinuousHealpixAccumulator;
use crate::gaia::xp::pilot_io::atomic_write_json;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PARTITION_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const LEDGER_FILENAME: &str = "bulk_reconciliation_ledger.json";
pub const ROOT_MANIFEST_FILENAME: &str = "root_manifest.json";

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
    #[serde(default)]
    pub partition_index: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionLedgerEntry {
    pub partition_index: u32,
    pub partition_filename: String,
    pub healpix_checksum: String,
    pub rows_valid: u64,
    pub rows_excluded: u64,
    pub rows_failed: u64,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootReconciliationManifest {
    pub schema_version: u32,
    pub cache_uuid: String,
    pub updated_at_utc: String,
    pub population_target_sources: u64,
    pub population_accumulated_valid: u64,
    pub population_accumulated_excluded: u64,
    pub population_accumulated_failed: u64,
    pub population_progress_fraction: f64,
    pub population_status: String,
    pub partitions_complete: u32,
    pub partitions_pending_estimate: u32,
    pub global_healpix_checksum: Option<String>,
    pub global_healpix_accumulator_path: Option<String>,
    pub partitions: Vec<PartitionLedgerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BulkReconciliationLedger {
    pub schema_version: u32,
    pub cache_uuid: String,
    pub updated_at_utc: String,
    pub population_target_sources: u64,
    #[serde(default)]
    pub population_accumulated_valid: u64,
    #[serde(default)]
    pub population_accumulated_excluded: u64,
    #[serde(default)]
    pub population_accumulated_failed: u64,
    #[serde(default)]
    pub population_progress_fraction: f64,
    pub population_status: String,
    pub partitions_complete: u32,
    pub partitions_pending_estimate: u32,
    pub cumulative: PartitionSourceCounts,
    pub cumulative_accumulator: PartitionAccumulatorTotals,
    #[serde(default)]
    pub merged_healpix_checksum: Option<String>,
    #[serde(default)]
    pub partition_index: Vec<PartitionLedgerEntry>,
    #[serde(default)]
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
    partition_index: u32,
    partition_filename: &str,
    bulk_checksum: &str,
    cache_uuid: &str,
    source_counts: PartitionSourceCounts,
    accumulator: PartitionAccumulatorTotals,
    reconciliation_ok: bool,
) -> PartitionReconciliationManifest {
    PartitionReconciliationManifest {
        schema_version: PARTITION_MANIFEST_SCHEMA_VERSION,
        partition_index,
        partition_filename: partition_filename.to_string(),
        bulk_checksum: bulk_checksum.to_string(),
        cache_uuid: cache_uuid.to_string(),
        processed_at_utc: crate::gaia::acquisition::usb_cache::utc_now_rfc3339(),
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
        updated_at_utc: crate::gaia::acquisition::usb_cache::utc_now_rfc3339(),
        population_target_sources: XP_CONTINUOUS_ONLY_POPULATION,
        population_accumulated_valid: 0,
        population_accumulated_excluded: 0,
        population_accumulated_failed: 0,
        population_progress_fraction: 0.0,
        population_status: "accumulating_partitions".to_string(),
        partitions_complete: 0,
        partitions_pending_estimate,
        cumulative: PartitionSourceCounts::default(),
        cumulative_accumulator: PartitionAccumulatorTotals::default(),
        merged_healpix_checksum: None,
        partition_index: Vec::new(),
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
    let manifest_ref = manifest_path.display().to_string();
    if let Some(entry) = ledger
        .partition_index
        .iter_mut()
        .find(|entry| entry.partition_filename == manifest.partition_filename)
    {
        let old_valid = entry.rows_valid;
        let old_excluded = entry.rows_excluded;
        let old_failed = entry.rows_failed;
        ledger.cumulative.rows_valid = ledger.cumulative.rows_valid.saturating_sub(old_valid)
            + manifest.source_counts.rows_valid;
        ledger.cumulative.rows_excluded =
            ledger.cumulative.rows_excluded.saturating_sub(old_excluded)
                + manifest.source_counts.rows_excluded;
        ledger.cumulative.rows_failed = ledger.cumulative.rows_failed.saturating_sub(old_failed)
            + manifest.source_counts.rows_failed;
        entry.partition_index = manifest.partition_index;
        entry.healpix_checksum = manifest.accumulator.healpix_checksum.clone();
        entry.rows_valid = manifest.source_counts.rows_valid;
        entry.rows_excluded = manifest.source_counts.rows_excluded;
        entry.rows_failed = manifest.source_counts.rows_failed;
        entry.manifest_path = manifest_ref.clone();
    } else {
        ledger.partitions_complete += 1;
        ledger.cumulative.rows_scanned += manifest.source_counts.rows_scanned;
        ledger.cumulative.rows_valid += manifest.source_counts.rows_valid;
        ledger.cumulative.rows_excluded += manifest.source_counts.rows_excluded;
        ledger.cumulative.rows_failed += manifest.source_counts.rows_failed;
        ledger.cumulative.processed_unique += manifest.source_counts.processed_unique;
        ledger.cumulative_accumulator.healpix_checksum =
            manifest.accumulator.healpix_checksum.clone();
        ledger.cumulative_accumulator.nside = manifest.accumulator.nside;
        ledger.cumulative_accumulator.occupied_pixels += manifest.accumulator.occupied_pixels;
        ledger.cumulative_accumulator.source_count += manifest.accumulator.source_count;
        ledger.cumulative_accumulator.valid_source_count += manifest.accumulator.valid_source_count;
        ledger.cumulative_accumulator.excluded_source_count +=
            manifest.accumulator.excluded_source_count;
        ledger.cumulative_accumulator.sum_flux += manifest.accumulator.sum_flux;
        ledger.partition_index.push(PartitionLedgerEntry {
            partition_index: manifest.partition_index,
            partition_filename: manifest.partition_filename.clone(),
            healpix_checksum: manifest.accumulator.healpix_checksum.clone(),
            rows_valid: manifest.source_counts.rows_valid,
            rows_excluded: manifest.source_counts.rows_excluded,
            rows_failed: manifest.source_counts.rows_failed,
            manifest_path: manifest_ref.clone(),
        });
    }
    ledger
        .partition_index
        .sort_by_key(|entry| entry.partition_index);
    ledger.population_accumulated_valid = ledger.cumulative.rows_valid;
    ledger.population_accumulated_excluded = ledger.cumulative.rows_excluded;
    ledger.population_accumulated_failed = ledger.cumulative.rows_failed;
    ledger.population_progress_fraction = if ledger.population_target_sources > 0 {
        ledger.population_accumulated_valid as f64 / ledger.population_target_sources as f64
    } else {
        0.0
    };
    if !ledger.partition_manifests.contains(&manifest_ref) {
        ledger.partition_manifests.push(manifest_ref);
    }
}

pub fn root_manifest_path(reconciliation_dir: &Path) -> PathBuf {
    reconciliation_dir.join(ROOT_MANIFEST_FILENAME)
}

pub fn write_root_manifest(
    reconciliation_dir: &Path,
    ledger: &BulkReconciliationLedger,
    global_healpix_checksum: Option<&str>,
    global_healpix_accumulator_path: Option<&Path>,
) -> Result<PathBuf> {
    fs::create_dir_all(reconciliation_dir)?;
    let root = RootReconciliationManifest {
        schema_version: 1,
        cache_uuid: ledger.cache_uuid.clone(),
        updated_at_utc: ledger.updated_at_utc.clone(),
        population_target_sources: ledger.population_target_sources,
        population_accumulated_valid: ledger.population_accumulated_valid,
        population_accumulated_excluded: ledger.population_accumulated_excluded,
        population_accumulated_failed: ledger.population_accumulated_failed,
        population_progress_fraction: ledger.population_progress_fraction,
        population_status: ledger.population_status.clone(),
        partitions_complete: ledger.partitions_complete,
        partitions_pending_estimate: ledger.partitions_pending_estimate,
        global_healpix_checksum: global_healpix_checksum.map(str::to_string),
        global_healpix_accumulator_path: global_healpix_accumulator_path
            .map(|path| path.display().to_string()),
        partitions: ledger.partition_index.clone(),
    };
    let path = root_manifest_path(reconciliation_dir);
    atomic_write_json(&path, &(serde_json::to_string_pretty(&root)? + "\n"))?;
    Ok(path)
}

pub fn load_partition_manifests(
    reconciliation_dir: &Path,
) -> Result<Vec<(PartitionReconciliationManifest, PathBuf)>> {
    let mut manifests = Vec::new();
    if !reconciliation_dir.is_dir() {
        return Ok(manifests);
    }
    for entry in fs::read_dir(reconciliation_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".reconciliation.json") {
            continue;
        }
        let manifest: PartitionReconciliationManifest =
            serde_json::from_str(&fs::read_to_string(&path)?)?;
        manifests.push((manifest, path));
    }
    manifests.sort_by_key(|(manifest, _)| manifest.partition_index);
    Ok(manifests)
}

pub fn rebuild_ledger_from_manifests(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partitions_pending_estimate: u32,
) -> Result<BulkReconciliationLedger> {
    let mut manifests = load_partition_manifests(reconciliation_dir)?;
    manifests
        .sort_by(|(left, _), (right, _)| left.partition_filename.cmp(&right.partition_filename));
    let mut ledger =
        load_or_init_ledger(reconciliation_dir, cache_uuid, partitions_pending_estimate)?;
    ledger.partition_index.clear();
    ledger.partition_manifests.clear();
    ledger.cumulative = PartitionSourceCounts::default();
    ledger.cumulative_accumulator = PartitionAccumulatorTotals::default();
    ledger.partitions_complete = 0;
    for (index, (mut manifest, path)) in manifests.into_iter().enumerate() {
        manifest.partition_index = (index + 1) as u32;
        manifest.cache_uuid = cache_uuid.to_string();
        write_partition_manifest(reconciliation_dir, &manifest)?;
        merge_partition_into_ledger(&mut ledger, &manifest, &path);
    }
    Ok(ledger)
}

pub fn partition_filename_from_output_stem(stem: &str) -> String {
    if stem.ends_with(".csv.gz") {
        stem.to_string()
    } else {
        format!("{stem}.csv.gz")
    }
}

pub fn discover_verified_cache_outputs(search_roots: &[PathBuf]) -> Result<Vec<(String, PathBuf)>> {
    let mut outputs = Vec::new();
    for root in search_roots {
        let verified = root.join("verified_cache_process");
        if !verified.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&verified)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let output_dir = entry.path();
            if !output_dir.join("phase5b_mini_pilot_metrics.json").is_file() {
                continue;
            }
            let stem = entry.file_name().to_string_lossy().into_owned();
            outputs.push((partition_filename_from_output_stem(&stem), output_dir));
        }
    }
    outputs.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(outputs)
}

pub fn backfill_partition_manifest_from_output(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    partition_filename: &str,
    output_dir: &Path,
) -> Result<(PartitionReconciliationManifest, PathBuf)> {
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
        0,
        partition_filename,
        metrics["bulk_checksum"].as_str().unwrap_or_default(),
        cache_uuid,
        source_counts,
        accumulator_totals,
        reconciliation_ok,
    );
    let manifest_path = write_partition_manifest(reconciliation_dir, &manifest)?;
    Ok((manifest, manifest_path))
}

pub fn backfill_reconciliation_from_verified_cache(
    reconciliation_dir: &Path,
    cache_uuid: &str,
    search_roots: &[PathBuf],
    partitions_pending_estimate: u32,
) -> Result<(
    BulkReconciliationLedger,
    Vec<PartitionReconciliationManifest>,
)> {
    fs::create_dir_all(reconciliation_dir)?;
    let mut written = Vec::new();
    for (partition_filename, output_dir) in discover_verified_cache_outputs(search_roots)? {
        let (manifest, _) = backfill_partition_manifest_from_output(
            reconciliation_dir,
            cache_uuid,
            &partition_filename,
            &output_dir,
        )?;
        written.push(manifest);
    }
    let ledger =
        rebuild_ledger_from_manifests(reconciliation_dir, cache_uuid, partitions_pending_estimate)?;
    write_ledger(reconciliation_dir, &ledger)?;
    Ok((ledger, written))
}

pub fn sync_ledger_from_merge_state(
    reconciliation_dir: &Path,
    merge_state: &crate::gaia::xp::bulk_healpix_merge::BulkHealpixMergeState,
    global_healpix_path: &Path,
) -> Result<(BulkReconciliationLedger, PathBuf)> {
    let cache_uuid = merge_state
        .merged_partitions
        .first()
        .map(|_| "bulk-cache")
        .unwrap_or("unknown");
    let mut ledger = rebuild_ledger_from_manifests(reconciliation_dir, cache_uuid, 3381)
        .unwrap_or_else(|_| load_or_init_ledger(reconciliation_dir, cache_uuid, 3381).unwrap());
    ledger.merged_healpix_checksum = Some(merge_state.global_healpix_checksum.clone());
    ledger.updated_at_utc = merge_state.updated_at_utc.clone();
    for merged in &merge_state.merged_partitions {
        if ledger
            .partition_index
            .iter()
            .any(|entry| entry.partition_filename == merged.partition_filename)
        {
            if let Some(entry) = ledger
                .partition_index
                .iter_mut()
                .find(|entry| entry.partition_filename == merged.partition_filename)
            {
                entry.healpix_checksum = merged.healpix_checksum.clone();
            }
            continue;
        }
        let partition_index = ledger.partitions_complete + 1;
        ledger.partition_index.push(PartitionLedgerEntry {
            partition_index,
            partition_filename: merged.partition_filename.clone(),
            healpix_checksum: merged.healpix_checksum.clone(),
            rows_valid: 0,
            rows_excluded: 0,
            rows_failed: 0,
            manifest_path: merged.accumulator_path.clone(),
        });
        ledger.partitions_complete += 1;
    }
    ledger
        .partition_index
        .sort_by_key(|entry| entry.partition_index);
    let _ledger_path = write_ledger(reconciliation_dir, &ledger)?;
    let root_path = write_root_manifest(
        reconciliation_dir,
        &ledger,
        Some(&merge_state.global_healpix_checksum),
        Some(global_healpix_path),
    )?;
    Ok((ledger, root_path))
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
    let mut ledger =
        load_or_init_ledger(reconciliation_dir, cache_uuid, partitions_pending_estimate)?;
    let manifest = build_partition_manifest(
        ledger.partitions_complete + 1,
        partition_filename,
        metrics["bulk_checksum"].as_str().unwrap_or_default(),
        cache_uuid,
        source_counts,
        accumulator_totals,
        reconciliation_ok,
    );
    let manifest_path = write_partition_manifest(reconciliation_dir, &manifest)?;
    merge_partition_into_ledger(&mut ledger, &manifest, &manifest_path);
    let ledger_path = write_ledger(reconciliation_dir, &ledger)?;
    write_root_manifest(reconciliation_dir, &ledger, None, None)?;
    Ok((manifest, manifest_path, ledger_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backfill_writes_manifests_and_rebuilds_ledger() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let work = dir.path().join("work");
        let verified = work.join("verified_cache_process/XpContinuousMeanSpectrum_000000-003111");
        fs::create_dir_all(&verified)?;
        let metrics = serde_json::json!({
            "rows_scanned": 200,
            "rows_valid": 200,
            "rows_excluded": 0,
            "rows_failed": 0,
            "bulk_checksum": "abc",
        });
        fs::write(
            verified.join("phase5b_mini_pilot_metrics.json"),
            serde_json::to_string_pretty(&metrics)? + "\n",
        )?;
        fs::write(
            verified.join("phase5b_mini_pilot_reconciliation.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "processed_unique": 200,
                "reconciliation_ok": true,
            }))? + "\n",
        )?;
        let mut acc = XpContinuousHealpixAccumulator::new(64)?;
        acc.accumulate_valid(0, 1.0, 0.1, 0.0)?;
        fs::write(
            verified.join("phase5b_healpix_accumulator.json"),
            serde_json::to_string_pretty(&acc)? + "\n",
        )?;
        let reconciliation = dir.path().join("reconciliation");
        let (ledger, backfilled) = backfill_reconciliation_from_verified_cache(
            &reconciliation,
            "cache-uuid",
            &[work],
            3385,
        )?;
        assert_eq!(backfilled.len(), 1);
        assert_eq!(ledger.cumulative.rows_valid, 200);
        Ok(())
    }

    #[test]
    fn ledger_accumulates_partition_counts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let manifest = build_partition_manifest(
            1,
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
