//! Strict normalization of official Gaia bulk checksum inventories.

use crate::dataset::Artifact;
use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::{GaiaProductConfig, OfficialChecksumAlgorithm};
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const INVENTORY_SCHEMA_VERSION: u32 = 1;

/// Immutable normalized source inventory for one Gaia bulk product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInventory {
    pub schema_version: u32,
    pub product_id: String,
    pub base_url: String,
    pub checksum_manifest_url: String,
    pub checksum_manifest_sha256: String,
    pub official_checksum_algorithm: OfficialChecksumAlgorithm,
    pub entries: Vec<SourceInventoryEntry>,
}

/// One immutable upstream partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInventoryEntry {
    pub partition_id: String,
    pub filename: String,
    pub url: String,
    pub official_checksum: String,
}

/// Load and reconcile the source products required by full Starlight.
///
/// Both inventories must be absent on a clean workspace or both must be
/// present and contain exactly the same partition ranges.
pub(crate) fn production_partition_ids(
    workspace: &Path,
    products: &[GaiaProductConfig],
) -> Result<Option<Vec<String>>> {
    let root = workspace.join("inventories");
    let mut inventories = Vec::with_capacity(products.len());
    let mut missing = Vec::new();
    for product in products {
        let path = root.join(format!("{}.inventory.json", product.id));
        if path.is_file() {
            inventories.push(load_inventory(&path, product)?);
        } else {
            missing.push(product.id.as_str());
        }
    }
    if inventories.is_empty() {
        return Ok(None);
    }
    if !missing.is_empty() {
        bail!(
            "Gaia inventory set is incomplete; missing {}",
            missing.join(", ")
        );
    }
    let reference = inventories
        .first()
        .context("production Starlight has no Gaia product inventories")?;
    let partition_ids: Vec<String> = reference
        .entries
        .iter()
        .map(|entry| entry.partition_id.clone())
        .collect();
    for inventory in inventories.iter().skip(1) {
        let candidate: Vec<&str> = inventory
            .entries
            .iter()
            .map(|entry| entry.partition_id.as_str())
            .collect();
        if candidate != partition_ids.iter().map(String::as_str).collect::<Vec<_>>() {
            bail!(
                "Gaia inventories {} and {} have different partition ranges",
                reference.product_id,
                inventory.product_id
            );
        }
    }
    Ok(Some(partition_ids))
}

/// Load one normalized inventory and verify it still matches its configuration.
pub(crate) fn load_inventory(path: &Path, product: &GaiaProductConfig) -> Result<SourceInventory> {
    let raw = fs::read(path)
        .with_context(|| format!("failed to read Gaia inventory {}", path.display()))?;
    let inventory: SourceInventory = serde_json::from_slice(&raw)
        .with_context(|| format!("invalid Gaia inventory {}", path.display()))?;
    if inventory.schema_version != INVENTORY_SCHEMA_VERSION {
        bail!(
            "unsupported Gaia inventory schema {} in {}",
            inventory.schema_version,
            path.display()
        );
    }
    if inventory.product_id != product.id
        || inventory.base_url != normalized_base_url(&product.base_url)?.to_string()
        || inventory.checksum_manifest_url != product.checksum_manifest_url
        || inventory.checksum_manifest_sha256 != product.checksum_manifest_sha256
        || inventory.official_checksum_algorithm != product.checksum_algorithm
    {
        bail!(
            "Gaia inventory {} does not match the current product configuration",
            path.display()
        );
    }
    if let Some(expected) = product.expected_partitions {
        if inventory.entries.len() != expected {
            bail!(
                "Gaia inventory {} has {} partitions; expected {}",
                product.id,
                inventory.entries.len(),
                expected
            );
        }
    }
    let mut previous: Option<&str> = None;
    for entry in &inventory.entries {
        validate_filename(product, &entry.filename, 0)?;
        validate_digest(product.checksum_algorithm, &entry.official_checksum, 0)?;
        let expected_partition = partition_id(
            &entry.filename,
            &product.filename_prefix,
            &product.filename_suffix,
        );
        let expected_url = normalized_base_url(&product.base_url)?
            .join(&entry.filename)?
            .to_string();
        if entry.partition_id != expected_partition || entry.url != expected_url {
            bail!(
                "Gaia inventory {} contains a non-canonical entry for {}",
                product.id,
                entry.filename
            );
        }
        if previous.is_some_and(|value| value >= entry.filename.as_str()) {
            bail!(
                "Gaia inventory {} entries are not strictly sorted",
                product.id
            );
        }
        previous = Some(&entry.filename);
    }
    Ok(inventory)
}

pub(crate) fn update_inventories(
    workspace: &Path,
    products: &[GaiaProductConfig],
) -> Result<Vec<Artifact>> {
    let client = Client::builder()
        .user_agent(concat!("nsb-data-tools/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build()?;
    let root = workspace.join("inventories");
    fs::create_dir_all(&root)?;
    let mut artifacts = Vec::new();
    for product in products {
        let response = client
            .get(&product.checksum_manifest_url)
            .send()
            .with_context(|| {
                format!(
                    "failed to fetch Gaia inventory {}",
                    product.checksum_manifest_url
                )
            })?
            .error_for_status()?;
        let raw = response.bytes()?;
        let actual_manifest_sha256 = checksum_io::sha256_bytes(&raw);
        if actual_manifest_sha256 != product.checksum_manifest_sha256 {
            bail!(
                "Gaia inventory {} checksum mismatch: expected {}, found {}",
                product.id,
                product.checksum_manifest_sha256,
                actual_manifest_sha256
            );
        }
        let inventory = parse_inventory(product, &raw)?;
        let upstream_path = root.join(format!("{}.upstream-checksums.txt", product.id));
        artifact_store::atomic_write(&upstream_path, &raw)?;
        artifacts.push(artifact(
            &format!("{}-upstream-checksums", product.id),
            &upstream_path,
        )?);

        let inventory_path = root.join(format!("{}.inventory.json", product.id));
        artifact_store::atomic_write(&inventory_path, &serde_json::to_vec_pretty(&inventory)?)?;
        artifacts.push(artifact(
            &format!("{}-inventory", product.id),
            &inventory_path,
        )?);
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    artifact_store::atomic_write(
        &root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

/// Parse and validate an upstream checksum manifest without network access.
pub fn parse_inventory(product: &GaiaProductConfig, raw: &[u8]) -> Result<SourceInventory> {
    validate_product_id(&product.id)?;
    let base = normalized_base_url(&product.base_url)?;
    let text = std::str::from_utf8(raw).context("Gaia checksum manifest is not UTF-8")?;
    let mut entries = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let digest = fields.next().unwrap_or_default().to_ascii_lowercase();
        let filename = fields
            .next()
            .with_context(|| format!("inventory line {} has no filename", line_index + 1))?;
        if fields.next().is_some() {
            bail!("inventory line {} has unexpected fields", line_index + 1);
        }
        validate_digest(product.checksum_algorithm, &digest, line_index + 1)?;
        if filename == "_MD5SUM.txt" && product.checksum_manifest_url.ends_with("/_MD5SUM.txt") {
            continue;
        }
        validate_filename(product, filename, line_index + 1)?;
        let url = base.join(filename)?;
        entries.push(SourceInventoryEntry {
            partition_id: partition_id(
                filename,
                &product.filename_prefix,
                &product.filename_suffix,
            ),
            filename: filename.to_string(),
            url: url.to_string(),
            official_checksum: digest,
        });
    }
    entries.sort_by(|left, right| left.filename.cmp(&right.filename));
    if entries.is_empty() {
        bail!("Gaia product {} inventory is empty", product.id);
    }
    if entries
        .windows(2)
        .any(|pair| pair[0].filename == pair[1].filename)
    {
        bail!("Gaia product {} inventory contains duplicates", product.id);
    }
    if let Some(expected) = product.expected_partitions {
        if entries.len() != expected {
            bail!(
                "Gaia product {} inventory has {} partitions; expected {}",
                product.id,
                entries.len(),
                expected
            );
        }
    }
    Ok(SourceInventory {
        schema_version: INVENTORY_SCHEMA_VERSION,
        product_id: product.id.clone(),
        base_url: base.to_string(),
        checksum_manifest_url: product.checksum_manifest_url.clone(),
        checksum_manifest_sha256: checksum_io::sha256_bytes(raw),
        official_checksum_algorithm: product.checksum_algorithm,
        entries,
    })
}

fn normalized_base_url(raw: &str) -> Result<Url> {
    let mut url = Url::parse(raw)?;
    if url.scheme() != "https" {
        bail!("Gaia bulk base URL must use HTTPS");
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn validate_product_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        bail!("Gaia product id must use lowercase ASCII, digits, '-' or '_'");
    }
    Ok(())
}

fn validate_digest(algorithm: OfficialChecksumAlgorithm, digest: &str, line: usize) -> Result<()> {
    if digest.len() != algorithm.digest_len()
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!(
            "inventory line {line} has invalid {} checksum",
            algorithm.as_str()
        );
    }
    Ok(())
}

fn validate_filename(product: &GaiaProductConfig, filename: &str, line: usize) -> Result<()> {
    if filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || !filename.starts_with(&product.filename_prefix)
        || !filename.ends_with(&product.filename_suffix)
    {
        bail!("inventory line {line} filename {filename:?} violates product filename contract");
    }
    Ok(())
}

fn partition_id(filename: &str, prefix: &str, suffix: &str) -> String {
    filename
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .unwrap_or(filename)
        .to_string()
}

fn artifact(name: &str, path: &Path) -> Result<Artifact> {
    Ok(Artifact {
        name: name.to_string(),
        path: PathBuf::from(path),
        sha256: checksum_io::sha256_file(path)?,
        bytes: fs::metadata(path)?.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product() -> GaiaProductConfig {
        GaiaProductConfig {
            id: "xp-continuous".to_string(),
            base_url: "https://cdn.example.test/gaia/xp".to_string(),
            checksum_manifest_url: "https://cdn.example.test/gaia/xp/_MD5SUM.txt".to_string(),
            checksum_manifest_sha256:
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            checksum_algorithm: OfficialChecksumAlgorithm::Md5,
            expected_partitions: Some(2),
            filename_prefix: "XpContinuous_".to_string(),
            filename_suffix: ".csv.gz".to_string(),
        }
    }

    #[test]
    fn normalizes_sorted_inventory_and_urls() {
        let raw = concat!(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb XpContinuous_20-29.csv.gz\n",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  XpContinuous_00-19.csv.gz\n",
        );
        let inventory = parse_inventory(&product(), raw.as_bytes()).unwrap();
        assert_eq!(inventory.entries[0].partition_id, "00-19");
        assert_eq!(
            inventory.entries[0].url,
            "https://cdn.example.test/gaia/xp/XpContinuous_00-19.csv.gz"
        );
        assert_eq!(inventory.checksum_manifest_sha256.len(), 64);
    }

    #[test]
    fn ignores_esac_self_checksum_entry() {
        let raw = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00-19.csv.gz\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb XpContinuous_20-29.csv.gz\n",
            "cccccccccccccccccccccccccccccccc _MD5SUM.txt\n",
        );
        assert_eq!(
            parse_inventory(&product(), raw.as_bytes())
                .unwrap()
                .entries
                .len(),
            2
        );
    }

    #[test]
    fn rejects_traversal_duplicate_bad_digest_and_wrong_count() {
        let invalid = [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa ../XpContinuous_00.csv.gz\n",
            "not-a-checksum XpContinuous_00.csv.gz\n",
            concat!(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00.csv.gz\n",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00.csv.gz\n"
            ),
        ];
        for raw in invalid {
            assert!(parse_inventory(&product(), raw.as_bytes()).is_err());
        }
        assert!(parse_inventory(
            &product(),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00.csv.gz\n"
        )
        .is_err());
    }

    #[test]
    fn reconciles_only_complete_matching_inventory_sets() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("inventories");
        fs::create_dir_all(&root)?;
        let mut first = product();
        let mut second = product();
        second.id = "gaia-source".to_string();
        second.filename_prefix = "GaiaSource_".to_string();
        let raw = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa XpContinuous_00-19.csv.gz\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb XpContinuous_20-29.csv.gz\n",
        );
        first.checksum_manifest_sha256 = checksum_io::sha256_bytes(raw.as_bytes());
        let first_inventory = parse_inventory(&first, raw.as_bytes())?;
        artifact_store::atomic_write(
            &root.join("xp-continuous.inventory.json"),
            &serde_json::to_vec_pretty(&first_inventory)?,
        )?;
        assert!(
            production_partition_ids(directory.path(), &[first.clone(), second.clone()])
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );

        let second_raw = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa GaiaSource_00-19.csv.gz\n",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb GaiaSource_20-29.csv.gz\n",
        );
        second.checksum_manifest_sha256 = checksum_io::sha256_bytes(second_raw.as_bytes());
        let second_inventory = parse_inventory(&second, second_raw.as_bytes())?;
        artifact_store::atomic_write(
            &root.join("gaia-source.inventory.json"),
            &serde_json::to_vec_pretty(&second_inventory)?,
        )?;
        assert_eq!(
            production_partition_ids(directory.path(), &[first, second])?.unwrap(),
            ["00-19", "20-29"]
        );
        Ok(())
    }
}
