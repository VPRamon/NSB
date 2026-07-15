//! Typed, streaming checksums for maintainer artefacts and official inventories.

use anyhow::{bail, Context, Result};
use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as Sha256Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;

const BUFFER_LEN: usize = 1024 * 1024;

/// Supported checksum algorithms with explicit provenance semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumAlgorithm {
    /// MD5, retained only for verification of official Gaia inventories.
    Md5,
    /// SHA-256 for NSB-generated provenance and artefacts.
    Sha256,
}

impl ChecksumAlgorithm {
    /// Canonical lowercase algorithm identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Md5 => "md5",
            Self::Sha256 => "sha256",
        }
    }

    const fn hex_len(self) -> usize {
        match self {
            Self::Md5 => 32,
            Self::Sha256 => 64,
        }
    }
}

/// Algorithm-qualified, validated lowercase hexadecimal checksum.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Checksum {
    algorithm: ChecksumAlgorithm,
    hex: String,
}

impl Checksum {
    /// Construct and validate a checksum value.
    pub fn new(algorithm: ChecksumAlgorithm, hex: impl Into<String>) -> Result<Self> {
        let hex = hex.into();
        if hex.len() != algorithm.hex_len()
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!(
                "invalid {} checksum: expected {} lowercase hexadecimal characters",
                algorithm.as_str(),
                algorithm.hex_len()
            );
        }
        Ok(Self { algorithm, hex })
    }

    /// Checksum algorithm.
    pub const fn algorithm(&self) -> ChecksumAlgorithm {
        self.algorithm
    }

    /// Unqualified lowercase hexadecimal digest.
    pub fn hex(&self) -> &str {
        &self.hex
    }
}

impl fmt::Display for Checksum {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.algorithm.as_str(), self.hex)
    }
}

impl FromStr for Checksum {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        if let Some((algorithm, hex)) = raw.split_once(':') {
            let algorithm = match algorithm {
                "md5" => ChecksumAlgorithm::Md5,
                "sha256" => ChecksumAlgorithm::Sha256,
                other => bail!("unsupported checksum algorithm {other:?}"),
            };
            return Self::new(algorithm, hex.to_string());
        }
        match raw.len() {
            32 => Self::new(ChecksumAlgorithm::Md5, raw.to_string()),
            64 => Self::new(ChecksumAlgorithm::Sha256, raw.to_string()),
            _ => bail!("checksum must be algorithm-qualified or have a recognized digest length"),
        }
    }
}

impl Serialize for Checksum {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Checksum {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn sha256_digest_file(path: &Path) -> Result<[u8; 32]> {
    let file = File::open(path)
        .with_context(|| format!("failed to open {} for SHA-256", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for SHA-256", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn md5_digest_file(path: &Path) -> Result<[u8; 16]> {
    let file =
        File::open(path).with_context(|| format!("failed to open {} for MD5", path.display()))?;
    let mut reader = BufReader::with_capacity(BUFFER_LEN, file);
    let mut hasher = Md5::new();
    let mut buffer = vec![0_u8; BUFFER_LEN];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {} for MD5", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

/// Compute an algorithm-qualified streaming checksum.
pub fn checksum_file(path: &Path, algorithm: ChecksumAlgorithm) -> Result<Checksum> {
    let hex = match algorithm {
        ChecksumAlgorithm::Md5 => encode_hex(&md5_digest_file(path)?),
        ChecksumAlgorithm::Sha256 => encode_hex(&sha256_digest_file(path)?),
    };
    Checksum::new(algorithm, hex)
}

/// Verify a file against a typed checksum without algorithm ambiguity.
pub fn verify_file(path: &Path, expected: &Checksum, label: &str) -> Result<()> {
    let actual = checksum_file(path, expected.algorithm())?;
    if &actual != expected {
        bail!(
            "{label} checksum mismatch for {}: expected {expected}, actual {actual}",
            path.display()
        );
    }
    Ok(())
}

/// Compute SHA-256 as unqualified hex for compatibility with existing reports.
pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(checksum_file(path, ChecksumAlgorithm::Sha256)?.hex)
}

/// Verify SHA-256 while accepting legacy unqualified values.
pub fn verify_sha256_file(path: &Path, expected: &str, label: &str) -> Result<()> {
    let checksum: Checksum = if expected.contains(':') {
        expected.parse()?
    } else {
        format!("sha256:{expected}").parse()?
    };
    if checksum.algorithm() != ChecksumAlgorithm::Sha256 {
        bail!("{label} requires a SHA-256 checksum, found {checksum}");
    }
    verify_file(path, &checksum, label)
}

/// Compute MD5 as unqualified hex for official Gaia inventory compatibility.
pub fn md5_file(path: &Path) -> Result<String> {
    Ok(checksum_file(path, ChecksumAlgorithm::Md5)?.hex)
}

/// Verify an official MD5 inventory value.
pub fn verify_md5_file(path: &Path, expected: &str, label: &str) -> Result<()> {
    let checksum: Checksum = if expected.contains(':') {
        expected.parse()?
    } else {
        format!("md5:{expected}").parse()?
    };
    if checksum.algorithm() != ChecksumAlgorithm::Md5 {
        bail!("{label} requires an MD5 checksum, found {checksum}");
    }
    verify_file(path, &checksum, label)
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
            checksum_file(&path, ChecksumAlgorithm::Sha256)?.to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            checksum_file(&path, ChecksumAlgorithm::Md5)?.to_string(),
            "md5:900150983cd24fb0d6963f7d28e17f72"
        );
        Ok(())
    }

    #[test]
    fn serde_is_canonical_and_legacy_hex_is_accepted() -> Result<()> {
        let checksum: Checksum =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".parse()?;
        assert_eq!(checksum.algorithm(), ChecksumAlgorithm::Sha256);
        assert_eq!(
            serde_json::to_string(&checksum)?,
            "\"sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\""
        );
        Ok(())
    }

    #[test]
    fn rejects_algorithm_mismatch() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fixture.bin");
        std::fs::write(&path, b"abc")?;
        let error = verify_sha256_file(&path, "md5:900150983cd24fb0d6963f7d28e17f72", "fixture")
            .expect_err("algorithm mismatch");
        assert!(error.to_string().contains("SHA-256"));
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
        assert_eq!(sha256_file(&path)?.len(), 64);
        Ok(())
    }
}
