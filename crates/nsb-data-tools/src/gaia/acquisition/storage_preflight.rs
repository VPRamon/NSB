//! Internal and USB storage preflight for Gaia DR3 XP continuous bulk production.

#![allow(unsafe_code)]

use crate::gaia::acquisition::bulk::{
    parse_md5_manifest, BulkFile, OFFICIAL_GAIA_XP_CONTINUOUS_BASE_URL,
};
use crate::gaia::acquisition::usb_cache::{
    current_cache_footprint_bytes, load_or_init_cache_state_manifest,
    read_or_create_cache_root_marker, simulate_or_run_cleanup, verify_usb_identity, UsbCacheLayout,
    UsbIdentityReport, MAX_USB_FILE_BYTES,
};
use crate::platform::checksum_io::{sha256_file, verify_sha256_file};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const XP_CONTINUOUS_ONLY_POPULATION: u64 = 184_729_270;
pub const OFFICIAL_BULK_FILE_COUNT: usize = 3_386;
pub const OFFICIAL_BULK_COMPRESSED_TIB: f64 = 3.3;
pub const PHASE5_POLICY_V1_SHA256: &str =
    "c525de3ec6d0022a6ed468f8f2bde2515e8f8364915f5a7a02492eee21947b74";

const STORAGE_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMeasurement {
    pub path: String,
    pub filesystem: Option<String>,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub measured_existing_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotatingCachePlan {
    pub max_cache_bytes: u64,
    pub max_usb_file_bytes: u64,
    pub max_single_file_observed_bytes: u64,
    pub files_over_usb_limit: usize,
    pub concurrent_inputs_recommended: u32,
    pub peak_rotating_cache_bytes: u64,
    pub checkpoint_storage_gib: f64,
    pub transient_work_gib: f64,
    pub healpix_output_gib: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeasibilityReport {
    pub population_xp_continuous_only: u64,
    pub rotating_cache_feasible: bool,
    pub internal_headroom_bytes: i64,
    pub usb_headroom_bytes: i64,
    pub required_peak_internal_bytes: u64,
    pub required_peak_usb_bytes: u64,
    pub can_process_full_population: bool,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoragePlan {
    pub schema_version: u32,
    pub timestamp_utc: String,
    pub population_xp_continuous_only: u64,
    pub bulk_inventory_files: usize,
    pub bulk_compressed_tib: f64,
    pub max_usb_file_bytes: u64,
    pub internal: DiskMeasurement,
    pub usb: Option<DiskMeasurement>,
    pub usb_identity: Option<UsbIdentityReport>,
    pub rotating_cache: RotatingCachePlan,
    pub preflight_gates: Vec<PreflightGate>,
    pub feasibility: FeasibilityReport,
    pub conclusion: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialInventoryReport {
    pub source_url: String,
    pub inventory_files: usize,
    pub checksum_manifest_sha256: String,
    pub max_file_bytes: u64,
    pub files_over_usb_limit: usize,
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct StoragePreflightConfig {
    pub work_dir: PathBuf,
    pub checkpoint_dir: PathBuf,
    pub output_dir: PathBuf,
    pub manifest_dir: PathBuf,
    pub input_cache_dir: PathBuf,
    pub usb_mountpoint: Option<PathBuf>,
    pub usb_cache_root: Option<PathBuf>,
    pub max_cache_bytes: u64,
    pub frozen_policy: Option<PathBuf>,
    pub official_checksum_manifest: PathBuf,
    pub measured_internal_existing_bytes: Option<u64>,
}

pub fn measure_disk(path: &Path) -> Result<DiskMeasurement> {
    let (total, available, filesystem) = statvfs_bytes(path)?;
    let used = total.saturating_sub(available);
    Ok(DiskMeasurement {
        path: path.display().to_string(),
        filesystem,
        total_bytes: total,
        available_bytes: available,
        used_bytes: used,
        measured_existing_bytes: None,
    })
}

pub fn directory_size_bytes(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0_u64;
    if path.is_file() {
        return Ok(fs::metadata(path)?.len());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total = total.saturating_add(directory_size_bytes(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

pub fn verify_policy_checksum(policy_path: &Path) -> Result<PreflightGate> {
    match verify_sha256_file(policy_path, PHASE5_POLICY_V1_SHA256, "phase5 policy v1") {
        Ok(()) => Ok(PreflightGate {
            name: "policy_checksum".to_string(),
            passed: true,
            detail: format!(
                "phase5_frozen_validation_policy_v1.json sha256:{PHASE5_POLICY_V1_SHA256}"
            ),
        }),
        Err(error) => Ok(PreflightGate {
            name: "policy_checksum".to_string(),
            passed: false,
            detail: error.to_string(),
        }),
    }
}

pub fn audit_official_inventory(manifest_path: &Path) -> Result<OfficialInventoryReport> {
    let text = fs::read_to_string(manifest_path).with_context(|| {
        format!(
            "failed to read official checksum manifest {}",
            manifest_path.display()
        )
    })?;
    let files = parse_md5_manifest(&text)?;
    let inventory_files = files.len();
    let mut max_file_bytes = 0_u64;
    let mut files_over_usb_limit = 0_usize;
    for file in &files {
        let local = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&file.filename);
        if local.is_file() {
            let size = fs::metadata(&local)?.len();
            max_file_bytes = max_file_bytes.max(size);
            if size > MAX_USB_FILE_BYTES {
                files_over_usb_limit += 1;
            }
        }
    }
    let checksum_manifest_sha256 = sha256_file(manifest_path)?;
    let passed = inventory_files == OFFICIAL_BULK_FILE_COUNT && files_over_usb_limit == 0;
    Ok(OfficialInventoryReport {
        source_url: OFFICIAL_GAIA_XP_CONTINUOUS_BASE_URL.to_string(),
        inventory_files,
        checksum_manifest_sha256,
        max_file_bytes,
        files_over_usb_limit,
        passed,
    })
}

pub fn inventory_pairs(files: &[BulkFile]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|file| (file.filename.clone(), file.md5.clone()))
        .collect()
}

pub fn build_rotating_cache_plan(
    inventory: &OfficialInventoryReport,
    max_cache_bytes: u64,
    workers: u32,
) -> RotatingCachePlan {
    let checkpoint_storage_gib = match workers {
        1 => 0.5,
        4 => 2.0,
        _ => 4.0,
    };
    let transient_work_gib = checkpoint_storage_gib * 0.1;
    let healpix_output_gib = 2.0;
    let peak_rotating_cache_bytes = max_cache_bytes;
    RotatingCachePlan {
        max_cache_bytes,
        max_usb_file_bytes: MAX_USB_FILE_BYTES,
        max_single_file_observed_bytes: inventory.max_file_bytes,
        files_over_usb_limit: inventory.files_over_usb_limit,
        concurrent_inputs_recommended: workers.min(4),
        peak_rotating_cache_bytes,
        checkpoint_storage_gib,
        transient_work_gib,
        healpix_output_gib,
    }
}

pub fn assess_feasibility(
    internal: &DiskMeasurement,
    usb: Option<&DiskMeasurement>,
    rotating: &RotatingCachePlan,
    max_cache_bytes: u64,
) -> FeasibilityReport {
    let mut rationale = Vec::new();
    let required_peak_internal_bytes = gib_to_bytes(
        rotating.checkpoint_storage_gib + rotating.transient_work_gib + rotating.healpix_output_gib,
    );
    let required_peak_usb_bytes = max_cache_bytes.max(rotating.peak_rotating_cache_bytes);

    let internal_headroom = internal.available_bytes as i64 - required_peak_internal_bytes as i64;
    let usb_headroom = usb
        .map(|disk| disk.available_bytes as i64 - required_peak_usb_bytes as i64)
        .unwrap_or(-1);

    let rotating_cache_feasible = usb
        .map(|disk| disk.available_bytes >= required_peak_usb_bytes)
        .unwrap_or(false);
    if rotating_cache_feasible {
        rationale.push(format!(
            "USB rotating cache fits within {required_peak_usb_bytes} byte ceiling"
        ));
    } else {
        rationale.push(
            "USB rotating cache cannot fit required peak footprint with available space"
                .to_string(),
        );
    }

    if internal_headroom >= 0 {
        rationale.push(format!(
            "internal work/checkpoint/output headroom: {internal_headroom} bytes"
        ));
    } else {
        rationale.push(format!(
            "internal storage shortfall: {} bytes",
            internal_headroom.abs()
        ));
    }

    let can_process_full_population =
        rotating_cache_feasible && internal_headroom >= 0 && rotating.files_over_usb_limit == 0;
    if can_process_full_population {
        rationale.push(
            "184,729,270 XP continuous-only sources are feasible with rotating USB cache"
                .to_string(),
        );
    } else {
        rationale.push("full-population processing blocked until storage gates pass".to_string());
    }

    FeasibilityReport {
        population_xp_continuous_only: XP_CONTINUOUS_ONLY_POPULATION,
        rotating_cache_feasible,
        internal_headroom_bytes: internal_headroom,
        usb_headroom_bytes: usb_headroom,
        required_peak_internal_bytes,
        required_peak_usb_bytes,
        can_process_full_population,
        rationale,
    }
}

pub fn run_storage_preflight(config: &StoragePreflightConfig) -> Result<StoragePlan> {
    fs::create_dir_all(&config.work_dir)?;
    fs::create_dir_all(&config.checkpoint_dir)?;
    fs::create_dir_all(&config.output_dir)?;
    fs::create_dir_all(&config.manifest_dir)?;
    fs::create_dir_all(&config.input_cache_dir)?;

    let mut gates = Vec::new();

    let inventory = audit_official_inventory(&config.official_checksum_manifest)?;
    gates.push(PreflightGate {
        name: "complete_official_inventory".to_string(),
        passed: inventory.passed,
        detail: format!(
            "inventory_files={} expected={OFFICIAL_BULK_FILE_COUNT} files_over_usb_limit={}",
            inventory.inventory_files, inventory.files_over_usb_limit
        ),
    });
    gates.push(PreflightGate {
        name: "checksums".to_string(),
        passed: !inventory.checksum_manifest_sha256.is_empty(),
        detail: format!(
            "official manifest sha256:{}",
            inventory.checksum_manifest_sha256
        ),
    });
    gates.push(PreflightGate {
        name: "file_size_limit".to_string(),
        passed: inventory.files_over_usb_limit == 0,
        detail: format!(
            "max_observed_bytes={} limit={MAX_USB_FILE_BYTES}",
            inventory.max_file_bytes
        ),
    });

    if let Some(policy_path) = &config.frozen_policy {
        gates.push(verify_policy_checksum(policy_path)?);
    } else {
        gates.push(PreflightGate {
            name: "policy_checksum".to_string(),
            passed: false,
            detail: "frozen policy path not provided".to_string(),
        });
    }

    let mut internal = measure_disk(&config.work_dir)?;
    internal.measured_existing_bytes = Some(
        config
            .measured_internal_existing_bytes
            .unwrap_or_else(|| directory_size_bytes(&config.work_dir).unwrap_or(0)),
    );

    let (usb_identity, usb_disk) =
        if let (Some(mount), Some(root)) = (&config.usb_mountpoint, &config.usb_cache_root) {
            let layout = UsbCacheLayout::from_env(mount, root, "xp-continuous");
            let identity = verify_usb_identity(&layout)?;
            gates.push(PreflightGate {
                name: "usb_mount_identity".to_string(),
                passed: identity.passed,
                detail: if identity.passed {
                    format!(
                        "mount={} uuid={}",
                        identity.mountpoint,
                        identity.cache_uuid.clone().unwrap_or_default()
                    )
                } else {
                    identity.failures.join("; ")
                },
            });

            let marker = read_or_create_cache_root_marker(&layout, false);
            if let Ok(marker) = marker {
                let text = fs::read_to_string(&config.official_checksum_manifest)?;
                let files = parse_md5_manifest(&text)?;
                let pairs = inventory_pairs(&files);
                let mut manifest = load_or_init_cache_state_manifest(
                    &layout,
                    &marker.cache_uuid,
                    config.max_cache_bytes,
                    &pairs,
                )?;
                let cleanup = simulate_or_run_cleanup(&layout, &mut manifest, true, None)?;
                gates.push(PreflightGate {
                    name: "cleanup_simulation".to_string(),
                    passed: cleanup.passed,
                    detail: format!(
                        "dry_run candidates={} bytes_reclaimed={}",
                        cleanup.candidates.len(),
                        cleanup.bytes_reclaimed
                    ),
                });
                let footprint = current_cache_footprint_bytes(&manifest);
                gates.push(PreflightGate {
                    name: "usb_cache_state".to_string(),
                    passed: footprint <= config.max_cache_bytes,
                    detail: format!(
                        "footprint_bytes={footprint} max_cache_bytes={}",
                        config.max_cache_bytes
                    ),
                });
            } else {
                gates.push(PreflightGate {
                    name: "cleanup_simulation".to_string(),
                    passed: false,
                    detail: "skipped: USB cache root marker missing".to_string(),
                });
            }

            let disk = measure_disk(root).ok();
            (Some(identity), disk)
        } else {
            gates.push(PreflightGate {
                name: "usb_mount_identity".to_string(),
                passed: false,
                detail: "USB mount/cache root not configured".to_string(),
            });
            (None, None)
        };

    let rotating = build_rotating_cache_plan(&inventory, config.max_cache_bytes, 4);
    let feasibility = assess_feasibility(
        &internal,
        usb_disk.as_ref(),
        &rotating,
        config.max_cache_bytes,
    );

    let storage_gate = feasibility.can_process_full_population;
    gates.push(PreflightGate {
        name: "storage_plan".to_string(),
        passed: storage_gate,
        detail: if storage_gate {
            "rotating USB cache and internal work volumes fit required peak footprint".to_string()
        } else {
            feasibility.rationale.join("; ")
        },
    });

    let passed = gates.iter().all(|gate| gate.passed);
    let conclusion = if passed { "PASS" } else { "FAIL" }.to_string();

    Ok(StoragePlan {
        schema_version: STORAGE_PLAN_SCHEMA_VERSION,
        timestamp_utc: utc_now(),
        population_xp_continuous_only: XP_CONTINUOUS_ONLY_POPULATION,
        bulk_inventory_files: inventory.inventory_files,
        bulk_compressed_tib: OFFICIAL_BULK_COMPRESSED_TIB,
        max_usb_file_bytes: MAX_USB_FILE_BYTES,
        internal,
        usb: usb_disk,
        usb_identity,
        rotating_cache: rotating,
        preflight_gates: gates,
        feasibility,
        conclusion,
        passed,
    })
}

pub fn write_storage_plan_json(path: &Path, plan: &StoragePlan) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(plan)? + "\n")?;
    Ok(())
}

pub fn render_storage_plan_markdown(plan: &StoragePlan) -> String {
    let usb_avail = plan
        .usb
        .as_ref()
        .map(|disk| format_bytes(disk.available_bytes))
        .unwrap_or_else(|| "n/a".to_string());
    let internal_avail = format_bytes(plan.internal.available_bytes);
    let gate_rows = plan
        .preflight_gates
        .iter()
        .map(|gate| {
            format!(
                "| {} | {} | {} |",
                gate.name,
                if gate.passed { "PASS" } else { "FAIL" },
                gate.detail.replace('|', "/")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "# Gaia DR3 XP continuous bulk storage plan\n\n\
         **Conclusion:** {conclusion}\n\n\
         ## Population\n\n\
         - XP continuous-only sources: {population}\n\
         - Official bulk files: {files}\n\
         - Official compressed volume: {tib:.1} TiB\n\n\
         ## Disk measurements\n\n\
         | Volume | Available | Existing measured |\n\
         | --- | --- | --- |\n\
         | internal (`{internal_path}`) | {internal_avail} | {internal_existing} |\n\
         | USB cache | {usb_avail} | n/a |\n\n\
         ## Rotating cache\n\n\
         - Max cache bytes: {max_cache}\n\
         - Max USB file bytes: {max_usb_file}\n\
         - Max observed file bytes: {max_observed}\n\
         - Files over USB limit: {over_limit}\n\
         - Peak rotating cache bytes: {peak_cache}\n\n\
         ## Feasibility\n\n\
         - Can process full population: {can_process}\n\
         - Internal headroom: {internal_headroom}\n\
         - USB headroom: {usb_headroom}\n\
         - Rationale:\n{rationale}\n\n\
         ## Preflight gates\n\n\
         | Gate | Status | Detail |\n\
         | --- | --- | --- |\n\
         {gate_rows}\n",
        conclusion = plan.conclusion,
        population = plan.population_xp_continuous_only,
        files = plan.bulk_inventory_files,
        tib = plan.bulk_compressed_tib,
        internal_path = plan.internal.path,
        internal_avail = internal_avail,
        internal_existing = plan
            .internal
            .measured_existing_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "n/a".to_string()),
        usb_avail = usb_avail,
        max_cache = format_bytes(plan.rotating_cache.max_cache_bytes),
        max_usb_file = format_bytes(plan.rotating_cache.max_usb_file_bytes),
        max_observed = format_bytes(plan.rotating_cache.max_single_file_observed_bytes),
        over_limit = plan.rotating_cache.files_over_usb_limit,
        peak_cache = format_bytes(plan.rotating_cache.peak_rotating_cache_bytes),
        can_process = plan.feasibility.can_process_full_population,
        internal_headroom = format_signed_bytes(plan.feasibility.internal_headroom_bytes),
        usb_headroom = format_signed_bytes(plan.feasibility.usb_headroom_bytes),
        rationale = plan
            .feasibility
            .rationale
            .iter()
            .map(|line| format!("  - {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        gate_rows = gate_rows,
    )
}

pub fn write_storage_plan_markdown(path: &Path, plan: &StoragePlan) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, render_storage_plan_markdown(plan))?;
    Ok(())
}

fn gib_to_bytes(gib: f64) -> u64 {
    (gib * 1024.0 * 1024.0 * 1024.0).round() as u64
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024u64.pow(4) {
        format!("{:.2} TiB", bytes as f64 / 1024f64.powi(4))
    } else if bytes >= 1024u64.pow(3) {
        format!("{:.2} GiB", bytes as f64 / 1024f64.powi(3))
    } else if bytes >= 1024u64.pow(2) {
        format!("{:.2} MiB", bytes as f64 / 1024f64.powi(2))
    } else {
        format!("{bytes} B")
    }
}

fn format_signed_bytes(bytes: i64) -> String {
    if bytes < 0 {
        format!("-{}", format_bytes(bytes.unsigned_abs()))
    } else {
        format_bytes(bytes as u64)
    }
}

fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(unix)]
fn statvfs_bytes(path: &Path) -> Result<(u64, u64, Option<String>)> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let c_path = CString::new(path.to_string_lossy().as_bytes())
        .context("path contains interior NUL byte")?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        bail!("statvfs failed for {}", path.display());
    }
    let stat = unsafe { stat.assume_init() };
    let fragment = stat.f_frsize;
    let total = stat.f_blocks * fragment;
    let available = stat.f_bavail * fragment;
    let filesystem = crate::gaia::acquisition::usb_cache::read_mount_info(path)
        .ok()
        .and_then(|(_, fstype)| fstype);
    Ok((total, available, filesystem))
}

#[cfg(not(unix))]
fn statvfs_bytes(path: &Path) -> Result<(u64, u64, Option<String>)> {
    let _ = path;
    bail!("disk measurement is only supported on unix targets")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feasibility_passes_with_generous_space() {
        let internal = DiskMeasurement {
            path: "/work".to_string(),
            filesystem: Some("ext4".to_string()),
            total_bytes: 500 * 1024u64.pow(3),
            available_bytes: 80 * 1024u64.pow(3),
            used_bytes: 420 * 1024u64.pow(3),
            measured_existing_bytes: Some(121 * 1024u64.pow(3)),
        };
        let usb = DiskMeasurement {
            path: "/usb".to_string(),
            filesystem: Some("vfat".to_string()),
            total_bytes: 477 * 1024u64.pow(3),
            available_bytes: 230 * 1024u64.pow(3),
            used_bytes: 247 * 1024u64.pow(3),
            measured_existing_bytes: None,
        };
        let inventory = OfficialInventoryReport {
            source_url: OFFICIAL_GAIA_XP_CONTINUOUS_BASE_URL.to_string(),
            inventory_files: OFFICIAL_BULK_FILE_COUNT,
            checksum_manifest_sha256: "abc".to_string(),
            max_file_bytes: 1_500_000_000,
            files_over_usb_limit: 0,
            passed: true,
        };
        let rotating = build_rotating_cache_plan(&inventory, 20 * 1024u64.pow(3), 4);
        let feasibility = assess_feasibility(&internal, Some(&usb), &rotating, 20 * 1024u64.pow(3));
        assert!(feasibility.can_process_full_population);
        assert!(feasibility.rotating_cache_feasible);
    }
}
