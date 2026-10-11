// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Regression coverage for Gaia source inventory admission and reconciliation.

use super::*;

const TWO_PARTITIONS: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00-19.csv.gz\n",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb XpContinuous_20-29.csv.gz\n",
);

fn product() -> GaiaProductConfig {
    GaiaProductConfig {
        id: "xp-continuous".into(),
        base_url: "https://cdn.example.test/gaia/xp".into(),
        checksum_manifest_url: "https://cdn.example.test/gaia/xp/_MD5SUM.txt".into(),
        checksum_manifest_sha256: checksum_io::sha256_bytes(TWO_PARTITIONS.as_bytes()),
        checksum_algorithm: OfficialChecksumAlgorithm::Md5,
        expected_partitions: Some(2),
        filename_prefix: "XpContinuous_".into(),
        filename_suffix: ".csv.gz".into(),
    }
}

fn parsed_inventory() -> SourceInventory {
    parse_inventory(&product(), TWO_PARTITIONS.as_bytes()).unwrap()
}

fn write_inventory(path: &Path, inventory: &SourceInventory) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(inventory)?)?;
    Ok(())
}

#[test]
fn parser_ignores_comments_and_blank_lines_and_canonicalizes_digests() -> Result<()> {
    let manifest = concat!(
        "# official checksum listing\r\n",
        "  \r\n",
        "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB XpContinuous_20-29.csv.gz\r\n",
        "  # another comment\r\n",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA XpContinuous_00-19.csv.gz  \r\n",
    );
    let inventory = parse_inventory(&product(), manifest.as_bytes())?;
    assert_eq!(inventory.entries.len(), 2);
    assert_eq!(inventory.entries[0].partition_id, "00-19");
    assert_eq!(inventory.entries[1].partition_id, "20-29");
    assert_eq!(
        inventory.entries[0].official_checksum,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        inventory.entries[1].url,
        "https://cdn.example.test/gaia/xp/XpContinuous_20-29.csv.gz"
    );
    assert_eq!(
        inventory.checksum_manifest_sha256,
        checksum_io::sha256_bytes(manifest.as_bytes())
    );
    Ok(())
}

#[test]
fn parser_rejects_malformed_manifest_lines_and_empty_inventories() {
    let mut no_count = product();
    no_count.expected_partitions = None;
    for (label, manifest) in [
        ("missing filename", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"),
        (
            "extra fields",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00.csv.gz extra\n",
        ),
        (
            "invalid hex",
            "gggggggggggggggggggggggggggggggg XpContinuous_00.csv.gz\n",
        ),
        ("short digest", "abc XpContinuous_00.csv.gz\n"),
        ("no entries", "\n # comment only\n"),
        (
            "duplicate",
            concat!(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00.csv.gz\n",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb XpContinuous_00.csv.gz\n"
            ),
        ),
    ] {
        assert!(
            parse_inventory(&no_count, manifest.as_bytes()).is_err(),
            "{label} was accepted"
        );
    }
    assert!(parse_inventory(&no_count, b"\xff").is_err());
}

#[test]
fn parser_rejects_invalid_product_identifiers_and_insecure_base_urls() {
    for id in [
        "",
        "XP",
        "xp continuous",
        "../xp",
        "xp/continuous",
        "xp\u{00e9}",
    ] {
        let mut config = product();
        config.id = id.into();
        assert!(
            parse_inventory(&config, TWO_PARTITIONS.as_bytes()).is_err(),
            "{id:?}"
        );
    }
    for base_url in ["http://cdn.example.test/data/", "file:///tmp/data/"] {
        let mut config = product();
        config.base_url = base_url.into();
        assert!(
            parse_inventory(&config, TWO_PARTITIONS.as_bytes()).is_err(),
            "{base_url}"
        );
    }
}

#[test]
fn parser_rejects_unsafe_or_nonconforming_filenames() {
    let mut config = product();
    config.expected_partitions = None;
    for filename in [
        "../XpContinuous_00.csv.gz",
        "XpContinuous_../00.csv.gz",
        "XpContinuous_00\\evil.csv.gz",
        "Other_00.csv.gz",
        "XpContinuous_00.csv",
        "XpContinuous_00.csv.gz/extra",
    ] {
        let manifest = format!("{} {filename}\n", "a".repeat(32));
        assert!(
            parse_inventory(&config, manifest.as_bytes()).is_err(),
            "{filename:?} was accepted"
        );
    }
}

#[test]
fn parser_supports_sha256_and_enforces_algorithm_specific_digest_length() -> Result<()> {
    let mut config = product();
    config.checksum_algorithm = OfficialChecksumAlgorithm::Sha256;
    config.expected_partitions = Some(1);
    let manifest = format!("{} XpContinuous_00-19.csv.gz\n", "A".repeat(64));
    let inventory = parse_inventory(&config, manifest.as_bytes())?;
    assert_eq!(
        inventory.official_checksum_algorithm,
        OfficialChecksumAlgorithm::Sha256
    );
    assert_eq!(inventory.entries[0].official_checksum, "a".repeat(64));
    assert!(parse_inventory(
        &config,
        b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00-19.csv.gz\n"
    )
    .is_err());
    Ok(())
}

#[test]
fn parser_only_ignores_matching_esac_self_checksum_entry() {
    let mut config = product();
    config.expected_partitions = None;
    let self_checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa _MD5SUM.txt\n";
    let inventory = parse_inventory(&config, self_checksum.as_bytes());
    assert!(inventory.is_err(), "a self-entry is not a partition");
    config.checksum_manifest_url = "https://cdn.example.test/other/checksums.txt".into();
    assert!(parse_inventory(&config, self_checksum.as_bytes()).is_err());
}

#[test]
fn load_inventory_rejects_version_and_configuration_drift() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inventory.json");
    let config = product();
    let inventory = parsed_inventory();
    write_inventory(&path, &inventory)?;
    assert_eq!(load_inventory(&path, &config)?, inventory);

    let mut wrong_schema = inventory.clone();
    wrong_schema.schema_version += 1;
    write_inventory(&path, &wrong_schema)?;
    assert!(load_inventory(&path, &config)
        .unwrap_err()
        .to_string()
        .contains("schema"));
    write_inventory(&path, &inventory)?;

    let mut variants = Vec::new();
    let mut changed = config.clone();
    changed.id = "another-product".into();
    variants.push(changed);
    let mut changed = config.clone();
    changed.base_url = "https://elsewhere.example.test/".into();
    variants.push(changed);
    let mut changed = config.clone();
    changed.checksum_manifest_url = "https://elsewhere.example.test/_MD5SUM.txt".into();
    variants.push(changed);
    let mut changed = config.clone();
    changed.checksum_manifest_sha256 = "0".repeat(64);
    variants.push(changed);
    let mut changed = config.clone();
    changed.checksum_algorithm = OfficialChecksumAlgorithm::Sha256;
    variants.push(changed);
    let mut changed = config.clone();
    changed.expected_partitions = Some(3);
    variants.push(changed);

    for changed in variants {
        assert!(
            load_inventory(&path, &changed).is_err(),
            "a changed product configuration was accepted: {changed:?}"
        );
    }
    Ok(())
}

#[test]
fn load_inventory_rejects_noncanonical_or_unsorted_entries() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inventory.json");
    let config = product();
    let original = parsed_inventory();

    let mut mutations = Vec::new();
    let mut wrong = original.clone();
    wrong.entries[0].partition_id = "wrong".into();
    mutations.push(wrong);
    let mut wrong = original.clone();
    wrong.entries[0].url = "https://attacker.example.test/partition".into();
    mutations.push(wrong);
    let mut wrong = original.clone();
    wrong.entries[0].official_checksum = "xyz".into();
    mutations.push(wrong);
    let mut wrong = original.clone();
    wrong.entries[0].filename = "XpContinuous_../evil.csv.gz".into();
    mutations.push(wrong);
    let mut wrong = original.clone();
    wrong.entries.reverse();
    mutations.push(wrong);
    let mut wrong = original.clone();
    wrong.entries[1] = wrong.entries[0].clone();
    mutations.push(wrong);

    for mutation in mutations {
        write_inventory(&path, &mutation)?;
        assert!(
            load_inventory(&path, &config).is_err(),
            "mutated inventory was accepted: {mutation:?}"
        );
    }
    Ok(())
}

#[test]
fn load_inventory_rejects_unknown_fields_and_missing_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("inventory.json");
    assert!(load_inventory(&path, &product()).is_err());

    let mut value = serde_json::to_value(parsed_inventory())?;
    value["unrecognized"] = serde_json::json!(true);
    fs::write(&path, serde_json::to_vec(&value)?)?;
    assert!(load_inventory(&path, &product()).is_err());
    Ok(())
}

#[test]
fn partition_reconciliation_handles_empty_and_mismatched_sets() -> Result<()> {
    let directory = tempfile::tempdir()?;
    assert_eq!(production_partition_ids(directory.path(), &[])?, None);
    let first = product();
    assert_eq!(
        production_partition_ids(directory.path(), std::slice::from_ref(&first))?,
        None
    );

    let root = directory.path().join("inventories");
    fs::create_dir_all(&root)?;
    write_inventory(
        &root.join("xp-continuous.inventory.json"),
        &parsed_inventory(),
    )?;
    assert_eq!(
        production_partition_ids(directory.path(), std::slice::from_ref(&first))?,
        Some(vec!["00-19".into(), "20-29".into()])
    );

    let mut second = product();
    second.id = "gaia-source".into();
    second.filename_prefix = "GaiaSource_".into();
    let second_manifest = concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa GaiaSource_00-19.csv.gz\n",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb GaiaSource_30-39.csv.gz\n",
    );
    second.checksum_manifest_sha256 = checksum_io::sha256_bytes(second_manifest.as_bytes());
    let second_inventory = parse_inventory(&second, second_manifest.as_bytes())?;
    write_inventory(&root.join("gaia-source.inventory.json"), &second_inventory)?;
    assert!(production_partition_ids(directory.path(), &[first, second])
        .unwrap_err()
        .to_string()
        .contains("different partition ranges"));
    Ok(())
}
