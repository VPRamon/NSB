//! Inventory, validation, deduplication, and spatial splitting of Gaia stratified
//! starlight sampling TAP results.

use crate::platform::checksum_io::sha256_file;
use crate::starlight::science::{DataPartition, SpatialSplitSpec};
use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, StringRecord};
use serde::{Deserialize, Serialize};
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub const SAMPLE_CSV_COLUMNS: [&str; 31] = [
    "source_id",
    "random_index",
    "has_xp_continuous",
    "has_xp_sampled",
    "ra",
    "dec",
    "l",
    "b",
    "phot_g_mean_mag",
    "phot_bp_mean_mag",
    "phot_rp_mean_mag",
    "bp_rp",
    "phot_g_mean_flux",
    "phot_bp_mean_flux",
    "phot_rp_mean_flux",
    "phot_g_mean_flux_error",
    "phot_bp_mean_flux_error",
    "phot_rp_mean_flux_error",
    "phot_g_mean_flux_over_error",
    "phot_bp_mean_flux_over_error",
    "phot_rp_mean_flux_over_error",
    "phot_bp_rp_excess_factor",
    "phot_bp_n_blended_transits",
    "phot_rp_n_blended_transits",
    "ipd_frac_multi_peak",
    "ruwe",
    "duplicated_source",
    "phot_variable_flag",
    "non_single_star",
    "in_qso_candidates",
    "in_galaxy_candidates",
];

pub const POPULATION_TOTALS: [(&str, u64); 4] = [
    ("xp_sampled", 34_468_373),
    ("xp_continuous_only", 184_729_270),
    ("no_xp", 1_592_512_128),
    ("total", 1_811_709_771),
];

pub fn default_spatial_split() -> SpatialSplitSpec {
    SpatialSplitSpec {
        algorithm: "splitmix64_spatial_cell_v1".to_string(),
        seed: 0x4751_4141_5354_5200,
        spatial_nside: 64,
        train_buckets: vec![0, 1, 2, 3, 4, 5],
        validation_buckets: vec![6, 7],
        test_buckets: vec![8, 9],
        bucket_modulus: 10,
    }
}

/// Frozen Gaia population inventory used for reconciliation (not sample weights).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulationInventory {
    pub xp_sampled: u64,
    pub xp_continuous_only: u64,
    pub no_xp: u64,
    pub total: u64,
}

impl Default for PopulationInventory {
    fn default() -> Self {
        Self {
            xp_sampled: POPULATION_TOTALS[0].1,
            xp_continuous_only: POPULATION_TOTALS[1].1,
            no_xp: POPULATION_TOTALS[2].1,
            total: POPULATION_TOTALS[3].1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobClassification {
    CompletedValid,
    CompletedInvalid,
    Pending,
    Running,
    ErrorRetryable,
    ErrorNonretryable,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInventoryEntry {
    pub job_id: String,
    pub population: Option<String>,
    pub stratum: Option<String>,
    pub query_path: Option<String>,
    pub query_sha256: Option<String>,
    pub remote_job_url: Option<String>,
    pub phase: Option<String>,
    pub http_status: Option<u16>,
    pub result_path: Option<String>,
    pub result_sha256: Option<String>,
    pub row_count: Option<u64>,
    pub expected_max_rows: Option<u64>,
    pub truncated: bool,
    pub valid: bool,
    pub last_update: Option<String>,
    pub error_class: Option<String>,
    pub action_required: Option<String>,
    pub classification: JobClassification,
}

#[derive(Debug, Clone, Deserialize)]
struct TapJobManifest {
    status: String,
    #[serde(default, rename = "query_path")]
    _query_path: Option<String>,
    output_path: Option<String>,
    job_url: Option<String>,
    http_status: Option<u16>,
    sha256: Option<String>,
    row_count: Option<u64>,
    maxrec: Option<u64>,
    truncated: bool,
    error: Option<String>,
    updated_unix_millis: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ValidatedSampleRow {
    pub source_id: u64,
    pub population: String,
    pub spatial_cell: u64,
    pub record: StringRecord,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsolidatedSample {
    pub inventory: Vec<JobInventoryEntry>,
    pub unique_sources: usize,
    pub membership_rows: usize,
    pub split_counts: BTreeMap<String, u64>,
}

/// Preregistered stratum names that must have a valid result or documented absence.
pub fn required_strata() -> Vec<(&'static str, Vec<&'static str>)> {
    let common = [
        "g_bright",
        "g_intermediate",
        "g_faint",
        "g_very_faint",
        "colour_blue",
        "colour_solar",
        "colour_red",
        "colour_very_red",
        "galactic_plane",
        "galactic_centre",
        "north_pole",
        "south_pole",
        "longitude_seam",
        "crowded_blended",
        "high_bp_rp_excess",
        "low_g_snr",
        "high_g_snr",
        "red_extinguished_plane",
        "duplicated",
        "variable",
        "extragalactic_candidates",
    ];
    vec![
        ("xp_sampled_overlap", common.to_vec()),
        ("xp_continuous_only", common.to_vec()),
        (
            "no_xp",
            common
                .iter()
                .chain([
                    &"branch_g_bp_rp_colour",
                    &"branch_partial_colour",
                    &"branch_g_only",
                    &"branch_no_photometry",
                ])
                .copied()
                .collect(),
        ),
    ]
}

pub fn inventory_jobs(jobs_root: &Path, results_root: &Path) -> Result<Vec<JobInventoryEntry>> {
    let mut entries = Vec::new();
    if jobs_root.join("stratified").is_dir() {
        for job_dir in fs::read_dir(jobs_root.join("stratified"))? {
            let job_dir = job_dir?;
            if job_dir.file_type()?.is_dir() {
                entries.push(inventory_one_job(&job_dir.path(), results_root, true)?);
            }
        }
    }
    for name in ["02_invalid_original", "03_excluded_sources_audit"] {
        let path = jobs_root.join(name);
        if path.is_dir() {
            entries.push(inventory_one_job(&path, results_root, false)?);
        }
    }
    entries.sort_by(|left, right| left.job_id.cmp(&right.job_id));
    Ok(entries)
}

fn inventory_one_job(
    job_dir: &Path,
    results_root: &Path,
    stratified: bool,
) -> Result<JobInventoryEntry> {
    let job_id = job_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string();
    let (population, stratum) = if stratified {
        parse_stratified_job_id(&job_id)
    } else {
        (None, None)
    };
    let query_path = job_dir.join("query.adql");
    let query_sha256 = if query_path.is_file() {
        Some(
            sha256_file(&query_path)?
                .trim_start_matches("sha256:")
                .to_string(),
        )
    } else {
        None
    };
    let manifest_path = job_dir.join("manifest.json");
    let status_path = job_dir.join("status.txt");
    let phase = read_last_phase(&status_path);
    let manifest = if manifest_path.is_file() {
        Some(
            serde_json::from_str::<TapJobManifest>(&fs::read_to_string(&manifest_path)?)
                .with_context(|| format!("parse {}", manifest_path.display()))?,
        )
    } else {
        None
    };
    let result_path = manifest
        .as_ref()
        .and_then(|entry| entry.output_path.clone())
        .or_else(|| {
            if stratified {
                Some(
                    results_root
                        .join("stratified")
                        .join(format!("{job_id}.csv"))
                        .display()
                        .to_string(),
                )
            } else {
                Some(
                    results_root
                        .join(format!("{job_id}.csv"))
                        .display()
                        .to_string(),
                )
            }
        });
    let result_file = result_path.as_ref().map(PathBuf::from);
    let validation = result_file
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| {
            if stratified {
                validate_sample_csv(path, manifest.as_ref().and_then(|m| m.maxrec))
            } else {
                validate_audit_csv(path)
            }
        })
        .transpose()?;
    let valid = validation.as_ref().is_some_and(|entry| entry.valid);
    let row_count = validation
        .as_ref()
        .map(|entry| entry.row_count)
        .or_else(|| manifest.as_ref().and_then(|entry| entry.row_count));
    let result_sha256 = result_file
        .as_ref()
        .filter(|path| path.is_file())
        .map(|path| sha256_file(path))
        .transpose()?;
    let truncated = manifest.as_ref().is_some_and(|entry| entry.truncated)
        || validation.as_ref().is_some_and(|entry| entry.truncated);
    let http_status = manifest.as_ref().and_then(|entry| entry.http_status);
    let classification = classify_job(
        manifest.as_ref(),
        result_file.as_deref(),
        valid,
        &phase,
        result_sha256.as_deref(),
    );
    let (error_class, action_required) = job_actions(&classification, manifest.as_ref(), valid);
    Ok(JobInventoryEntry {
        job_id,
        population,
        stratum,
        query_path: query_path
            .is_file()
            .then(|| query_path.display().to_string()),
        query_sha256,
        remote_job_url: manifest.as_ref().and_then(|entry| entry.job_url.clone()),
        phase,
        http_status,
        result_path: result_file
            .filter(|path| path.is_file())
            .map(|path| path.display().to_string()),
        result_sha256,
        row_count,
        expected_max_rows: manifest.as_ref().and_then(|entry| entry.maxrec),
        truncated,
        valid,
        last_update: manifest
            .as_ref()
            .and_then(|entry| entry.updated_unix_millis)
            .map(|value| value.to_string()),
        error_class,
        action_required,
        classification,
    })
}

fn parse_stratified_job_id(job_id: &str) -> (Option<String>, Option<String>) {
    if let Some(rest) = job_id.strip_prefix("xp_sampled_overlap_") {
        return (
            Some("xp_sampled_overlap".to_string()),
            Some(rest.to_string()),
        );
    }
    if let Some(rest) = job_id.strip_prefix("xp_continuous_only_") {
        return (
            Some("xp_continuous_only".to_string()),
            Some(rest.to_string()),
        );
    }
    if let Some(rest) = job_id.strip_prefix("no_xp_") {
        return (Some("no_xp".to_string()), Some(rest.to_string()));
    }
    (None, None)
}

fn read_last_phase(status_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(status_path).ok()?;
    raw.lines()
        .rev()
        .find_map(|line| {
            line.split("UWS PHASE ")
                .nth(1)
                .map(str::trim)
                .map(String::from)
        })
        .or_else(|| {
            raw.lines().last().map(|line| {
                line.split_whitespace()
                    .skip(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
}

fn classify_job(
    manifest: Option<&TapJobManifest>,
    result_path: Option<&Path>,
    valid: bool,
    phase: &Option<String>,
    result_sha256: Option<&str>,
) -> JobClassification {
    let Some(manifest) = manifest else {
        return JobClassification::Missing;
    };
    if manifest.status == "failed" {
        if manifest.http_status == Some(400)
            || manifest
                .error
                .as_deref()
                .is_some_and(|error| error.contains("400") || error.contains("ADQL"))
        {
            return JobClassification::ErrorNonretryable;
        }
        return JobClassification::ErrorRetryable;
    }
    if phase.as_deref().is_some_and(|value| {
        matches!(
            value.to_ascii_uppercase().as_str(),
            "EXECUTING" | "QUEUED" | "PENDING"
        )
    }) {
        return JobClassification::Running;
    }
    if manifest.status == "completed" {
        let result_ok = result_path.is_some_and(|path| path.is_file());
        let checksum_ok =
            manifest
                .sha256
                .as_deref()
                .zip(result_sha256)
                .is_none_or(|(expected, actual)| {
                    normalize_sha256(expected) == normalize_sha256(actual)
                });
        if result_ok && valid && checksum_ok {
            return JobClassification::CompletedValid;
        }
        return JobClassification::CompletedInvalid;
    }
    JobClassification::Pending
}

fn normalize_sha256(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("sha256:")
        .to_ascii_lowercase()
}

fn job_actions(
    classification: &JobClassification,
    manifest: Option<&TapJobManifest>,
    valid: bool,
) -> (Option<String>, Option<String>) {
    match classification {
        JobClassification::CompletedValid => (None, None),
        JobClassification::CompletedInvalid => (
            Some("validation_failed".to_string()),
            Some(if valid {
                "verify checksum and CSV schema".to_string()
            } else {
                "revalidate result columns and row bounds".to_string()
            }),
        ),
        JobClassification::ErrorNonretryable => (
            manifest
                .and_then(|entry| entry.error.clone())
                .or_else(|| Some("http_400_or_adql".to_string())),
            Some("fix query generator and regenerate ADQL".to_string()),
        ),
        JobClassification::ErrorRetryable => (
            manifest.and_then(|entry| entry.error.clone()),
            Some("resume or retry with backoff".to_string()),
        ),
        JobClassification::Running | JobClassification::Pending => (
            Some("in_progress".to_string()),
            Some("poll UWS job until terminal phase".to_string()),
        ),
        JobClassification::Missing => (
            Some("missing_manifest".to_string()),
            Some("submit query and create manifest".to_string()),
        ),
    }
}

#[derive(Debug)]
pub struct CsvValidation {
    valid: bool,
    row_count: u64,
    truncated: bool,
}

pub fn validate_sample_csv(path: &Path, maxrec: Option<u64>) -> Result<CsvValidation> {
    validate_csv_with_columns(path, maxrec, &SAMPLE_CSV_COLUMNS)
}

pub fn validate_audit_csv(path: &Path) -> Result<CsvValidation> {
    validate_csv_with_columns(path, None, &["source_id"])
}

fn validate_csv_with_columns(
    path: &Path,
    maxrec: Option<u64>,
    required: &[&str],
) -> Result<CsvValidation> {
    let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if raw.starts_with(b"<") || raw.starts_with(b"<?xml") {
        bail!("result {} looks like HTML/XML, not CSV", path.display());
    }
    let text = std::str::from_utf8(&raw).context("sample CSV must be UTF-8")?;
    let mut reader = ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader.headers()?.clone();
    for column in required {
        if !headers.iter().any(|field| field == *column) {
            bail!(
                "sample CSV {} missing required column {column}",
                path.display()
            );
        }
    }
    let row_count = reader.records().count() as u64;
    let truncated = maxrec.is_some_and(|limit| row_count >= limit);
    Ok(CsvValidation {
        valid: true,
        row_count,
        truncated,
    })
}

pub type ConsolidatedStratifiedSamples = (
    Vec<StringRecord>,
    Vec<(u64, String, String)>,
    BTreeMap<String, u64>,
);

pub fn consolidate_stratified_samples(
    inventory: &[JobInventoryEntry],
    _results_root: &Path,
    split: &SpatialSplitSpec,
) -> Result<ConsolidatedStratifiedSamples> {
    split.validate()?;
    let grid = HealpixGrid::new(
        Nside::new(split.spatial_nside).context("invalid spatial nside")?,
        HealpixOrdering::Ring,
    )
    .context("HEALPix grid")?;
    let mut sources: HashMap<u64, (StringRecord, u64)> = HashMap::new();
    let mut memberships = Vec::new();
    let mut split_counts: BTreeMap<String, u64> = BTreeMap::new();
    for job in inventory.iter().filter(|entry| {
        entry.classification == JobClassification::CompletedValid && entry.population.is_some()
    }) {
        let Some(result_path) = job.result_path.as_deref() else {
            continue;
        };
        let population = job
            .population
            .clone()
            .context("stratified job missing population")?;
        let stratum = job
            .stratum
            .clone()
            .context("stratified job missing stratum")?;
        let mut reader = ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_path(result_path)?;
        let headers = reader.headers()?.clone();
        let index = |name: &str| -> Result<usize> {
            headers
                .iter()
                .position(|field| field == name)
                .with_context(|| format!("missing column {name} in {result_path}"))
        };
        for record in reader.records() {
            let record = record?;
            let source_id = record
                .get(index("source_id")?)
                .context("source_id")?
                .parse::<u64>()?;
            let lon = record.get(index("l")?).context("l")?.parse::<f64>()?;
            let lat = record.get(index("b")?).context("b")?.parse::<f64>()?;
            let spatial_cell = galactic_spatial_cell(&grid, lon, lat)?;
            memberships.push((source_id, population.clone(), stratum.clone()));
            sources.entry(source_id).or_insert((record, spatial_cell));
        }
    }
    for (_, cell) in sources.values() {
        let partition = split.partition(*cell)?;
        *split_counts.entry(partition_label(partition)).or_default() += 1;
    }
    let mut master: Vec<_> = sources.into_iter().collect();
    master.sort_by_key(|(source_id, _)| *source_id);
    let records = master.into_iter().map(|(_, (record, _))| record).collect();
    Ok((records, memberships, split_counts))
}

fn partition_label(partition: DataPartition) -> String {
    match partition {
        DataPartition::Train => "train".to_string(),
        DataPartition::Validation => "validation".to_string(),
        DataPartition::Test => "test".to_string(),
    }
}

fn galactic_spatial_cell(grid: &HealpixGrid, lon_deg: f64, lat_deg: f64) -> Result<u64> {
    let lon = lon_deg.to_radians();
    let lat = lat_deg.to_radians();
    let cos_lat = lat.cos();
    let direction = CartesianDirection::<Galactic>::from_array([
        cos_lat * lon.cos(),
        cos_lat * lon.sin(),
        lat.sin(),
    ]);
    Ok(grid.direction_to_pixel(direction)?.get())
}

pub fn xp_population_label(has_xp_continuous: bool, has_xp_sampled: bool) -> &'static str {
    match (has_xp_continuous, has_xp_sampled) {
        (true, true) => "xp_sampled_overlap",
        (true, false) => "xp_continuous_only",
        (false, false) => "no_xp",
        (false, true) => "invalid_xp_sampled_only",
    }
}

pub fn parse_gaia_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

pub fn photometry_branch(record: &StringRecord, headers: &StringRecord) -> Result<String> {
    let field = |name: &str| -> Result<Option<f64>> {
        let idx = headers
            .iter()
            .position(|entry| entry == name)
            .with_context(|| format!("missing {name}"))?;
        Ok(record
            .get(idx)
            .filter(|value| !value.trim().is_empty())
            .and_then(|value| value.parse::<f64>().ok().filter(|entry| entry.is_finite())))
    };
    let g = field("phot_g_mean_flux")?;
    let bp = field("phot_bp_mean_flux")?;
    let rp = field("phot_rp_mean_flux")?;
    let bp_rp = field("bp_rp")?;
    if g.is_some() && bp.is_some() && rp.is_some() && bp_rp.is_some() {
        Ok("branch_g_bp_rp_colour".to_string())
    } else if g.is_some() && bp.is_none() && rp.is_none() {
        Ok("branch_g_only".to_string())
    } else if g.is_some()
        && ((bp.is_none() && rp.is_some()) || (bp.is_some() && rp.is_none()) || bp_rp.is_none())
    {
        Ok("branch_partial_colour".to_string())
    } else {
        Ok("branch_no_photometry".to_string())
    }
}

pub fn write_sha256sum(dir: &Path, files: &[PathBuf]) -> Result<()> {
    let mut lines = Vec::new();
    for path in files {
        if path.is_file() {
            let digest = sha256_file(path)?;
            lines.push(format!(
                "{}\t{}",
                digest.trim_start_matches("sha256:"),
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("file")
            ));
        }
    }
    lines.sort();
    fs::write(dir.join("sampling.sha256sum"), lines.join("\n") + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stratified_ids() {
        let (pop, stratum) = parse_stratified_job_id("xp_continuous_only_low_g_snr");
        assert_eq!(pop.as_deref(), Some("xp_continuous_only"));
        assert_eq!(stratum.as_deref(), Some("low_g_snr"));
        let (pop, stratum) = parse_stratified_job_id("no_xp_branch_g_only");
        assert_eq!(pop.as_deref(), Some("no_xp"));
        assert_eq!(stratum.as_deref(), Some("branch_g_only"));
    }

    #[test]
    fn validates_fixture_csv_header() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("sample.csv");
        let header = SAMPLE_CSV_COLUMNS.join(",");
        fs::write(&path, format!("{header}\n1,2,true,false,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,false,VARIABLE,0,false,false\n"))?;
        let validation = validate_sample_csv(&path, Some(512))?;
        assert_eq!(validation.row_count, 1);
        assert!(!validation.truncated);
        Ok(())
    }

    #[test]
    fn photometry_branch_treats_empty_flux_as_missing() -> Result<()> {
        let headers = csv::StringRecord::from(SAMPLE_CSV_COLUMNS.as_slice());
        let mut record = csv::StringRecord::new();
        for col in SAMPLE_CSV_COLUMNS {
            record.push_field(match col {
                "source_id" => "1",
                "phot_g_mean_flux" => "10",
                "phot_bp_mean_flux" | "phot_rp_mean_flux" | "bp_rp" => "",
                _ => "0",
            });
        }
        assert_eq!(photometry_branch(&record, &headers)?, "branch_g_only");
        Ok(())
    }

    #[test]
    fn spatial_split_assignments_are_stable() -> Result<()> {
        let split = default_spatial_split();
        let grid = HealpixGrid::new(Nside::new(split.spatial_nside)?, HealpixOrdering::Ring)?;
        let cell = galactic_spatial_cell(&grid, 30.0, 0.0)?;
        assert_eq!(split.partition(cell)?, split.partition(cell)?);
        Ok(())
    }

    #[test]
    fn spatial_split_bucket_lists_are_disjoint() -> Result<()> {
        default_spatial_split().validate()?;
        Ok(())
    }
}
