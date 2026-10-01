//! Proper-motion-aware, deterministic Gaia crossmatch and classification.

use super::policy::{BrightStarPrecedencePolicy, SupplementClass};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const MAS_PER_DEGREE: f64 = 3_600_000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HipparcosAstrometry {
    pub hip: u32,
    pub ra_deg_j1991_25: f64,
    pub dec_deg_j1991_25: f64,
    /// Hipparcos convention: mu_alpha_star = d(alpha)/dt cos(delta), mas/yr.
    pub pm_ra_cosdec_mas_per_year: f64,
    pub pm_dec_mas_per_year: f64,
    pub parallax_mas: f64,
    pub position_uncertainty_mas: f64,
    pub proper_motion_uncertainty_mas_per_year: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropagatedPosition {
    pub ra_deg_j2016: f64,
    pub dec_deg_j2016: f64,
    pub angular_displacement_arcsec: f64,
    pub positional_uncertainty_arcsec: f64,
}

/// Propagate the full space-motion vector when a positive parallax and radial
/// velocity are available. This quantifies the perspective term relative to
/// the two-dimensional matching model.
pub fn propagate_hipparcos_3d_to_j2016(
    source: &HipparcosAstrometry,
    radial_velocity_km_s: f64,
) -> Result<PropagatedPosition> {
    let two_dimensional = propagate_hipparcos_to_j2016(source)?;
    if !source.parallax_mas.is_finite()
        || source.parallax_mas <= 0.0
        || !radial_velocity_km_s.is_finite()
    {
        bail!("3D propagation requires positive parallax and finite radial velocity");
    }
    const YEARS: f64 = 24.75;
    const MAS_TO_RAD: f64 = std::f64::consts::PI / (180.0 * 3_600_000.0);
    const KM_S_TO_PC_YR: f64 = 1.022_712_165_053_707_7e-6;
    let ra = source.ra_deg_j1991_25.to_radians();
    let dec = source.dec_deg_j1991_25.to_radians();
    let (sin_ra, cos_ra) = ra.sin_cos();
    let (sin_dec, cos_dec) = dec.sin_cos();
    let radial = [cos_dec * cos_ra, cos_dec * sin_ra, sin_dec];
    let alpha = [-sin_ra, cos_ra, 0.0];
    let delta = [-sin_dec * cos_ra, -sin_dec * sin_ra, cos_dec];
    let distance_pc = 1_000.0 / source.parallax_mas;
    let mu_alpha = source.pm_ra_cosdec_mas_per_year * MAS_TO_RAD;
    let mu_delta = source.pm_dec_mas_per_year * MAS_TO_RAD;
    let rv = radial_velocity_km_s * KM_S_TO_PC_YR;
    let mut position = [0.0; 3];
    for axis in 0..3 {
        let velocity =
            rv * radial[axis] + distance_pc * (mu_alpha * alpha[axis] + mu_delta * delta[axis]);
        position[axis] = distance_pc * radial[axis] + YEARS * velocity;
    }
    let norm = position
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        bail!("3D propagation produced an invalid position");
    }
    let unit = position.map(|value| value / norm);
    let ra_deg = unit[1].atan2(unit[0]).to_degrees().rem_euclid(360.0);
    let dec_deg = unit[2].asin().to_degrees();
    Ok(PropagatedPosition {
        ra_deg_j2016: ra_deg,
        dec_deg_j2016: dec_deg,
        angular_displacement_arcsec: angular_separation_arcsec(
            source.ra_deg_j1991_25,
            source.dec_deg_j1991_25,
            ra_deg,
            dec_deg,
        ),
        positional_uncertainty_arcsec: two_dimensional.positional_uncertainty_arcsec,
    })
}

pub fn propagation_2d_3d_difference_arcsec(
    source: &HipparcosAstrometry,
    radial_velocity_km_s: f64,
) -> Result<f64> {
    let two = propagate_hipparcos_to_j2016(source)?;
    let three = propagate_hipparcos_3d_to_j2016(source, radial_velocity_km_s)?;
    Ok(angular_separation_arcsec(
        two.ra_deg_j2016,
        two.dec_deg_j2016,
        three.ra_deg_j2016,
        three.dec_deg_j2016,
    ))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaiaMatchRow {
    pub gaia_source_id: u64,
    pub ra_deg_j2016: f64,
    pub dec_deg_j2016: f64,
    pub gaia_g_mag: Option<f64>,
    pub gaia_xp_usable: bool,
    pub gaia_photometric_usable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatchCandidate {
    pub gaia_source_id: u64,
    pub separation_arcsec: f64,
    pub gaia_g_mag: Option<f64>,
    pub gaia_xp_usable: bool,
    pub gaia_photometric_usable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossmatchDecision {
    pub class: SupplementClass,
    pub gaia_source_id: Option<u64>,
    pub reason: String,
}

/// Propagate J1991.25 Hipparcos astrometry to J2016.0 using the tangent-plane
/// two-dimensional proper motion. Perspective effects are deliberately not
/// hidden here; callers may compare against a 3D propagation diagnostic.
pub fn propagate_hipparcos_to_j2016(source: &HipparcosAstrometry) -> Result<PropagatedPosition> {
    let values = [
        source.ra_deg_j1991_25,
        source.dec_deg_j1991_25,
        source.pm_ra_cosdec_mas_per_year,
        source.pm_dec_mas_per_year,
        source.parallax_mas,
        source.position_uncertainty_mas,
        source.proper_motion_uncertainty_mas_per_year,
    ];
    if values.iter().any(|value| !value.is_finite())
        || !(0.0..360.0).contains(&source.ra_deg_j1991_25)
        || !(-90.0..=90.0).contains(&source.dec_deg_j1991_25)
        || source.position_uncertainty_mas < 0.0
        || source.proper_motion_uncertainty_mas_per_year < 0.0
    {
        bail!("invalid Hipparcos astrometry for HIP {}", source.hip);
    }
    const YEARS: f64 = 24.75;
    let dec_rad = source.dec_deg_j1991_25.to_radians();
    let cos_dec = dec_rad.cos();
    if cos_dec.abs() < 1.0e-12 {
        bail!("two-dimensional propagation is singular at the celestial pole");
    }
    let delta_ra_deg = source.pm_ra_cosdec_mas_per_year * YEARS / (MAS_PER_DEGREE * cos_dec);
    let delta_dec_deg = source.pm_dec_mas_per_year * YEARS / MAS_PER_DEGREE;
    let ra = (source.ra_deg_j1991_25 + delta_ra_deg).rem_euclid(360.0);
    let dec = source.dec_deg_j1991_25 + delta_dec_deg;
    if !(-90.0..=90.0).contains(&dec) {
        bail!("propagated declination is outside physical bounds");
    }
    let displacement = YEARS
        * source
            .pm_ra_cosdec_mas_per_year
            .hypot(source.pm_dec_mas_per_year)
        / 1000.0;
    let uncertainty = source
        .position_uncertainty_mas
        .hypot(YEARS * source.proper_motion_uncertainty_mas_per_year)
        / 1000.0;
    Ok(PropagatedPosition {
        ra_deg_j2016: ra,
        dec_deg_j2016: dec,
        angular_displacement_arcsec: displacement,
        positional_uncertainty_arcsec: uncertainty,
    })
}

/// Construct positional candidates after propagation. Official identity rows
/// should be supplied alone by the caller and therefore take precedence.
pub fn positional_match_candidates(
    position: &PropagatedPosition,
    gaia_rows: &[GaiaMatchRow],
    match_radius_arcsec: f64,
) -> Result<Vec<MatchCandidate>> {
    validate_radius(match_radius_arcsec)?;
    if !position.ra_deg_j2016.is_finite() || !position.dec_deg_j2016.is_finite() {
        bail!("propagated position is not finite");
    }
    let mut out = Vec::new();
    for row in gaia_rows {
        if !row.ra_deg_j2016.is_finite()
            || !row.dec_deg_j2016.is_finite()
            || !(0.0..360.0).contains(&row.ra_deg_j2016)
            || !(-90.0..=90.0).contains(&row.dec_deg_j2016)
            || row.gaia_g_mag.is_some_and(|g| !g.is_finite())
        {
            bail!("invalid Gaia crossmatch row {}", row.gaia_source_id);
        }
        let separation = angular_separation_arcsec(
            position.ra_deg_j2016,
            position.dec_deg_j2016,
            row.ra_deg_j2016,
            row.dec_deg_j2016,
        );
        if separation <= match_radius_arcsec {
            out.push(MatchCandidate {
                gaia_source_id: row.gaia_source_id,
                separation_arcsec: separation,
                gaia_g_mag: row.gaia_g_mag,
                gaia_xp_usable: row.gaia_xp_usable,
                gaia_photometric_usable: row.gaia_photometric_usable,
            });
        }
    }
    out.sort_by(|a, b| {
        a.separation_arcsec
            .total_cmp(&b.separation_arcsec)
            .then(a.gaia_source_id.cmp(&b.gaia_source_id))
    });
    Ok(out)
}

pub fn classify_match(
    policy: &BrightStarPrecedencePolicy,
    matches: &[MatchCandidate],
    match_radius_arcsec: f64,
) -> Result<CrossmatchDecision> {
    policy.validate()?;
    validate_radius(match_radius_arcsec)?;
    for candidate in matches {
        if !candidate.separation_arcsec.is_finite()
            || candidate.separation_arcsec < 0.0
            || candidate.gaia_g_mag.is_some_and(|g| !g.is_finite())
        {
            bail!("invalid Gaia match candidate {}", candidate.gaia_source_id);
        }
    }
    let mut within: Vec<&MatchCandidate> = matches
        .iter()
        .filter(|m| m.separation_arcsec <= match_radius_arcsec)
        .collect();
    within.sort_by(|a, b| {
        a.separation_arcsec
            .total_cmp(&b.separation_arcsec)
            .then(a.gaia_source_id.cmp(&b.gaia_source_id))
    });
    if within.is_empty() {
        return Ok(CrossmatchDecision {
            class: SupplementClass::SupplementOnly,
            gaia_source_id: None,
            reason: "no_gaia_match_after_j2016_propagation".into(),
        });
    }
    if within.len() > 1 {
        return Ok(CrossmatchDecision {
            class: SupplementClass::AmbiguousManualReview,
            gaia_source_id: None,
            reason: "multiple_gaia_matches".into(),
        });
    }
    let candidate = within[0];
    if candidate.gaia_xp_usable || candidate.gaia_photometric_usable {
        return Ok(CrossmatchDecision {
            class: SupplementClass::MatchedAndRejectedAsDuplicate,
            gaia_source_id: Some(candidate.gaia_source_id),
            reason: "gaia_reliable_retain_primary".into(),
        });
    }
    Ok(CrossmatchDecision {
        class: SupplementClass::MatchedAndReplacesPrimary,
        gaia_source_id: Some(candidate.gaia_source_id),
        reason: "gaia_unusable_replace".into(),
    })
}

fn validate_radius(radius: f64) -> Result<()> {
    if !radius.is_finite() || radius <= 0.0 {
        bail!("match_radius_arcsec must be finite and positive");
    }
    Ok(())
}

fn angular_separation_arcsec(ra1: f64, dec1: f64, ra2: f64, dec2: f64) -> f64 {
    let (ra1, dec1, ra2, dec2) = (
        ra1.to_radians(),
        dec1.to_radians(),
        ra2.to_radians(),
        dec2.to_radians(),
    );
    let cosine =
        (dec1.sin() * dec2.sin() + dec1.cos() * dec2.cos() * (ra1 - ra2).cos()).clamp(-1.0, 1.0);
    cosine.acos().to_degrees() * 3600.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: u64, separation: f64, usable: bool) -> MatchCandidate {
        MatchCandidate {
            gaia_source_id: id,
            separation_arcsec: separation,
            gaia_g_mag: Some(3.0),
            gaia_xp_usable: usable,
            gaia_photometric_usable: false,
        }
    }

    #[test]
    fn propagation_handles_high_proper_motion() {
        let p = propagate_hipparcos_to_j2016(&HipparcosAstrometry {
            hip: 1,
            ra_deg_j1991_25: 10.0,
            dec_deg_j1991_25: 20.0,
            pm_ra_cosdec_mas_per_year: 10_000.0,
            pm_dec_mas_per_year: 0.0,
            parallax_mas: 100.0,
            position_uncertainty_mas: 1.0,
            proper_motion_uncertainty_mas_per_year: 1.0,
        })
        .unwrap();
        assert!((p.angular_displacement_arcsec - 247.5).abs() < 1e-12);
        assert!(p.ra_deg_j2016 > 10.0);
    }

    #[test]
    fn three_dimensional_propagation_exposes_perspective_term() {
        let source = HipparcosAstrometry {
            hip: 1,
            ra_deg_j1991_25: 10.0,
            dec_deg_j1991_25: 20.0,
            pm_ra_cosdec_mas_per_year: 10_000.0,
            pm_dec_mas_per_year: 2_000.0,
            parallax_mas: 500.0,
            position_uncertainty_mas: 1.0,
            proper_motion_uncertainty_mas_per_year: 1.0,
        };
        let difference = propagation_2d_3d_difference_arcsec(&source, 100.0).unwrap();
        assert!(difference.is_finite() && difference > 0.0);
        assert_eq!(
            propagation_2d_3d_difference_arcsec(&source, f64::NAN)
                .unwrap_err()
                .to_string(),
            "3D propagation requires positive parallax and finite radial velocity"
        );
    }

    #[test]
    fn invalid_geometry_fails_closed() {
        let policy = BrightStarPrecedencePolicy::v1();
        assert!(classify_match(&policy, &[], f64::NAN).is_err());
        assert!(classify_match(&policy, &[candidate(1, f64::NAN, false)], 1.0).is_err());
        assert!(classify_match(&policy, &[candidate(1, -1.0, false)], 1.0).is_err());
    }

    #[test]
    fn classification_is_order_independent() {
        let policy = BrightStarPrecedencePolicy::v1();
        let a = classify_match(
            &policy,
            &[candidate(2, 0.2, true), candidate(1, 0.1, true)],
            1.0,
        )
        .unwrap();
        let b = classify_match(
            &policy,
            &[candidate(1, 0.1, true), candidate(2, 0.2, true)],
            1.0,
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.class, SupplementClass::AmbiguousManualReview);
    }

    #[test]
    fn no_match_supplement_reliable_gaia_rejected_unusable_replaced() {
        let policy = BrightStarPrecedencePolicy::v1();
        assert_eq!(
            classify_match(&policy, &[], 1.0).unwrap().class,
            SupplementClass::SupplementOnly
        );
        assert_eq!(
            classify_match(&policy, &[candidate(1, 0.1, true)], 1.0)
                .unwrap()
                .class,
            SupplementClass::MatchedAndRejectedAsDuplicate
        );
        assert_eq!(
            classify_match(&policy, &[candidate(1, 0.1, false)], 1.0)
                .unwrap()
                .class,
            SupplementClass::MatchedAndReplacesPrimary
        );
    }
}
