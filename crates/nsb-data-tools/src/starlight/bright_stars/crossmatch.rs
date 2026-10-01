//! Deterministic Gaia crossmatch classification for bright-star supplements.

use super::policy::{BrightStarPrecedencePolicy, SupplementClass};
use serde::{Deserialize, Serialize};

/// One positional / identifier match candidate against Gaia DR3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchCandidate {
    pub gaia_source_id: u64,
    /// Angular separation after proper-motion propagation (arcsec).
    pub separation_arcsec: f64,
    pub gaia_g_mag: Option<f64>,
    pub gaia_xp_usable: bool,
    pub gaia_photometric_usable: bool,
}

/// Result of applying the v1 precedence policy to zero-or-more matches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossmatchDecision {
    pub class: SupplementClass,
    pub gaia_source_id: Option<u64>,
    pub reason: String,
}

/// Classify a supplement source given its Gaia match set.
///
/// Fail-closed on ambiguity: two or more candidates inside the match radius
/// without a unique identifier prefer → `AmbiguousManualReview`.
pub fn classify_match(
    policy: &BrightStarPrecedencePolicy,
    matches: &[MatchCandidate],
    match_radius_arcsec: f64,
) -> CrossmatchDecision {
    let within: Vec<&MatchCandidate> = matches
        .iter()
        .filter(|m| m.separation_arcsec.is_finite() && m.separation_arcsec <= match_radius_arcsec)
        .collect();

    if within.is_empty() {
        if policy.replace_when_missing_from_gaia {
            return CrossmatchDecision {
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                reason: "no_gaia_match_within_radius".to_string(),
            };
        }
        return CrossmatchDecision {
            class: SupplementClass::AmbiguousManualReview,
            gaia_source_id: None,
            reason: "no_gaia_match_and_replace_disabled".to_string(),
        };
    }

    if within.len() > 1 {
        return CrossmatchDecision {
            class: SupplementClass::AmbiguousManualReview,
            gaia_source_id: None,
            reason: "multiple_gaia_matches".to_string(),
        };
    }

    let m = within[0];
    let gaia_usable = m.gaia_xp_usable || m.gaia_photometric_usable;
    let bright = m
        .gaia_g_mag
        .is_some_and(|g| g.is_finite() && g <= policy.replace_when_gaia_g_brighter_than);

    if policy.prefer_valid_gaia_xp && gaia_usable && !bright {
        return CrossmatchDecision {
            class: SupplementClass::MatchedAndRejectedAsDuplicate,
            gaia_source_id: Some(m.gaia_source_id),
            reason: "gaia_reliable_retain_primary".to_string(),
        };
    }

    if policy.prefer_valid_gaia_xp && gaia_usable && bright {
        // Bright but usable Gaia: still prefer Gaia unless XP/photometry failed.
        return CrossmatchDecision {
            class: SupplementClass::MatchedAndRejectedAsDuplicate,
            gaia_source_id: Some(m.gaia_source_id),
            reason: "gaia_bright_but_usable_retain_primary".to_string(),
        };
    }

    // Matched Gaia that fails quality / has no usable spectrum or photometry.
    CrossmatchDecision {
        class: SupplementClass::MatchedAndReplacesPrimary,
        gaia_source_id: Some(m.gaia_source_id),
        reason: if bright {
            "gaia_bright_and_unusable_replace".to_string()
        } else {
            "gaia_unusable_replace".to_string()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight::bright_stars::policy::BrightStarPrecedencePolicy;

    fn cand(id: u64, sep: f64, g: Option<f64>, xp: bool, phot: bool) -> MatchCandidate {
        MatchCandidate {
            gaia_source_id: id,
            separation_arcsec: sep,
            gaia_g_mag: g,
            gaia_xp_usable: xp,
            gaia_photometric_usable: phot,
        }
    }

    #[test]
    fn no_match_is_supplement_only() {
        let d = classify_match(&BrightStarPrecedencePolicy::v1(), &[], 1.0);
        assert_eq!(d.class, SupplementClass::SupplementOnly);
    }

    #[test]
    fn ambiguous_multiple_matches() {
        let matches = vec![
            cand(1, 0.1, Some(5.0), true, true),
            cand(2, 0.2, Some(5.0), true, true),
        ];
        let d = classify_match(&BrightStarPrecedencePolicy::v1(), &matches, 1.0);
        assert_eq!(d.class, SupplementClass::AmbiguousManualReview);
    }

    #[test]
    fn reliable_gaia_rejected_as_duplicate() {
        let matches = vec![cand(42, 0.05, Some(6.0), true, false)];
        let d = classify_match(&BrightStarPrecedencePolicy::v1(), &matches, 1.0);
        assert_eq!(d.class, SupplementClass::MatchedAndRejectedAsDuplicate);
        assert_eq!(d.gaia_source_id, Some(42));
    }

    #[test]
    fn unusable_bright_gaia_is_replaced() {
        let matches = vec![cand(7, 0.1, Some(2.5), false, false)];
        let d = classify_match(&BrightStarPrecedencePolicy::v1(), &matches, 1.0);
        assert_eq!(d.class, SupplementClass::MatchedAndReplacesPrimary);
    }

    #[test]
    fn outside_radius_counts_as_no_match() {
        let matches = vec![cand(1, 5.0, Some(2.0), false, false)];
        let d = classify_match(&BrightStarPrecedencePolicy::v1(), &matches, 1.0);
        assert_eq!(d.class, SupplementClass::SupplementOnly);
    }
}
