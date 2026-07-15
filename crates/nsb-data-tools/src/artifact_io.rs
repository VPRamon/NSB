//! Transactional persistence helpers for manifests and generated artefacts.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

fn part_path(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("atomic output path must have a UTF-8 file name")?;
    Ok(path.with_file_name(format!(".{name}.part")))
}

/// Write bytes through a flushed sibling temporary file and atomically rename it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let part = part_path(path)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&part)
            .with_context(|| format!("failed to create {}", part.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", part.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to flush {}", part.display()))?;
        fs::rename(&part, path).with_context(|| {
            format!(
                "failed to atomically promote {} to {}",
                part.display(),
                path.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Serialize a value as pretty JSON with a trailing newline and persist atomically.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).context("failed to serialize JSON artefact")?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)
}

/// Strictly deserialize a typed JSON artefact from disk.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse typed JSON artefact {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fixture {
        schema_version: u32,
        value: String,
    }

    #[test]
    fn atomic_json_roundtrip() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("nested/report.json");
        let expected = Fixture {
            schema_version: 1,
            value: "ok".to_string(),
        };
        write_json_atomic(&path, &expected)?;
        assert_eq!(read_json::<Fixture>(&path)?, expected);
        assert!(!dir.path().join("nested/.report.json.part").exists());
        Ok(())
    }

    #[test]
    fn strict_json_rejects_unknown_fields() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("report.json");
        write_atomic(
            &path,
            br#"{"schema_version":1,"value":"ok","unexpected":true}"#,
        )?;
        assert!(read_json::<Fixture>(&path).is_err());
        Ok(())
    }
}
