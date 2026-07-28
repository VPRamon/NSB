//! Production processing for one immutable Gaia partition pair.

use super::config::GaiaProductConfig;
use super::map::accumulator::PartitionShard;
use super::sources::acquisition;
use super::xp::{
    integrate_photon_flux, integrate_photon_flux_uncertainty, GaiaXpContinuousCalibrator,
};
use crate::dataset::Artifact;
use crate::platform::artifact_store;
use anyhow::{bail, Context, Result};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

const CSV_BUFFER_CAPACITY: usize = 1024 * 1024;

pub(crate) fn build_partitions(
    configured_workspace: &Path,
    products: &[GaiaProductConfig],
    partitions: &[String],
    concurrency: usize,
    canonical_nside: u32,
) -> Result<Vec<Artifact>> {
    if partitions.is_empty() {
        return Ok(Vec::new());
    }
    let (shared_workspace, worker_invocation) = workspace_roots(configured_workspace);
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;
    let chunk_size = partitions.len().div_ceil(concurrency.max(1)).max(1);
    let artifacts = std::thread::scope(|scope| -> Result<Vec<Artifact>> {
        let mut handles = Vec::new();
        for chunk in partitions.chunks(chunk_size) {
            let calibrator = &calibrator;
            let shared_workspace = shared_workspace.as_path();
            handles.push(scope.spawn(move || -> Result<Vec<Artifact>> {
                chunk
                    .iter()
                    .map(|partition| {
                        build_partition(
                            shared_workspace,
                            configured_workspace,
                            worker_invocation,
                            products,
                            partition,
                            calibrator,
                            canonical_nside,
                        )
                    })
                    .collect()
            }));
        }
        let mut artifacts = Vec::with_capacity(partitions.len());
        for handle in handles {
            artifacts.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("Starlight partition worker panicked"))??,
            );
        }
        Ok(artifacts)
    })?;
    let mut artifacts = artifacts;
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(artifacts)
}

fn build_partition(
    shared_workspace: &Path,
    configured_workspace: &Path,
    worker_invocation: bool,
    products: &[GaiaProductConfig],
    partition_id: &str,
    calibrator: &GaiaXpContinuousCalibrator,
    canonical_nside: u32,
) -> Result<Artifact> {
    let gaia_path = acquisition::verified_object_for_partition(
        shared_workspace,
        products,
        "gaia-source",
        partition_id,
    )?;
    let xp_path = acquisition::verified_object_for_partition(
        shared_workspace,
        products,
        "xp-continuous",
        partition_id,
    )?;
    let gaia_sources = load_gaia_source_ids(&gaia_path)?;
    let mut shard = PartitionShard::new(partition_id, canonical_nside)?;
    let mut stream = super::xp::stream_bulk_ecsv_gz(&xp_path)?;
    while let Some(record) = stream.next_record()? {
        let source_id = record
            .source_id
            .parse::<u64>()
            .with_context(|| format!("invalid XP source_id {}", record.source_id))?;
        if !gaia_sources.contains(&source_id) {
            shard.exclude(source_id, "no_gaia_source_match")?;
            continue;
        }
        let product = match calibrator.calibrate(&record) {
            Ok(product) => product,
            Err(_) => {
                shard.exclude(source_id, "calibration_failed")?;
                continue;
            }
        };
        let flux = match integrate_photon_flux(&product) {
            Ok(flux) if flux.is_finite() && flux > 0.0 => flux,
            _ => {
                shard.exclude(source_id, "invalid_flux")?;
                continue;
            }
        };
        let statistical_uncertainty = match integrate_photon_flux_uncertainty(&product) {
            Ok(uncertainty) if uncertainty.is_finite() && uncertainty >= 0.0 => uncertainty,
            _ => {
                shard.exclude(source_id, "invalid_uncertainty")?;
                continue;
            }
        };
        // The frozen calibration carries statistical covariance only. No
        // independent systematic term is supplied by the upstream product.
        shard.admit(source_id, flux, statistical_uncertainty, 0.0)?;
    }
    shard.validate()?;
    let shard_path = if worker_invocation {
        configured_workspace.join("shard.json")
    } else {
        shared_workspace
            .join("workers")
            .join(partition_id)
            .join("shard.json")
    };
    let sha256 = shard.write(&shard_path)?;
    Ok(Artifact {
        name: format!("shards/{partition_id}.json"),
        bytes: shard_path.metadata()?.len(),
        path: shard_path,
        sha256,
    })
}

fn load_gaia_source_ids(path: &Path) -> Result<HashSet<u64>> {
    let decoder = GzDecoder::new(
        File::open(path).with_context(|| format!("open GaiaSource object {}", path.display()))?,
    );
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .buffer_capacity(CSV_BUFFER_CAPACITY)
        .from_reader(BufReader::new(decoder));
    let headers = reader.headers()?.clone();
    let source_index = headers
        .iter()
        .position(|header| header.trim() == "source_id")
        .context("GaiaSource partition has no source_id column")?;
    let mut source_ids = HashSet::new();
    for (row_index, row) in reader.records().enumerate() {
        let row = row.with_context(|| {
            format!(
                "read GaiaSource row {} from {}",
                row_index + 2,
                path.display()
            )
        })?;
        let source_id = row
            .get(source_index)
            .context("GaiaSource row has no source_id field")?
            .trim()
            .parse::<u64>()
            .context("GaiaSource source_id is not u64")?;
        if !source_ids.insert(source_id) {
            bail!("GaiaSource partition contains duplicate source_id {source_id}");
        }
    }
    Ok(source_ids)
}

fn workspace_roots(configured: &Path) -> (PathBuf, bool) {
    let is_worker = configured
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "workers");
    if is_worker {
        (
            configured
                .parent()
                .and_then(Path::parent)
                .unwrap_or(configured)
                .to_path_buf(),
            true,
        )
    } else {
        (configured.to_path_buf(), false)
    }
}

pub(crate) fn write_artifact_index(root: &Path, artifacts: &[Artifact]) -> Result<()> {
    artifact_store::atomic_write(
        &root.join("outputs/artifacts.json"),
        &serde_json::to_vec_pretty(artifacts)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use crate::starlight::config::OfficialChecksumAlgorithm;
    use crate::starlight::sources::acquisition::AcquisitionReceipt;
    use crate::starlight::sources::inventory::{SourceInventory, SourceInventoryEntry};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use serde_json::Value;
    use std::fs;
    use std::io::Write;

    #[test]
    fn paired_partition_builds_a_strict_shard_and_maps() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let partition = "000000-003111";
        let oracle: Value = serde_json::from_slice(&fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/gaiaxpy_oracle/record-01.json"),
        )?)?;
        let source_id = oracle["source_id"].as_str().context("oracle source_id")?;

        let gaia_bytes = gzip_bytes(format!("source_id\n{source_id}\n").as_bytes())?;
        let correlations = vec![0.0; 55 * 54 / 2];
        let arrays = |name: &str| serde_json::to_string(&oracle[name]).unwrap();
        let mut xp_csv = csv::Writer::from_writer(Vec::new());
        xp_csv.write_record([
            "source_id",
            "bp_n_parameters",
            "bp_standard_deviation",
            "rp_n_parameters",
            "rp_standard_deviation",
            "bp_coefficients",
            "bp_coefficient_errors",
            "bp_coefficient_correlations",
            "rp_coefficients",
            "rp_coefficient_errors",
            "rp_coefficient_correlations",
            "bp_n_relevant_bases",
            "rp_n_relevant_bases",
        ])?;
        xp_csv.write_record([
            source_id,
            "55",
            oracle["bp_standard_deviation"]
                .as_f64()
                .context("bp standard deviation")?
                .to_string()
                .as_str(),
            "55",
            oracle["rp_standard_deviation"]
                .as_f64()
                .context("rp standard deviation")?
                .to_string()
                .as_str(),
            &arrays("bp_coefficients"),
            &arrays("bp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            &arrays("rp_coefficients"),
            &arrays("rp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            "55",
            "55",
        ])?;
        let xp_bytes = gzip_bytes(&xp_csv.into_inner()?)?;

        let products = vec![
            product("gaia-source", "GaiaSource_"),
            product("xp-continuous", "XpContinuousMeanSpectrum_"),
        ];
        install_object_and_receipt(
            workspace,
            &products[0],
            partition,
            &gaia_bytes,
            "GaiaSource_000000-003111.csv.gz",
        )?;
        install_object_and_receipt(
            workspace,
            &products[1],
            partition,
            &xp_bytes,
            "XpContinuousMeanSpectrum_000000-003111.csv.gz",
        )?;

        let canonical_nside = 256;
        let artifacts = build_partitions(
            workspace,
            &products,
            &[partition.to_string()],
            1,
            canonical_nside,
        )?;
        assert_eq!(artifacts.len(), 1);
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&artifacts[0].path)?)?;
        assert_eq!(shard.nside, canonical_nside);
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.admitted_sources)
                .sum::<u64>(),
            1
        );
        let reconciled = workspace
            .join("outputs/shards")
            .join(format!("{partition}.json"));
        artifact_store::atomic_write(&reconciled, &fs::read(&artifacts[0].path)?)?;
        let maps = crate::starlight::map::product::emit_maps(
            workspace,
            &[partition.to_string()],
            canonical_nside,
        )?;
        assert_eq!(maps.len(), 2);
        crate::starlight::map::product::validate_map(
            &workspace
                .join("outputs")
                .join(format!("starlight_nside{canonical_nside}.csv")),
            canonical_nside,
        )?;
        Ok(())
    }

    fn product(id: &str, prefix: &str) -> GaiaProductConfig {
        GaiaProductConfig {
            id: id.to_string(),
            base_url: format!("https://example.test/{id}/"),
            checksum_manifest_url: format!("https://example.test/{id}/checksums"),
            checksum_manifest_sha256: "a".repeat(64),
            checksum_algorithm: OfficialChecksumAlgorithm::Md5,
            expected_partitions: Some(1),
            filename_prefix: prefix.to_string(),
            filename_suffix: ".csv.gz".to_string(),
        }
    }

    fn install_object_and_receipt(
        workspace: &Path,
        product: &GaiaProductConfig,
        partition: &str,
        bytes: &[u8],
        filename: &str,
    ) -> Result<()> {
        let object_sha = checksum_io::sha256_bytes(bytes);
        let object_path = workspace.join("cache/objects/sha256").join(&object_sha);
        artifact_store::atomic_write(&object_path, bytes)?;
        let entry = SourceInventoryEntry {
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            url: format!("{}{filename}", product.base_url),
            official_checksum: "0".repeat(32),
        };
        let inventory = SourceInventory {
            schema_version: 1,
            product_id: product.id.clone(),
            base_url: product.base_url.clone(),
            checksum_manifest_url: product.checksum_manifest_url.clone(),
            checksum_manifest_sha256: product.checksum_manifest_sha256.clone(),
            official_checksum_algorithm: product.checksum_algorithm,
            entries: vec![entry.clone()],
        };
        artifact_store::atomic_write(
            &workspace
                .join("inventories")
                .join(format!("{}.inventory.json", product.id)),
            &serde_json::to_vec_pretty(&inventory)?,
        )?;
        let receipt = AcquisitionReceipt {
            schema_version: 1,
            product_id: product.id.clone(),
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            source_url: entry.url,
            official_checksum_algorithm: product.checksum_algorithm,
            official_checksum: entry.official_checksum,
            sha256: object_sha,
            bytes: bytes.len() as u64,
            object_path,
        };
        artifact_store::atomic_write(
            &workspace
                .join("cache/receipts")
                .join(&product.id)
                .join(format!("{partition}.json")),
            &serde_json::to_vec_pretty(&receipt)?,
        )
    }

    fn gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes)?;
        Ok(encoder.finish()?)
    }
}
