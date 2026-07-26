//! Exclusive partition leases for multi-partition bulk production.

use crate::platform::file_lock::{try_lock_exclusive, ExclusiveFileLock};
use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Held exclusive claim on one bulk partition filename.
#[derive(Debug)]
pub struct PartitionClaim {
    filename: String,
    claim_path: PathBuf,
    _lock: ExclusiveFileLock,
}

impl PartitionClaim {
    /// Claimed inventory filename (e.g. `XpContinuousMeanSpectrum_….csv.gz`).
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Path of the claim lock file.
    pub fn claim_path(&self) -> &Path {
        &self.claim_path
    }
}

impl Drop for PartitionClaim {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.claim_path);
    }
}

/// Directory used for partition claim lock files under the checkpoint root.
pub fn claims_dir(checkpoint_dir: &Path) -> PathBuf {
    checkpoint_dir.join("claims")
}

fn claim_path_for(claims_dir: &Path, filename: &str) -> PathBuf {
    let stem = filename.trim_end_matches(".csv.gz");
    claims_dir.join(format!("{stem}.claim"))
}

/// Try to claim `filename`. Returns `None` if another live process holds the claim.
///
/// Stale claims from crashed processes are reclaimed automatically: flock succeeds when
/// the previous holder exited, then we overwrite the claim metadata.
pub fn try_claim_partition(
    checkpoint_dir: &Path,
    filename: &str,
) -> Result<Option<PartitionClaim>> {
    let dir = claims_dir(checkpoint_dir);
    fs::create_dir_all(&dir).with_context(|| format!("create claims dir {}", dir.display()))?;
    let claim_path = claim_path_for(&dir, filename);
    let Some(lock) = try_lock_exclusive(&claim_path)? else {
        return Ok(None);
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&claim_path)
        .with_context(|| format!("rewrite claim {}", claim_path.display()))?;
    let meta = format!(
        "pid={}\nfilename={}\nclaimed_at_utc={}\n",
        std::process::id(),
        filename,
        crate::gaia::acquisition::usb_cache::utc_now_rfc3339()
    );
    file.write_all(meta.as_bytes())
        .with_context(|| format!("write claim metadata {}", claim_path.display()))?;
    Ok(Some(PartitionClaim {
        filename: filename.to_string(),
        claim_path,
        _lock: lock,
    }))
}

/// Claim the first available filename from `candidates`.
pub fn claim_next_partition(
    checkpoint_dir: &Path,
    candidates: &[String],
) -> Result<Option<PartitionClaim>> {
    for filename in candidates {
        if let Some(claim) = try_claim_partition(checkpoint_dir, filename)? {
            return Ok(Some(claim));
        }
    }
    Ok(None)
}

/// Claim up to `limit` distinct partitions from `candidates`.
pub fn claim_partitions(
    checkpoint_dir: &Path,
    candidates: &[String],
    limit: usize,
) -> Result<Vec<PartitionClaim>> {
    let mut claimed = Vec::new();
    for filename in candidates {
        if claimed.len() >= limit {
            break;
        }
        if claimed.iter().any(|c: &PartitionClaim| c.filename() == filename) {
            continue;
        }
        if let Some(claim) = try_claim_partition(checkpoint_dir, filename)? {
            claimed.push(claim);
        }
    }
    Ok(claimed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_claim_fails_while_held() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let name = "XpContinuousMeanSpectrum_000000-003111.csv.gz";
        let first = try_claim_partition(dir.path(), name)?.expect("first claim");
        assert!(try_claim_partition(dir.path(), name)?.is_none());
        drop(first);
        assert!(try_claim_partition(dir.path(), name)?.is_some());
        Ok(())
    }
}
