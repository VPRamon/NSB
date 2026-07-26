//! USB-backed rotating cache for official Gaia DR3 XP continuous bulk files.
//!
//! Enforces mountpoint identity, vfat-safe file sizes, transactional `.part`
//! writes, and explicit per-input lifecycle states.

use crate::gaia::xp::pilot_io::atomic_write_json;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// vfat/exFAT single-file size ceiling used for USB cache safety.
pub const MAX_USB_FILE_BYTES: u64 = 4_000_000_000;

/// Marker filename written at the USB cache root to prevent accidental writes.
pub const CACHE_ROOT_MARKER_FILENAME: &str = ".nsb-gaia-cache-root.json";

/// Rotating cache state manifest filename.
pub const CACHE_STATE_MANIFEST_FILENAME: &str = "cache_state_manifest.json";

/// Official Gaia bulk checksum manifest copied into the cache.
pub const OFFICIAL_CHECKSUM_MANIFEST: &str = "_MD5SUM.txt";

const CACHE_ROOT_MARKER_SCHEMA_VERSION: u32 = 1;
const CACHE_STATE_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Explicit lifecycle states for each cached bulk input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheInputState {
    Planned,
    Downloading,
    Downloaded,
    ChecksumVerified,
    Processing,
    Processed,
    OutputVerified,
    Releasable,
    Deleted,
    Failed,
}

impl CacheInputState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Deleted | Self::Failed)
    }

    pub fn allows_processing(self) -> bool {
        matches!(self, Self::ChecksumVerified | Self::Processing)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbCacheRootMarker {
    pub schema_version: u32,
    pub cache_uuid: String,
    pub created_at_utc: String,
    pub purpose: String,
    pub mountpoint: String,
    pub cache_root: String,
    pub filesystem: String,
    pub max_usb_file_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStateEntry {
    pub filename: String,
    pub official_md5: String,
    pub size_bytes: Option<u64>,
    pub state: CacheInputState,
    /// Path relative to the cache directory.
    pub local_path: String,
    pub updated_at_utc: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStateManifest {
    pub schema_version: u32,
    pub cache_uuid: String,
    pub cache_dir: String,
    pub max_cache_bytes: u64,
    pub max_usb_file_bytes: u64,
    pub entries: Vec<CacheStateEntry>,
}

#[derive(Debug, Clone)]
pub struct UsbCacheLayout {
    pub mountpoint: PathBuf,
    pub cache_root: PathBuf,
    pub cache_dir: PathBuf,
    pub manifests_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub reconciliation_dir: PathBuf,
}

impl UsbCacheLayout {
    pub fn from_env(
        mountpoint: impl Into<PathBuf>,
        cache_root: impl Into<PathBuf>,
        cache_subdir: &str,
    ) -> Self {
        let mountpoint = mountpoint.into();
        let cache_root = cache_root.into();
        Self {
            cache_dir: cache_root.join(cache_subdir),
            manifests_dir: cache_root.join("manifests"),
            logs_dir: cache_root.join("logs"),
            reconciliation_dir: cache_root.join("reconciliation"),
            mountpoint,
            cache_root,
        }
    }

    pub fn marker_path(&self) -> PathBuf {
        self.cache_root.join(CACHE_ROOT_MARKER_FILENAME)
    }

    pub fn state_manifest_path(&self) -> PathBuf {
        self.manifests_dir.join(CACHE_STATE_MANIFEST_FILENAME)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbIdentityReport {
    pub mountpoint_exists: bool,
    pub mountpoint: String,
    pub marker_exists: bool,
    pub marker_valid: bool,
    pub cache_uuid: Option<String>,
    pub filesystem: Option<String>,
    pub mount_device: Option<String>,
    pub passed: bool,
    pub failures: Vec<String>,
}

pub fn utc_now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

pub fn verify_mountpoint_exists(mountpoint: &Path) -> Result<()> {
    if !mountpoint.exists() {
        bail!("USB mountpoint does not exist: {}", mountpoint.display());
    }
    let metadata = fs::metadata(mountpoint)
        .with_context(|| format!("failed to stat mountpoint {}", mountpoint.display()))?;
    if !metadata.is_dir() {
        bail!(
            "USB mountpoint is not a directory: {}",
            mountpoint.display()
        );
    }
    Ok(())
}

pub fn read_mount_info(mountpoint: &Path) -> Result<(Option<String>, Option<String>)> {
    let mounts = fs::read_to_string("/proc/mounts").context("failed to read /proc/mounts")?;
    let target = fs::canonicalize(mountpoint)
        .with_context(|| format!("failed to canonicalize {}", mountpoint.display()))?;
    let target = target.to_string_lossy();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let device = parts.next();
        let mount = parts.next();
        let fstype = parts.next();
        if let (Some(device), Some(mount), Some(fstype)) = (device, mount, fstype) {
            if mount == target.as_ref() {
                return Ok((Some(device.to_string()), Some(fstype.to_string())));
            }
        }
    }
    Ok((None, None))
}

pub fn read_or_create_cache_root_marker(
    layout: &UsbCacheLayout,
    create_if_missing: bool,
) -> Result<UsbCacheRootMarker> {
    verify_mountpoint_exists(&layout.mountpoint)?;
    let marker_path = layout.marker_path();
    if marker_path.is_file() {
        let marker: UsbCacheRootMarker = serde_json::from_str(&fs::read_to_string(&marker_path)?)
            .with_context(|| {
            format!("invalid cache root marker at {}", marker_path.display())
        })?;
        if marker.schema_version != CACHE_ROOT_MARKER_SCHEMA_VERSION {
            bail!(
                "unsupported cache root marker schema version {}",
                marker.schema_version
            );
        }
        return Ok(marker);
    }
    if !create_if_missing {
        bail!("USB cache root marker missing at {}", marker_path.display());
    }
    fs::create_dir_all(&layout.cache_root)?;
    let (device, fstype) = read_mount_info(&layout.mountpoint)?;
    let marker = UsbCacheRootMarker {
        schema_version: CACHE_ROOT_MARKER_SCHEMA_VERSION,
        cache_uuid: uuid_v4_from_time(),
        created_at_utc: utc_now_rfc3339(),
        purpose: "gaia_dr3_xp_continuous_bulk_rotating_cache".to_string(),
        mountpoint: layout.mountpoint.display().to_string(),
        cache_root: layout.cache_root.display().to_string(),
        filesystem: fstype.unwrap_or_else(|| "unknown".to_string()),
        max_usb_file_bytes: MAX_USB_FILE_BYTES,
    };
    let _ = device;
    atomic_write_json(
        &marker_path,
        &(serde_json::to_string_pretty(&marker)? + "\n"),
    )?;
    Ok(marker)
}

pub fn verify_usb_identity(layout: &UsbCacheLayout) -> Result<UsbIdentityReport> {
    let mut failures = Vec::new();
    let mountpoint_exists = layout.mountpoint.exists();
    if !mountpoint_exists {
        failures.push(format!(
            "mountpoint missing: {}",
            layout.mountpoint.display()
        ));
        return Ok(UsbIdentityReport {
            mountpoint_exists,
            mountpoint: layout.mountpoint.display().to_string(),
            marker_exists: false,
            marker_valid: false,
            cache_uuid: None,
            filesystem: None,
            mount_device: None,
            passed: false,
            failures,
        });
    }

    let (mount_device, filesystem) = read_mount_info(&layout.mountpoint)?;
    let marker_path = layout.marker_path();
    let marker_exists = marker_path.is_file();
    let mut marker_valid = false;
    let mut cache_uuid = None;

    if !marker_exists {
        failures.push(format!(
            "cache root marker missing: {}",
            marker_path.display()
        ));
    } else {
        match serde_json::from_str::<UsbCacheRootMarker>(&fs::read_to_string(&marker_path)?) {
            Ok(marker) => {
                marker_valid = true;
                cache_uuid = Some(marker.cache_uuid.clone());
                if marker.cache_root != layout.cache_root.display().to_string() {
                    failures.push(format!(
                        "marker cache_root mismatch: expected {}, found {}",
                        layout.cache_root.display(),
                        marker.cache_root
                    ));
                    marker_valid = false;
                }
                if marker.max_usb_file_bytes != MAX_USB_FILE_BYTES {
                    failures.push(format!(
                        "marker max_usb_file_bytes mismatch: expected {MAX_USB_FILE_BYTES}, found {}",
                        marker.max_usb_file_bytes
                    ));
                    marker_valid = false;
                }
            }
            Err(error) => {
                failures.push(format!("invalid cache root marker JSON: {error}"));
            }
        }
    }

    let passed = failures.is_empty();
    Ok(UsbIdentityReport {
        mountpoint_exists,
        mountpoint: layout.mountpoint.display().to_string(),
        marker_exists,
        marker_valid,
        cache_uuid,
        filesystem,
        mount_device,
        passed,
        failures,
    })
}

pub fn assert_file_size_usb_safe(size_bytes: u64, filename: &str) -> Result<()> {
    if size_bytes > MAX_USB_FILE_BYTES {
        bail!("file {filename} exceeds vfat-safe limit: {size_bytes} > {MAX_USB_FILE_BYTES}");
    }
    Ok(())
}

pub fn atomic_write_part_then_rename(path: &Path, payload: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("part");
    {
        let mut file = File::create(&temp)?;
        file.write_all(payload)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn planned_entries_from_inventory(inventory: &[(String, String)]) -> Vec<CacheStateEntry> {
    inventory
        .iter()
        .map(|(filename, md5)| CacheStateEntry {
            filename: filename.clone(),
            official_md5: md5.clone(),
            size_bytes: None,
            state: CacheInputState::Planned,
            local_path: filename.clone(),
            updated_at_utc: utc_now_rfc3339(),
            error: None,
        })
        .collect()
}

pub fn load_or_init_cache_state_manifest(
    layout: &UsbCacheLayout,
    cache_uuid: &str,
    max_cache_bytes: u64,
    inventory: &[(String, String)],
) -> Result<CacheStateManifest> {
    fs::create_dir_all(&layout.manifests_dir)?;
    let path = layout.state_manifest_path();
    if path.is_file() {
        let manifest: CacheStateManifest = serde_json::from_str(&fs::read_to_string(&path)?)
            .with_context(|| format!("invalid cache state manifest at {}", path.display()))?;
        if manifest.cache_uuid != cache_uuid {
            bail!(
                "cache state manifest UUID mismatch: expected {cache_uuid}, found {}",
                manifest.cache_uuid
            );
        }
        return Ok(manifest);
    }

    let manifest = CacheStateManifest {
        schema_version: CACHE_STATE_MANIFEST_SCHEMA_VERSION,
        cache_uuid: cache_uuid.to_string(),
        cache_dir: layout.cache_dir.display().to_string(),
        max_cache_bytes,
        max_usb_file_bytes: MAX_USB_FILE_BYTES,
        entries: planned_entries_from_inventory(inventory),
    };
    write_cache_state_manifest(&path, &manifest)?;
    Ok(manifest)
}

pub fn write_cache_state_manifest(path: &Path, manifest: &CacheStateManifest) -> Result<()> {
    atomic_write_json(path, &(serde_json::to_string_pretty(manifest)? + "\n"))
}

pub fn transition_entry_state(
    manifest: &mut CacheStateManifest,
    filename: &str,
    next: CacheInputState,
    error: Option<String>,
) -> Result<()> {
    let entry = manifest
        .entries
        .iter_mut()
        .find(|entry| entry.filename == filename)
        .with_context(|| format!("cache entry not found for {filename}"))?;
    entry.state = next;
    entry.updated_at_utc = utc_now_rfc3339();
    entry.error = error;
    Ok(())
}

pub fn cache_bytes_by_state(manifest: &CacheStateManifest) -> BTreeMap<String, u64> {
    let mut totals = BTreeMap::new();
    for entry in &manifest.entries {
        let size = entry.size_bytes.unwrap_or(0);
        if matches!(
            entry.state,
            CacheInputState::Deleted | CacheInputState::Planned | CacheInputState::Failed
        ) {
            continue;
        }
        *totals
            .entry(format!("{:?}", entry.state).to_lowercase())
            .or_insert(0) += size;
    }
    totals
}

pub fn current_cache_footprint_bytes(manifest: &CacheStateManifest) -> u64 {
    manifest
        .entries
        .iter()
        .filter(|entry| {
            !matches!(
                entry.state,
                CacheInputState::Deleted | CacheInputState::Planned | CacheInputState::Failed
            )
        })
        .filter_map(|entry| entry.size_bytes)
        .sum()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSimulationReport {
    pub dry_run: bool,
    pub candidates: Vec<String>,
    pub bytes_reclaimed: u64,
    pub post_cleanup_footprint_bytes: u64,
    pub passed: bool,
}

pub fn simulate_or_run_cleanup(
    layout: &UsbCacheLayout,
    manifest: &mut CacheStateManifest,
    dry_run: bool,
    cleanup_limit: Option<usize>,
) -> Result<CleanupSimulationReport> {
    let mut candidates = Vec::new();
    let mut bytes_reclaimed = 0_u64;
    for entry in &manifest.entries {
        if entry.state == CacheInputState::Releasable {
            candidates.push(entry.filename.clone());
            bytes_reclaimed += entry.size_bytes.unwrap_or(0);
        }
    }
    candidates.sort();
    if let Some(limit) = cleanup_limit {
        if limit == 0 {
            candidates.clear();
            bytes_reclaimed = 0;
        } else {
            bytes_reclaimed = candidates
                .iter()
                .take(limit)
                .filter_map(|filename| {
                    manifest
                        .entries
                        .iter()
                        .find(|entry| &entry.filename == filename)
                        .and_then(|entry| entry.size_bytes)
                })
                .sum();
            candidates.truncate(limit);
        }
    }

    if !dry_run {
        for filename in &candidates {
            let entry = manifest
                .entries
                .iter_mut()
                .find(|entry| &entry.filename == filename)
                .expect("candidate must exist");
            let path = layout.cache_dir.join(&entry.local_path);
            if path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("failed to delete cached input {}", path.display()))?;
            }
            entry.state = CacheInputState::Deleted;
            entry.updated_at_utc = utc_now_rfc3339();
        }
        write_cache_state_manifest(&layout.state_manifest_path(), manifest)?;
    }

    let post_cleanup = if dry_run {
        current_cache_footprint_bytes(manifest).saturating_sub(bytes_reclaimed)
    } else {
        current_cache_footprint_bytes(manifest)
    };

    Ok(CleanupSimulationReport {
        dry_run,
        candidates,
        bytes_reclaimed,
        post_cleanup_footprint_bytes: post_cleanup,
        passed: true,
    })
}

pub fn append_session_log(layout: &UsbCacheLayout, event: &str) -> Result<()> {
    fs::create_dir_all(&layout.logs_dir)?;
    let path = layout.logs_dir.join("cache_session.log");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{} {event}", utc_now_rfc3339())?;
    file.sync_all()?;
    Ok(())
}

fn uuid_v4_from_time() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfat_limit_rejects_oversized_files() {
        let error = assert_file_size_usb_safe(MAX_USB_FILE_BYTES + 1, "big.csv.gz")
            .expect_err("oversized file must fail");
        assert!(error.to_string().contains("vfat-safe"));
    }

    #[test]
    fn cache_state_transitions_are_deterministic() -> Result<()> {
        let mut manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: "/cache".to_string(),
            max_cache_bytes: 10,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: vec![CacheStateEntry {
                filename: "a.csv.gz".to_string(),
                official_md5: "abc".to_string(),
                size_bytes: Some(5),
                state: CacheInputState::Planned,
                local_path: "a.csv.gz".to_string(),
                updated_at_utc: "0".to_string(),
                error: None,
            }],
        };
        transition_entry_state(
            &mut manifest,
            "a.csv.gz",
            CacheInputState::Downloading,
            None,
        )?;
        assert_eq!(manifest.entries[0].state, CacheInputState::Downloading);
        Ok(())
    }

    #[test]
    fn cleanup_simulation_counts_releasable_inputs() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = UsbCacheLayout::from_env(dir.path(), dir.path().join("gaia-bulk"), "cache");
        fs::create_dir_all(&layout.cache_dir)?;
        fs::write(layout.cache_dir.join("a.csv.gz"), b"abc")?;
        let mut manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: layout.cache_dir.display().to_string(),
            max_cache_bytes: 100,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: vec![CacheStateEntry {
                filename: "a.csv.gz".to_string(),
                official_md5: "abc".to_string(),
                size_bytes: Some(3),
                state: CacheInputState::Releasable,
                local_path: "a.csv.gz".to_string(),
                updated_at_utc: "0".to_string(),
                error: None,
            }],
        };
        let dry = simulate_or_run_cleanup(&layout, &mut manifest, true, None)?;
        assert_eq!(dry.candidates, vec!["a.csv.gz".to_string()]);
        assert_eq!(dry.bytes_reclaimed, 3);
        assert!(layout.cache_dir.join("a.csv.gz").exists());

        let live = simulate_or_run_cleanup(&layout, &mut manifest, false, None)?;
        assert_eq!(live.post_cleanup_footprint_bytes, 0);
        assert!(!layout.cache_dir.join("a.csv.gz").exists());
        assert_eq!(manifest.entries[0].state, CacheInputState::Deleted);
        Ok(())
    }

    #[test]
    fn cleanup_limit_restricts_live_deletes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let layout = UsbCacheLayout::from_env(dir.path(), dir.path().join("gaia-bulk"), "cache");
        fs::create_dir_all(&layout.cache_dir)?;
        fs::write(layout.cache_dir.join("a.csv.gz"), b"aaa")?;
        fs::write(layout.cache_dir.join("b.csv.gz"), b"bbb")?;
        let mut manifest = CacheStateManifest {
            schema_version: 1,
            cache_uuid: "test".to_string(),
            cache_dir: layout.cache_dir.display().to_string(),
            max_cache_bytes: 100,
            max_usb_file_bytes: MAX_USB_FILE_BYTES,
            entries: vec![
                CacheStateEntry {
                    filename: "a.csv.gz".to_string(),
                    official_md5: "abc".to_string(),
                    size_bytes: Some(3),
                    state: CacheInputState::Releasable,
                    local_path: "a.csv.gz".to_string(),
                    updated_at_utc: "0".to_string(),
                    error: None,
                },
                CacheStateEntry {
                    filename: "b.csv.gz".to_string(),
                    official_md5: "def".to_string(),
                    size_bytes: Some(3),
                    state: CacheInputState::Releasable,
                    local_path: "b.csv.gz".to_string(),
                    updated_at_utc: "0".to_string(),
                    error: None,
                },
            ],
        };
        let live = simulate_or_run_cleanup(&layout, &mut manifest, false, Some(1))?;
        assert_eq!(live.candidates.len(), 1);
        assert_eq!(live.candidates[0], "a.csv.gz");
        assert!(!layout.cache_dir.join("a.csv.gz").exists());
        assert!(layout.cache_dir.join("b.csv.gz").exists());
        assert_eq!(manifest.entries[0].state, CacheInputState::Deleted);
        assert_eq!(manifest.entries[1].state, CacheInputState::Releasable);
        Ok(())
    }
}
