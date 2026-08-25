//! Resumable, checksum-verified Gaia partition acquisition.

use super::inventory::{load_inventory, SourceInventoryEntry};
use crate::dataset::Artifact;
use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::{AcquisitionConfig, GaiaProductConfig, OfficialChecksumAlgorithm};
use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use reqwest::header::RANGE;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const RECEIPT_SCHEMA_VERSION: u32 = 1;

/// Immutable proof that one upstream object entered the local cache unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionReceipt {
    pub schema_version: u32,
    pub product_id: String,
    pub partition_id: String,
    pub filename: String,
    pub source_url: String,
    pub official_checksum_algorithm: OfficialChecksumAlgorithm,
    pub official_checksum: String,
    pub sha256: String,
    pub bytes: u64,
    pub object_path: PathBuf,
}

/// Acquire every required product for one reconciled Gaia partition.
pub(crate) fn acquire_partition(
    workspace: &Path,
    products: &[GaiaProductConfig],
    policy: &AcquisitionConfig,
    partition_id: &str,
) -> Result<Vec<Artifact>> {
    if policy.connect_timeout_seconds == 0
        || policy.request_timeout_seconds == 0
        || policy.max_attempts == 0
    {
        bail!("Starlight acquisition timeouts and max_attempts must be greater than zero");
    }
    let client = Client::builder()
        .user_agent(concat!("nsb-data-tools/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(policy.connect_timeout_seconds))
        .timeout(Duration::from_secs(policy.request_timeout_seconds))
        .build()?;
    let mut artifacts = Vec::with_capacity(products.len());
    for product in products {
        let inventory_path = workspace
            .join("inventories")
            .join(format!("{}.inventory.json", product.id));
        let inventory = load_inventory(&inventory_path, product)?;
        let entry = inventory
            .entries
            .iter()
            .find(|entry| entry.partition_id == partition_id)
            .with_context(|| {
                format!(
                    "partition {partition_id:?} is absent from Gaia product {}",
                    product.id
                )
            })?;
        let receipt = acquire_entry(workspace, product, entry, policy, &client)?;
        artifacts.push(Artifact {
            name: format!("{}/{}", product.id, entry.filename),
            path: receipt.object_path,
            sha256: receipt.sha256,
            bytes: receipt.bytes,
        });
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(artifacts)
}

fn acquire_entry(
    workspace: &Path,
    product: &GaiaProductConfig,
    entry: &SourceInventoryEntry,
    policy: &AcquisitionConfig,
    client: &Client,
) -> Result<AcquisitionReceipt> {
    let cache = workspace.join("cache");
    let receipt_path = cache
        .join("receipts")
        .join(&product.id)
        .join(format!("{}.json", entry.partition_id));
    if receipt_path.is_file() {
        let receipt = read_receipt(&receipt_path)?;
        verify_receipt(&receipt, product, entry)?;
        return Ok(receipt);
    }

    let lock_path = cache
        .join("locks")
        .join(&product.id)
        .join(format!("{}.lock", entry.partition_id));
    let _lock = AcquisitionLock::acquire(&lock_path)?;
    if receipt_path.is_file() {
        let receipt = read_receipt(&receipt_path)?;
        verify_receipt(&receipt, product, entry)?;
        return Ok(receipt);
    }

    let partial = cache
        .join("partial")
        .join(&product.id)
        .join(format!("{}.part", entry.filename));
    if let Some(parent) = partial.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut last_error = None;
    for attempt in 1..=policy.max_attempts {
        match download_attempt(client, &entry.url, &partial)
            .and_then(|()| verify_official(&partial, product, entry))
        {
            Ok(()) => {
                let receipt = commit_verified(&cache, product, entry, &partial)?;
                artifact_store::atomic_write(&receipt_path, &serde_json::to_vec_pretty(&receipt)?)?;
                return Ok(receipt);
            }
            Err(error) => {
                last_error = Some(error);
                if attempt < policy.max_attempts {
                    thread::sleep(Duration::from_millis(
                        250_u64.saturating_mul(1_u64 << (attempt - 1).min(6)),
                    ));
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("Gaia acquisition failed without an error"))
        .context(format!(
            "failed to acquire {} after {} attempts",
            entry.url, policy.max_attempts
        )))
}

fn download_attempt(client: &Client, url: &str, partial: &Path) -> Result<()> {
    let existing = partial.metadata().map_or(0, |metadata| metadata.len());
    let mut request = client.get(url);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let response = request
        .send()
        .with_context(|| format!("request failed for {url}"))?;
    let append = existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    if !append && !response.status().is_success() {
        return Err(response
            .error_for_status()
            .expect_err("non-success status checked above")
            .into());
    }
    stream_response(response, partial, append)
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

fn verify_official(
    path: &Path,
    product: &GaiaProductConfig,
    entry: &SourceInventoryEntry,
) -> Result<()> {
    match product.checksum_algorithm {
        OfficialChecksumAlgorithm::Md5 => {
            checksum_io::verify_md5_file(path, &entry.official_checksum, &entry.filename)
        }
        OfficialChecksumAlgorithm::Sha256 => checksum_io::verify_sha256_file(
            path,
            &format!("sha256:{}", entry.official_checksum),
            &entry.filename,
        ),
    }
}

fn commit_verified(
    cache: &Path,
    product: &GaiaProductConfig,
    entry: &SourceInventoryEntry,
    partial: &Path,
) -> Result<AcquisitionReceipt> {
    let sha256 = checksum_io::sha256_file(partial)?;
    let bytes = partial.metadata()?.len();
    let object_path = cache.join("objects/sha256").join(&sha256);
    if object_path.is_file() {
        checksum_io::verify_sha256_file(
            &object_path,
            &format!("sha256:{sha256}"),
            "cached Gaia object",
        )?;
        fs::remove_file(partial)?;
    } else {
        let parent = object_path
            .parent()
            .context("content-addressed object has no parent")?;
        fs::create_dir_all(parent)?;
        fs::rename(partial, &object_path)?;
        File::open(&object_path)?.sync_all()?;
        File::open(parent)?.sync_all()?;
    }
    Ok(AcquisitionReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        product_id: product.id.clone(),
        partition_id: entry.partition_id.clone(),
        filename: entry.filename.clone(),
        source_url: entry.url.clone(),
        official_checksum_algorithm: product.checksum_algorithm,
        official_checksum: entry.official_checksum.clone(),
        sha256,
        bytes,
        object_path,
    })
}

fn read_receipt(path: &Path) -> Result<AcquisitionReceipt> {
    serde_json::from_reader(BufReader::new(File::open(path)?))
        .with_context(|| format!("invalid acquisition receipt {}", path.display()))
}

fn verify_receipt(
    receipt: &AcquisitionReceipt,
    product: &GaiaProductConfig,
    entry: &SourceInventoryEntry,
) -> Result<()> {
    if receipt.schema_version != RECEIPT_SCHEMA_VERSION
        || receipt.product_id != product.id
        || receipt.partition_id != entry.partition_id
        || receipt.filename != entry.filename
        || receipt.source_url != entry.url
        || receipt.official_checksum_algorithm != product.checksum_algorithm
        || receipt.official_checksum != entry.official_checksum
    {
        bail!(
            "acquisition receipt for {}/{} does not match its inventory",
            product.id,
            entry.partition_id
        );
    }
    checksum_io::verify_sha256_file(
        &receipt.object_path,
        &format!("sha256:{}", receipt.sha256),
        "cached Gaia object",
    )?;
    if receipt.object_path.metadata()?.len() != receipt.bytes {
        bail!("cached Gaia object size does not match its receipt");
    }
    Ok(())
}

/// Resolve one verified content-addressed object from its immutable receipt.
pub(crate) fn verified_object_for_partition(
    workspace: &Path,
    products: &[GaiaProductConfig],
    product_id: &str,
    partition_id: &str,
) -> Result<PathBuf> {
    let product = products
        .iter()
        .find(|product| product.id == product_id)
        .with_context(|| format!("Starlight product {product_id:?} is not configured"))?;
    let inventory_path = workspace
        .join("inventories")
        .join(format!("{}.inventory.json", product.id));
    let inventory = load_inventory(&inventory_path, product)?;
    let entry = inventory
        .entries
        .iter()
        .find(|entry| entry.partition_id == partition_id)
        .with_context(|| {
            format!(
                "partition {partition_id:?} is absent from Gaia product {}",
                product.id
            )
        })?;
    let receipt_path = workspace
        .join("cache/receipts")
        .join(&product.id)
        .join(format!("{partition_id}.json"));
    let receipt = read_receipt(&receipt_path)
        .with_context(|| format!("run Starlight update for {product_id}/{partition_id}"))?;
    verify_receipt(&receipt, product, entry)?;
    Ok(receipt.object_path)
}

struct AcquisitionLock {
    path: PathBuf,
}

impl AcquisitionLock {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .with_context(|| {
                format!(
                    "partition acquisition is already locked at {}; retry after the active worker finishes",
                    path.display()
                )
            })?
            .write_all(format!("pid={}\n", std::process::id()).as_bytes())?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for AcquisitionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_content_addressed_object_and_rejects_tampering() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let partial = directory.path().join("cache/partial/product/file.part");
        fs::create_dir_all(partial.parent().unwrap())?;
        fs::write(&partial, b"abc")?;
        let product = GaiaProductConfig {
            id: "product".to_string(),
            base_url: "https://example.test/".to_string(),
            checksum_manifest_url: "https://example.test/_MD5SUM.txt".to_string(),
            checksum_manifest_sha256:
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            checksum_algorithm: OfficialChecksumAlgorithm::Md5,
            expected_partitions: Some(1),
            filename_prefix: "File_".to_string(),
            filename_suffix: ".csv.gz".to_string(),
        };
        let entry = SourceInventoryEntry {
            partition_id: "00-01".to_string(),
            filename: "File_00-01.csv.gz".to_string(),
            url: "https://example.test/File_00-01.csv.gz".to_string(),
            official_checksum: "900150983cd24fb0d6963f7d28e17f72".to_string(),
        };
        verify_official(&partial, &product, &entry)?;
        let receipt = commit_verified(&directory.path().join("cache"), &product, &entry, &partial)?;
        assert_eq!(
            receipt.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        verify_receipt(&receipt, &product, &entry)?;
        fs::write(&receipt.object_path, b"tampered")?;
        assert!(verify_receipt(&receipt, &product, &entry).is_err());
        Ok(())
    }

    #[test]
    fn lock_is_exclusive_and_released_on_drop() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("locks/partition.lock");
        let first = AcquisitionLock::acquire(&path)?;
        assert!(AcquisitionLock::acquire(&path).is_err());
        drop(first);
        AcquisitionLock::acquire(&path)?;
        Ok(())
    }
}
