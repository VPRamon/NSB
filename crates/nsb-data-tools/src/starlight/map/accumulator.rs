//! Deterministic sparse accumulators for independently produced Starlight shards.

use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const SHARD_SCHEMA_VERSION: u32 = 1;
const GAIA_HEALPIX_ORDER: u32 = 12;
const GAIA_SOURCE_ID_HEALPIX_SHIFT: u32 = 35;

/// Compensated finite sum. Shards are merged in canonical partition order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StableSum {
    sum: f64,
    compensation: f64,
}

impl StableSum {
    fn add(&mut self, value: f64) -> Result<()> {
        if !value.is_finite() {
            bail!("cannot accumulate a non-finite value");
        }
        let adjusted = value - self.compensation;
        let next = self.sum + adjusted;
        self.compensation = (next - self.sum) - adjusted;
        self.sum = next;
        if !self.sum.is_finite() || !self.compensation.is_finite() {
            bail!("numeric overflow in Starlight accumulator");
        }
        Ok(())
    }

    /// Current compensated sum.
    pub fn value(self) -> f64 {
        self.sum
    }
}

/// Per-pixel scientific and accounting totals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelAccumulator {
    pub flux_ph_m2_s: StableSum,
    pub statistical_variance: StableSum,
    pub systematic_variance: StableSum,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
}

impl PixelAccumulator {
    fn merge(&mut self, other: &Self) -> Result<()> {
        self.flux_ph_m2_s.add(other.flux_ph_m2_s.value())?;
        self.statistical_variance
            .add(other.statistical_variance.value())?;
        self.systematic_variance
            .add(other.systematic_variance.value())?;
        self.observed_sources = self
            .observed_sources
            .checked_add(other.observed_sources)
            .context("observed source count overflow")?;
        self.admitted_sources = self
            .admitted_sources
            .checked_add(other.admitted_sources)
            .context("admitted source count overflow")?;
        self.excluded_sources = self
            .excluded_sources
            .checked_add(other.excluded_sources)
            .context("excluded source count overflow")?;
        Ok(())
    }
}

/// Sparse result emitted by one immutable source partition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionShard {
    pub schema_version: u32,
    pub partition_id: String,
    pub nside: u32,
    pub pixels: BTreeMap<u32, PixelAccumulator>,
    pub exclusion_reasons: BTreeMap<String, u64>,
}

impl PartitionShard {
    /// Create an empty shard at a supported nested HEALPix resolution.
    pub fn new(partition_id: impl Into<String>, nside: u32) -> Result<Self> {
        validate_nside(nside)?;
        let partition_id = partition_id.into();
        if partition_id.is_empty()
            || !partition_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            bail!("partition id must contain lowercase ASCII, digits, and '-'");
        }
        Ok(Self {
            schema_version: SHARD_SCHEMA_VERSION,
            partition_id,
            nside,
            pixels: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
        })
    }

    /// Accumulate one admitted Gaia source.
    pub fn admit(
        &mut self,
        source_id: u64,
        flux_ph_m2_s: f64,
        statistical_uncertainty: f64,
        systematic_uncertainty: f64,
    ) -> Result<()> {
        if !flux_ph_m2_s.is_finite()
            || flux_ph_m2_s <= 0.0
            || !statistical_uncertainty.is_finite()
            || statistical_uncertainty < 0.0
            || !systematic_uncertainty.is_finite()
            || systematic_uncertainty < 0.0
        {
            bail!("admitted source requires positive flux and finite non-negative uncertainties");
        }
        let pixel = source_id_to_pixel(source_id, self.nside)?;
        let accumulator = self.pixels.entry(pixel).or_default();
        accumulator.flux_ph_m2_s.add(flux_ph_m2_s)?;
        accumulator
            .statistical_variance
            .add(statistical_uncertainty.powi(2))?;
        accumulator
            .systematic_variance
            .add(systematic_uncertainty.powi(2))?;
        accumulator.observed_sources = accumulator
            .observed_sources
            .checked_add(1)
            .context("observed source count overflow")?;
        accumulator.admitted_sources = accumulator
            .admitted_sources
            .checked_add(1)
            .context("admitted source count overflow")?;
        Ok(())
    }

    /// Record one source rejected by a stable scientific reason code.
    pub fn exclude(&mut self, source_id: u64, reason: &str) -> Result<()> {
        if reason.is_empty()
            || !reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            bail!("exclusion reason must be lowercase snake_case");
        }
        let pixel = source_id_to_pixel(source_id, self.nside)?;
        let accumulator = self.pixels.entry(pixel).or_default();
        accumulator.observed_sources = accumulator
            .observed_sources
            .checked_add(1)
            .context("observed source count overflow")?;
        accumulator.excluded_sources = accumulator
            .excluded_sources
            .checked_add(1)
            .context("excluded source count overflow")?;
        let count = self
            .exclusion_reasons
            .entry(reason.to_string())
            .or_default();
        *count = count
            .checked_add(1)
            .context("exclusion reason count overflow")?;
        Ok(())
    }

    /// Validate internal geometry and exact source accounting.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SHARD_SCHEMA_VERSION {
            bail!("unsupported Starlight shard schema {}", self.schema_version);
        }
        validate_nside(self.nside)?;
        let npix = 12_u64 * u64::from(self.nside) * u64::from(self.nside);
        let mut excluded = 0_u64;
        for (pixel, accumulator) in &self.pixels {
            if u64::from(*pixel) >= npix {
                bail!("pixel {pixel} is outside nside={}", self.nside);
            }
            if accumulator.observed_sources
                != accumulator
                    .admitted_sources
                    .checked_add(accumulator.excluded_sources)
                    .context("pixel source accounting overflow")?
            {
                bail!("pixel {pixel} violates exact source accounting");
            }
            excluded = excluded
                .checked_add(accumulator.excluded_sources)
                .context("excluded source total overflow")?;
        }
        let reason_total = self
            .exclusion_reasons
            .values()
            .try_fold(0_u64, |total, count| total.checked_add(*count))
            .context("exclusion reason total overflow")?;
        if excluded != reason_total {
            bail!("per-pixel exclusions do not match exclusion reason totals");
        }
        Ok(())
    }

    /// Persist a strict, canonical JSON checkpoint and return its SHA-256.
    pub fn write(&self, path: &Path) -> Result<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        artifact_store::atomic_write(path, &bytes)?;
        Ok(checksum_io::sha256_bytes(&bytes))
    }
}

/// Merge shards independently of their arrival order.
pub fn merge_shards(shards: impl IntoIterator<Item = PartitionShard>) -> Result<PartitionShard> {
    let mut shards: Vec<_> = shards.into_iter().collect();
    if shards.is_empty() {
        bail!("cannot merge an empty Starlight shard set");
    }
    shards.sort_by(|left, right| left.partition_id.cmp(&right.partition_id));
    if shards
        .windows(2)
        .any(|pair| pair[0].partition_id == pair[1].partition_id)
    {
        bail!("cannot merge duplicate Starlight partitions");
    }
    for shard in &shards {
        shard.validate()?;
    }
    let nside = shards[0].nside;
    let mut merged = PartitionShard::new("merged", nside)?;
    for shard in shards {
        if shard.nside != nside {
            bail!("cannot merge Starlight shards with different nside values");
        }
        for (pixel, source) in shard.pixels {
            merged.pixels.entry(pixel).or_default().merge(&source)?;
        }
        for (reason, count) in shard.exclusion_reasons {
            let merged_count = merged.exclusion_reasons.entry(reason).or_default();
            *merged_count = merged_count
                .checked_add(count)
                .context("merged exclusion count overflow")?;
        }
    }
    merged.validate()?;
    Ok(merged)
}

/// Convert a Gaia DR3 source identifier to a nested target HEALPix pixel.
pub fn source_id_to_pixel(source_id: u64, target_nside: u32) -> Result<u32> {
    validate_nside(target_nside)?;
    let target_order = target_nside.trailing_zeros();
    let level_12_pixel = source_id >> GAIA_SOURCE_ID_HEALPIX_SHIFT;
    let shift = 2 * (GAIA_HEALPIX_ORDER - target_order);
    u32::try_from(level_12_pixel >> shift).context("target HEALPix pixel exceeds u32")
}

fn validate_nside(nside: u32) -> Result<()> {
    if nside == 0 || !nside.is_power_of_two() || nside.trailing_zeros() > GAIA_HEALPIX_ORDER {
        bail!("nside must be a power of two between 1 and 4096");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gaia_source_id_from_embedded_level_12_healpix() -> Result<()> {
        let level_12_pixel = 123_456_u64;
        let source_id = (level_12_pixel << GAIA_SOURCE_ID_HEALPIX_SHIFT) | 17;
        assert_eq!(
            source_id_to_pixel(source_id, 128)?,
            (level_12_pixel >> 10) as u32
        );
        Ok(())
    }

    #[test]
    fn merge_bytes_are_independent_of_completion_order() -> Result<()> {
        let mut first = PartitionShard::new("000-099", 128)?;
        first.admit(0, 1.0, 0.1, 0.2)?;
        first.exclude(1_u64 << 35, "invalid_flux")?;
        let mut second = PartitionShard::new("100-199", 128)?;
        second.admit(2_u64 << 35, 3.0, 0.3, 0.4)?;

        let forward = merge_shards([first.clone(), second.clone()])?;
        let reverse = merge_shards([second, first])?;
        assert_eq!(serde_json::to_vec(&forward)?, serde_json::to_vec(&reverse)?);
        Ok(())
    }

    #[test]
    fn rejects_accounting_corruption_and_duplicate_partitions() -> Result<()> {
        let shard = PartitionShard::new("000-099", 128)?;
        assert!(merge_shards([shard.clone(), shard]).is_err());

        let mut corrupt = PartitionShard::new("100-199", 128)?;
        corrupt.pixels.insert(
            0,
            PixelAccumulator {
                observed_sources: 2,
                admitted_sources: 1,
                ..PixelAccumulator::default()
            },
        );
        assert!(corrupt.validate().is_err());
        Ok(())
    }
}
