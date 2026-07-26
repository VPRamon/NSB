//! Atomic checkpoint I/O and integrity helpers for Phase 5B bulk checkpoint.

use anyhow::{bail, Result};
use md5::{Digest, Md5};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub fn atomic_write_bytes(path: &Path, payload: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("part");
    {
        let mut file = File::create(&temp)?;
        file.write_all(payload)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn atomic_write_json(path: &Path, payload: &str) -> Result<()> {
    atomic_write_bytes(path, payload.as_bytes())
}

pub fn checkpoint_state_checksum(
    bulk_checksum: &str,
    row_index: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix_checksum: &str,
) -> String {
    let mut hasher = Md5::new();
    hasher.update(bulk_checksum.as_bytes());
    hasher.update(row_index.to_le_bytes());
    hasher.update(rows_valid.to_le_bytes());
    hasher.update(rows_excluded.to_le_bytes());
    hasher.update(rows_failed.to_le_bytes());
    hasher.update(healpix_checksum.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn verify_checkpoint_state_checksum(
    bulk_checksum: &str,
    row_index: u64,
    rows_valid: u64,
    rows_excluded: u64,
    rows_failed: u64,
    healpix_checksum: &str,
    expected: &str,
) -> Result<()> {
    let actual = checkpoint_state_checksum(
        bulk_checksum,
        row_index,
        rows_valid,
        rows_excluded,
        rows_failed,
        healpix_checksum,
    );
    if actual != expected {
        bail!("checkpoint state checksum mismatch: expected {expected}, computed {actual}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_roundtrip() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("checkpoint.json");
        atomic_write_json(&path, "{\"ok\":true}\n")?;
        let text = fs::read_to_string(path)?;
        assert!(text.contains("\"ok\": true") || text.contains("\"ok\":true"));
        Ok(())
    }

    #[test]
    fn corrupted_state_checksum_is_detected() {
        let expected = checkpoint_state_checksum("abc", 10, 8, 1, 1, "deadbeef");
        assert!(
            verify_checkpoint_state_checksum("abc", 10, 8, 1, 1, "deadbeef", &expected).is_ok()
        );
        assert!(
            verify_checkpoint_state_checksum("abc", 10, 8, 2, 1, "deadbeef", &expected).is_err()
        );
    }
}
