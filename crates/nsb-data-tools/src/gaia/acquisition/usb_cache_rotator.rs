//! USB cache rotator: wires official bulk download to cache state transitions.
//!
//! Download path: `planned` → `downloading` → `downloaded` → `checksum_verified`
//! Processing path (downstream): `processing` → `processed` → `output_verified` → `releasable`

use crate::gaia::acquisition::bulk::{
    parse_md5_manifest, BulkFileStatus, BulkOutputManifest, BulkPaths, BulkReport,
};
use crate::gaia::acquisition::usb_cache::{
    append_session_log, assert_file_size_usb_safe, current_cache_footprint_bytes,
    load_or_init_cache_state_manifest, read_or_create_cache_root_marker, simulate_or_run_cleanup,
    transition_entry_state, verify_usb_identity, write_cache_state_manifest, CacheInputState,
    CacheStateManifest, CleanupSimulationReport, UsbCacheLayout, UsbCacheRootMarker,
    OFFICIAL_CHECKSUM_MANIFEST,
};
use crate::platform::checksum_io::verify_md5_file;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SESSION_MANIFEST_FILENAME: &str = "usb_cache_download_session.json";

/// Configuration for a USB-backed rotating cache download session.
#[derive(Debug, Clone)]
pub struct UsbCacheRotatorConfig {
    pub layout: UsbCacheLayout,
    pub max_cache_bytes: u64,
    pub init_usb_marker: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbCacheDownloadSession {
    pub schema_version: u32,
    pub session_id: String,
    pub cache_uuid: String,
    pub cache_dir: String,
    pub max_cache_bytes: u64,
    pub file_limit: Option<usize>,
    pub resume: bool,
    pub started_at_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbCacheSyncReport {
    pub files_synced: usize,
    pub checksum_verified: usize,
    pub failed: usize,
    pub skipped_already_verified: usize,
    pub footprint_bytes: u64,
    pub passed: bool,
    pub failures: Vec<String>,
}

pub struct UsbCacheRotator {
    pub layout: UsbCacheLayout,
    pub marker: UsbCacheRootMarker,
    pub manifest: CacheStateManifest,
    pub max_cache_bytes: u64,
}

impl UsbCacheRotator {
    pub fn prepare(config: UsbCacheRotatorConfig) -> Result<Self> {
        let identity = verify_usb_identity(&config.layout)?;
        if !identity.passed {
            bail!(
                "USB cache identity preflight failed: {}",
                identity.failures.join("; ")
            );
        }

        let marker = read_or_create_cache_root_marker(&config.layout, config.init_usb_marker)?;
        fs::create_dir_all(&config.layout.cache_dir)?;
        fs::create_dir_all(&config.layout.manifests_dir)?;

        let checksum_path = config.layout.cache_dir.join(OFFICIAL_CHECKSUM_MANIFEST);
        if !checksum_path.is_file() {
            bail!(
                "official checksum manifest missing at {}; copy or fetch _MD5SUM.txt before download",
                checksum_path.display()
            );
        }

        let inventory_text = fs::read_to_string(&checksum_path)?;
        let files = parse_md5_manifest(&inventory_text)?;
        let inventory = files
            .iter()
            .map(|file| (file.filename.clone(), file.md5.clone()))
            .collect::<Vec<_>>();
        let mut manifest = load_or_init_cache_state_manifest(
            &config.layout,
            &marker.cache_uuid,
            config.max_cache_bytes,
            &inventory,
        )?;
        reconcile_existing_files(&config.layout, &mut manifest)?;

        Ok(Self {
            layout: config.layout,
            marker,
            manifest,
            max_cache_bytes: config.max_cache_bytes,
        })
    }

    pub fn bulk_paths(&self) -> BulkPaths {
        BulkPaths::continuous(&self.layout.cache_dir)
    }

    pub fn write_session_manifest(
        &self,
        file_limit: Option<usize>,
        resume: bool,
    ) -> Result<UsbCacheDownloadSession> {
        let session = UsbCacheDownloadSession {
            schema_version: 1,
            session_id: format!(
                "usb-download-{}",
                crate::gaia::acquisition::usb_cache::utc_now_rfc3339()
            ),
            cache_uuid: self.marker.cache_uuid.clone(),
            cache_dir: self.layout.cache_dir.display().to_string(),
            max_cache_bytes: self.max_cache_bytes,
            file_limit,
            resume,
            started_at_utc: crate::gaia::acquisition::usb_cache::utc_now_rfc3339(),
        };
        let path = self.layout.manifests_dir.join(SESSION_MANIFEST_FILENAME);
        crate::gaia::xp::pilot_io::atomic_write_json(
            &path,
            &(serde_json::to_string_pretty(&session)? + "\n"),
        )?;
        append_session_log(
            &self.layout,
            &format!(
                "download session {} file_limit={file_limit:?} resume={resume}",
                session.session_id
            ),
        )?;
        Ok(session)
    }

    /// Mark selected files as downloading without overwriting unrelated concurrent transitions.
    pub fn mark_files_downloading(&mut self, filenames: &[String]) -> Result<()> {
        self.update_manifest_locked(|manifest| {
            for filename in filenames {
                let state = manifest
                    .entries
                    .iter()
                    .find(|entry| entry.filename == *filename)
                    .with_context(|| format!("cache entry not found for {filename}"))?
                    .state;
                if matches!(
                    state,
                    CacheInputState::ChecksumVerified
                        | CacheInputState::Processing
                        | CacheInputState::Processed
                        | CacheInputState::OutputVerified
                        | CacheInputState::Releasable
                        | CacheInputState::Deleted
                ) {
                    continue;
                }
                transition_entry_state(manifest, filename, CacheInputState::Downloading, None)?;
            }
            Ok(())
        })
    }

    pub fn apply_bulk_output_manifest(
        &mut self,
        output: &BulkOutputManifest,
    ) -> Result<UsbCacheSyncReport> {
        let cache_dir = self.layout.cache_dir.clone();
        let max_cache_bytes = self.max_cache_bytes;
        let report = self.update_manifest_locked(|manifest| {
            let mut report = UsbCacheSyncReport {
                files_synced: 0,
                checksum_verified: 0,
                failed: 0,
                skipped_already_verified: 0,
                footprint_bytes: 0,
                passed: true,
                failures: Vec::new(),
            };

            for file in &output.files {
                report.files_synced += 1;
                let local_path = cache_dir.join(&file.local_path);
                let current_state = manifest
                    .entries
                    .iter()
                    .find(|entry| entry.filename == file.filename)
                    .with_context(|| format!("cache entry not found for {}", file.filename))?
                    .state;

                match file.status {
                    BulkFileStatus::Downloaded | BulkFileStatus::Resumed => {
                        let expected_md5 = manifest
                            .entries
                            .iter()
                            .find(|entry| entry.filename == file.filename)
                            .expect("entry checked above")
                            .official_md5
                            .clone();
                        let size = file.size_bytes.unwrap_or_else(|| {
                            fs::metadata(&local_path).map(|metadata| metadata.len()).unwrap_or(0)
                        });
                        let verification = assert_file_size_usb_safe(size, &file.filename)
                            .and_then(|_| {
                                if !file.official_md5.eq_ignore_ascii_case(&expected_md5) {
                                    bail!(
                                        "bulk output checksum metadata mismatch for {}: expected {}, found {}",
                                        file.filename,
                                        expected_md5,
                                        file.official_md5
                                    );
                                }
                                verify_md5_file(
                                    &local_path,
                                    &expected_md5,
                                    &format!("cached Gaia partition {}", file.filename),
                                )
                            });
                        if let Err(error) = verification {
                            report.failed += 1;
                            report.passed = false;
                            report
                                .failures
                                .push(format!("{}: {error:#}", file.filename));
                            transition_entry_state(
                                manifest,
                                &file.filename,
                                CacheInputState::Failed,
                                Some(error.to_string()),
                            )?;
                            continue;
                        }

                        if let Some(entry) = manifest
                            .entries
                            .iter_mut()
                            .find(|entry| entry.filename == file.filename)
                        {
                            entry.size_bytes = Some(size);
                        }
                        if is_verified_or_later(current_state) {
                            report.skipped_already_verified += 1;
                        } else {
                            transition_entry_state(
                                manifest,
                                &file.filename,
                                CacheInputState::ChecksumVerified,
                                None,
                            )?;
                        }
                        report.checksum_verified += 1;
                    }
                    BulkFileStatus::Failed => {
                        if is_verified_or_later(current_state) {
                            report.skipped_already_verified += 1;
                            continue;
                        }
                        report.failed += 1;
                        report.passed = false;
                        report
                            .failures
                            .push(format!("{}: download failed", file.filename));
                        transition_entry_state(
                            manifest,
                            &file.filename,
                            CacheInputState::Failed,
                            Some("bulk download failed".to_string()),
                        )?;
                    }
                    BulkFileStatus::Pending => {
                        if !is_verified_or_later(current_state) {
                            transition_entry_state(
                                manifest,
                                &file.filename,
                                CacheInputState::Planned,
                                None,
                            )?;
                        }
                    }
                }
            }

            report.footprint_bytes = current_cache_footprint_bytes(manifest);
            if report.footprint_bytes > max_cache_bytes {
                report.passed = false;
                report.failures.push(format!(
                    "cache footprint {} exceeds max_cache_bytes {}",
                    report.footprint_bytes, max_cache_bytes
                ));
            }
            Ok(report)
        })?;

        append_session_log(
            &self.layout,
            &format!(
                "sync complete verified={} failed={} footprint={}",
                report.checksum_verified, report.failed, report.footprint_bytes
            ),
        )?;
        Ok(report)
    }

    pub fn apply_bulk_report(&mut self, report: &BulkReport) -> Result<UsbCacheSyncReport> {
        let output_path = PathBuf::from(&report.output_manifest_path);
        let output: BulkOutputManifest =
            serde_json::from_str(&fs::read_to_string(&output_path).with_context(|| {
                format!(
                    "failed to read bulk output manifest {}",
                    output_path.display()
                )
            })?)?;
        self.apply_bulk_output_manifest(&output)
    }

    /// Persist the current snapshot. Prefer transition methods for concurrent operation.
    pub fn commit_manifest(&mut self) -> Result<()> {
        let local = self.manifest.clone();
        self.update_manifest_locked(|latest| {
            for local_entry in local.entries {
                if let Some(entry) = latest
                    .entries
                    .iter_mut()
                    .find(|entry| entry.filename == local_entry.filename)
                {
                    if local_entry.updated_at_utc > entry.updated_at_utc {
                        *entry = local_entry;
                    }
                }
            }
            Ok(())
        })
    }

    pub fn transition(
        &mut self,
        filename: &str,
        next: CacheInputState,
        error: Option<String>,
    ) -> Result<()> {
        self.update_manifest_locked(|manifest| {
            transition_entry_state(manifest, filename, next, error)
        })
    }

    pub fn mark_processing(&mut self, filename: &str) -> Result<()> {
        self.transition(filename, CacheInputState::Processing, None)
    }

    pub fn mark_processed(&mut self, filename: &str) -> Result<()> {
        self.transition(filename, CacheInputState::Processed, None)
    }

    pub fn mark_output_verified(&mut self, filename: &str) -> Result<()> {
        self.transition(filename, CacheInputState::OutputVerified, None)
    }

    pub fn mark_releasable(&mut self, filename: &str) -> Result<()> {
        self.transition(filename, CacheInputState::Releasable, None)
    }

    /// Advance cache entry through post-processing states after a successful mini-pilot.
    pub fn advance_after_mini_pilot(&mut self, filename: &str) -> Result<()> {
        self.update_manifest_locked(|manifest| {
            for state in [
                CacheInputState::Processing,
                CacheInputState::Processed,
                CacheInputState::OutputVerified,
                CacheInputState::Releasable,
            ] {
                transition_entry_state(manifest, filename, state, None)?;
            }
            Ok(())
        })?;
        append_session_log(
            &self.layout,
            &format!("mini-pilot complete; marked releasable: {filename}"),
        )
    }

    /// Run cleanup for releasable inputs; optional `cleanup_limit` restricts live deletes.
    pub fn run_input_cleanup(
        &mut self,
        dry_run: bool,
        cleanup_limit: Option<usize>,
    ) -> Result<CleanupSimulationReport> {
        let layout = self.layout.clone();
        self.update_manifest_locked(|manifest| {
            simulate_or_run_cleanup(&layout, manifest, dry_run, cleanup_limit)
        })
    }

    pub fn reload_manifest(&mut self) -> Result<()> {
        let path = self.layout.state_manifest_path();
        let lock_path = path.with_extension("json.lock");
        let _lock = crate::platform::file_lock::lock_exclusive(&lock_path)?;
        self.manifest = read_cache_manifest(&path)?;
        validate_manifest_identity(&self.manifest, &self.marker.cache_uuid)?;
        Ok(())
    }

    pub fn entry_state(&self, filename: &str) -> Option<CacheInputState> {
        self.manifest
            .entries
            .iter()
            .find(|entry| entry.filename == filename)
            .map(|entry| entry.state)
    }

    fn update_manifest_locked<T>(
        &mut self,
        update: impl FnOnce(&mut CacheStateManifest) -> Result<T>,
    ) -> Result<T> {
        let path = self.layout.state_manifest_path();
        let lock_path = path.with_extension("json.lock");
        let _lock = crate::platform::file_lock::lock_exclusive(&lock_path)?;
        let mut latest = if path.is_file() {
            read_cache_manifest(&path)?
        } else {
            self.manifest.clone()
        };
        validate_manifest_identity(&latest, &self.marker.cache_uuid)?;
        let value = update(&mut latest)?;
        write_cache_state_manifest(&path, &latest)?;
        self.manifest = latest;
        Ok(value)
    }
}

fn read_cache_manifest(path: &Path) -> Result<CacheStateManifest> {
    serde_json::from_str(&fs::read_to_string(path)?)
        .with_context(|| format!("failed to read cache manifest at {}", path.display()))
}

fn validate_manifest_identity(manifest: &CacheStateManifest, expected_uuid: &str) -> Result<()> {
    if manifest.cache_uuid != expected_uuid {
        bail!(
            "cache manifest UUID mismatch: expected {expected_uuid}, found {}",
            manifest.cache_uuid
        );
    }
    Ok(())
}

fn is_verified_or_later(state: CacheInputState) -> bool {
    matches!(
        state,
        CacheInputState::ChecksumVerified
            | CacheInputState::Processing
            | CacheInputState::Processed
            | CacheInputState::OutputVerified
            | CacheInputState::Releasable
            | CacheInputState::Deleted
    )
}

/// Reconcile on-disk files with the cache manifest while holding its lock.
pub fn reconcile_existing_files(
    layout: &UsbCacheLayout,
    manifest: &mut CacheStateManifest,
) -> Result<()> {
    let path = layout.state_manifest_path();
    let lock_path = path.with_extension("json.lock");
    let _lock = crate::platform::file_lock::lock_exclusive(&lock_path)?;
    let mut latest = if path.is_file() {
        read_cache_manifest(&path)?
    } else {
        manifest.clone()
    };
    validate_manifest_identity(&latest, &manifest.cache_uuid)?;

    for entry in &mut latest.entries {
        let input = layout.cache_dir.join(&entry.local_path);
        if !input.is_file() {
            if entry.state == CacheInputState::Downloading {
                entry.state = CacheInputState::Planned;
                entry.error = None;
            } else if is_verified_or_later(entry.state)
                || entry.state == CacheInputState::Downloaded
            {
                entry.state = CacheInputState::Planned;
                entry.size_bytes = None;
                entry.error = None;
            }
            continue;
        }

        let size = fs::metadata(&input)?.len();
        assert_file_size_usb_safe(size, &entry.filename)?;
        entry.size_bytes = Some(size);
        match verify_md5_file(
            &input,
            &entry.official_md5,
            &format!("cached Gaia partition {}", entry.filename),
        ) {
            Ok(()) => {
                entry.error = None;
                if matches!(
                    entry.state,
                    CacheInputState::Planned
                        | CacheInputState::Downloading
                        | CacheInputState::Downloaded
                        | CacheInputState::Failed
                        | CacheInputState::Deleted
                ) {
                    entry.state = CacheInputState::ChecksumVerified;
                }
            }
            Err(error) => {
                entry.state = CacheInputState::Failed;
                entry.error = Some(error.to_string());
            }
        }
        entry.updated_at_utc = crate::gaia::acquisition::usb_cache::utc_now_rfc3339();
    }

    write_cache_state_manifest(&path, &latest)?;
    *manifest = latest;
    Ok(())
}

pub fn filenames_for_download(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    select_filenames(
        manifest,
        file_limit,
        |state| {
            matches!(
                state,
                CacheInputState::Planned | CacheInputState::Failed | CacheInputState::Downloading
            )
        },
        false,
    )
}

pub fn filenames_checksum_verified(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    select_filenames(
        manifest,
        file_limit,
        |state| state == CacheInputState::ChecksumVerified,
        false,
    )
}

pub fn filenames_releasable(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    select_filenames(
        manifest,
        file_limit,
        |state| state == CacheInputState::Releasable,
        false,
    )
}

pub fn filenames_for_production(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    select_filenames(
        manifest,
        file_limit,
        |state| {
            matches!(
                state,
                CacheInputState::Planned
                    | CacheInputState::Failed
                    | CacheInputState::ChecksumVerified
                    | CacheInputState::Processing
            )
        },
        true,
    )
}

fn select_filenames(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
    include: impl Fn(CacheInputState) -> bool,
    prioritize_processing: bool,
) -> Vec<String> {
    let mut names = manifest
        .entries
        .iter()
        .filter(|entry| include(entry.state))
        .map(|entry| entry.filename.clone())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| {
        let processing = manifest
            .entries
            .iter()
            .find(|entry| entry.filename == *name)
            .is_some_and(|entry| entry.state == CacheInputState::Processing);
        (prioritize_processing && !processing, name.clone())
    });
    if let Some(limit) = file_limit {
        names.truncate(limit);
    }
    names
}

pub fn bulk_filename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaia::acquisition::usb_cache::{
        planned_entries_from_inventory, CacheStateEntry, MAX_USB_FILE_BYTES,
    };

    fn test_layout(root: &Path) -> UsbCacheLayout {
        UsbCacheLayout::from_env(root, root.join("gaia-bulk"), "xp-continuous")
    }

    fn marker(layout: &UsbCacheLayout) -> UsbCacheRootMarker {
        UsbCacheRootMarker {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            created_at_utc: "0".to_string(),
            purpose: "test".to_string(),
            mountpoint: layout.mountpoint.display().to_string(),
            cache_root: layout.cache_root.display().to_string(),
            filesystem: "tmpfs".to_string(),
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
        }
    }

    fn manifest(layout: &UsbCacheLayout, files: &[(&str, &str)]) -> CacheStateManifest {
        CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: layout.cache_dir.display().to_string(),
            max_cache_bytes: MAX_USB_FILE_BYTES,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: planned_entries_from_inventory(
                &files
                    .iter()
                    .map(|(name, md5)| ((*name).to_string(), (*md5).to_string()))
                    .collect::<Vec<_>>(),
            ),
        }
    }

    fn rotator(layout: UsbCacheLayout, manifest: CacheStateManifest) -> UsbCacheRotator {
        UsbCacheRotator {
            marker: marker(&layout),
            layout,
            manifest,
            max_cache_bytes: MAX_USB_FILE_BYTES,
        }
    }

    #[test]
    fn filenames_for_production_prioritizes_processing() {
        let mut manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: "/tmp".to_string(),
            max_cache_bytes: MAX_USB_FILE_BYTES,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: vec![
                CacheStateEntry {
                    filename: "b.csv.gz".to_string(),
                    official_md5: "b".to_string(),
                    size_bytes: None,
                    state: CacheInputState::Planned,
                    local_path: "b.csv.gz".to_string(),
                    updated_at_utc: "0".to_string(),
                    error: None,
                },
                CacheStateEntry {
                    filename: "a.csv.gz".to_string(),
                    official_md5: "a".to_string(),
                    size_bytes: None,
                    state: CacheInputState::Processing,
                    local_path: "a.csv.gz".to_string(),
                    updated_at_utc: "0".to_string(),
                    error: None,
                },
            ],
        };
        manifest.entries.reverse();
        assert_eq!(
            filenames_for_production(&manifest, None),
            vec!["a.csv.gz", "b.csv.gz"]
        );
    }

    #[test]
    fn reconciliation_verifies_existing_file_md5() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = test_layout(dir.path());
        fs::create_dir_all(&layout.cache_dir)?;
        fs::create_dir_all(&layout.manifests_dir)?;
        fs::write(layout.cache_dir.join("a.csv.gz"), b"abc")?;
        let mut manifest = manifest(&layout, &[("a.csv.gz", "900150983cd24fb0d6963f7d28e17f72")]);
        write_cache_state_manifest(&layout.state_manifest_path(), &manifest)?;

        reconcile_existing_files(&layout, &mut manifest)?;

        assert_eq!(manifest.entries[0].state, CacheInputState::ChecksumVerified);
        assert!(manifest.entries[0].error.is_none());
        Ok(())
    }

    #[test]
    fn reconciliation_rejects_corrupt_existing_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = test_layout(dir.path());
        fs::create_dir_all(&layout.cache_dir)?;
        fs::create_dir_all(&layout.manifests_dir)?;
        fs::write(layout.cache_dir.join("a.csv.gz"), b"corrupt")?;
        let mut manifest = manifest(&layout, &[("a.csv.gz", "900150983cd24fb0d6963f7d28e17f72")]);
        write_cache_state_manifest(&layout.state_manifest_path(), &manifest)?;

        reconcile_existing_files(&layout, &mut manifest)?;

        assert_eq!(manifest.entries[0].state, CacheInputState::Failed);
        assert!(manifest.entries[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("checksum mismatch")));
        Ok(())
    }

    #[test]
    fn concurrent_transitions_preserve_both_entries() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = test_layout(dir.path());
        fs::create_dir_all(&layout.manifests_dir)?;
        let manifest = manifest(
            &layout,
            &[
                ("a.csv.gz", "900150983cd24fb0d6963f7d28e17f72"),
                ("b.csv.gz", "900150983cd24fb0d6963f7d28e17f72"),
            ],
        );
        write_cache_state_manifest(&layout.state_manifest_path(), &manifest)?;
        let mut left = rotator(layout.clone(), manifest.clone());
        let mut right = rotator(layout.clone(), manifest);

        left.mark_processing("a.csv.gz")?;
        right.mark_processing("b.csv.gz")?;
        right.reload_manifest()?;

        assert_eq!(
            right.entry_state("a.csv.gz"),
            Some(CacheInputState::Processing)
        );
        assert_eq!(
            right.entry_state("b.csv.gz"),
            Some(CacheInputState::Processing)
        );
        Ok(())
    }
}
