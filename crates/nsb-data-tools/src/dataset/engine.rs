//! Dataset engine entry points with transactional Starlight publication replacement.

#[allow(unused_imports)]
use super::{config, execution, model, pipeline, slurm};
use crate::platform::artifact_store;
use anyhow::{bail, Context, Result};
use model::{DatasetName, Executor, Operation};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

type SourceConfig = config::SourceConfig;

#[path = "engine_core.rs"]
mod core;

pub(super) fn read_manifest(path: &Path) -> Result<model::RunManifest> {
    core::read_manifest(path)
}

pub(super) fn write_manifest(path: &Path, manifest: &model::RunManifest) -> Result<()> {
    core::write_manifest(path, manifest)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    core::atomic_write(path, bytes)
}

pub fn execute(
    config_path: &Path,
    dataset: DatasetName,
    operation: Operation,
    executor: Option<Executor>,
    concurrency: Option<usize>,
    requested_partitions: &[String],
) -> Result<()> {
    let transaction = if dataset == DatasetName::Starlight && operation == Operation::Publish {
        StarlightPublishTransaction::prepare(config_path)?
    } else {
        None
    };

    let result = core::execute(
        config_path,
        dataset,
        operation,
        executor,
        concurrency,
        requested_partitions,
    );

    match (result, transaction) {
        (Ok(()), Some(transaction)) => transaction.commit(),
        (Ok(()), None) => Ok(()),
        (Err(error), Some(mut transaction)) => match transaction.rollback() {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(error.context(format!(
                "Starlight publish failed and repository rollback also failed: {rollback_error:#}"
            ))),
        },
        (Err(error), None) => Err(error),
    }
}

pub fn status(path: &Path) -> Result<()> {
    core::status(path)
}

pub fn resume(path: &Path) -> Result<()> {
    let manifest = core::read_manifest(path)?;
    if manifest.operation != Operation::Publish {
        return core::resume(path);
    }
    execute(
        &manifest.config_path,
        manifest.dataset,
        manifest.operation,
        Some(manifest.executor),
        None,
        &manifest.partitions,
    )
}

pub fn run_worker(
    config_path: &Path,
    dataset: DatasetName,
    operation: Operation,
    partition: Option<&str>,
    partition_manifest: Option<&Path>,
) -> Result<()> {
    core::run_worker(
        config_path,
        dataset,
        operation,
        partition,
        partition_manifest,
    )
}

struct FileBackup {
    original: PathBuf,
    backup: PathBuf,
}

struct StarlightPublishTransaction {
    data_root: PathBuf,
    manifest_path: PathBuf,
    original_manifest: Vec<u8>,
    current_map: String,
    backup_root: PathBuf,
    backups: Vec<FileBackup>,
    active: bool,
}

impl StarlightPublishTransaction {
    fn prepare(config_path: &Path) -> Result<Option<Self>> {
        let config = config::RunConfig::load(config_path)?;
        if config.dataset != DatasetName::Starlight {
            return Ok(None);
        }
        let Some(starlight) = config.starlight.as_ref() else {
            return Ok(None);
        };

        let publish = config
            .publish
            .as_ref()
            .context("publish.repository_root is required")?;
        let data_root = publish.repository_root.join("crates/nsb/data");
        let manifest_path = data_root.join("manifest.toml");
        let original_manifest = fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let mut document =
            std::str::from_utf8(&original_manifest)?.parse::<toml_edit::DocumentMut>()?;
        let current_map = format!("starlight_nside{}.csv", starlight.map.canonical_nside);
        retire_other_canonical_entries(&mut document, &current_map)?;

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let backup_root = publish.repository_root.join(format!(
            ".nsb-data-publish-backup-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&backup_root)
            .with_context(|| format!("failed to create {}", backup_root.display()))?;

        let mut transaction = Self {
            data_root,
            manifest_path,
            original_manifest,
            current_map: current_map.clone(),
            backup_root,
            backups: Vec::new(),
            active: true,
        };

        let mut touched = collect_starlight_map_files(&transaction.data_root)?;
        touched.insert("merge_report.json".to_string());
        touched.insert(current_map);
        for (index, name) in touched.into_iter().enumerate() {
            let original = transaction.data_root.join(&name);
            if !original.exists() {
                continue;
            }
            if !original.is_file() {
                bail!(
                    "publish target {} is not a regular file",
                    original.display()
                );
            }
            let backup = transaction.backup_root.join(format!("{index}-{name}"));
            fs::rename(&original, &backup).with_context(|| {
                format!(
                    "failed to move existing publish target {} to {}",
                    original.display(),
                    backup.display()
                )
            })?;
            transaction.backups.push(FileBackup { original, backup });
        }

        artifact_store::atomic_write(&transaction.manifest_path, document.to_string().as_bytes())?;
        Ok(Some(transaction))
    }

    fn commit(mut self) -> Result<()> {
        if !self.published_state_is_complete()? {
            self.rollback()?;
            return Ok(());
        }
        self.active = false;
        self.cleanup_backup();
        Ok(())
    }

    fn published_state_is_complete(&self) -> Result<bool> {
        if !self.data_root.join(&self.current_map).is_file()
            || !self.data_root.join("merge_report.json").is_file()
        {
            return Ok(false);
        }
        let document =
            fs::read_to_string(&self.manifest_path)?.parse::<toml_edit::DocumentMut>()?;
        let assets = document["assets"]
            .as_array_of_tables()
            .context("manifest is missing [[assets]]")?;
        let maps = assets
            .iter()
            .filter_map(|asset| asset["path"].as_str())
            .filter(|path| is_starlight_nside_map_name(path))
            .collect::<BTreeSet<_>>();
        let report_registered = assets
            .iter()
            .any(|asset| asset["path"].as_str() == Some("merge_report.json"));
        Ok(maps == BTreeSet::from([self.current_map.as_str()]) && report_registered)
    }

    fn rollback(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }

        if self.data_root.is_dir() {
            for entry in fs::read_dir(&self.data_root)? {
                let path = entry?.path();
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if (is_starlight_nside_map_name(name) || name == "merge_report.json")
                    && path.is_file()
                {
                    fs::remove_file(&path).with_context(|| {
                        format!("failed to remove partial publish target {}", path.display())
                    })?;
                }
            }
        }

        // Keep the backups intact until every file and the manifest have been restored.
        // This makes a failed rollback retryable and prevents Drop from deleting files
        // that a previous rollback attempt already recovered.
        for backup in &self.backups {
            if let Some(parent) = backup.original.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&backup.backup, &backup.original).with_context(|| {
                format!(
                    "failed to restore publish target {} from {}",
                    backup.original.display(),
                    backup.backup.display()
                )
            })?;
        }
        artifact_store::atomic_write(&self.manifest_path, &self.original_manifest)?;

        self.active = false;
        self.cleanup_backup();
        Ok(())
    }

    fn cleanup_backup(&self) {
        if self.backup_root.is_dir() {
            if let Err(error) = fs::remove_dir_all(&self.backup_root) {
                eprintln!(
                    "warning: failed to remove Starlight publish backup {}: {error}",
                    self.backup_root.display()
                );
            }
        }
    }
}

impl Drop for StarlightPublishTransaction {
    fn drop(&mut self) {
        if self.active {
            let _ = self.rollback();
        }
    }
}

fn retire_other_canonical_entries(
    document: &mut toml_edit::DocumentMut,
    current_map: &str,
) -> Result<Vec<String>> {
    let assets = document["assets"]
        .as_array_of_tables_mut()
        .context("manifest is missing [[assets]]")?;
    let mut retired = Vec::new();
    let mut index = 0;
    while index < assets.len() {
        let asset = assets
            .get(index)
            .context("manifest asset index disappeared during retirement")?;
        let Some(path) = asset["path"].as_str().map(str::to_owned) else {
            index += 1;
            continue;
        };
        if path == current_map || !is_starlight_nside_map_name(&path) {
            index += 1;
            continue;
        }
        let schema = asset["schema"].as_str().unwrap_or_default().to_owned();
        let protected = asset["calibration_status"].as_str() == Some("production")
            || asset["runtime_embedded"].as_bool() == Some(true);
        if !schema.starts_with("nsb-healpix-starlight-candidate-") {
            bail!("refusing to retire ambiguous Starlight map {path:?} with schema {schema:?}");
        }
        if protected {
            bail!("candidate publish cannot retire production runtime asset {path:?}");
        }
        retired.push(path);
        assets.remove(index);
    }
    Ok(retired)
}

fn collect_starlight_map_files(data_root: &Path) -> Result<BTreeSet<String>> {
    let mut maps = BTreeSet::new();
    if !data_root.is_dir() {
        return Ok(maps);
    }
    for entry in fs::read_dir(data_root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_starlight_nside_map_name(&name) {
            maps.insert(name);
        }
    }
    Ok(maps)
}

fn is_starlight_nside_map_name(name: &str) -> bool {
    name.strip_prefix("starlight_nside")
        .and_then(|suffix| suffix.strip_suffix(".csv"))
        .is_some_and(|nside| !nside.is_empty() && nside.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use model::{Artifact, ValidationReport, RUN_SCHEMA_VERSION};
    use serde_json::json;
    use tempfile::TempDir;

    const MANIFEST_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const OBJECT_SHA: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const PARTITION: &str = "000000-000001";

    fn repository_fixture(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
        let repository_root = temp.path().join("repository");
        let data_root = repository_root.join("crates/nsb/data");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&data_root).unwrap();
        fs::create_dir_all(&workspace).unwrap();

        let manifest = r#"schema_version = 1

[[assets]]
path = "unrelated_asset.dat"
schema = "nsb-unrelated-v1"
calibration_status = "experimental"
runtime_embedded = true

[[assets]]
path = "starlight_nside128.csv"
schema = "nsb-healpix-starlight-candidate-v2"
calibration_status = "candidate"
runtime_embedded = false

[[assets]]
path = "merge_report.json"
schema = "nsb-starlight-merge-report-v3"
calibration_status = "candidate"
runtime_embedded = false
"#;
        fs::write(data_root.join("manifest.toml"), manifest).unwrap();
        fs::write(data_root.join("unrelated_asset.dat"), b"unrelated").unwrap();
        fs::write(data_root.join("starlight_nside128.csv"), b"old-map").unwrap();
        fs::write(data_root.join("merge_report.json"), b"old-report").unwrap();
        (repository_root, data_root, workspace)
    }

    fn write_config(
        temp: &TempDir,
        repository_root: &Path,
        workspace: &Path,
        nside: u32,
    ) -> PathBuf {
        let config_path = temp.path().join("run.toml");
        fs::write(
            &config_path,
            format!(
                r#"schema_version = 1
dataset = "starlight"

[workspace]
root = {:?}

[publish]
repository_root = {:?}

[starlight]

[starlight.map]
canonical_nside = {nside}

[[starlight.gaia_products]]
id = "gaia-source"
base_url = "https://example.com/gaia-source/"
checksum_manifest_url = "https://example.com/gaia-source/checksums.txt"
checksum_manifest_sha256 = "{MANIFEST_SHA}"
checksum_algorithm = "sha256"
expected_partitions = 1
filename_prefix = "GaiaSource_"
filename_suffix = ".csv.gz"

[[starlight.gaia_products]]
id = "xp-continuous"
base_url = "https://example.com/xp-continuous/"
checksum_manifest_url = "https://example.com/xp-continuous/checksums.txt"
checksum_manifest_sha256 = "{MANIFEST_SHA}"
checksum_algorithm = "sha256"
expected_partitions = 1
filename_prefix = "XpContinuousMeanSpectrum_"
filename_suffix = ".csv.gz"
"#,
                workspace, repository_root
            ),
        )
        .unwrap();
        config_path
    }

    fn write_inventory(
        workspace: &Path,
        product_id: &str,
        base_url: &str,
        checksum_manifest_url: &str,
        filename_prefix: &str,
    ) {
        let root = workspace.join("inventories");
        fs::create_dir_all(&root).unwrap();
        let filename = format!("{filename_prefix}{PARTITION}.csv.gz");
        let inventory = json!({
            "schema_version": 1,
            "product_id": product_id,
            "base_url": base_url,
            "checksum_manifest_url": checksum_manifest_url,
            "checksum_manifest_sha256": MANIFEST_SHA,
            "official_checksum_algorithm": "sha256",
            "entries": [{
                "partition_id": PARTITION,
                "filename": filename,
                "url": format!("{base_url}{filename}"),
                "official_checksum": OBJECT_SHA
            }]
        });
        fs::write(
            root.join(format!("{product_id}.inventory.json")),
            serde_json::to_vec_pretty(&inventory).unwrap(),
        )
        .unwrap();
    }

    fn artifact(name: &str, path: &Path) -> Artifact {
        Artifact {
            name: name.to_string(),
            path: path.to_path_buf(),
            sha256: checksum_io::sha256_file(path).unwrap(),
            bytes: path.metadata().unwrap().len(),
        }
    }

    fn write_validation_report(workspace: &Path, artifacts: Vec<Artifact>) {
        let report = ValidationReport {
            schema_version: RUN_SCHEMA_VERSION,
            dataset: DatasetName::Starlight,
            passed: true,
            gates: Vec::new(),
            artifacts,
        };
        fs::write(
            workspace.join("validation.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn manifest_transition_keeps_only_the_new_canonical_map() {
        let mut document = r#"schema_version = 1

[[assets]]
path = "unrelated_asset.dat"
schema = "nsb-unrelated-v1"
calibration_status = "experimental"
runtime_embedded = true

[[assets]]
path = "starlight_nside128.csv"
schema = "nsb-healpix-starlight-candidate-v2"
calibration_status = "candidate"
runtime_embedded = false

[[assets]]
path = "merge_report.json"
schema = "nsb-starlight-merge-report-v3"
calibration_status = "candidate"
runtime_embedded = false
"#
        .parse::<toml_edit::DocumentMut>()
        .unwrap();

        let retired =
            retire_other_canonical_entries(&mut document, "starlight_nside256.csv").unwrap();
        assert_eq!(retired, ["starlight_nside128.csv"]);
        let assets = document["assets"].as_array_of_tables().unwrap();
        assert!(assets
            .iter()
            .any(|asset| asset["path"].as_str() == Some("unrelated_asset.dat")));
        assert!(!assets
            .iter()
            .any(|asset| asset["path"].as_str() == Some("starlight_nside128.csv")));
    }

    #[test]
    fn manifest_transition_refuses_to_remove_runtime_production_map() {
        let mut document = r#"schema_version = 1

[[assets]]
path = "starlight_nside128.csv"
schema = "nsb-healpix-starlight-candidate-v2"
calibration_status = "production"
runtime_embedded = true
"#
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
        assert!(retire_other_canonical_entries(&mut document, "starlight_nside256.csv").is_err());
    }

    #[test]
    fn failed_publish_rolls_back_manifest_and_files() {
        let temp = TempDir::new().unwrap();
        let (repository_root, data_root, workspace) = repository_fixture(&temp);
        let original_manifest = fs::read(data_root.join("manifest.toml")).unwrap();
        let config_path = write_config(&temp, &repository_root, &workspace, 256);

        let mut transaction = StarlightPublishTransaction::prepare(&config_path)
            .unwrap()
            .unwrap();
        fs::write(data_root.join("starlight_nside256.csv"), b"new-map").unwrap();
        fs::write(data_root.join("merge_report.json"), b"new-report").unwrap();
        transaction.rollback().unwrap();
        transaction.rollback().unwrap();

        assert_eq!(
            fs::read(data_root.join("manifest.toml")).unwrap(),
            original_manifest
        );
        assert_eq!(
            fs::read(data_root.join("starlight_nside128.csv")).unwrap(),
            b"old-map"
        );
        assert_eq!(
            fs::read(data_root.join("merge_report.json")).unwrap(),
            b"old-report"
        );
        assert!(!data_root.join("starlight_nside256.csv").exists());
    }

    #[test]
    fn completed_publish_without_new_outputs_restores_previous_state() {
        let temp = TempDir::new().unwrap();
        let (repository_root, data_root, workspace) = repository_fixture(&temp);
        let original_manifest = fs::read(data_root.join("manifest.toml")).unwrap();
        let config_path = write_config(&temp, &repository_root, &workspace, 128);

        let transaction = StarlightPublishTransaction::prepare(&config_path)
            .unwrap()
            .unwrap();
        transaction.commit().unwrap();

        assert_eq!(
            fs::read(data_root.join("manifest.toml")).unwrap(),
            original_manifest
        );
        assert_eq!(
            fs::read(data_root.join("starlight_nside128.csv")).unwrap(),
            b"old-map"
        );
        assert_eq!(
            fs::read(data_root.join("merge_report.json")).unwrap(),
            b"old-report"
        );
    }

    #[test]
    fn successful_publish_replaces_nside128_with_nside256_end_to_end() {
        let temp = TempDir::new().unwrap();
        let (repository_root, data_root, workspace) = repository_fixture(&temp);
        let config_path = write_config(&temp, &repository_root, &workspace, 256);
        write_inventory(
            &workspace,
            "gaia-source",
            "https://example.com/gaia-source/",
            "https://example.com/gaia-source/checksums.txt",
            "GaiaSource_",
        );
        write_inventory(
            &workspace,
            "xp-continuous",
            "https://example.com/xp-continuous/",
            "https://example.com/xp-continuous/checksums.txt",
            "XpContinuousMeanSpectrum_",
        );

        let outputs = workspace.join("outputs");
        fs::create_dir_all(&outputs).unwrap();
        let map_path = outputs.join("starlight_nside256.csv");
        let report_path = outputs.join("merge_report.json");
        fs::write(&map_path, b"new-map-256").unwrap();
        fs::write(&report_path, b"new-report-256").unwrap();
        let map_artifact = artifact("starlight_nside256.csv", &map_path);
        let report_artifact = artifact("merge_report.json", &report_path);
        write_validation_report(
            &workspace,
            vec![map_artifact.clone(), report_artifact.clone()],
        );

        execute(
            &config_path,
            DatasetName::Starlight,
            Operation::Publish,
            Some(Executor::Local),
            None,
            &[],
        )
        .unwrap();

        assert!(!data_root.join("starlight_nside128.csv").exists());
        assert_eq!(
            fs::read(data_root.join("starlight_nside256.csv")).unwrap(),
            b"new-map-256"
        );
        assert_eq!(
            fs::read(data_root.join("merge_report.json")).unwrap(),
            b"new-report-256"
        );
        assert_eq!(
            fs::read(data_root.join("unrelated_asset.dat")).unwrap(),
            b"unrelated"
        );

        let document = fs::read_to_string(data_root.join("manifest.toml"))
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        let assets = document["assets"].as_array_of_tables().unwrap();
        let maps = assets
            .iter()
            .filter_map(|asset| asset["path"].as_str())
            .filter(|path| is_starlight_nside_map_name(path))
            .collect::<Vec<_>>();
        assert_eq!(maps, ["starlight_nside256.csv"]);
        assert!(assets
            .iter()
            .any(|asset| asset["path"].as_str() == Some("unrelated_asset.dat")));
        assert!(assets.iter().any(|asset| {
            asset["path"].as_str() == Some("starlight_nside256.csv")
                && asset["sha256"].as_str() == Some(map_artifact.sha256.as_str())
        }));
        assert!(assets.iter().any(|asset| {
            asset["path"].as_str() == Some("merge_report.json")
                && asset["sha256"].as_str() == Some(report_artifact.sha256.as_str())
        }));
        assert!(!fs::read_dir(&repository_root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".nsb-data-publish-backup-")
        }));
    }
}
