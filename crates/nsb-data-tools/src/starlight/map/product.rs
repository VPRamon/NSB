//! Deterministic merge, map emission, and production validation.

use super::accumulator::{merge_shards, PartitionShard};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const REPORT_SCHEMA_VERSION: u32 = 2;
const ADMISSION_POLICY_ID: &str = "gaia-dr3-xp-continuous-join-v1";
const POPULATION_POLICY_ID: &str = "selection-function-identity-stub-v1";
const SPECTRAL_POLICY_ID: &str = "gaia-xp-continuous-336-650-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeReport {
    pub schema_version: u32,
    pub nside: u32,
    pub shard_count: usize,
    pub partition_ids: Vec<String>,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub exclusion_reasons: BTreeMap<String, u64>,
    pub science_policy: SciencePolicyReport,
    pub map_sha256: BTreeMap<String, String>,
    pub deterministic_reference: DeterministicReference,
}

/// Machine-readable declaration of what the current candidate does and does not model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SciencePolicyReport {
    pub schema_version: u32,
    pub admission_policy_id: String,
    pub admission_rules: Vec<String>,
    pub population_correction: PopulationCorrectionReport,
    pub spectral_coverage: SpectralCoverageReport,
}

/// Versioned no-op placeholder for the not-yet-calibrated Gaia selection model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationCorrectionReport {
    pub policy_id: String,
    pub applied: bool,
    pub minimum_weight: f64,
    pub maximum_weight: f64,
    pub residual_faint_tail_estimated: bool,
    pub limitation: String,
}

/// Explicit passband coverage declaration for the XP-only candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralCoverageReport {
    pub policy_id: String,
    pub target_band_nm: [u16; 2],
    pub directly_integrated_band_nm: [u16; 2],
    pub ultraviolet_correction_applied: bool,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterministicReference {
    pub pixel: u32,
    pub canonical_sha256: String,
    pub independent_partial_merge_sha256: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct MapPixel {
    flux: f64,
    compensation: f64,
    admitted: u64,
    excluded: u64,
}

impl MapPixel {
    fn add(&mut self, flux: f64, admitted: u64, excluded: u64) -> Result<()> {
        let adjusted = flux - self.compensation;
        let next = self.flux + adjusted;
        self.compensation = (next - self.flux) - adjusted;
        self.flux = next;
        self.admitted = self
            .admitted
            .checked_add(admitted)
            .context("map admitted count overflow")?;
        self.excluded = self
            .excluded
            .checked_add(excluded)
            .context("map excluded count overflow")?;
        Ok(())
    }
}

pub(crate) fn emit_maps(workspace: &Path, expected_partitions: &[String]) -> Result<Vec<Artifact>> {
    let shard_root = workspace.join("outputs/shards");
    let mut shard_paths = if shard_root.is_dir() {
        fs::read_dir(&shard_root)?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    shard_paths.sort();
    if shard_paths.is_empty() {
        bail!("no reconciled Starlight shards; run build workers before validate");
    }
    let mut shards = Vec::with_capacity(shard_paths.len());
    for path in shard_paths {
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("parse Starlight shard {}", path.display()))?;
        shard.validate()?;
        shards.push(shard);
    }
    shards.sort_by(|left, right| left.partition_id.cmp(&right.partition_id));
    let partition_ids = shards
        .iter()
        .map(|shard| shard.partition_id.clone())
        .collect::<Vec<_>>();
    if partition_ids != expected_partitions {
        bail!(
            "reconciled Starlight shard set is incomplete: found {} of {} expected partitions",
            partition_ids.len(),
            expected_partitions.len()
        );
    }
    let merged = merge_shards(shards.clone())?;
    let independent = independently_merge_partials(&shards)?;
    let deterministic_reference = deterministic_reference(&merged, &independent)?;

    let output_root = workspace.join("outputs");
    let map128 = output_root.join("starlight_nside128.csv");
    let map64 = output_root.join("starlight_nside64.csv");
    let map256 = output_root.join("starlight_nside256.csv");
    let map512 = output_root.join("starlight_nside512.csv");
    write_map(&map128, 128, map_pixels_128(&merged))?;
    write_map(&map64, 64, downsample_64(&merged)?)?;
    write_map(&map256, 256, upsample_256(&merged)?)?;
    write_map(&map512, 512, upsample_512(&merged)?)?;

    let mut map_sha256 = BTreeMap::new();
    for (name, path) in [
        ("starlight_nside128.csv", &map128),
        ("starlight_nside64.csv", &map64),
        ("starlight_nside256.csv", &map256),
        ("starlight_nside512.csv", &map512),
    ] {
        map_sha256.insert(name.to_string(), checksum_io::sha256_file(path)?);
    }
    let (observed_sources, admitted_sources, excluded_sources) = population_totals(&merged)?;
    let report = MergeReport {
        schema_version: REPORT_SCHEMA_VERSION,
        nside: merged.nside,
        shard_count: shards.len(),
        partition_ids,
        observed_sources,
        admitted_sources,
        excluded_sources,
        exclusion_reasons: merged.exclusion_reasons.clone(),
        science_policy: science_policy_report(),
        map_sha256,
        deterministic_reference,
    };
    let report_path = output_root.join("merge_report.json");
    artifact_store::atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;

    let mut artifacts = Vec::new();
    for (name, path) in [
        ("merge_report.json", report_path),
        ("starlight_nside128.csv", map128),
        ("starlight_nside256.csv", map256),
        ("starlight_nside512.csv", map512),
        ("starlight_nside64.csv", map64),
    ] {
        artifacts.push(Artifact {
            name: name.to_string(),
            sha256: checksum_io::sha256_file(&path)?,
            bytes: path.metadata()?.len(),
            path,
        });
    }
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    artifact_store::atomic_write(
        &output_root.join("artifacts.json"),
        &serde_json::to_vec_pretty(&artifacts)?,
    )?;
    Ok(artifacts)
}

pub(crate) fn scientific_gates(workspace: &Path) -> Result<Vec<ValidationGate>> {
    let report_path = workspace.join("outputs/merge_report.json");
    let report: MergeReport = serde_json::from_slice(&fs::read(&report_path)?)
        .with_context(|| format!("parse {}", report_path.display()))?;
    let accounting_passed = report
        .admitted_sources
        .checked_add(report.excluded_sources)
        .is_some_and(|total| total == report.observed_sources);
    let coverage = galactic_plane_coverage(&workspace.join("outputs/starlight_nside128.csv"))?;
    let declared_policy = science_policy_is_declared(&report.science_policy);
    Ok(vec![
        ValidationGate {
            name: "pixel-coverage-galactic-plane".to_string(),
            passed: coverage >= 0.70,
            detail: format!("{coverage:.6} (required >= 0.70)"),
        },
        ValidationGate {
            name: "population-accounting".to_string(),
            passed: accounting_passed,
            detail: format!(
                "{} admitted + {} excluded = {} observed",
                report.admitted_sources, report.excluded_sources, report.observed_sources
            ),
        },
        ValidationGate {
            name: "declared-science-policy".to_string(),
            passed: declared_policy,
            detail: format!(
                "admission={}; population={} applied={}; spectral={} ultraviolet_correction={}",
                report.science_policy.admission_policy_id,
                report.science_policy.population_correction.policy_id,
                report.science_policy.population_correction.applied,
                report.science_policy.spectral_coverage.policy_id,
                report
                    .science_policy
                    .spectral_coverage
                    .ultraviolet_correction_applied
            ),
        },
        ValidationGate {
            name: "deterministic-independent-partial-merge".to_string(),
            passed: report.deterministic_reference.stable,
            detail: format!(
                "pixel {} canonical={} partial={}",
                report.deterministic_reference.pixel,
                report.deterministic_reference.canonical_sha256,
                report
                    .deterministic_reference
                    .independent_partial_merge_sha256
            ),
        },
    ])
}

pub(crate) fn validate_map(path: &Path, expected_nside: u32) -> Result<()> {
    let text = fs::read_to_string(path)?;
    for header in [
        "# map_type=healpix",
        "# coordinate_frame=galactic",
        &format!("# nside={expected_nside}"),
    ] {
        if !text.lines().any(|line| line.trim() == header) {
            bail!("{} is missing header {header}", path.display());
        }
    }
    let mut rows = 0_usize;
    for line in text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .skip(1)
    {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 4 {
            bail!("{} contains a malformed map row", path.display());
        }
        let pixel = fields[0].parse::<u64>()?;
        let flux = fields[1].parse::<f64>()?;
        let _: u64 = fields[2].parse()?;
        let _: u64 = fields[3].parse()?;
        if pixel >= 12 * u64::from(expected_nside).pow(2) || !flux.is_finite() {
            bail!(
                "{} contains an invalid pixel or non-finite flux",
                path.display()
            );
        }
        rows += 1;
    }
    if rows == 0 {
        bail!("{} contains no occupied map pixels", path.display());
    }
    Ok(())
}

pub(crate) fn validate_report(path: &Path) -> Result<()> {
    let report: MergeReport = serde_json::from_slice(&fs::read(path)?)?;
    if report.schema_version != REPORT_SCHEMA_VERSION
        || report.nside != 128
        || !science_policy_is_declared(&report.science_policy)
    {
        bail!("unsupported Starlight merge report");
    }
    let expected_maps = BTreeSet::from([
        "starlight_nside64.csv",
        "starlight_nside128.csv",
        "starlight_nside256.csv",
        "starlight_nside512.csv",
    ]);
    if report
        .map_sha256
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_maps
    {
        bail!("Starlight merge report does not declare the complete resolution sweep");
    }
    for (name, expected) in &report.map_sha256 {
        let map_path = path
            .parent()
            .context("merge report has no parent")?
            .join(name);
        if checksum_io::sha256_file(&map_path)? != *expected {
            bail!("merge report checksum mismatch for {name}");
        }
    }
    Ok(())
}

fn map_pixels_128(merged: &PartitionShard) -> BTreeMap<u32, MapPixel> {
    merged
        .pixels
        .iter()
        .map(|(pixel, value)| {
            (
                *pixel,
                MapPixel {
                    flux: value.flux_ph_m2_s.value(),
                    admitted: value.admitted_sources,
                    excluded: value.excluded_sources,
                    ..MapPixel::default()
                },
            )
        })
        .collect()
}

fn downsample_64(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    let mut pixels = BTreeMap::<u32, MapPixel>::new();
    for (pixel, value) in &merged.pixels {
        pixels.entry(*pixel >> 2).or_default().add(
            value.flux_ph_m2_s.value(),
            value.admitted_sources,
            value.excluded_sources,
        )?;
    }
    Ok(pixels)
}

fn upsample_256(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    upsample(merged, 1)
}

fn upsample_512(merged: &PartitionShard) -> Result<BTreeMap<u32, MapPixel>> {
    upsample(merged, 2)
}

fn upsample(merged: &PartitionShard, order_delta: u32) -> Result<BTreeMap<u32, MapPixel>> {
    let child_count = 1_u32
        .checked_shl(2 * order_delta)
        .context("invalid HEALPix upsample order")?;
    let mut pixels = BTreeMap::new();
    for (pixel, value) in &merged.pixels {
        for child in 0..child_count {
            pixels.insert(
                (*pixel << (2 * order_delta)) | child,
                MapPixel {
                    // Diagnostic nearest-neighbour upsampling preserves surface
                    // brightness; it does not claim new source localization.
                    flux: value.flux_ph_m2_s.value(),
                    admitted: value.admitted_sources,
                    excluded: value.excluded_sources,
                    ..MapPixel::default()
                },
            );
        }
    }
    Ok(pixels)
}

fn science_policy_report() -> SciencePolicyReport {
    SciencePolicyReport {
        schema_version: 1,
        admission_policy_id: ADMISSION_POLICY_ID.to_string(),
        admission_rules: vec![
            "require_gaia_source_match".to_string(),
            "exclude_calibration_failed".to_string(),
            "exclude_non_positive_or_non_finite_flux".to_string(),
            "exclude_invalid_statistical_uncertainty".to_string(),
        ],
        population_correction: PopulationCorrectionReport {
            policy_id: POPULATION_POLICY_ID.to_string(),
            applied: false,
            minimum_weight: 1.0,
            maximum_weight: 1.0,
            residual_faint_tail_estimated: false,
            limitation: "No validated sky-, magnitude-, and colour-conditioned Gaia selection-function model is configured; admitted source weights remain exactly one.".to_string(),
        },
        spectral_coverage: SpectralCoverageReport {
            policy_id: SPECTRAL_POLICY_ID.to_string(),
            target_band_nm: [300, 650],
            directly_integrated_band_nm: [336, 650],
            ultraviolet_correction_applied: false,
            limitation: "The frozen GaiaXPy design begins at 336 nm; no independently calibrated 300-336 nm correction is applied.".to_string(),
        },
    }
}

fn science_policy_is_declared(policy: &SciencePolicyReport) -> bool {
    policy.schema_version == 1
        && policy.admission_policy_id == ADMISSION_POLICY_ID
        && policy.admission_rules
            == [
                "require_gaia_source_match",
                "exclude_calibration_failed",
                "exclude_non_positive_or_non_finite_flux",
                "exclude_invalid_statistical_uncertainty",
            ]
        && policy.population_correction.policy_id == POPULATION_POLICY_ID
        && !policy.population_correction.applied
        && policy.population_correction.minimum_weight == 1.0
        && policy.population_correction.maximum_weight == 1.0
        && !policy.population_correction.residual_faint_tail_estimated
        && policy.spectral_coverage.policy_id == SPECTRAL_POLICY_ID
        && policy.spectral_coverage.target_band_nm == [300, 650]
        && policy.spectral_coverage.directly_integrated_band_nm == [336, 650]
        && !policy.spectral_coverage.ultraviolet_correction_applied
}

fn write_map(path: &Path, nside: u32, pixels: BTreeMap<u32, MapPixel>) -> Result<()> {
    let mut text = format!(
        "# map_type=healpix\n# coordinate_frame=galactic\n# nside={nside}\npixel,flux_ph_m2_s,admitted_sources,excluded_sources\n"
    );
    for (pixel, value) in pixels {
        text.push_str(&format!(
            "{pixel},{:.17e},{},{}\n",
            value.flux, value.admitted, value.excluded
        ));
    }
    artifact_store::atomic_write(path, text.as_bytes())
}

fn independently_merge_partials(shards: &[PartitionShard]) -> Result<PartitionShard> {
    if shards.len() == 1 {
        return merge_shards(shards.to_vec());
    }
    let midpoint = shards.len().div_ceil(2);
    let mut left = merge_shards(shards[..midpoint].to_vec())?;
    let mut right = merge_shards(shards[midpoint..].iter().rev().cloned().collect::<Vec<_>>())?;
    left.partition_id = "partial-left".to_string();
    right.partition_id = "partial-right".to_string();
    merge_shards([right, left])
}

fn deterministic_reference(
    canonical: &PartitionShard,
    independent: &PartitionShard,
) -> Result<DeterministicReference> {
    for (pixel, value) in &canonical.pixels {
        let Some(other) = independent.pixels.get(pixel) else {
            continue;
        };
        let canonical_sha256 = checksum_io::sha256_bytes(&serde_json::to_vec(value)?);
        let independent_sha256 = checksum_io::sha256_bytes(&serde_json::to_vec(other)?);
        if canonical_sha256 == independent_sha256 {
            return Ok(DeterministicReference {
                pixel: *pixel,
                canonical_sha256,
                independent_partial_merge_sha256: independent_sha256,
                stable: true,
            });
        }
    }
    bail!("no pixel is stable across independent partial Starlight merges")
}

fn population_totals(merged: &PartitionShard) -> Result<(u64, u64, u64)> {
    let mut observed = 0_u64;
    let mut admitted = 0_u64;
    let mut excluded = 0_u64;
    for pixel in merged.pixels.values() {
        observed = observed
            .checked_add(pixel.observed_sources)
            .context("observed source total overflow")?;
        admitted = admitted
            .checked_add(pixel.admitted_sources)
            .context("admitted source total overflow")?;
        excluded = excluded
            .checked_add(pixel.excluded_sources)
            .context("excluded source total overflow")?;
    }
    Ok((observed, admitted, excluded))
}

fn galactic_plane_coverage(path: &Path) -> Result<f64> {
    let text = fs::read_to_string(path)?;
    let occupied = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split(',');
            let pixel = fields.next()?.parse::<u32>().ok()?;
            let _flux = fields.next()?;
            let admitted = fields.next()?.parse::<u64>().ok()?;
            (admitted > 0).then_some(pixel)
        })
        .collect::<BTreeSet<_>>();
    let mut plane_pixels = 0_u64;
    let mut covered = 0_u64;
    let plane_sin_latitude_limit = 20_f64.to_radians().sin();
    for pixel in 0..12_u32 * 128 * 128 {
        if nested_pixel_center_sin_latitude(128, pixel).abs() < plane_sin_latitude_limit {
            plane_pixels += 1;
            if occupied.contains(&pixel) {
                covered += 1;
            }
        }
    }
    if plane_pixels == 0 {
        bail!("HEALPix geometry produced no Galactic-plane pixels");
    }
    Ok(covered as f64 / plane_pixels as f64)
}

/// Return `sin(latitude)` for the centre of a NESTED HEALPix pixel.
///
/// The coverage gate only needs latitude, so decoding the face-local Morton
/// index and applying the standard HEALPix ring-coordinate equation avoids
/// pulling a full geometry crate (and its serialization dependency) into the
/// production binary.
fn nested_pixel_center_sin_latitude(nside: u32, pixel: u32) -> f64 {
    debug_assert!(nside.is_power_of_two());
    debug_assert!(pixel < 12 * nside * nside);

    const JRLL: [u32; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];

    let pixels_per_face = nside * nside;
    let face = pixel / pixels_per_face;
    let face_pixel = pixel % pixels_per_face;
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut source_bit = 0_u32;
    let mut coordinate_bit = 1_u32;
    while coordinate_bit < nside {
        x |= ((face_pixel >> source_bit) & 1) * coordinate_bit;
        y |= ((face_pixel >> (source_bit + 1)) & 1) * coordinate_bit;
        source_bit += 2;
        coordinate_bit <<= 1;
    }

    let ring = i64::from(JRLL[face as usize] * nside) - i64::from(x) - i64::from(y) - 1;
    let nside_i64 = i64::from(nside);
    let nside_f64 = f64::from(nside);
    if ring < nside_i64 {
        1.0 - (ring * ring) as f64 / (3.0 * nside_f64 * nside_f64)
    } else if ring > 3 * nside_i64 {
        let south_ring = 4 * nside_i64 - ring;
        -1.0 + (south_ring * south_ring) as f64 / (3.0 * nside_f64 * nside_f64)
    } else {
        (2 * nside_i64 - ring) as f64 * (2.0 / (3.0 * nside_f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_nested_latitudes_cover_the_expected_galactic_plane_pixels() {
        let north = 2.0 / 3.0;
        for pixel in 0..4 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), north);
        }
        for pixel in 4..8 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), 0.0);
        }
        for pixel in 8..12 {
            assert_eq!(nested_pixel_center_sin_latitude(1, pixel), -north);
        }

        let plane_sin_latitude_limit = 20_f64.to_radians().sin();
        let mut plane_pixels = 0_usize;
        for pixel in 0..12_u32 * 128 * 128 {
            plane_pixels += usize::from(
                nested_pixel_center_sin_latitude(128, pixel).abs() < plane_sin_latitude_limit,
            );
        }
        assert_eq!(plane_pixels, 67_072);
    }

    #[test]
    fn nested_resolution_transforms_preserve_expected_pixel_relationships() {
        let mut shard = PartitionShard::new("fixture", 128).unwrap();
        shard.admit(42_u64 << 45, 10.0, 1.0, 0.0).unwrap();
        let pixel = *shard.pixels.keys().next().unwrap();
        let down = downsample_64(&shard).unwrap();
        assert!(down.contains_key(&(pixel >> 2)));
        let up = upsample_256(&shard).unwrap();
        assert!((0..4).all(|child| up.contains_key(&((pixel << 2) | child))));
        let up512 = upsample_512(&shard).unwrap();
        assert!((0..16).all(|child| up512.contains_key(&((pixel << 4) | child))));
    }

    #[test]
    fn independent_partial_merge_has_a_stable_pixel_checksum() {
        let mut first = PartitionShard::new("first", 128).unwrap();
        first.admit(0, 1.0, 0.1, 0.0).unwrap();
        let mut second = PartitionShard::new("second", 128).unwrap();
        second.admit(1_u64 << 45, 2.0, 0.2, 0.0).unwrap();
        let mut third = PartitionShard::new("third", 128).unwrap();
        third.admit(2_u64 << 45, 3.0, 0.3, 0.0).unwrap();
        let shards = vec![first, second, third];
        let canonical = merge_shards(shards.clone()).unwrap();
        let independent = independently_merge_partials(&shards).unwrap();
        assert!(
            deterministic_reference(&canonical, &independent)
                .unwrap()
                .stable
        );
    }

    #[test]
    fn candidate_science_limitations_are_versioned_and_explicit() {
        let policy = science_policy_report();
        assert_eq!(policy.schema_version, 1);
        assert_eq!(policy.admission_policy_id, ADMISSION_POLICY_ID);
        assert_eq!(policy.population_correction.policy_id, POPULATION_POLICY_ID);
        assert!(!policy.population_correction.applied);
        assert_eq!(policy.population_correction.minimum_weight, 1.0);
        assert_eq!(policy.population_correction.maximum_weight, 1.0);
        assert_eq!(policy.spectral_coverage.policy_id, SPECTRAL_POLICY_ID);
        assert_eq!(
            policy.spectral_coverage.directly_integrated_band_nm,
            [336, 650]
        );
        assert!(!policy.spectral_coverage.ultraviolet_correction_applied);
        assert!(science_policy_is_declared(&policy));
    }
}
