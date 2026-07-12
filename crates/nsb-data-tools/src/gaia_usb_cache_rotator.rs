//! USB cache rotator: wires official bulk download to cache state transitions.
//!
//! Download path: `planned` → `downloading` → `downloaded` → `checksum_verified`
//! Processing path (downstream): `processing` → `processed` → `output_verified` → `releasable`

use crate::gaia_bulk::{
    parse_md5_manifest, BulkFileStatus, BulkOutputManifest, BulkPaths, BulkReport,
};
use crate::gaia_usb_cache::{
    append_session_log, assert_file_size_usb_safe, current_cache_footprint_bytes,
    load_or_init_cache_state_manifest, read_or_create_cache_root_marker, simulate_or_run_cleanup,
    transition_entry_state, verify_usb_identity, write_cache_state_manifest, CacheInputState,
    CacheStateManifest, CleanupSimulationReport, UsbCacheLayout, UsbCacheRootMarker,
    OFFICIAL_CHECKSUM_MANIFEST,
};
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
        let pairs = files
            .iter()
            .map(|file| (file.filename.clone(), file.md5.clone()))
            .collect::<Vec<_>>();
        let mut manifest = load_or_init_cache_state_manifest(
            &config.layout,
            &marker.cache_uuid,
            config.max_cache_bytes,
            &pairs,
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
            session_id: format!("usb-download-{}", crate::gaia_usb_cache::utc_now_rfc3339()),
            cache_uuid: self.marker.cache_uuid.clone(),
            cache_dir: self.layout.cache_dir.display().to_string(),
            max_cache_bytes: self.max_cache_bytes,
            file_limit,
            resume,
            started_at_utc: crate::gaia_usb_cache::utc_now_rfc3339(),
        };
        let path = self.layout.manifests_dir.join(SESSION_MANIFEST_FILENAME);
        crate::gaia_xp_continuous_pilot_io::atomic_write_json(
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

    pub fn mark_files_downloading(&mut self, filenames: &[String]) -> Result<()> {
        for filename in filenames {
            if let Some(entry) = self
                .manifest
                .entries
                .iter()
                .find(|e| e.filename == *filename)
            {
                if entry.state == CacheInputState::ChecksumVerified {
                    continue;
                }
            }
            transition_entry_state(
                &mut self.manifest,
                filename,
                CacheInputState::Downloading,
                None,
            )?;
        }
        self.commit_manifest()?;
        Ok(())
    }

    pub fn apply_bulk_output_manifest(
        &mut self,
        output: &BulkOutputManifest,
    ) -> Result<UsbCacheSyncReport> {
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
            let local_path = self.layout.cache_dir.join(&file.local_path);
            let next_state = match file.status {
                BulkFileStatus::Downloaded | BulkFileStatus::Resumed => {
                    let size = file
                        .size_bytes
                        .unwrap_or_else(|| fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0));
                    if let Err(error) = assert_file_size_usb_safe(size, &file.filename) {
                        report.passed = false;
                        report.failures.push(error.to_string());
                        transition_entry_state(
                            &mut self.manifest,
                            &file.filename,
                            CacheInputState::Failed,
                            Some(error.to_string()),
                        )?;
                        report.failed += 1;
                        continue;
                    }
                    if let Some(entry) = self
                        .manifest
                        .entries
                        .iter_mut()
                        .find(|entry| entry.filename == file.filename)
                    {
                        entry.size_bytes = Some(size);
                    }
                    report.checksum_verified += 1;
                    CacheInputState::ChecksumVerified
                }
                BulkFileStatus::Failed => {
                    report.failed += 1;
                    report.passed = false;
                    if let Some(entry) = self
                        .manifest
                        .entries
                        .iter()
                        .find(|e| e.filename == file.filename)
                    {
                        if let Some(err) = &entry.error {
                            report.failures.push(format!("{}: {err}", file.filename));
                        } else {
                            report
                                .failures
                                .push(format!("{}: download failed", file.filename));
                        }
                    }
                    CacheInputState::Failed
                }
                BulkFileStatus::Pending => CacheInputState::Planned,
            };

            if matches!(next_state, CacheInputState::ChecksumVerified)
                && self
                    .manifest
                    .entries
                    .iter()
                    .find(|e| e.filename == file.filename)
                    .is_some_and(|e| e.state == CacheInputState::ChecksumVerified)
            {
                report.skipped_already_verified += 1;
                continue;
            }

            let error = if next_state == CacheInputState::Failed {
                Some("bulk download failed".to_string())
            } else {
                None
            };
            transition_entry_state(&mut self.manifest, &file.filename, next_state, error)?;
        }

        report.footprint_bytes = current_cache_footprint_bytes(&self.manifest);
        if report.footprint_bytes > self.max_cache_bytes {
            report.passed = false;
            report.failures.push(format!(
                "cache footprint {} exceeds max_cache_bytes {}",
                report.footprint_bytes, self.max_cache_bytes
            ));
        }
        self.commit_manifest()?;
        self.reset_orphaned_downloading_states(
            &output
                .files
                .iter()
                .map(|file| file.filename.clone())
                .collect::<Vec<_>>(),
        )?;
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

    pub fn commit_manifest(&self) -> Result<()> {
        write_cache_state_manifest(&self.layout.state_manifest_path(), &self.manifest)
    }

    pub fn transition(
        &mut self,
        filename: &str,
        next: CacheInputState,
        error: Option<String>,
    ) -> Result<()> {
        transition_entry_state(&mut self.manifest, filename, next, error)?;
        self.commit_manifest()
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
        self.mark_processing(filename)?;
        self.mark_processed(filename)?;
        self.mark_output_verified(filename)?;
        self.mark_releasable(filename)?;
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
        simulate_or_run_cleanup(&self.layout, &mut self.manifest, dry_run, cleanup_limit)
    }

    pub fn reload_manifest(&mut self) -> Result<()> {
        let path = self.layout.state_manifest_path();
        self.manifest = serde_json::from_str(&fs::read_to_string(&path)?)
            .with_context(|| format!("failed to reload cache manifest at {}", path.display()))?;
        Ok(())
    }

    pub fn entry_state(&self, filename: &str) -> Option<CacheInputState> {
        self.manifest
            .entries
            .iter()
            .find(|entry| entry.filename == filename)
            .map(|entry| entry.state)
    }

    fn reset_orphaned_downloading_states(&mut self, synced_filenames: &[String]) -> Result<()> {
        let synced: std::collections::BTreeSet<&str> =
            synced_filenames.iter().map(String::as_str).collect();
        for entry in &mut self.manifest.entries {
            if entry.state == CacheInputState::Downloading
                && !synced.contains(entry.filename.as_str())
            {
                entry.state = CacheInputState::Planned;
                entry.updated_at_utc = crate::gaia_usb_cache::utc_now_rfc3339();
                entry.error = None;
            }
        }
        self.commit_manifest()
    }
}

/// Reconcile on-disk files with cache manifest (e.g. after resume or manual copy).
pub fn reconcile_existing_files(
    layout: &UsbCacheLayout,
    manifest: &mut CacheStateManifest,
) -> Result<()> {
    for entry in &mut manifest.entries {
        let path = layout.cache_dir.join(&entry.local_path);
        if !path.is_file() {
            if entry.state == CacheInputState::Downloading {
                entry.state = CacheInputState::Planned;
                entry.error = None;
            } else if matches!(
                entry.state,
                CacheInputState::ChecksumVerified
                    | CacheInputState::Downloaded
                    | CacheInputState::Processing
                    | CacheInputState::Processed
                    | CacheInputState::OutputVerified
                    | CacheInputState::Releasable
            ) {
                entry.state = CacheInputState::Planned;
                entry.size_bytes = None;
            }
            continue;
        }
        let size = fs::metadata(&path)?.len();
        assert_file_size_usb_safe(size, &entry.filename)?;
        entry.size_bytes = Some(size);
        if matches!(
            entry.state,
            CacheInputState::Planned | CacheInputState::Downloading | CacheInputState::Downloaded
        ) {
            entry.state = CacheInputState::ChecksumVerified;
        }
    }
    write_cache_state_manifest(&layout.state_manifest_path(), manifest)?;
    Ok(())
}

pub fn filenames_for_download(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    let mut names: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.state,
                CacheInputState::Planned | CacheInputState::Failed | CacheInputState::Downloading
            )
        })
        .map(|entry| entry.filename.clone())
        .collect();
    names.sort();
    if let Some(limit) = file_limit {
        names.truncate(limit);
    }
    names
}

pub fn filenames_checksum_verified(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    let mut names: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| entry.state == CacheInputState::ChecksumVerified)
        .map(|entry| entry.filename.clone())
        .collect();
    names.sort();
    if let Some(limit) = file_limit {
        names.truncate(limit);
    }
    names
}

pub fn filenames_releasable(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    let mut names: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| entry.state == CacheInputState::Releasable)
        .map(|entry| entry.filename.clone())
        .collect();
    names.sort();
    if let Some(limit) = file_limit {
        names.truncate(limit);
    }
    names
}

pub fn filenames_for_production(
    manifest: &CacheStateManifest,
    file_limit: Option<usize>,
) -> Vec<String> {
    let mut processing: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| entry.state == CacheInputState::Processing)
        .map(|entry| entry.filename.clone())
        .collect();
    let mut rest: Vec<String> = manifest
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.state,
                CacheInputState::Planned
                    | CacheInputState::Failed
                    | CacheInputState::ChecksumVerified
            )
        })
        .map(|entry| entry.filename.clone())
        .collect();
    processing.sort();
    rest.sort();
    processing.append(&mut rest);
    if let Some(limit) = file_limit {
        processing.truncate(limit);
    }
    processing
}

pub fn bulk_filename(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaia_usb_cache::{
        planned_entries_from_inventory, write_cache_state_manifest, CacheStateEntry,
        MAX_USB_FILE_BYTES,
    };
    use std::path::Path;

    fn test_layout(dir: &Path) -> UsbCacheLayout {
        UsbCacheLayout::from_env(dir, dir.join("gaia-bulk"), "xp-continuous")
    }

    #[test]
    fn filenames_for_production_prioritizes_processing() {
        let manifest = CacheStateManifest {
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
        let names = filenames_for_production(&manifest, None);
        assert_eq!(names, vec!["a.csv.gz", "b.csv.gz"]);
    }

    #[test]
    fn bulk_status_maps_to_checksum_verified() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = test_layout(dir.path());
        fs::create_dir_all(&layout.cache_dir)?;
        fs::write(layout.cache_dir.join("a.csv.gz"), b"abc")?;

        let manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: layout.cache_dir.display().to_string(),
            max_cache_bytes: MAX_USB_FILE_BYTES,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: planned_entries_from_inventory(&[(
                "a.csv.gz".to_string(),
                "deadbeef".to_string(),
            )]),
        };
        write_cache_state_manifest(&layout.state_manifest_path(), &manifest)?;

        let marker = UsbCacheRootMarker {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            created_at_utc: "0".to_string(),
            purpose: "test".to_string(),
            mountpoint: dir.path().display().to_string(),
            cache_root: layout.cache_root.display().to_string(),
            filesystem: "tmpfs".to_string(),
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
        };

        let mut rotator = UsbCacheRotator {
            layout,
            marker,
            manifest,
            max_cache_bytes: MAX_USB_FILE_BYTES,
        };

        let output = BulkOutputManifest {
            schema_version: 1,
            product: "test".to_string(),
            source_url: "https://example.test/".to_string(),
            checksum_algorithm: "MD5".to_string(),
            inventory_total_files: 1,
            requested_files: 1,
            complete: true,
            complete_inventory: false,
            files: vec![crate::gaia_bulk::BulkOutputFile {
                filename: "a.csv.gz".to_string(),
                official_md5: "deadbeef".to_string(),
                size_bytes: Some(3),
                local_path: "a.csv.gz".to_string(),
                status: BulkFileStatus::Downloaded,
            }],
        };

        let report = rotator.apply_bulk_output_manifest(&output)?;
        assert_eq!(report.checksum_verified, 1);
        assert_eq!(
            rotator.manifest.entries[0].state,
            CacheInputState::ChecksumVerified
        );
        Ok(())
    }

    #[test]
    fn oversized_download_fails_vfat_gate() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = test_layout(dir.path());
        fs::create_dir_all(&layout.cache_dir)?;

        let manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: layout.cache_dir.display().to_string(),
            max_cache_bytes: MAX_USB_FILE_BYTES,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: planned_entries_from_inventory(&[(
                "big.csv.gz".to_string(),
                "abc".to_string(),
            )]),
        };

        let marker = UsbCacheRootMarker {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            created_at_utc: "0".to_string(),
            purpose: "test".to_string(),
            mountpoint: dir.path().display().to_string(),
            cache_root: layout.cache_root.display().to_string(),
            filesystem: "tmpfs".to_string(),
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
        };

        let mut rotator = UsbCacheRotator {
            layout,
            marker,
            manifest,
            max_cache_bytes: MAX_USB_FILE_BYTES,
        };

        let output = BulkOutputManifest {
            schema_version: 1,
            product: "test".to_string(),
            source_url: "https://example.test/".to_string(),
            checksum_algorithm: "MD5".to_string(),
            inventory_total_files: 1,
            requested_files: 1,
            complete: false,
            complete_inventory: false,
            files: vec![crate::gaia_bulk::BulkOutputFile {
                filename: "big.csv.gz".to_string(),
                official_md5: "abc".to_string(),
                size_bytes: Some(MAX_USB_FILE_BYTES + 1),
                local_path: "big.csv.gz".to_string(),
                status: BulkFileStatus::Downloaded,
            }],
        };

        let report = rotator.apply_bulk_output_manifest(&output)?;
        assert!(!report.passed);
        assert_eq!(rotator.manifest.entries[0].state, CacheInputState::Failed);
        Ok(())
    }
}
