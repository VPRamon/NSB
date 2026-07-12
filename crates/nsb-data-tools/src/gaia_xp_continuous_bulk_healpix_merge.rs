//! Deterministic, resumable cross-partition HEALPix merge for XP continuous bulk.

use crate::gaia_xp_continuous_healpix::XpContinuousHealpixAccumulator;
use crate::gaia_xp_continuous_pilot_io::atomic_write_json;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const BULK_HEALPIX_ACCUMULATOR_FILENAME: &str = "bulk_healpix_accumulator.json";
pub const BULK_HEALPIX_MERGE_STATE_FILENAME: &str = "bulk_healpix_merge_state.json";
pub const DETERMINISTIC_MERGE_REPORT_FILENAME: &str = "bulk_healpix_deterministic_merge.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicMergeReport {
    pub order_12_checksum: String,
    pub order_21_checksum: String,
    pub single_worker_checksum: String,
    pub multi_worker_checksum: String,
    pub order_independent: bool,
    pub single_multi_identical: bool,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionCheckpointRef {
    pub partition_filename: String,
    pub accumulator_path: String,
    pub healpix_checksum: String,
    pub merged_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkHealpixMergeState {
    pub schema_version: u32,
    pub nside: u32,
    pub merged_partitions: Vec<PartitionCheckpointRef>,
    pub global_healpix_checksum: String,
    pub updated_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkHealpixMergeReport {
    pub passed: bool,
    pub partitions_discovered: usize,
    pub partitions_merged_this_run: usize,
    pub partitions_skipped_already_merged: usize,
    pub total_partitions_merged: usize,
    pub global_healpix_checksum: String,
    pub deterministic_merge: Option<DeterministicMergeReport>,
    pub bulk_accumulator_path: String,
    pub merge_state_path: String,
    pub deterministic_merge_path: Option<String>,
}

pub fn bulk_accumulator_path(checkpoint_dir: &Path) -> PathBuf {
    checkpoint_dir.join(BULK_HEALPIX_ACCUMULATOR_FILENAME)
}

pub fn merge_state_path(checkpoint_dir: &Path) -> PathBuf {
    checkpoint_dir.join(BULK_HEALPIX_MERGE_STATE_FILENAME)
}

pub fn deterministic_merge_report_path(checkpoint_dir: &Path) -> PathBuf {
    checkpoint_dir.join(DETERMINISTIC_MERGE_REPORT_FILENAME)
}

pub fn partition_filename_from_stem(stem: &str) -> String {
    if stem.ends_with(".csv.gz") {
        stem.to_string()
    } else {
        format!("{stem}.csv.gz")
    }
}

pub fn partition_stem_from_filename(filename: &str) -> String {
    filename.trim_end_matches(".csv.gz").to_string()
}

pub fn partition_checkpoint_path(checkpoint_dir: &Path, partition_filename: &str) -> PathBuf {
    checkpoint_dir.join(format!(
        "{}_healpix_accumulator.json",
        partition_stem_from_filename(partition_filename)
    ))
}

pub fn validate_deterministic_merge(
    accumulators: &[XpContinuousHealpixAccumulator],
) -> Result<DeterministicMergeReport> {
    if accumulators.len() < 2 {
        bail!("deterministic merge requires at least two accumulators");
    }
    let nside = accumulators[0].nside;
    let mut order_12 = XpContinuousHealpixAccumulator::new(nside)?;
    order_12.merge(&accumulators[0])?;
    order_12.merge(&accumulators[1])?;

    let mut order_21 = XpContinuousHealpixAccumulator::new(nside)?;
    order_21.merge(&accumulators[1])?;
    order_21.merge(&accumulators[0])?;

    let mut single_worker = XpContinuousHealpixAccumulator::new(nside)?;
    for acc in accumulators {
        single_worker.merge(acc)?;
    }

    let mut multi_worker = XpContinuousHealpixAccumulator::new(nside)?;
    multi_worker.merge(&accumulators[0])?;
    multi_worker.merge(&accumulators[1])?;

    let order_independent = order_12.checksum() == order_21.checksum();
    let single_multi_identical = single_worker.checksum() == multi_worker.checksum();
    Ok(DeterministicMergeReport {
        order_12_checksum: order_12.checksum(),
        order_21_checksum: order_21.checksum(),
        single_worker_checksum: single_worker.checksum(),
        multi_worker_checksum: multi_worker.checksum(),
        order_independent,
        single_multi_identical,
        passed: order_independent && single_multi_identical,
    })
}

pub fn load_accumulator(path: &Path) -> Result<XpContinuousHealpixAccumulator> {
    Ok(serde_json::from_str(
        &fs::read_to_string(path)
            .with_context(|| format!("failed to read HEALPix accumulator at {}", path.display()))?,
    )?)
}

pub fn load_or_init_merge_state(
    checkpoint_dir: &Path,
    nside: u32,
) -> Result<BulkHealpixMergeState> {
    fs::create_dir_all(checkpoint_dir)?;
    let path = merge_state_path(checkpoint_dir);
    if path.is_file() {
        return Ok(serde_json::from_str(&fs::read_to_string(&path)?)?);
    }
    Ok(BulkHealpixMergeState {
        schema_version: 1,
        nside,
        merged_partitions: Vec::new(),
        global_healpix_checksum: String::new(),
        updated_at_utc: crate::gaia_usb_cache::utc_now_rfc3339(),
    })
}

pub fn save_merge_state(checkpoint_dir: &Path, state: &BulkHealpixMergeState) -> Result<()> {
    atomic_write_json(
        &merge_state_path(checkpoint_dir),
        &(serde_json::to_string_pretty(state)? + "\n"),
    )
}

pub fn save_bulk_accumulator(
    checkpoint_dir: &Path,
    acc: &XpContinuousHealpixAccumulator,
) -> Result<()> {
    atomic_write_json(
        &bulk_accumulator_path(checkpoint_dir),
        &(serde_json::to_string_pretty(acc)? + "\n"),
    )
}

pub fn discover_partition_checkpoints(
    checkpoint_dir: &Path,
    search_roots: &[PathBuf],
) -> Result<Vec<(String, PathBuf)>> {
    let mut by_filename: BTreeMap<String, PathBuf> = BTreeMap::new();

    if checkpoint_dir.is_dir() {
        for entry in fs::read_dir(checkpoint_dir)? {
            let entry = entry?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with("_healpix_accumulator.json") {
                continue;
            }
            if name == BULK_HEALPIX_ACCUMULATOR_FILENAME {
                continue;
            }
            let stem = name.trim_end_matches("_healpix_accumulator.json");
            let filename = partition_filename_from_stem(stem);
            by_filename.insert(filename, path);
        }
    }

    for root in search_roots {
        let verified = root.join("verified_cache_process");
        if verified.is_dir() {
            for entry in fs::read_dir(&verified)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let stem = entry.file_name().to_string_lossy().into_owned();
                let acc = entry.path().join("phase5b_healpix_accumulator.json");
                if acc.is_file() {
                    let filename = partition_filename_from_stem(&stem);
                    by_filename.entry(filename).or_insert(acc);
                }
            }
        }
        let production = root.join("production_loop");
        if production.is_dir() {
            for entry in fs::read_dir(&production)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let stem = entry.file_name().to_string_lossy().into_owned();
                let acc = entry.path().join("phase5b_healpix_accumulator.json");
                if acc.is_file() {
                    let filename = partition_filename_from_stem(&stem);
                    by_filename.entry(filename).or_insert(acc);
                }
            }
        }
    }

    Ok(by_filename.into_iter().collect())
}

pub fn merge_all_partition_checkpoints(
    checkpoint_dir: &Path,
    search_roots: &[PathBuf],
) -> Result<BulkHealpixMergeReport> {
    fs::create_dir_all(checkpoint_dir)?;
    let discovered = discover_partition_checkpoints(checkpoint_dir, search_roots)?;
    if discovered.is_empty() {
        bail!("no partition HEALPix checkpoints discovered for merge");
    }

    let first = load_accumulator(&discovered[0].1)?;
    let nside = first.nside;
    let mut merge_state = load_or_init_merge_state(checkpoint_dir, nside)?;
    let already_merged: BTreeSet<String> = merge_state
        .merged_partitions
        .iter()
        .map(|entry| entry.partition_filename.clone())
        .collect();

    let mut merged_this_run = 0_usize;
    let mut skipped = 0_usize;
    let mut bulk = if bulk_accumulator_path(checkpoint_dir).is_file() {
        load_accumulator(&bulk_accumulator_path(checkpoint_dir))?
    } else {
        XpContinuousHealpixAccumulator::new(nside)?
    };

    for (filename, path) in &discovered {
        if already_merged.contains(filename) {
            skipped += 1;
            continue;
        }
        let partition_acc = load_accumulator(path)?;
        if partition_acc.nside != nside {
            bail!(
                "partition {filename} nside {} != global nside {nside}",
                partition_acc.nside
            );
        }
        bulk.merge(&partition_acc)?;
        merge_state.merged_partitions.push(PartitionCheckpointRef {
            partition_filename: filename.clone(),
            accumulator_path: path.display().to_string(),
            healpix_checksum: partition_acc.checksum(),
            merged_at_utc: crate::gaia_usb_cache::utc_now_rfc3339(),
        });
        merge_state.global_healpix_checksum = bulk.checksum();
        merge_state.updated_at_utc = crate::gaia_usb_cache::utc_now_rfc3339();
        save_bulk_accumulator(checkpoint_dir, &bulk)?;
        save_merge_state(checkpoint_dir, &merge_state)?;
        merged_this_run += 1;
    }

    if bulk_accumulator_path(checkpoint_dir).is_file() {
        bulk = load_accumulator(&bulk_accumulator_path(checkpoint_dir))?;
    }
    merge_state.global_healpix_checksum = bulk.checksum();
    merge_state.updated_at_utc = crate::gaia_usb_cache::utc_now_rfc3339();
    save_merge_state(checkpoint_dir, &merge_state)?;

    let partition_accumulators = discovered
        .iter()
        .map(|(_, path)| load_accumulator(path))
        .collect::<Result<Vec<_>>>()?;

    let mut canonical = XpContinuousHealpixAccumulator::new(nside)?;
    for acc in &partition_accumulators {
        canonical.merge(acc)?;
    }
    if canonical.checksum() != bulk.checksum() {
        bulk = canonical;
        save_bulk_accumulator(checkpoint_dir, &bulk)?;
    }
    merge_state.global_healpix_checksum = bulk.checksum();
    merge_state.updated_at_utc = crate::gaia_usb_cache::utc_now_rfc3339();
    save_merge_state(checkpoint_dir, &merge_state)?;

    let deterministic_merge = if partition_accumulators.len() >= 2 {
        let mut report = validate_deterministic_merge(&partition_accumulators[..2])?;
        let mut reverse = XpContinuousHealpixAccumulator::new(nside)?;
        for acc in partition_accumulators.iter().rev() {
            reverse.merge(acc)?;
        }
        report.single_worker_checksum = bulk.checksum();
        report.multi_worker_checksum = reverse.checksum();
        report.single_multi_identical = bulk.checksum() == reverse.checksum();
        report.passed = report.order_independent && report.single_multi_identical;
        Some(report)
    } else {
        None
    };
    let deterministic_merge_path = if let Some(report) = &deterministic_merge {
        let path = deterministic_merge_report_path(checkpoint_dir);
        atomic_write_json(&path, &(serde_json::to_string_pretty(report)? + "\n"))?;
        Some(path.display().to_string())
    } else {
        None
    };

    let passed = deterministic_merge
        .as_ref()
        .map(|report| report.passed)
        .unwrap_or(true);
    Ok(BulkHealpixMergeReport {
        passed,
        partitions_discovered: discovered.len(),
        partitions_merged_this_run: merged_this_run,
        partitions_skipped_already_merged: skipped,
        total_partitions_merged: merge_state.merged_partitions.len(),
        global_healpix_checksum: bulk.checksum(),
        deterministic_merge,
        bulk_accumulator_path: bulk_accumulator_path(checkpoint_dir).display().to_string(),
        merge_state_path: merge_state_path(checkpoint_dir).display().to_string(),
        deterministic_merge_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_merge_is_commutative_for_disjoint_accumulators() -> Result<()> {
        let mut left = XpContinuousHealpixAccumulator::new(64)?;
        left.accumulate_valid(0, 1.0, 0.1, 0.0)?;
        let mut right = XpContinuousHealpixAccumulator::new(64)?;
        right.accumulate_valid(1 << 12, 2.0, 0.2, 0.0)?;
        let report = validate_deterministic_merge(&[left, right])?;
        assert!(report.passed);
        Ok(())
    }

    #[test]
    fn merge_is_resumable_and_idempotent() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let checkpoint_dir = dir.path().join("checkpoints");
        fs::create_dir_all(&checkpoint_dir)?;
        let work = dir.path().join("work");
        let verified = work.join("verified_cache_process");
        for (stem, pixel, flux) in [
            ("XpContinuousMeanSpectrum_000000-003111", 0_u64, 1.0),
            ("XpContinuousMeanSpectrum_003112-005263", 1 << 12, 2.0),
        ] {
            let out = verified.join(stem);
            fs::create_dir_all(&out)?;
            let mut acc = XpContinuousHealpixAccumulator::new(64)?;
            acc.accumulate_valid(pixel, flux, 0.1, 0.0)?;
            fs::write(
                out.join("phase5b_healpix_accumulator.json"),
                serde_json::to_string_pretty(&acc)? + "\n",
            )?;
        }

        let first = merge_all_partition_checkpoints(&checkpoint_dir, std::slice::from_ref(&work))?;
        assert_eq!(first.partitions_merged_this_run, 2);
        assert!(first.passed);

        let second = merge_all_partition_checkpoints(&checkpoint_dir, &[work])?;
        assert_eq!(second.partitions_merged_this_run, 0);
        assert_eq!(second.partitions_skipped_already_merged, 2);
        assert_eq!(
            first.global_healpix_checksum,
            second.global_healpix_checksum
        );
        Ok(())
    }
}
