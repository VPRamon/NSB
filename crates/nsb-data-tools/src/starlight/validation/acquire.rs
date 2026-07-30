//! Resumable, checksum-verified acquisition for independent validation
//! references.
//!
//! Most literature references relevant to independent all-sky starlight
//! validation are not machine-downloadable (digitized tables from decades-old
//! papers, data requested from an author, etc.). This module therefore
//! supports two sources per reference: an HTTPS URL (resumable download) or a
//! human-supplied local file (`--source id=/path/to/file`). A reference with
//! neither is reported as requiring manual acquisition; that is not an error.

use super::references::{ReferenceEntry, ReferencesDocument};
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RECEIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceAcquisitionReceipt {
    pub schema_version: u32,
    pub reference_id: String,
    pub source: String,
    pub acquired_at_unix_seconds: u64,
    pub sha256: String,
    pub bytes: u64,
    pub object_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquisitionOutcome {
    /// Already acquired in a previous run; nothing changed.
    AlreadySatisfied,
    /// Newly downloaded or copied and verified in this run.
    Acquired,
    /// Neither a CLI override nor a document URL is available; a human must
    /// supply the file out of band.
    ManualAcquisitionRequired,
}

#[derive(Debug, Clone)]
pub struct AcquisitionResult {
    pub reference_id: String,
    pub outcome: AcquisitionOutcome,
    pub detail: String,
}

/// Attempt to acquire every reference in `document`. Sources in `overrides`
/// (reference id -> URL or local path) take priority over the document's own
/// `acquisition_url`. Fails closed (returns `Err`) on any checksum mismatch;
/// returns `Ok` with per-reference outcomes otherwise, including references
/// that still require manual acquisition.
pub fn acquire_references(
    document: &ReferencesDocument,
    workspace: &Path,
    overrides: &BTreeMap<String, String>,
) -> Result<Vec<AcquisitionResult>> {
    document.validate()?;
    let client = Client::builder()
        .user_agent(concat!("nsb-data-tools/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build HTTP client for reference acquisition")?;
    let mut results = Vec::with_capacity(document.references.len());
    for reference in &document.references {
        let source = overrides
            .get(&reference.id)
            .cloned()
            .or_else(|| reference.acquisition_url.clone());
        let Some(source) = source else {
            results.push(AcquisitionResult {
                reference_id: reference.id.clone(),
                outcome: AcquisitionOutcome::ManualAcquisitionRequired,
                detail: "no acquisition URL or --source override supplied; acquire the file manually and re-run with --source".to_string(),
            });
            continue;
        };
        let result = acquire_one(&client, workspace, reference, &source)?;
        results.push(result);
    }
    Ok(results)
}

fn receipt_path(workspace: &Path, reference_id: &str) -> PathBuf {
    workspace
        .join("receipts")
        .join(format!("{reference_id}.json"))
}

fn acquire_one(
    client: &Client,
    workspace: &Path,
    reference: &ReferenceEntry,
    source: &str,
) -> Result<AcquisitionResult> {
    let receipt_path = receipt_path(workspace, &reference.id);
    if receipt_path.is_file() {
        let receipt = read_receipt(&receipt_path)?;
        if receipt.source == source {
            verify_receipt(&receipt, reference)?;
            return Ok(AcquisitionResult {
                reference_id: reference.id.clone(),
                outcome: AcquisitionOutcome::AlreadySatisfied,
                detail: format!("already acquired at {}", receipt.object_path.display()),
            });
        }
    }

    let is_local = Path::new(source).is_file();
    let staged = if is_local {
        Path::new(source).to_path_buf()
    } else {
        download_to_staging(client, workspace, &reference.id, source)?
    };

    let sha256 = checksum_io::sha256_file(&staged)
        .with_context(|| format!("hash staged reference file {}", staged.display()))?;
    if let Some(expected) = &reference.sha256 {
        if expected != &sha256 {
            if !is_local {
                let _ = fs::remove_file(&staged);
            }
            bail!(
                "reference {} checksum mismatch: expected {expected}, actual {sha256}; refusing to write a receipt",
                reference.id
            );
        }
    }
    let bytes = staged
        .metadata()
        .with_context(|| format!("stat staged reference file {}", staged.display()))?
        .len();

    let object_path = workspace.join("objects/sha256").join(&sha256);
    if !object_path.is_file() {
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if is_local {
            fs::copy(&staged, &object_path).with_context(|| {
                format!("copy {} into content-addressed store", staged.display())
            })?;
        } else {
            fs::rename(&staged, &object_path).with_context(|| {
                format!("commit {} into content-addressed store", staged.display())
            })?;
        }
    } else if !is_local {
        let _ = fs::remove_file(&staged);
    }

    let receipt = ReferenceAcquisitionReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        reference_id: reference.id.clone(),
        source: source.to_string(),
        acquired_at_unix_seconds: unix_seconds()?,
        sha256,
        bytes,
        object_path,
    };
    artifact_store::atomic_write(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
    Ok(AcquisitionResult {
        reference_id: reference.id.clone(),
        outcome: AcquisitionOutcome::Acquired,
        detail: format!(
            "acquired from {source} and verified at {}",
            receipt.object_path.display()
        ),
    })
}

fn download_to_staging(
    client: &Client,
    workspace: &Path,
    reference_id: &str,
    url: &str,
) -> Result<PathBuf> {
    let partial = workspace
        .join("partial")
        .join(format!("{reference_id}.part"));
    if let Some(parent) = partial.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = partial.metadata().map_or(0, |metadata| metadata.len());
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .with_context(|| format!("request failed for reference URL {url}"))?;
    let append = existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    if !append && !response.status().is_success() {
        bail!(
            "reference download failed for {url}: HTTP {}",
            response.status()
        );
    }
    stream_response(response, &partial, append)?;
    Ok(partial)
}

fn stream_response(mut response: Response, partial: &Path, append: bool) -> Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(partial)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    io::copy(&mut response, &mut writer)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}

fn read_receipt(path: &Path) -> Result<ReferenceAcquisitionReceipt> {
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("invalid reference acquisition receipt {}", path.display()))
}

fn verify_receipt(receipt: &ReferenceAcquisitionReceipt, reference: &ReferenceEntry) -> Result<()> {
    if receipt.schema_version != RECEIPT_SCHEMA_VERSION || receipt.reference_id != reference.id {
        bail!(
            "acquisition receipt for {} does not match the reference registry",
            reference.id
        );
    }
    if let Some(expected) = &reference.sha256 {
        if expected != &receipt.sha256 {
            bail!(
                "acquisition receipt checksum for {} no longer matches the pinned registry checksum",
                reference.id
            );
        }
    }
    checksum_io::verify_sha256_file(
        &receipt.object_path,
        &format!("sha256:{}", receipt.sha256),
        "cached reference object",
    )?;
    if receipt.object_path.metadata()?.len() != receipt.bytes {
        bail!(
            "cached reference object size does not match its receipt for {}",
            reference.id
        );
    }
    Ok(())
}

fn unix_seconds() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// Whether reference `id` currently has a valid, verified acquisition
/// receipt under `workspace`.
pub fn is_satisfied(workspace: &Path, reference: &ReferenceEntry) -> Result<bool> {
    let path = receipt_path(workspace, &reference.id);
    if !path.is_file() {
        return Ok(false);
    }
    match read_receipt(&path)
        .and_then(|receipt| verify_receipt(&receipt, reference).map(|()| receipt))
    {
        Ok(_) => Ok(true),
        Err(error)
            if error.chain().any(|cause| {
                cause
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == io::ErrorKind::NotFound)
            }) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::validation::references::{ReferenceStatus, REFERENCES_SCHEMA_VERSION};
    use tempfile::TempDir;

    fn document(entries: Vec<ReferenceEntry>) -> ReferencesDocument {
        ReferencesDocument {
            schema_version: REFERENCES_SCHEMA_VERSION,
            acquisition_required: true,
            notes: "test".to_string(),
            references: entries,
        }
    }

    fn pending_entry(id: &str) -> ReferenceEntry {
        ReferenceEntry {
            id: id.to_string(),
            citation: "Author (Year)".to_string(),
            description: "test".to_string(),
            coverage: "all-sky".to_string(),
            wavelength_band_nm: [300.0, 650.0],
            spectral_quantity: "photon radiance".to_string(),
            transformation_to_target: "identity".to_string(),
            acquisition_url: None,
            license: "unknown, requires acquisition-time licence check".to_string(),
            status: ReferenceStatus::PendingAcquisition,
            sha256: None,
            filename: format!("{id}.dat"),
            acquisition_notes: "manual".to_string(),
        }
    }

    #[test]
    fn missing_source_requires_manual_acquisition() {
        let temp = TempDir::new().unwrap();
        let document = document(vec![pending_entry("a"), pending_entry("b")]);
        let results = acquire_references(&document, temp.path(), &BTreeMap::new()).unwrap();
        assert!(results
            .iter()
            .all(|result| result.outcome == AcquisitionOutcome::ManualAcquisitionRequired));
    }

    #[test]
    fn local_file_override_is_acquired_and_resumable() {
        let temp = TempDir::new().unwrap();
        let source_file = temp.path().join("source.dat");
        fs::write(&source_file, b"reference bytes").unwrap();
        let document = document(vec![pending_entry("a"), pending_entry("b")]);
        let overrides =
            BTreeMap::from([("a".to_string(), source_file.to_string_lossy().into_owned())]);
        let workspace = temp.path().join("workspace");
        let results = acquire_references(&document, &workspace, &overrides).unwrap();
        let a = results
            .iter()
            .find(|result| result.reference_id == "a")
            .unwrap();
        assert_eq!(a.outcome, AcquisitionOutcome::Acquired);
        assert!(is_satisfied(&workspace, &document.references[0]).unwrap());

        // Re-running is idempotent and does not re-copy.
        let results = acquire_references(&document, &workspace, &overrides).unwrap();
        let a = results
            .iter()
            .find(|result| result.reference_id == "a")
            .unwrap();
        assert_eq!(a.outcome, AcquisitionOutcome::AlreadySatisfied);
    }

    #[test]
    fn checksum_mismatch_fails_closed_and_writes_no_receipt() {
        let temp = TempDir::new().unwrap();
        let source_file = temp.path().join("source.dat");
        fs::write(&source_file, b"reference bytes").unwrap();
        let mut entry = pending_entry("a");
        entry.sha256 = Some("0".repeat(64));
        entry.status = ReferenceStatus::Acquired;
        let mut document = document(vec![entry, pending_entry("b")]);
        document.acquisition_required = false;
        let overrides =
            BTreeMap::from([("a".to_string(), source_file.to_string_lossy().into_owned())]);
        let workspace = temp.path().join("workspace");
        let error = acquire_references(&document, &workspace, &overrides).unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
        assert!(!receipt_path(&workspace, "a").exists());
    }
}
