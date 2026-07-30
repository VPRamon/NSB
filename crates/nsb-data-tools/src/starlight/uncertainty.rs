//! Correlation-scope vocabulary for Starlight uncertainty terms.
//!
//! [`super::uv::SystematicCorrelation`] encodes exactly the two source-to-source
//! correlation regimes needed by the UV correction contract today
//! (`IndependentBetweenSources` and `FullyCorrelatedBetweenSources`).
//! [`CorrelationScope`] is a broader vocabulary for describing *where in space*
//! (as opposed to *between which sources*) a systematic term's correlation
//! lives, for use in diagnostics and scientific sidecars that need to reason
//! about pixel-, region-, and sky-wide correlation without conflating it with
//! the narrower per-artifact `SystematicCorrelation` contract.
//!
//! The two vocabularies are related but not identical. Today's mapping from
//! the existing artifact-level contract is:
//!
//! - [`SystematicCorrelation::IndependentBetweenSources`] maps to
//!   [`CorrelationScope::IndependentSource`]: the term is drawn independently
//!   per admitted source, so it partially cancels both within a pixel (as
//!   `sqrt(sum(sigma_i^2))`) and across pixels.
//! - [`SystematicCorrelation::FullyCorrelatedBetweenSources`] maps to
//!   [`CorrelationScope::GlobalCorrelated`]: today's only fully-correlated
//!   systematic (the UV 300-336 nm correction model bias) applies identically
//!   to every admitted source regardless of sky position, so it does not
//!   cancel within a pixel (`sum(sigma_i)`, see
//!   [`super::map::accumulator::PixelAccumulator`]) *or* across pixels (see
//!   `PartitionShard::merge`, which sums the per-pixel correlated
//!   accumulators linearly rather than in quadrature).
//!
//! [`CorrelationScope::PixelCorrelated`] and [`CorrelationScope::RegionCorrelated`]
//! are not produced by any artifact contract yet; they are reserved for future
//! systematics whose correlation length is smaller than the whole sky (e.g. a
//! per-HEALPix-pixel calibration offset, or a per-sky-region selection-function
//! bias) so that diagnostics code does not need another schema-breaking change
//! when such a term is introduced.
//!
//! [`SystematicCorrelation::IndependentBetweenSources`]: super::uv::SystematicCorrelation::IndependentBetweenSources
//! [`SystematicCorrelation::FullyCorrelatedBetweenSources`]: super::uv::SystematicCorrelation::FullyCorrelatedBetweenSources

use serde::{Deserialize, Serialize};

use super::uv::SystematicCorrelation;

/// Spatial/population extent over which an uncertainty term's error is shared.
///
/// Ordered from the smallest to the largest correlation length. This is a
/// diagnostic vocabulary: it does not change how any accumulator combines
/// values, it only names the regime a given `SystematicCorrelation` (or a
/// future, more granular term) belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CorrelationScope {
    /// Drawn independently per admitted source; combines in quadrature both
    /// within a pixel and across pixels.
    IndependentSource,
    /// Shared by every source that lands in the same HEALPix pixel, but
    /// independent across pixels. No current artifact contract produces this
    /// scope; reserved for future per-pixel systematics.
    PixelCorrelated,
    /// Shared by every source within a declared sky region (for example one
    /// selection-function calibration tile), but independent across regions.
    /// No current artifact contract produces this scope; reserved for future
    /// region-conditioned systematics.
    RegionCorrelated,
    /// Shared by every admitted source in the entire candidate regardless of
    /// sky position; combines linearly (never in quadrature), both within a
    /// pixel and when pixels are merged into a global total.
    GlobalCorrelated,
}

impl CorrelationScope {
    /// Stable lowercase label matching the `kebab-case` serialization.
    pub const fn label(self) -> &'static str {
        match self {
            Self::IndependentSource => "independent-source",
            Self::PixelCorrelated => "pixel-correlated",
            Self::RegionCorrelated => "region-correlated",
            Self::GlobalCorrelated => "global-correlated",
        }
    }
}

impl From<SystematicCorrelation> for CorrelationScope {
    fn from(value: SystematicCorrelation) -> Self {
        match value {
            SystematicCorrelation::IndependentBetweenSources => Self::IndependentSource,
            SystematicCorrelation::FullyCorrelatedBetweenSources => Self::GlobalCorrelated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_existing_systematic_correlation_contract() {
        assert_eq!(
            CorrelationScope::from(SystematicCorrelation::IndependentBetweenSources),
            CorrelationScope::IndependentSource
        );
        assert_eq!(
            CorrelationScope::from(SystematicCorrelation::FullyCorrelatedBetweenSources),
            CorrelationScope::GlobalCorrelated
        );
    }

    #[test]
    fn serializes_to_stable_kebab_case_labels() {
        for scope in [
            CorrelationScope::IndependentSource,
            CorrelationScope::PixelCorrelated,
            CorrelationScope::RegionCorrelated,
            CorrelationScope::GlobalCorrelated,
        ] {
            let json = serde_json::to_string(&scope).unwrap();
            assert_eq!(json, format!("{:?}", scope.label()));
            let round_tripped: CorrelationScope = serde_json::from_str(&json).unwrap();
            assert_eq!(round_tripped, scope);
        }
    }
}
