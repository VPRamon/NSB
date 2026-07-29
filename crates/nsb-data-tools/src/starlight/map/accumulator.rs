//! Deterministic sparse accumulators for independently produced Starlight shards.

use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::StarlightProductBand;
use crate::starlight::uv::{
    ApplicabilityStatus, CalibrationStatus, CombinedBandFlux, ModelResponse, SystematicCorrelation,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

const SHARD_SCHEMA_VERSION: u32 = 3;
const GAIA_HEALPIX_ORDER: u32 = 12;
const GAIA_SOURCE_ID_HEALPIX_SHIFT: u32 = 35;
const EXACT_SUM_LIMBS: usize = 33;
const EXACT_SUM_BASE_EXPONENT: i32 = -1074;

/// Exact, mergeable sum of finite non-negative IEEE-754 binary64 values.
///
/// Each input is decomposed into its integer significand and stored in a sparse
/// little-endian limb array whose unit is `2^-1074`. Integer limb addition is
/// associative and commutative, so independently grouped shard merges retain
/// exactly the same state and round to binary64 only when a value is requested.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StableSum {
    limbs: BTreeMap<u16, u64>,
}

impl StableSum {
    fn add(&mut self, value: f64) -> Result<()> {
        if !value.is_finite() || (value.is_sign_negative() && value != 0.0) {
            bail!("cannot accumulate a non-finite or negative value");
        }
        if value == 0.0 {
            return Ok(());
        }

        let bits = value.to_bits();
        let exponent_bits = ((bits >> 52) & 0x7ff) as usize;
        let fraction = bits & ((1_u64 << 52) - 1);
        let (significand, shift) = if exponent_bits == 0 {
            (fraction, 0)
        } else {
            ((1_u64 << 52) | fraction, exponent_bits - 1)
        };
        self.add_shifted(significand, shift)?;
        if !self.value().is_finite() {
            bail!("numeric overflow in Starlight accumulator");
        }
        Ok(())
    }

    fn merge(&mut self, other: &Self) -> Result<()> {
        other.validate()?;
        for (index, value) in &other.limbs {
            self.add_limb(usize::from(*index), *value)?;
        }
        if !self.value().is_finite() {
            bail!("numeric overflow in Starlight accumulator");
        }
        Ok(())
    }

    /// Correctly rounded binary64 value of the exact accumulated sum.
    pub fn value(&self) -> f64 {
        let Some(mut highest) = self.highest_bit() else {
            return 0.0;
        };

        if highest < 52 {
            return f64::from_bits(self.limbs.get(&0).copied().unwrap_or_default());
        }

        let shift = highest - 52;
        let mut significand = self.bits(shift, 53);
        if shift > 0 {
            let round_bit = self.bit(shift - 1);
            let sticky = self.any_bits_below(shift - 1);
            if round_bit && (sticky || significand & 1 == 1) {
                significand += 1;
                if significand == 1_u64 << 53 {
                    significand >>= 1;
                    highest += 1;
                }
            }
        }

        let unbiased_exponent = highest as i32 + EXACT_SUM_BASE_EXPONENT;
        if unbiased_exponent > 1023 {
            return f64::INFINITY;
        }
        let exponent_bits = (unbiased_exponent + 1023) as u64;
        let fraction = significand & ((1_u64 << 52) - 1);
        f64::from_bits((exponent_bits << 52) | fraction)
    }

    pub(crate) fn append_canonical_bytes(&self, bytes: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        let limb_count =
            u16::try_from(self.limbs.len()).context("exact-sum limb count overflow")?;
        bytes.extend_from_slice(&limb_count.to_be_bytes());
        for (index, value) in &self.limbs {
            bytes.extend_from_slice(&index.to_be_bytes());
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self
            .limbs
            .iter()
            .any(|(index, value)| usize::from(*index) >= EXACT_SUM_LIMBS || *value == 0)
        {
            bail!("Starlight exact-sum state contains an invalid limb");
        }
        if !self.value().is_finite() {
            bail!("Starlight exact-sum state is not representable as finite binary64");
        }
        Ok(())
    }

    fn add_shifted(&mut self, significand: u64, shift: usize) -> Result<()> {
        if significand == 0 {
            return Ok(());
        }
        let index = shift / 64;
        let offset = shift % 64;
        self.add_limb(index, significand << offset)?;
        if offset != 0 {
            self.add_limb(index + 1, significand >> (64 - offset))?;
        }
        Ok(())
    }

    fn add_limb(&mut self, mut index: usize, mut value: u64) -> Result<()> {
        while value != 0 {
            if index >= EXACT_SUM_LIMBS {
                bail!("numeric overflow in Starlight exact accumulator");
            }
            let key = u16::try_from(index).context("exact-sum limb index overflow")?;
            let current = self.limbs.get(&key).copied().unwrap_or_default();
            let (sum, carry) = current.overflowing_add(value);
            if sum == 0 {
                self.limbs.remove(&key);
            } else {
                self.limbs.insert(key, sum);
            }
            value = u64::from(carry);
            index += 1;
        }
        Ok(())
    }

    fn highest_bit(&self) -> Option<usize> {
        self.limbs
            .last_key_value()
            .map(|(index, value)| usize::from(*index) * 64 + (63 - value.leading_zeros() as usize))
    }

    fn bits(&self, start: usize, width: usize) -> u64 {
        debug_assert!(width > 0 && width < 64);
        let limb = start / 64;
        let offset = start % 64;
        let mut value = self.limbs.get(&(limb as u16)).copied().unwrap_or_default() >> offset;
        if offset + width > 64 {
            value |= self
                .limbs
                .get(&((limb + 1) as u16))
                .copied()
                .unwrap_or_default()
                << (64 - offset);
        }
        value & ((1_u64 << width) - 1)
    }

    fn bit(&self, index: usize) -> bool {
        let limb = index / 64;
        let offset = index % 64;
        self.limbs
            .get(&(limb as u16))
            .is_some_and(|value| value & (1_u64 << offset) != 0)
    }

    fn any_bits_below(&self, bit_count: usize) -> bool {
        let full_limbs = bit_count / 64;
        if (0..full_limbs).any(|index| {
            self.limbs
                .get(&(index as u16))
                .is_some_and(|value| *value != 0)
        }) {
            return true;
        }
        let remaining = bit_count % 64;
        remaining != 0
            && self
                .limbs
                .get(&(full_limbs as u16))
                .is_some_and(|value| value & ((1_u64 << remaining) - 1) != 0)
    }
}

/// Per-pixel scientific and accounting totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelAccumulator {
    /// Selected product-band flux used by the canonical map.
    pub flux_ph_m2_s: StableSum,
    /// Selected product statistical variance.
    pub statistical_variance: StableSum,
    /// Independent selected-product systematic variance.
    pub systematic_variance: StableSum,
    /// Fully correlated selected-product systematic uncertainty.
    pub systematic_correlated_uncertainty: StableSum,
    pub flux_300_336_ph_m2_s: StableSum,
    pub flux_336_650_ph_m2_s: StableSum,
    pub flux_300_650_ph_m2_s: StableSum,
    pub statistical_variance_300_336: StableSum,
    pub statistical_variance_336_650: StableSum,
    pub statistical_variance_300_650: StableSum,
    pub systematic_variance_300_336_independent: StableSum,
    pub systematic_uncertainty_300_336_correlated: StableSum,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
}

impl PixelAccumulator {
    fn merge(&mut self, other: &Self) -> Result<()> {
        self.flux_ph_m2_s.merge(&other.flux_ph_m2_s)?;
        self.statistical_variance
            .merge(&other.statistical_variance)?;
        self.systematic_variance.merge(&other.systematic_variance)?;
        self.systematic_correlated_uncertainty
            .merge(&other.systematic_correlated_uncertainty)?;
        self.flux_300_336_ph_m2_s
            .merge(&other.flux_300_336_ph_m2_s)?;
        self.flux_336_650_ph_m2_s
            .merge(&other.flux_336_650_ph_m2_s)?;
        self.flux_300_650_ph_m2_s
            .merge(&other.flux_300_650_ph_m2_s)?;
        self.statistical_variance_300_336
            .merge(&other.statistical_variance_300_336)?;
        self.statistical_variance_336_650
            .merge(&other.statistical_variance_336_650)?;
        self.statistical_variance_300_650
            .merge(&other.statistical_variance_300_650)?;
        self.systematic_variance_300_336_independent
            .merge(&other.systematic_variance_300_336_independent)?;
        self.systematic_uncertainty_300_336_correlated
            .merge(&other.systematic_uncertainty_300_336_correlated)?;
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

    fn validate(&self) -> Result<()> {
        self.flux_ph_m2_s.validate()?;
        self.statistical_variance.validate()?;
        self.systematic_variance
            .validate()
            .and_then(|_| self.systematic_correlated_uncertainty.validate())
            .and_then(|_| self.flux_300_336_ph_m2_s.validate())
            .and_then(|_| self.flux_336_650_ph_m2_s.validate())
            .and_then(|_| self.flux_300_650_ph_m2_s.validate())
            .and_then(|_| self.statistical_variance_300_336.validate())
            .and_then(|_| self.statistical_variance_336_650.validate())
            .and_then(|_| self.statistical_variance_300_650.validate())
            .and_then(|_| self.systematic_variance_300_336_independent.validate())
            .and_then(|_| self.systematic_uncertainty_300_336_correlated.validate())
    }

    pub(crate) fn selected_systematic_uncertainty(&self) -> f64 {
        self.systematic_variance
            .value()
            .sqrt()
            .hypot(self.systematic_correlated_uncertainty.value())
    }
}

/// Artifact identity and policy copied into every corrected shard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvCorrectionShardMetadata {
    pub model_id: String,
    pub artifact_sha256: String,
    pub calibration_status: CalibrationStatus,
    pub response: ModelResponse,
    pub measured_correction_statistical_correlation_bits: u64,
    pub systematic_correlation: SystematicCorrelation,
}

/// Sparse result emitted by one immutable source partition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionShard {
    pub schema_version: u32,
    pub partition_id: String,
    pub nside: u32,
    pub product_band: StarlightProductBand,
    pub ultraviolet_correction: Option<UvCorrectionShardMetadata>,
    pub pixels: BTreeMap<u32, PixelAccumulator>,
    pub exclusion_reasons: BTreeMap<String, u64>,
    pub ultraviolet_applicability: BTreeMap<ApplicabilityStatus, u64>,
}

impl PartitionShard {
    /// Create an empty shard at a supported nested HEALPix resolution.
    pub fn new(partition_id: impl Into<String>, nside: u32) -> Result<Self> {
        Self::new_with_policy(
            partition_id,
            nside,
            StarlightProductBand::Measured336To650,
            None,
        )
    }

    pub fn new_with_policy(
        partition_id: impl Into<String>,
        nside: u32,
        product_band: StarlightProductBand,
        ultraviolet_correction: Option<UvCorrectionShardMetadata>,
    ) -> Result<Self> {
        validate_nside(nside)?;
        let partition_id = partition_id.into();
        if partition_id.is_empty()
            || !partition_id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            bail!("partition id must contain lowercase ASCII, digits, and '-'");
        }
        if (product_band == StarlightProductBand::Combined300To650)
            != ultraviolet_correction.is_some()
        {
            bail!("300–650 nm shards require UV correction metadata and measured shards forbid it");
        }
        if let Some(metadata) = &ultraviolet_correction {
            let statistical_correlation =
                f64::from_bits(metadata.measured_correction_statistical_correlation_bits);
            if metadata.model_id.trim().is_empty()
                || !is_sha256(&metadata.artifact_sha256)
                || metadata.calibration_status != CalibrationStatus::Validated
                || metadata.response.validate().is_err()
                || !statistical_correlation.is_finite()
                || !(-1.0..=1.0).contains(&statistical_correlation)
            {
                bail!("UV correction shard metadata is invalid or not validated");
            }
        }
        Ok(Self {
            schema_version: SHARD_SCHEMA_VERSION,
            partition_id,
            nside,
            product_band,
            ultraviolet_correction,
            pixels: BTreeMap::new(),
            exclusion_reasons: BTreeMap::new(),
            ultraviolet_applicability: BTreeMap::new(),
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
        self.admit_components(
            source_id,
            SourceFluxComponents {
                flux_300_336_ph_m2_s: 0.0,
                flux_336_650_ph_m2_s: flux_ph_m2_s,
                flux_300_650_ph_m2_s: flux_ph_m2_s,
                statistical_uncertainty_300_336_ph_m2_s: 0.0,
                statistical_uncertainty_336_650_ph_m2_s: statistical_uncertainty,
                statistical_uncertainty_300_650_ph_m2_s: statistical_uncertainty,
                systematic_uncertainty_300_336_ph_m2_s: systematic_uncertainty,
                systematic_uncertainty_300_650_ph_m2_s: systematic_uncertainty,
                systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
                applicability_status: None,
            },
        )
    }

    /// Accumulate an explicitly separated corrected source.
    pub fn admit_corrected(&mut self, source_id: u64, flux: &CombinedBandFlux) -> Result<()> {
        if self.product_band != StarlightProductBand::Combined300To650 {
            bail!("cannot admit a UV-corrected source into a measured-only shard");
        }
        let metadata = self
            .ultraviolet_correction
            .as_ref()
            .context("corrected shard has no UV metadata")?;
        if metadata.model_id != flux.model_id
            || metadata.artifact_sha256 != flux.artifact_sha256
            || metadata.systematic_correlation != flux.systematic_correlation
        {
            bail!("corrected source identity does not match shard UV metadata");
        }
        self.admit_components(
            source_id,
            SourceFluxComponents {
                flux_300_336_ph_m2_s: flux.flux_300_336_ph_m2_s,
                flux_336_650_ph_m2_s: flux.flux_336_650_ph_m2_s,
                flux_300_650_ph_m2_s: flux.flux_300_650_ph_m2_s,
                statistical_uncertainty_300_336_ph_m2_s: flux
                    .statistical_uncertainty_300_336_ph_m2_s,
                statistical_uncertainty_336_650_ph_m2_s: flux
                    .statistical_uncertainty_336_650_ph_m2_s,
                statistical_uncertainty_300_650_ph_m2_s: flux
                    .statistical_uncertainty_300_650_ph_m2_s,
                systematic_uncertainty_300_336_ph_m2_s: flux.systematic_uncertainty_300_336_ph_m2_s,
                systematic_uncertainty_300_650_ph_m2_s: flux.systematic_uncertainty_300_650_ph_m2_s,
                systematic_correlation: flux.systematic_correlation,
                applicability_status: Some(flux.applicability_status),
            },
        )
    }

    fn admit_components(&mut self, source_id: u64, flux: SourceFluxComponents) -> Result<()> {
        let selected_flux = match self.product_band {
            StarlightProductBand::Measured336To650 => flux.flux_336_650_ph_m2_s,
            StarlightProductBand::Combined300To650 => flux.flux_300_650_ph_m2_s,
        };
        let selected_statistical = match self.product_band {
            StarlightProductBand::Measured336To650 => flux.statistical_uncertainty_336_650_ph_m2_s,
            StarlightProductBand::Combined300To650 => flux.statistical_uncertainty_300_650_ph_m2_s,
        };
        let selected_systematic = match self.product_band {
            StarlightProductBand::Measured336To650 => 0.0,
            StarlightProductBand::Combined300To650 => flux.systematic_uncertainty_300_650_ph_m2_s,
        };
        let numeric = [
            selected_flux,
            selected_statistical,
            selected_systematic,
            flux.flux_300_336_ph_m2_s,
            flux.flux_336_650_ph_m2_s,
            flux.flux_300_650_ph_m2_s,
            flux.statistical_uncertainty_300_336_ph_m2_s,
            flux.statistical_uncertainty_336_650_ph_m2_s,
            flux.statistical_uncertainty_300_650_ph_m2_s,
            flux.systematic_uncertainty_300_336_ph_m2_s,
            flux.systematic_uncertainty_300_650_ph_m2_s,
        ];
        if selected_flux <= 0.0
            || numeric
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("admitted source requires positive flux and finite non-negative uncertainties");
        }
        let pixel = source_id_to_pixel(source_id, self.nside)?;
        let accumulator = self.pixels.entry(pixel).or_default();
        accumulator.flux_ph_m2_s.add(selected_flux)?;
        accumulator
            .statistical_variance
            .add(selected_statistical.powi(2))?;
        match flux.systematic_correlation {
            SystematicCorrelation::IndependentBetweenSources => {
                accumulator
                    .systematic_variance
                    .add(selected_systematic.powi(2))?;
                accumulator
                    .systematic_variance_300_336_independent
                    .add(flux.systematic_uncertainty_300_336_ph_m2_s.powi(2))?;
            }
            SystematicCorrelation::FullyCorrelatedBetweenSources => {
                accumulator
                    .systematic_correlated_uncertainty
                    .add(selected_systematic)?;
                accumulator
                    .systematic_uncertainty_300_336_correlated
                    .add(flux.systematic_uncertainty_300_336_ph_m2_s)?;
            }
        }
        accumulator
            .flux_300_336_ph_m2_s
            .add(flux.flux_300_336_ph_m2_s)?;
        accumulator
            .flux_336_650_ph_m2_s
            .add(flux.flux_336_650_ph_m2_s)?;
        accumulator
            .flux_300_650_ph_m2_s
            .add(flux.flux_300_650_ph_m2_s)?;
        accumulator
            .statistical_variance_300_336
            .add(flux.statistical_uncertainty_300_336_ph_m2_s.powi(2))?;
        accumulator
            .statistical_variance_336_650
            .add(flux.statistical_uncertainty_336_650_ph_m2_s.powi(2))?;
        accumulator
            .statistical_variance_300_650
            .add(flux.statistical_uncertainty_300_650_ph_m2_s.powi(2))?;
        accumulator.observed_sources = accumulator
            .observed_sources
            .checked_add(1)
            .context("observed source count overflow")?;
        accumulator.admitted_sources = accumulator
            .admitted_sources
            .checked_add(1)
            .context("admitted source count overflow")?;
        if let Some(status) = flux.applicability_status {
            let count = self.ultraviolet_applicability.entry(status).or_default();
            *count = count
                .checked_add(1)
                .context("UV applicability count overflow")?;
        }
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

    /// Validate internal geometry, exact numeric state, and source accounting.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SHARD_SCHEMA_VERSION {
            bail!("unsupported Starlight shard schema {}", self.schema_version);
        }
        if (self.product_band == StarlightProductBand::Combined300To650)
            != self.ultraviolet_correction.is_some()
        {
            bail!("Starlight shard product band and UV metadata disagree");
        }
        validate_nside(self.nside)?;
        let npix = 12_u64 * u64::from(self.nside) * u64::from(self.nside);
        let mut excluded = 0_u64;
        for (pixel, accumulator) in &self.pixels {
            if u64::from(*pixel) >= npix {
                bail!("pixel {pixel} is outside nside={}", self.nside);
            }
            accumulator.validate()?;
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
        let uv_total = self
            .ultraviolet_applicability
            .values()
            .try_fold(0_u64, |total, count| total.checked_add(*count))
            .context("UV applicability count overflow")?;
        let admitted = self
            .pixels
            .values()
            .try_fold(0_u64, |total, pixel| {
                total.checked_add(pixel.admitted_sources)
            })
            .context("admitted source total overflow")?;
        if (self.ultraviolet_correction.is_some() && uv_total != admitted)
            || (self.ultraviolet_correction.is_none() && uv_total != 0)
        {
            bail!("UV applicability diagnostics do not match admitted sources");
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
    let product_band = shards[0].product_band;
    let ultraviolet_correction = shards[0].ultraviolet_correction.clone();
    let mut merged = PartitionShard::new_with_policy(
        "merged",
        nside,
        product_band,
        ultraviolet_correction.clone(),
    )?;
    for shard in shards {
        if shard.nside != nside {
            bail!("cannot merge Starlight shards with different nside values");
        }
        if shard.product_band != product_band
            || shard.ultraviolet_correction != ultraviolet_correction
        {
            bail!("cannot merge Starlight shards with different spectral policies");
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
        for (status, count) in shard.ultraviolet_applicability {
            let merged_count = merged.ultraviolet_applicability.entry(status).or_default();
            *merged_count = merged_count
                .checked_add(count)
                .context("merged UV applicability count overflow")?;
        }
    }
    merged.validate()?;
    Ok(merged)
}

#[derive(Debug, Clone, Copy)]
struct SourceFluxComponents {
    flux_300_336_ph_m2_s: f64,
    flux_336_650_ph_m2_s: f64,
    flux_300_650_ph_m2_s: f64,
    statistical_uncertainty_300_336_ph_m2_s: f64,
    statistical_uncertainty_336_650_ph_m2_s: f64,
    statistical_uncertainty_300_650_ph_m2_s: f64,
    systematic_uncertainty_300_336_ph_m2_s: f64,
    systematic_uncertainty_300_650_ph_m2_s: f64,
    systematic_correlation: SystematicCorrelation,
    applicability_status: Option<ApplicabilityStatus>,
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
    fn exact_sum_is_independent_of_reduction_tree() -> Result<()> {
        let mut large = StableSum::default();
        large.add(1.0e16)?;
        let mut one = StableSum::default();
        one.add(1.0)?;
        let mut another_one = StableSum::default();
        another_one.add(1.0)?;

        let mut sequential = StableSum::default();
        sequential.merge(&large)?;
        sequential.merge(&one)?;
        sequential.merge(&another_one)?;

        let mut partial = one.clone();
        partial.merge(&another_one)?;
        let mut grouped = large;
        grouped.merge(&partial)?;

        assert_eq!(sequential, grouped);
        assert_eq!(sequential.value().to_bits(), grouped.value().to_bits());
        assert_eq!(sequential.value(), 10_000_000_000_000_002.0);
        Ok(())
    }

    #[test]
    fn exact_sum_rounds_halfway_values_to_even() -> Result<()> {
        let half_ulp = 2.0_f64.powi(-53);
        let mut sum = StableSum::default();
        sum.add(1.0)?;
        sum.add(half_ulp)?;
        assert_eq!(sum.value(), 1.0);

        sum.add(half_ulp)?;
        assert_eq!(sum.value(), f64::from_bits(1.0_f64.to_bits() + 1));
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

    #[test]
    fn rejects_ambiguous_pre_uv_shard_schema() -> Result<()> {
        let shard = PartitionShard::new("schema-test", 128)?;
        let mut json = serde_json::to_value(shard)?;
        json["schema_version"] = serde_json::json!(2);
        let old: PartitionShard = serde_json::from_value(json)?;
        assert!(old.validate().is_err());
        Ok(())
    }
}
