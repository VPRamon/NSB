//! Transactional filesystem persistence for run metadata and artifacts.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// POSIX-filesystem store with durable temporary-write-and-rename commits.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    /// Create a store rooted at an explicit path.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolve a store-relative path.
    pub fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    /// Atomically and durably replace one store-relative file.
    pub fn write(&self, relative: impl AsRef<Path>, bytes: &[u8]) -> Result<PathBuf> {
        let destination = self.path(relative);
        atomic_write(&destination, bytes)?;
        Ok(destination)
    }

    /// Serialize pretty JSON and commit it atomically.
    pub fn write_json<T: Serialize>(
        &self,
        relative: impl AsRef<Path>,
        value: &T,
    ) -> Result<PathBuf> {
        self.write(relative, &serde_json::to_vec_pretty(value)?)
    }

    /// Read strict JSON from a store-relative path.
    pub fn read_json<T: DeserializeOwned>(&self, relative: impl AsRef<Path>) -> Result<T> {
        let path = self.path(relative);
        serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("failed to parse {}", path.display()))
    }
}

/// Atomically replace a file and fsync both file and containing directory.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("destination {} has no parent", path.display()))?;
    fs::create_dir_all(parent)?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("destination filename is not valid UTF-8")?;
    let temporary = parent.join(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        schema_version: u32,
        value: String,
    }

    #[test]
    fn store_replaces_bytes_and_round_trips_strict_json() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path());
        store.write("artifact.bin", b"old").unwrap();
        store.write("artifact.bin", b"new").unwrap();
        assert_eq!(fs::read(store.path("artifact.bin")).unwrap(), b"new");

        let expected = Record {
            schema_version: 1,
            value: "stable".to_string(),
        };
        store.write_json("state/run.json", &expected).unwrap();
        assert_eq!(
            store.read_json::<Record>("state/run.json").unwrap(),
            expected
        );
    }

    #[test]
    fn failed_serialized_contract_rejects_unknown_fields() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path());
        store
            .write(
                "run.json",
                br#"{"schema_version":1,"value":"ok","unknown":true}"#,
            )
            .unwrap();
        assert!(store.read_json::<Record>("run.json").is_err());
    }
}
