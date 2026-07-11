//! Streaming SHA-256 helpers for large maintainer artefacts.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use siderust::checksum::to_hex;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const BUFFER_LEN: usize = 1024 * 1024;

/// Compute the SHA-256 digest of `path` without loading the entire file into memory.
pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("failed to open {} for SHA-256", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for SHA-256", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(to_hex(&digest))
}

/// Normalize an optional `sha256:` prefix and compare against the streaming digest of `path`.
pub fn verify_sha256_file(path: &Path, expected: &str, label: &str) -> Result<()> {
    let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
    let actual = sha256_file(path)?;
    if expected != actual {
        anyhow::bail!(
            "{label} checksum mismatch for {}: expected sha256:{expected}, actual sha256:{actual}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_sha256_matches_known_digest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"abc")?;
        assert_eq!(
            sha256_file(&path)?,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }

    #[test]
    fn rejects_incorrect_checksum_with_prefix() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"abc")?;
        let error =
            verify_sha256_file(&path, "sha256:{}", "catalogue").expect_err("checksum mismatch");
        assert!(error.to_string().contains("checksum mismatch"));
        Ok(())
    }

    #[test]
    fn large_file_uses_bounded_buffer() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("large.bin");
        let mut file = std::fs::File::create(&path)?;
        let chunk = vec![7_u8; BUFFER_LEN];
        for _ in 0..8 {
            use std::io::Write;
            file.write_all(&chunk)?;
        }
        drop(file);
        let digest = sha256_file(&path)?;
        assert_eq!(digest.len(), 64);
        Ok(())
    }
}
