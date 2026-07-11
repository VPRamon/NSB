//! Gaia DR3 XP continuous bulk file index and source_id routing.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const XP_CONTINUOUS_BULK_PREFIX: &str = "XpContinuousMeanSpectrum_";
pub const GAIA_SOURCE_HEALPIX_SHIFT: u32 = 43;
pub const OFFICIAL_GAIA_XP_CONTINUOUS_BASE_URL: &str =
    "https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_continuous_mean_spectrum/";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BulkFileValidationStatus {
    NotDownloaded,
    Downloaded,
    ChecksumVerified,
    ChecksumMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkFileIndexEntry {
    pub file_name: String,
    pub source_id_min: u64,
    pub source_id_max: u64,
    pub healpix_index_min: u64,
    pub healpix_index_max: u64,
    pub size_bytes: u64,
    pub checksum: String,
    pub download_url: String,
    pub downloaded: bool,
    pub local_path: Option<String>,
    pub validation_status: BulkFileValidationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkFileIndex {
    pub schema_version: u32,
    pub inventory_total_files: usize,
    pub entries: Vec<BulkFileIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceLocateResult {
    pub source_id: String,
    pub healpix_index: u64,
    pub file_name: String,
    pub source_id_min: u64,
    pub source_id_max: u64,
    pub healpix_index_min: u64,
    pub healpix_index_max: u64,
    pub downloaded: bool,
    pub local_path: Option<String>,
    pub validation_status: BulkFileValidationStatus,
    pub expected_checksum: String,
    pub observed_checksum: Option<String>,
    pub row_found: Option<bool>,
}

pub fn gaia_source_healpix_index(source_id: u64) -> u64 {
    source_id >> GAIA_SOURCE_HEALPIX_SHIFT
}

pub fn continuous_bulk_range(path: &Path) -> Option<(u64, u64)> {
    let name = path.file_name()?.to_str()?;
    let range = name
        .strip_prefix(XP_CONTINUOUS_BULK_PREFIX)?
        .strip_suffix(".csv.gz")?;
    let (lower, upper) = range.split_once('-')?;
    Some((lower.parse().ok()?, upper.parse().ok()?))
}

pub fn bulk_file_for_healpix_index(
    index: &BulkFileIndex,
    healpix_index: u64,
) -> Result<&BulkFileIndexEntry> {
    let matches: Vec<_> = index
        .entries
        .iter()
        .filter(|entry| {
            entry.healpix_index_min <= healpix_index && healpix_index <= entry.healpix_index_max
        })
        .collect();
    match matches.len() {
        0 => bail!("no bulk file covers Gaia HEALPix index {healpix_index}"),
        1 => Ok(matches[0]),
        n => bail!("expected one bulk file for Gaia HEALPix index {healpix_index}, found {n}"),
    }
}

pub fn locate_source_id(index: &BulkFileIndex, source_id: u64) -> Result<SourceLocateResult> {
    let healpix_index = gaia_source_healpix_index(source_id);
    let entry = bulk_file_for_healpix_index(index, healpix_index)?;
    Ok(SourceLocateResult {
        source_id: source_id.to_string(),
        healpix_index,
        file_name: entry.file_name.clone(),
        source_id_min: entry.source_id_min,
        source_id_max: entry.source_id_max,
        healpix_index_min: entry.healpix_index_min,
        healpix_index_max: entry.healpix_index_max,
        downloaded: entry.downloaded,
        local_path: entry.local_path.clone(),
        validation_status: entry.validation_status.clone(),
        expected_checksum: entry.checksum.clone(),
        observed_checksum: None,
        row_found: None,
    })
}

pub fn locate_and_verify_row(index: &BulkFileIndex, source_id: u64) -> Result<SourceLocateResult> {
    let mut result = locate_source_id(index, source_id)?;
    if let Some(local_path) = result.local_path.clone() {
        let path = Path::new(&local_path);
        let observed_matches = verify_md5(path, &result.expected_checksum)?;
        result.observed_checksum = if path.is_file() {
            Some(if observed_matches {
                result.expected_checksum.clone()
            } else {
                compute_file_md5(path)?
            })
        } else {
            None
        };
        if result.downloaded {
            result.row_found = Some(
                crate::gaia_xp_continuous_canonical::find_bulk_source(
                    path,
                    &source_id.to_string(),
                )?
                .is_some(),
            );
        }
        if !observed_matches && path.is_file() {
            result.validation_status = BulkFileValidationStatus::ChecksumMismatch;
        }
    }
    Ok(result)
}

fn compute_file_md5(path: &Path) -> Result<String> {
    use md5::{Digest, Md5};
    let mut file = fs::File::open(path)?;
    let mut hasher = Md5::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn parse_official_md5_manifest(text: &str) -> Result<BTreeMap<String, (String, u64)>> {
    let mut entries = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((md5, filename)) = line.split_once("  ") else {
            continue;
        };
        entries.insert(filename.trim().to_string(), (md5.trim().to_lowercase(), 0));
    }
    Ok(entries)
}

pub fn build_index(
    md5_manifest_path: &Path,
    download_dir: &Path,
    _nsb_manifest_path: Option<&Path>,
) -> Result<BulkFileIndex> {
    let md5_text = fs::read_to_string(md5_manifest_path)
        .with_context(|| format!("read MD5 manifest {}", md5_manifest_path.display()))?;
    let md5_entries = parse_official_md5_manifest(&md5_text)?;

    let mut entries = Vec::new();
    for (filename, (md5, _)) in md5_entries {
        let (healpix_index_min, healpix_index_max) = continuous_bulk_range(Path::new(&filename))
            .with_context(|| format!("parse bulk filename range for {filename}"))?;
        let path = download_dir.join(&filename);
        let size_bytes = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        let downloaded_flag = path.is_file();
        let validation_status = if !downloaded_flag {
            BulkFileValidationStatus::NotDownloaded
        } else if verify_md5(&path, &md5)? {
            BulkFileValidationStatus::ChecksumVerified
        } else {
            BulkFileValidationStatus::ChecksumMismatch
        };
        entries.push(BulkFileIndexEntry {
            file_name: filename.clone(),
            source_id_min: healpix_index_min << GAIA_SOURCE_HEALPIX_SHIFT,
            source_id_max: ((healpix_index_max + 1) << GAIA_SOURCE_HEALPIX_SHIFT).saturating_sub(1),
            healpix_index_min,
            healpix_index_max,
            size_bytes,
            checksum: md5.clone(),
            download_url: format!("{OFFICIAL_GAIA_XP_CONTINUOUS_BASE_URL}{filename}"),
            downloaded: downloaded_flag,
            local_path: downloaded_flag.then(|| path.display().to_string()),
            validation_status,
        });
    }
    entries.sort_by(|a, b| a.healpix_index_min.cmp(&b.healpix_index_min));
    Ok(BulkFileIndex {
        schema_version: 1,
        inventory_total_files: entries.len(),
        entries,
    })
}

fn verify_md5(path: &Path, expected: &str) -> Result<bool> {
    if !path.is_file() {
        return Ok(false);
    }
    use md5::{Digest, Md5};
    let mut file = fs::File::open(path)?;
    let mut hasher = Md5::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()) == expected.to_lowercase())
}

pub fn write_index_csv(path: &Path, index: &BulkFileIndex) -> Result<()> {
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    for entry in &index.entries {
        writer.serialize(entry)?;
    }
    writer.flush()?;
    Ok(())
}

pub fn select_bulk_files_for_source_ids(
    index: &BulkFileIndex,
    source_ids: impl Iterator<Item = u64>,
) -> Result<Vec<&BulkFileIndexEntry>> {
    let mut selected = BTreeMap::new();
    for source_id in source_ids {
        let entry = bulk_file_for_healpix_index(index, gaia_source_healpix_index(source_id))?;
        selected.insert(entry.file_name.clone(), entry);
    }
    Ok(selected.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_bulk_range_parses_pilot_filename() {
        let path = Path::new("XpContinuousMeanSpectrum_000000-003111.csv.gz");
        assert_eq!(continuous_bulk_range(path), Some((0, 3111)));
    }

    #[test]
    fn gaia_healpix_index_matches_first_pilot_source() {
        assert_eq!(gaia_source_healpix_index(4_295_806_720), 0);
    }

    #[test]
    fn index_builds_from_official_md5_manifest_head() {
        let md5_path = Path::new(
            "/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk/_MD5SUM.txt",
        );
        if !md5_path.is_file() {
            return;
        }
        let text = std::fs::read_to_string(md5_path).unwrap();
        let lines: String = text.lines().take(3).collect::<Vec<_>>().join("\n") + "\n";
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("_MD5SUM.txt");
        std::fs::write(&partial, lines).unwrap();
        let index = build_index(
            &partial,
            Path::new("/path/to/nsb-data/starlight-gaia-release/pilot-xp-continuous-bulk/bulk"),
            None,
        )
        .unwrap();
        assert_eq!(index.entries.len(), 3);
    }

    #[test]
    fn missing_healpix_index_errors() {
        let index = BulkFileIndex {
            schema_version: 1,
            inventory_total_files: 0,
            entries: vec![],
        };
        assert!(bulk_file_for_healpix_index(&index, 999).is_err());
    }

    #[test]
    fn locate_second_prefix_healpix_range() {
        let index = BulkFileIndex {
            schema_version: 1,
            inventory_total_files: 2,
            entries: vec![
                BulkFileIndexEntry {
                    file_name: "XpContinuousMeanSpectrum_000000-003111.csv.gz".to_string(),
                    source_id_min: 0,
                    source_id_max: (3112 << GAIA_SOURCE_HEALPIX_SHIFT) - 1,
                    healpix_index_min: 0,
                    healpix_index_max: 3111,
                    size_bytes: 1,
                    checksum: "abc".to_string(),
                    download_url: String::new(),
                    downloaded: true,
                    local_path: Some("/tmp/x1.csv.gz".to_string()),
                    validation_status: BulkFileValidationStatus::ChecksumVerified,
                },
                BulkFileIndexEntry {
                    file_name: "XpContinuousMeanSpectrum_003112-005263.csv.gz".to_string(),
                    source_id_min: 3112 << GAIA_SOURCE_HEALPIX_SHIFT,
                    source_id_max: ((5264) << GAIA_SOURCE_HEALPIX_SHIFT) - 1,
                    healpix_index_min: 3112,
                    healpix_index_max: 5263,
                    size_bytes: 1,
                    checksum: "def".to_string(),
                    download_url: String::new(),
                    downloaded: true,
                    local_path: Some("/tmp/x2.csv.gz".to_string()),
                    validation_status: BulkFileValidationStatus::ChecksumVerified,
                },
            ],
        };
        let second_prefix_source = 3112_u64 << GAIA_SOURCE_HEALPIX_SHIFT;
        let located = locate_source_id(&index, second_prefix_source).unwrap();
        assert_eq!(
            located.file_name,
            "XpContinuousMeanSpectrum_003112-005263.csv.gz"
        );
    }

    #[test]
    fn locate_source_id_finds_first_file() {
        let index = BulkFileIndex {
            schema_version: 1,
            inventory_total_files: 1,
            entries: vec![BulkFileIndexEntry {
                file_name: "XpContinuousMeanSpectrum_000000-003111.csv.gz".to_string(),
                source_id_min: 0,
                source_id_max: (3112 << GAIA_SOURCE_HEALPIX_SHIFT) - 1,
                healpix_index_min: 0,
                healpix_index_max: 3111,
                size_bytes: 1,
                checksum: "abc".to_string(),
                download_url: String::new(),
                downloaded: true,
                local_path: Some("/tmp/x.csv.gz".to_string()),
                validation_status: BulkFileValidationStatus::ChecksumVerified,
            }],
        };
        let located = locate_source_id(&index, 4_295_806_720).unwrap();
        assert_eq!(
            located.file_name,
            "XpContinuousMeanSpectrum_000000-003111.csv.gz"
        );
        assert_eq!(located.expected_checksum, "abc");
        assert_eq!(located.healpix_index_min, 0);
    }
}
