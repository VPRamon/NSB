//! Deterministic sparse accumulators for independently produced Starlight shards.

use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::StarlightProductBand;
use crate::starlight::healpix::{self, galactic_nested_pixel_from_icrs_position, IcrsSkyPosition};
use crate::starlight::uv::{
    ApplicabilityStatus, CalibrationStatus, CombinedBandFlux, ModelResponse, SystematicCorrelation,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use siderust::healpix::HealpixIndex;
use std::collections::BTreeMap;
use std::path::Path;

const SHARD_SCHEMA_VERSION: u32 = 3;
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
    /// Accumulate one finite non-negative value. Order-independent.
    pub(crate) fn add(&mut self, value: f64) -> Result<()> {
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

    pub(crate) fn merge(&mut self, other: &Self) -> Result<()> {
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
    /// Named catalogue/system correlation groups. Values add linearly within
    /// a group and distinct groups combine in quadrature.
    #[serde(default)]
    pub systematic_correlated_groups: BTreeMap<String, StableSum>,
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
        for (group, value) in &other.systematic_correlated_groups {
            self.systematic_correlated_groups
                .entry(group.clone())
                .or_default()
                .merge(value)?;
        }
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
            .and_then(|_| {
                for (group, value) in &self.systematic_correlated_groups {
                    if group.trim().is_empty() {
                        bail!("systematic correlation group id must not be empty");
                    }
                    value.validate()?;
                }
                Ok(())
            })
    }

    pub(crate) fn selected_systematic_uncertainty(&self) -> f64 {
        let grouped_variance: f64 = self
            .systematic_correlated_groups
            .values()
            .map(|sum| sum.value().powi(2))
            .sum();
        self.systematic_variance
            .value()
            .sqrt()
            .hypot(self.systematic_correlated_uncertainty.value())
            .hypot(grouped_variance.sqrt())
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
    pub measured_conditional_residual_statistical_correlation_bits: u64,
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
    /// Replacement identities declared by the synthetic bright-star shard.
    #[serde(default)]
    pub bright_star_replacement_gaia_ids: std::collections::BTreeSet<u64>,
    /// Exact Gaia identities actually excluded by primary-source workers.
    #[serde(default)]
    pub bright_star_suppressed_gaia_ids: std::collections::BTreeSet<u64>,
    /// Replacement identities that nevertheless reached Gaia admission.
    #[serde(default)]
    pub bright_star_base_admitted_replacement_gaia_ids: std::collections::BTreeSet<u64>,
    /// Checksum-verified bright-star supplement identity, when present.
    #[serde(default)]
    pub bright_star_supplement_provenance:
        Option<crate::starlight::bright_stars::BrightStarSupplementProvenance>,
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
        healpix::gaia_nested_nside(nside)?;
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
            let measured_residual_correlation =
                f64::from_bits(metadata.measured_conditional_residual_statistical_correlation_bits);
            if metadata.model_id.trim().is_empty()
                || !is_sha256(&metadata.artifact_sha256)
                || metadata.calibration_status != CalibrationStatus::Validated
                || metadata.response.validate().is_err()
                || !measured_residual_correlation.is_finite()
                || !(-1.0..=1.0).contains(&measured_residual_correlation)
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
            bright_star_replacement_gaia_ids: std::collections::BTreeSet::new(),
            bright_star_suppressed_gaia_ids: std::collections::BTreeSet::new(),
            bright_star_base_admitted_replacement_gaia_ids: std::collections::BTreeSet::new(),
            bright_star_supplement_provenance: None,
        })
    }

    /// Create the synthetic combined-band bright-star shard. Its UV component
    /// is CK04-derived and must not impersonate the Gaia UV correction route.
    pub(crate) fn new_bright_star_combined(
        partition_id: impl Into<String>,
        nside: u32,
        provenance: crate::starlight::bright_stars::BrightStarSupplementProvenance,
    ) -> Result<Self> {
        provenance.validate()?;
        if provenance.product_band
            != crate::starlight::bright_stars::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID
        {
            bail!("combined bright-star shard requires combined supplement provenance");
        }
        let mut shard = Self::new_with_policy(
            partition_id,
            nside,
            StarlightProductBand::Measured336To650,
            None,
        )?;
        shard.product_band = StarlightProductBand::Combined300To650;
        shard.bright_star_supplement_provenance = Some(provenance);
        Ok(shard)
    }

    /// Accumulate one admitted Gaia source at its ICRS sky position.
    pub fn admit(
        &mut self,
        position: IcrsSkyPosition,
        flux_ph_m2_s: f64,
        statistical_uncertainty: f64,
        systematic_uncertainty: f64,
    ) -> Result<()> {
        self.admit_components(
            position,
            SourceFluxComponents {
                flux_300_336_ph_m2_s: 0.0,
                flux_336_650_ph_m2_s: flux_ph_m2_s,
                flux_300_650_ph_m2_s: flux_ph_m2_s,
                statistical_uncertainty_300_336_ph_m2_s: 0.0,
                statistical_uncertainty_336_650_ph_m2_s: statistical_uncertainty,
                statistical_uncertainty_300_650_ph_m2_s: statistical_uncertainty,
                // Measured-only products have no UV 300–336 contribution; keep the
                // UV systematic buckets at zero and file photometric/selection
                // systematics only into the selected / 300–650 systematic path.
                systematic_uncertainty_300_336_ph_m2_s: 0.0,
                systematic_uncertainty_300_650_ph_m2_s: systematic_uncertainty,
                systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
                applicability_status: None,
            },
        )
    }

    /// Admit one bright-star source while preserving named catalogue
    /// correlation groups. Measured-only supplements may enter measured shards;
    /// Combined300To650 supplements may enter combined shards with an explicit
    /// justified 300--336 nm term. Cross-band mixing fails closed.
    pub fn admit_bright_star_source(
        &mut self,
        source: &crate::starlight::bright_stars::BrightStarSourceRecord,
    ) -> Result<()> {
        use crate::starlight::config::StarlightProductBand;
        use crate::starlight::uv::SystematicCorrelation;

        match self.product_band {
            StarlightProductBand::Measured336To650 => {
                if source.flux_300_336_ph_m2_s.is_some() {
                    bail!("combined bright-star supplement cannot enter a measured-only shard");
                }
                let position = IcrsSkyPosition::new(source.ra_deg_j2016, source.dec_deg_j2016)?;
                self.admit(
                    position,
                    source.flux_336_650_ph_m2_s,
                    source.statistical_uncertainty_ph_m2_s,
                    source.systematic_independent_uncertainty_ph_m2_s,
                )?;
            }
            StarlightProductBand::Combined300To650 => {
                if source.flux_300_336_ph_m2_s.is_none() {
                    bail!("measured-only bright-star supplement cannot enter a combined 300-650 shard");
                }
                let uv = source
                    .flux_300_336_ph_m2_s
                    .context("combined bright-star source missing 300-336 nm flux")?;
                let uv_stat = source.statistical_uncertainty_300_336_ph_m2_s.context(
                    "combined bright-star source missing 300-336 nm statistical uncertainty",
                )?;
                let uv_sys = source
                    .systematic_independent_uncertainty_300_336_ph_m2_s
                    .context(
                        "combined bright-star source missing 300-336 nm independent systematic",
                    )?;
                let measured = source.flux_336_650_ph_m2_s;
                let measured_stat = source.statistical_uncertainty_ph_m2_s;
                let measured_sys = source.systematic_independent_uncertainty_ph_m2_s;
                let position = IcrsSkyPosition::new(source.ra_deg_j2016, source.dec_deg_j2016)?;
                self.admit_components(
                    position,
                    SourceFluxComponents {
                        flux_300_336_ph_m2_s: uv,
                        flux_336_650_ph_m2_s: measured,
                        flux_300_650_ph_m2_s: uv + measured,
                        statistical_uncertainty_300_336_ph_m2_s: uv_stat,
                        statistical_uncertainty_336_650_ph_m2_s: measured_stat,
                        statistical_uncertainty_300_650_ph_m2_s: uv_stat + measured_stat,
                        systematic_uncertainty_300_336_ph_m2_s: uv_sys,
                        systematic_uncertainty_300_650_ph_m2_s: uv_sys + measured_sys,
                        systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
                        // Gaia UV applicability does not describe the distinct
                        // Hp-scaled CK04 bright-star completion route.
                        applicability_status: None,
                    },
                )?;
            }
        }
        let pixel = galactic_nested_pixel_from_icrs_position(
            source.ra_deg_j2016,
            source.dec_deg_j2016,
            self.nside,
        )?;
        let accumulator = self
            .pixels
            .get_mut(&pixel)
            .context("admitted bright-star pixel missing")?;
        for term in &source.systematic_catalogue_correlated {
            accumulator
                .systematic_correlated_groups
                .entry(term.correlation_group_id.clone())
                .or_default()
                .add(term.uncertainty_ph_m2_s)?;
        }
        Ok(())
    }

    /// Accumulate an explicitly separated corrected source.
    pub fn admit_corrected(
        &mut self,
        position: IcrsSkyPosition,
        flux: &CombinedBandFlux,
    ) -> Result<()> {
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
            position,
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

    fn admit_components(
        &mut self,
        position: IcrsSkyPosition,
        flux: SourceFluxComponents,
    ) -> Result<()> {
        let selected_flux = match self.product_band {
            StarlightProductBand::Measured336To650 => flux.flux_336_650_ph_m2_s,
            StarlightProductBand::Combined300To650 => flux.flux_300_650_ph_m2_s,
        };
        let selected_statistical = match self.product_band {
            StarlightProductBand::Measured336To650 => flux.statistical_uncertainty_336_650_ph_m2_s,
            StarlightProductBand::Combined300To650 => flux.statistical_uncertainty_300_650_ph_m2_s,
        };
        // Measured-band callers pass photometric/selection systematics through
        // admit(); combined-band callers supply the full 300–650 systematic.
        let selected_systematic = flux.systematic_uncertainty_300_650_ph_m2_s;
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
        let pixel = galactic_nested_pixel_from_icrs_position(
            position.ra_deg,
            position.dec_deg,
            self.nside,
        )?;
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
    pub fn exclude(&mut self, position: IcrsSkyPosition, reason: &str) -> Result<()> {
        if reason.is_empty()
            || !reason
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            bail!("exclusion reason must be lowercase snake_case");
        }
        let pixel = galactic_nested_pixel_from_icrs_position(
            position.ra_deg,
            position.dec_deg,
            self.nside,
        )?;
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
        if self.product_band == StarlightProductBand::Measured336To650
            && self.ultraviolet_correction.is_some()
        {
            bail!("measured Starlight shard must not carry Gaia UV metadata");
        }
        if self.product_band == StarlightProductBand::Combined300To650
            && self.ultraviolet_correction.is_none()
            && self.bright_star_supplement_provenance.is_none()
        {
            bail!("combined Starlight shard requires a declared UV reconstruction route");
        }
        healpix::gaia_nested_nside(self.nside)?;
        let grid = healpix::gaia_nested_grid(self.nside)?;
        let mut excluded = 0_u64;
        for (pixel, accumulator) in &self.pixels {
            grid.validate_index(HealpixIndex::new(u64::from(*pixel)))
                .with_context(|| format!("pixel {pixel} is outside nside={}", self.nside))?;
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
        let suppressed_count = self
            .exclusion_reasons
            .get("bright_star_replaced_by_supplement")
            .copied()
            .unwrap_or_default();
        if suppressed_count != self.bright_star_suppressed_gaia_ids.len() as u64 {
            bail!("bright-star suppressed Gaia identities do not match exclusion accounting");
        }
        if let Some(provenance) = &self.bright_star_supplement_provenance {
            provenance.validate()?;
        }
        if !self.bright_star_replacement_gaia_ids.is_empty()
            && self.bright_star_supplement_provenance.is_none()
        {
            bail!("bright-star replacement identities require checksum-verified supplement provenance");
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
        let has_supplement = self.bright_star_supplement_provenance.is_some();
        if (self.ultraviolet_correction.is_some()
            && ((!has_supplement && uv_total != admitted)
                || (has_supplement && uv_total > admitted)))
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
    let ultraviolet_correction = shards
        .iter()
        .find_map(|shard| shard.ultraviolet_correction.clone());
    let mut merged = if product_band == StarlightProductBand::Combined300To650
        && ultraviolet_correction.is_none()
    {
        let provenance = shards
            .iter()
            .find_map(|shard| shard.bright_star_supplement_provenance.clone())
            .context("combined shards without Gaia UV metadata require bright-star provenance")?;
        PartitionShard::new_bright_star_combined("merged", nside, provenance)?
    } else {
        PartitionShard::new_with_policy(
            "merged",
            nside,
            product_band,
            ultraviolet_correction.clone(),
        )?
    };
    for shard in shards {
        if shard.nside != nside {
            bail!("cannot merge Starlight shards with different nside values");
        }
        if shard.product_band != product_band {
            bail!("cannot merge Starlight shards with different product bands");
        }
        if let Some(route) = &shard.ultraviolet_correction {
            if Some(route) != ultraviolet_correction.as_ref() {
                bail!(
                    "cannot merge Starlight shards with incompatible Gaia UV correction identities"
                );
            }
        } else if product_band == StarlightProductBand::Combined300To650
            && shard.bright_star_supplement_provenance.is_none()
        {
            bail!("combined shard without Gaia UV metadata requires bright-star route provenance");
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
        for gaia_source_id in shard.bright_star_replacement_gaia_ids {
            if !merged
                .bright_star_replacement_gaia_ids
                .insert(gaia_source_id)
            {
                bail!("duplicate bright-star replacement Gaia source id across shards");
            }
        }
        for gaia_source_id in shard.bright_star_suppressed_gaia_ids {
            if !merged
                .bright_star_suppressed_gaia_ids
                .insert(gaia_source_id)
            {
                bail!("duplicate suppressed Gaia source id across shards");
            }
        }
        merged
            .bright_star_base_admitted_replacement_gaia_ids
            .extend(shard.bright_star_base_admitted_replacement_gaia_ids);
        match (
            merged.bright_star_supplement_provenance.as_ref(),
            shard.bright_star_supplement_provenance,
        ) {
            (_, None) => {}
            (None, Some(provenance)) => {
                merged.bright_star_supplement_provenance = Some(provenance);
            }
            (Some(existing), Some(provenance)) if existing == &provenance => {}
            (Some(_), Some(_)) => {
                bail!("cannot merge Starlight shards with incompatible bright-star supplement provenance");
            }
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

/// Galactic nested accumulation pixel for a Gaia ICRS sky position.
pub fn galactic_accumulation_pixel(position: IcrsSkyPosition, target_nside: u32) -> Result<u32> {
    galactic_nested_pixel_from_icrs_position(position.ra_deg, position.dec_deg, target_nside)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::healpix::test_support::fixture_icrs_from_source_id;

    #[test]
    fn galactic_pixel_differs_from_equatorial_bit_shift() -> Result<()> {
        let level_12_pixel = 123_456_u64;
        let source_id = (level_12_pixel << healpix::GAIA_SOURCE_ID_HEALPIX_SHIFT) | 17;
        let equatorial = healpix::gaia_source_id_equatorial_nested_pixel(source_id, 128)?;
        let position = fixture_icrs_from_source_id(source_id);
        let galactic = galactic_accumulation_pixel(position, 128)?;
        assert_ne!(equatorial, galactic);
        Ok(())
    }

    #[test]
    fn merge_bytes_are_independent_of_completion_order() -> Result<()> {
        let mut first = PartitionShard::new("000-099", 128)?;
        first.admit(fixture_icrs_from_source_id(0), 1.0, 0.1, 0.2)?;
        first.exclude(fixture_icrs_from_source_id(1_u64 << 35), "invalid_flux")?;
        let mut second = PartitionShard::new("100-199", 128)?;
        second.admit(fixture_icrs_from_source_id(2_u64 << 35), 3.0, 0.3, 0.4)?;

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

    fn fixture_uv_metadata(fill: char) -> UvCorrectionShardMetadata {
        UvCorrectionShardMetadata {
            model_id: "gaia-uv-fixture".into(),
            artifact_sha256: fill.to_string().repeat(64),
            calibration_status: CalibrationStatus::Validated,
            response: ModelResponse::AbsoluteUvPhotonFlux,
            measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
            systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
        }
    }

    fn fixture_combined_bright_star_provenance(
    ) -> crate::starlight::bright_stars::BrightStarSupplementProvenance {
        crate::starlight::bright_stars::BrightStarSupplementProvenance {
            artifact_sha256: "b".repeat(64),
            model_id: crate::starlight::bright_stars::BRIGHT_STAR_COMBINED_MODEL_ID.into(),
            product_band:
                crate::starlight::bright_stars::BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID.into(),
            uv_completion_model_id: Some(
                crate::starlight::bright_stars::UV_COMPLETION_MODEL_ID_V1.into(),
            ),
            population_policy_id: crate::starlight::bright_stars::POPULATION_POLICY_ID_V1.into(),
            precedence_policy_id: crate::starlight::bright_stars::PRECEDENCE_POLICY_ID_V1.into(),
            build_commit: "a".repeat(40),
            spectral_route: "Hipparcos/XHIP-selected CK04; Hp-scaled CK04 336-650 nm plus Hp-scaled CK04 300-336 nm completion".into(),
            redistribution_scope:
                "derived map only; source catalogue bytes are not embedded".into(),
            inputs: Vec::new(),
        }
    }

    #[test]
    fn mixed_gaia_and_bright_star_uv_routes_merge_without_losing_identity() -> Result<()> {
        let gaia = PartitionShard::new_with_policy(
            "gaia",
            128,
            StarlightProductBand::Combined300To650,
            Some(fixture_uv_metadata('a')),
        )?;
        let bright = PartitionShard::new_bright_star_combined(
            "bright-star-supplement",
            128,
            fixture_combined_bright_star_provenance(),
        )?;
        let merged = merge_shards([gaia, bright])?;
        assert_eq!(
            merged.ultraviolet_correction,
            Some(fixture_uv_metadata('a'))
        );
        assert_eq!(
            merged
                .bright_star_supplement_provenance
                .as_ref()
                .unwrap()
                .uv_completion_model_id
                .as_deref(),
            Some(crate::starlight::bright_stars::UV_COMPLETION_MODEL_ID_V1)
        );

        let incompatible = PartitionShard::new_with_policy(
            "other-gaia",
            128,
            StarlightProductBand::Combined300To650,
            Some(fixture_uv_metadata('c')),
        )?;
        assert!(merge_shards([merged.clone(), incompatible]).is_err());
        assert!(merge_shards([merged, PartitionShard::new("measured", 128)?]).is_err());
        Ok(())
    }
}
