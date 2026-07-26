//! Deterministic HEALPix accumulation for Gaia XP continuous bulk reconstruction.

use anyhow::{bail, Result};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DEFAULT_PILOT_NSIDE: u32 = 64;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct StableSum {
    sum: f64,
    compensation: f64,
}

impl StableSum {
    fn add(&mut self, value: f64) -> Result<()> {
        let adjusted = value - self.compensation;
        let next = self.sum + adjusted;
        self.compensation = (next - self.sum) - adjusted;
        self.sum = next;
        if !self.sum.is_finite() || !self.compensation.is_finite() {
            bail!("numeric overflow in HEALPix accumulator");
        }
        Ok(())
    }

    fn value(self) -> f64 {
        self.sum
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PixelAccumulator {
    pub sum_flux_336_650: StableSum,
    pub sum_statistical_variance: StableSum,
    pub sum_systematic_variance: StableSum,
    pub source_count: u64,
    pub valid_source_count: u64,
    pub excluded_source_count: u64,
}

impl PixelAccumulator {
    fn merge(&mut self, other: &Self) -> Result<()> {
        self.sum_flux_336_650.add(other.sum_flux_336_650.value())?;
        self.sum_statistical_variance
            .add(other.sum_statistical_variance.value())?;
        self.sum_systematic_variance
            .add(other.sum_systematic_variance.value())?;
        self.source_count += other.source_count;
        self.valid_source_count += other.valid_source_count;
        self.excluded_source_count += other.excluded_source_count;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XpContinuousHealpixAccumulator {
    pub schema_version: u32,
    pub nside: u32,
    pub pixels: Vec<PixelAccumulator>,
    pub quality_flag_counts: BTreeMap<String, u64>,
}

impl XpContinuousHealpixAccumulator {
    pub fn new(nside: u32) -> Result<Self> {
        if nside == 0 || !nside.is_power_of_two() {
            bail!("HEALPix nside must be a power of two, found {nside}");
        }
        let npix = 12 * nside as usize * nside as usize;
        Ok(Self {
            schema_version: 1,
            nside,
            pixels: vec![PixelAccumulator::default(); npix],
            quality_flag_counts: BTreeMap::new(),
        })
    }

    pub fn npix(&self) -> usize {
        self.pixels.len()
    }

    pub fn gaia_healpix_to_nside_pixel(
        gaia_healpix_index: u64,
        target_nside: u32,
    ) -> Result<usize> {
        const GAIA_ORDER: u32 = 12;
        let target_order = target_nside.trailing_zeros();
        if target_order > GAIA_ORDER {
            bail!("target nside {target_nside} exceeds Gaia embedded order");
        }
        let shift = 2 * (GAIA_ORDER - target_order);
        Ok((gaia_healpix_index >> shift) as usize)
    }

    pub fn accumulate_valid(
        &mut self,
        gaia_healpix_index: u64,
        flux_336_650: f64,
        statistical_uncertainty: f64,
        systematic_uncertainty: f64,
    ) -> Result<()> {
        if !flux_336_650.is_finite() || flux_336_650 <= 0.0 {
            bail!("valid accumulation requires finite positive flux");
        }
        let pixel = Self::gaia_healpix_to_nside_pixel(gaia_healpix_index, self.nside)?;
        if pixel >= self.pixels.len() {
            bail!("pixel index {pixel} out of range for nside {}", self.nside);
        }
        let cell = &mut self.pixels[pixel];
        cell.sum_flux_336_650.add(flux_336_650)?;
        cell.sum_statistical_variance
            .add(statistical_uncertainty.max(0.0).powi(2))?;
        cell.sum_systematic_variance
            .add(systematic_uncertainty.max(0.0).powi(2))?;
        cell.source_count += 1;
        cell.valid_source_count += 1;
        Ok(())
    }

    pub fn record_exclusion(&mut self, gaia_healpix_index: u64, reason_code: &str) -> Result<()> {
        *self
            .quality_flag_counts
            .entry(reason_code.to_string())
            .or_default() += 1;
        let pixel = Self::gaia_healpix_to_nside_pixel(gaia_healpix_index, self.nside)?;
        let cell = &mut self.pixels[pixel];
        cell.source_count += 1;
        cell.excluded_source_count += 1;
        Ok(())
    }

    pub fn merge(&mut self, other: &Self) -> Result<()> {
        if self.nside != other.nside || self.pixels.len() != other.pixels.len() {
            bail!("cannot merge HEALPix accumulators with different geometry");
        }
        for (left, right) in self.pixels.iter_mut().zip(&other.pixels) {
            left.merge(right)?;
        }
        for (flag, count) in &other.quality_flag_counts {
            *self.quality_flag_counts.entry(flag.clone()).or_default() += count;
        }
        Ok(())
    }

    pub fn checksum(&self) -> String {
        let mut hasher = Md5::new();
        hasher.update(self.nside.to_le_bytes());
        for (index, pixel) in self.pixels.iter().enumerate() {
            if pixel.source_count == 0 {
                continue;
            }
            hasher.update((index as u64).to_le_bytes());
            hasher.update(pixel.sum_flux_336_650.value().to_le_bytes());
            hasher.update(pixel.valid_source_count.to_le_bytes());
            hasher.update(pixel.excluded_source_count.to_le_bytes());
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn totals(&self) -> AccumulatorTotals {
        let mut totals = AccumulatorTotals::default();
        for pixel in &self.pixels {
            totals.sum_flux += pixel.sum_flux_336_650.value();
            totals.source_count += pixel.source_count;
            totals.valid_source_count += pixel.valid_source_count;
            totals.excluded_source_count += pixel.excluded_source_count;
        }
        totals
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct AccumulatorTotals {
    pub sum_flux: f64,
    pub source_count: u64,
    pub valid_source_count: u64,
    pub excluded_source_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_matches_single_worker() -> Result<()> {
        let mut full = XpContinuousHealpixAccumulator::new(64)?;
        full.accumulate_valid(0, 1.0, 0.1, 0.0)?;
        full.accumulate_valid(1 << 12, 2.0, 0.2, 0.0)?;

        let mut left = XpContinuousHealpixAccumulator::new(64)?;
        left.accumulate_valid(0, 1.0, 0.1, 0.0)?;

        let mut right = XpContinuousHealpixAccumulator::new(64)?;
        right.accumulate_valid(1 << 12, 2.0, 0.2, 0.0)?;

        let mut merged = XpContinuousHealpixAccumulator::new(64)?;
        merged.merge(&left)?;
        merged.merge(&right)?;
        assert_eq!(merged.checksum(), full.checksum());
        Ok(())
    }

    #[test]
    fn variance_accumulation_is_order_independent() -> Result<()> {
        let mut a = XpContinuousHealpixAccumulator::new(64)?;
        a.accumulate_valid(0, 1.0, 0.2, 0.1)?;
        a.accumulate_valid(1 << 12, 2.0, 0.3, 0.0)?;

        let mut b = XpContinuousHealpixAccumulator::new(64)?;
        b.accumulate_valid(1 << 12, 2.0, 0.3, 0.0)?;
        b.accumulate_valid(0, 1.0, 0.2, 0.1)?;

        assert_eq!(a.checksum(), b.checksum());
        Ok(())
    }

    #[test]
    fn exclusion_counts_are_mergeable() -> Result<()> {
        let mut left = XpContinuousHealpixAccumulator::new(64)?;
        left.record_exclusion(0, "non_positive_flux")?;

        let mut right = XpContinuousHealpixAccumulator::new(64)?;
        right.record_exclusion(1 << 12, "missing_gaiaxpy_outcome")?;

        let mut merged = left.clone();
        merged.merge(&right)?;
        assert_eq!(merged.totals().excluded_source_count, 2);
        assert_eq!(merged.quality_flag_counts.len(), 2);
        Ok(())
    }
}
