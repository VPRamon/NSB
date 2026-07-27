//! Deterministic merge, map emission, and production validation.

use super::accumulator::{merge_shards, PartitionShard};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const REPORT_SCHEMA_VERSION: u32 = 1;

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
    pub map_sha256: BTreeMap<String, String>,
    pub deterministic_reference: DeterministicReference,
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
    write_map(&map128, 128, map_pixels_128(&merged))?;
    write_map(&map64, 64, downsample_64(&merged)?)?;
    write_map(&map256, 256, upsample_256(&merged)?)?;

    let mut map_sha256 = BTreeMap::new();
    for (name, path) in [
        ("starlight_nside128.csv", &map128),
        ("starlight_nside64.csv", &map64),
        ("starlight_nside256.csv", &map256),
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
    if report.schema_version != REPORT_SCHEMA_VERSION || report.nside != 128 {
        bail!("unsupported Starlight merge report");
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
    let mut pixels = BTreeMap::new();
    for (pixel, value) in &merged.pixels {
        for child in 0..4 {
            pixels.insert(
                (*pixel << 2) | child,
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
    let depth = 7_u8;
    for pixel in 0..12_u32 * 128 * 128 {
        let (_, latitude) = cdshealpix::nested::center(depth, u64::from(pixel));
        if latitude.to_degrees().abs() < 20.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_resolution_transforms_preserve_expected_pixel_relationships() {
        let mut shard = PartitionShard::new("fixture", 128).unwrap();
        shard.admit(42_u64 << 45, 10.0, 1.0, 0.0).unwrap();
        let pixel = *shard.pixels.keys().next().unwrap();
        let down = downsample_64(&shard).unwrap();
        assert!(down.contains_key(&(pixel >> 2)));
        let up = upsample_256(&shard).unwrap();
        assert!((0..4).all(|child| up.contains_key(&((pixel << 2) | child))));
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
}
