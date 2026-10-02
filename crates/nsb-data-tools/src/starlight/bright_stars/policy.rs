//! Versioned bright-star supplement policies.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Population boundary policy id (design-v1).
pub const POPULATION_POLICY_ID_V1: &str = "bright-stars-population-v1";
/// Gaia precedence / replacement policy id (design-v1).
pub const PRECEDENCE_POLICY_ID_V1: &str = "bright-stars-gaia-precedence-v1";

/// How a supplement source relates to the Gaia primary map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplementClass {
    /// No secure Gaia match; admit supplement flux.
    SupplementOnly,
    /// Gaia match exists but fails the bright-end quality gate; replace Gaia.
    MatchedAndReplacesPrimary,
    /// Gaia match is reliable; discard supplement (no double counting).
    MatchedAndRejectedAsDuplicate,
    /// Ambiguous identity; excluded from v1 maps.
    AmbiguousManualReview,
}

/// Completeness-motivated population cut (not residual-tuned).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarPopulationPolicy {
    pub policy_id: String,
    /// Hipparcos Hp threshold (inclusive).
    pub hp_max: f64,
    /// Tycho V_T threshold when Hp is absent (inclusive).
    pub vt_max: f64,
    /// Gaia G threshold used only with an additional quality failure.
    pub gaia_g_max_with_quality_failure: f64,
    pub catalogue_epoch: String,
    pub target_epoch: String,
}

impl BrightStarPopulationPolicy {
    pub fn v1() -> Self {
        Self {
            policy_id: POPULATION_POLICY_ID_V1.to_string(),
            hp_max: 4.0,
            vt_max: 4.0,
            gaia_g_max_with_quality_failure: 3.0,
            catalogue_epoch: "J1991.25".to_string(),
            target_epoch: "J2016.0".to_string(),
        }
    }

    /// Whether a source magnitude meets the v1 bright cut.
    pub fn admits_hp_or_vt(&self, hp: Option<f64>, vt: Option<f64>) -> bool {
        if let Some(hp) = hp {
            if hp.is_finite() && hp <= self.hp_max {
                return true;
            }
        }
        if hp.is_none() {
            if let Some(vt) = vt {
                if vt.is_finite() && vt <= self.vt_max {
                    return true;
                }
            }
        }
        false
    }

    pub fn validate(&self) -> Result<()> {
        if self.policy_id != POPULATION_POLICY_ID_V1
            || !self.hp_max.is_finite()
            || !self.vt_max.is_finite()
            || !self.gaia_g_max_with_quality_failure.is_finite()
            || self.catalogue_epoch != "J1991.25"
            || self.target_epoch != "J2016.0"
        {
            bail!("unknown or invalid bright-star population policy");
        }
        Ok(())
    }
}

/// When to replace Gaia with supplement flux.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarPrecedencePolicy {
    pub policy_id: String,
    /// Prefer Gaia XP when calibrated and quality gate passes.
    pub prefer_valid_gaia_xp: bool,
    /// Replace Gaia when missing from DR3.
    pub replace_when_missing_from_gaia: bool,
    /// Replace when Gaia G is brighter than this and XP/photometry fails.
    pub replace_when_gaia_g_brighter_than: f64,
    /// Maximum 2D-vs-3D propagation difference as a fraction of the
    /// effective source matching radius. Above this, identity fails closed.
    pub perspective_motion_max_fraction_of_match_radius: f64,
}

impl BrightStarPrecedencePolicy {
    pub fn v1() -> Self {
        Self {
            policy_id: PRECEDENCE_POLICY_ID_V1.to_string(),
            prefer_valid_gaia_xp: true,
            replace_when_missing_from_gaia: true,
            replace_when_gaia_g_brighter_than: 3.0,
            perspective_motion_max_fraction_of_match_radius: 0.1,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.policy_id != PRECEDENCE_POLICY_ID_V1
            || !self.prefer_valid_gaia_xp
            || !self.replace_when_missing_from_gaia
            || !self.replace_when_gaia_g_brighter_than.is_finite()
            || !self
                .perspective_motion_max_fraction_of_match_radius
                .is_finite()
            || !(0.0..1.0).contains(&self.perspective_motion_max_fraction_of_match_radius)
        {
            bail!("unknown or invalid bright-star precedence policy");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn population_v1_admits_hp_at_floor() {
        let p = BrightStarPopulationPolicy::v1();
        assert!(p.admits_hp_or_vt(Some(4.0), None));
        assert!(!p.admits_hp_or_vt(Some(4.01), None));
        assert!(p.admits_hp_or_vt(None, Some(3.5)));
        assert!(!p.admits_hp_or_vt(None, Some(4.5)));
        // Hp present and faint wins over bright Vt (Hp is authoritative).
        assert!(!p.admits_hp_or_vt(Some(5.0), Some(2.0)));
    }
}
