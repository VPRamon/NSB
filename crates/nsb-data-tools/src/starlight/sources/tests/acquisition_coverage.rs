// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Offline regression tests for verified Gaia source acquisition.

use super::*;
use crate::starlight::sources::inventory::parse_inventory;

const MANIFEST: &str = "900150983cd24fb0d6963f7d28e17f72 File_00-01.csv.gz\n";
const SHA256_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn product() -> GaiaProductConfig {
    GaiaProductConfig {
        id: "product".into(),
        base_url: "https://cdn.example.test/gaia/".into(),
        checksum_manifest_url: "https://cdn.example.test/gaia/_MD5SUM.txt".into(),
        checksum_manifest_sha256: checksum_io::sha256_bytes(MANIFEST.as_bytes()),
        checksum_algorithm: OfficialChecksumAlgorithm::Md5,
        expected_partitions: Some(1),
        filename_prefix: "File_".into(),
        filename_suffix: ".csv.gz".into(),
    }
}

fn entry() -> SourceInventoryEntry {
    SourceInventoryEntry {
        partition_id: "00-01".into(),
        filename: "File_00-01.csv.gz".into(),
        url: "https://cdn.example.test/gaia/File_00-01.csv.gz".into(),
        official_checksum: "900150983cd24fb0d6963f7d28e17f72".into(),
    }
}

fn fixture(
    workspace: &Path,
) -> Result<(GaiaProductConfig, SourceInventoryEntry, AcquisitionReceipt)> {
    let product = product();
    let entry = entry();
    let inventories = workspace.join("inventories");
    fs::create_dir_all(&inventories)?;
    let inventory = parse_inventory(&product, MANIFEST.as_bytes())?;
    fs::write(
        inventories.join("product.inventory.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;

    let cache = workspace.join("cache");
    let partial = cache.join("partial/product/File_00-01.csv.gz.part");
    fs::create_dir_all(partial.parent().unwrap())?;
    fs::write(&partial, b"abc")?;
    verify_official(&partial, &product, &entry)?;
    let receipt = commit_verified(&cache, &product, &entry, &partial)?;
    let receipts = cache.join("receipts/product");
    fs::create_dir_all(&receipts)?;
    fs::write(
        receipts.join("00-01.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok((product, entry, receipt))
}

#[test]
fn cached_receipts_allow_offline_acquisition_and_object_resolution() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, _, receipt) = fixture(directory.path())?;
    let products = [product];
    let artifacts = acquire_partition(
        directory.path(),
        &products,
        &AcquisitionConfig::default(),
        "00-01",
    )?;
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].name, "product/File_00-01.csv.gz");
    assert_eq!(artifacts[0].path, receipt.object_path);
    assert_eq!(artifacts[0].sha256, SHA256_ABC);
    assert_eq!(artifacts[0].bytes, 3);
    assert_eq!(
        verified_object_for_partition(directory.path(), &products, "product", "00-01")?,
        receipt.object_path
    );
    Ok(())
}

#[test]
fn offline_resolution_rejects_unknown_products_partitions_and_missing_receipts() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, _, _) = fixture(directory.path())?;
    let products = [product];
    assert!(
        verified_object_for_partition(directory.path(), &products, "unknown", "00-01")
            .unwrap_err()
            .to_string()
            .contains("not configured")
    );
    assert!(
        verified_object_for_partition(directory.path(), &products, "product", "missing")
            .unwrap_err()
            .to_string()
            .contains("absent")
    );
    assert!(acquire_partition(
        directory.path(),
        &products,
        &AcquisitionConfig::default(),
        "missing"
    )
    .unwrap_err()
    .to_string()
    .contains("absent"));
    fs::remove_file(directory.path().join("cache/receipts/product/00-01.json"))?;
    assert!(
        verified_object_for_partition(directory.path(), &products, "product", "00-01")
            .unwrap_err()
            .to_string()
            .contains("run Starlight update")
    );
    Ok(())
}

#[test]
fn acquisition_rejects_zero_timeouts_or_retry_budget_before_network_access() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let defaults = AcquisitionConfig::default();
    for policy in [
        AcquisitionConfig {
            connect_timeout_seconds: 0,
            ..defaults.clone()
        },
        AcquisitionConfig {
            request_timeout_seconds: 0,
            ..defaults.clone()
        },
        AcquisitionConfig {
            max_attempts: 0,
            ..defaults.clone()
        },
    ] {
        assert!(acquire_partition(directory.path(), &[], &policy, "00-01")
            .unwrap_err()
            .to_string()
            .contains("greater than zero"));
    }
    assert!(
        acquire_partition(directory.path(), &[product()], &defaults, "00-01").is_err(),
        "an absent inventory must fail before a download is attempted"
    );
    Ok(())
}

#[test]
fn receipt_verification_rejects_each_identity_mutation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, entry, original) = fixture(directory.path())?;

    let mut mutants = Vec::new();
    let mut invalid = original.clone();
    invalid.schema_version += 1;
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.product_id = "wrong".into();
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.partition_id = "wrong".into();
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.filename = "wrong.csv.gz".into();
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.source_url = "https://other.example.test/file".into();
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.official_checksum_algorithm = OfficialChecksumAlgorithm::Sha256;
    mutants.push(invalid);
    let mut invalid = original.clone();
    invalid.official_checksum = "0".repeat(32);
    mutants.push(invalid);

    for invalid in mutants {
        assert!(
            verify_receipt(&invalid, &product, &entry)
                .unwrap_err()
                .to_string()
                .contains("does not match its inventory"),
            "a mutated receipt was accepted: {invalid:?}"
        );
    }
    Ok(())
}

#[test]
fn receipt_verification_rejects_bad_checksum_size_and_missing_object() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, entry, original) = fixture(directory.path())?;
    let mut wrong_digest = original.clone();
    wrong_digest.sha256 = "0".repeat(64);
    assert!(verify_receipt(&wrong_digest, &product, &entry).is_err());

    let mut wrong_size = original.clone();
    wrong_size.bytes += 1;
    assert!(verify_receipt(&wrong_size, &product, &entry)
        .unwrap_err()
        .to_string()
        .contains("size does not match"));

    fs::remove_file(&original.object_path)?;
    assert!(verify_receipt(&original, &product, &entry).is_err());
    Ok(())
}

#[test]
fn receipt_reader_fails_closed_on_invalid_json_and_unknown_fields() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, entry, receipt) = fixture(directory.path())?;
    let path = directory.path().join("cache/receipts/product/00-01.json");
    assert_eq!(read_receipt(&path)?, receipt);
    fs::write(&path, b"not-json")?;
    assert!(read_receipt(&path).is_err());
    let mut value = serde_json::to_value(&receipt)?;
    value["unexpected"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec(&value)?)?;
    assert!(read_receipt(&path).is_err());
    assert!(verify_receipt(&receipt, &product, &entry).is_ok());
    Ok(())
}

#[test]
fn official_sha256_and_md5_checksums_are_enforced() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("download.part");
    fs::write(&path, b"abc")?;
    let mut sha256_product = product();
    sha256_product.checksum_algorithm = OfficialChecksumAlgorithm::Sha256;
    let mut sha256_entry = entry();
    sha256_entry.official_checksum = SHA256_ABC.into();
    verify_official(&path, &sha256_product, &sha256_entry)?;

    sha256_entry.official_checksum = "0".repeat(64);
    assert!(verify_official(&path, &sha256_product, &sha256_entry).is_err());
    let mut md5_entry = entry();
    md5_entry.official_checksum = "0".repeat(32);
    assert!(verify_official(&path, &product(), &md5_entry).is_err());
    Ok(())
}

#[test]
fn content_addressed_cache_reuses_verified_objects_and_rejects_corruption() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let (product, entry, first) = fixture(directory.path())?;
    let partial = directory.path().join("second.part");
    fs::write(&partial, b"abc")?;
    let second = commit_verified(&directory.path().join("cache"), &product, &entry, &partial)?;
    assert_eq!(first, second);
    assert!(!partial.exists(), "duplicate partial must be removed");
    assert_eq!(fs::read(&second.object_path)?, b"abc");

    fs::write(&partial, b"abc")?;
    fs::write(&first.object_path, b"corrupted")?;
    assert!(commit_verified(&directory.path().join("cache"), &product, &entry, &partial).is_err());
    Ok(())
}
