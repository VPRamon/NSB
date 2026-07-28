//! Read-only migration helpers for the pre-lifecycle Starlight checkpoints.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct LegacyLedger {
    schema_version: u32,
    partition_index: Vec<LegacyPartition>,
}

#[derive(Debug, Deserialize)]
struct LegacyPartition {
    partition_filename: String,
    healpix_checksum: String,
    rows_failed: u64,
}

/// Load successfully processed legacy XP partition filenames.
///
/// The legacy accumulator payloads are deliberately not read: this set is only
/// a scheduling hint, never scientific shard evidence.
pub fn load_completed_partition_ids(checkpoints_dir: &Path) -> Result<HashSet<String>> {
    let path = checkpoints_dir.join("bulk_reconciliation_ledger.json");
    let bytes = fs::read(&path)
        .with_context(|| format!("read legacy reconciliation ledger {}", path.display()))?;
    let ledger: LegacyLedger = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse legacy reconciliation ledger {}", path.display()))?;
    if ledger.schema_version != 1 {
        bail!(
            "unsupported legacy reconciliation ledger schema {}",
            ledger.schema_version
        );
    }
    let mut completed = HashSet::with_capacity(ledger.partition_index.len());
    for partition in ledger.partition_index {
        if partition.rows_failed != 0 || partition.healpix_checksum.trim().is_empty() {
            continue;
        }
        let name = partition.partition_filename;
        if !name.starts_with("XpContinuousMeanSpectrum_") || !name.ends_with(".csv.gz") {
            bail!("legacy ledger contains invalid XP partition filename {name:?}");
        }
        if !completed.insert(name.clone()) {
            bail!("legacy ledger contains duplicate partition {name:?}");
        }
    }
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_accumulators_are_not_required_or_loaded() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("bulk_reconciliation_ledger.json"),
            r#"{
                "schema_version": 1,
                "partition_index": [
                    {
                        "partition_filename": "XpContinuousMeanSpectrum_017659-018028.csv.gz",
                        "healpix_checksum": "abc",
                        "rows_failed": 0
                    },
                    {
                        "partition_filename": "XpContinuousMeanSpectrum_018029-018472.csv.gz",
                        "healpix_checksum": "def",
                        "rows_failed": 1
                    }
                ]
            }"#,
        )
        .unwrap();
        let completed = load_completed_partition_ids(directory.path()).unwrap();
        assert_eq!(
            completed,
            HashSet::from([String::from(
                "XpContinuousMeanSpectrum_017659-018028.csv.gz"
            )])
        );
    }
}
