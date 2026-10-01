//! Deterministic assembly of classified, measured-band bright-star artifacts.

use super::artifact::{
    BrightStarArtifact, BrightStarInputProvenance, BrightStarSourceRecord, CorrelatedUncertainty,
};
use super::catalogue::{HipGaiaIdentityMatch, Hipparcos2Record, Tycho2Photometry};
use super::crossmatch::{
    classify_match, positional_match_candidates, propagate_hipparcos_to_j2016, GaiaMatchRow,
    MatchCandidate,
};
use super::policy::{BrightStarPopulationPolicy, BrightStarPrecedencePolicy, SupplementClass};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpectralEstimate {
    pub hip: u32,
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub systematic_catalogue_correlated: Vec<CorrelatedUncertainty>,
    pub route: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarBuildDiagnostics {
    pub schema_version: u32,
    pub input_hipparcos: u64,
    pub passes_population_cut: u64,
    pub matched_gaia: u64,
    pub supplement_only: u64,
    pub replacement: u64,
    pub duplicate_rejected: u64,
    pub ambiguous: u64,
    pub spectral_reconstruction_failed: u64,
    pub final_admitted: u64,
    pub max_angular_displacement_arcsec: f64,
    pub median_angular_displacement_arcsec: f64,
    pub displacement_over_1_arcsec: u64,
    pub displacement_over_10_arcsec: u64,
    pub nside_pixel_scale_arcsec: f64,
    pub largest_proper_motion_sources: Vec<ProperMotionDiagnostic>,
    pub sources: Vec<BrightStarSourceDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProperMotionDiagnostic {
    pub hip: u32,
    pub displacement_arcsec: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarSourceDiagnostic {
    pub supplement_source_id: String,
    pub hip: u32,
    pub tycho_id: Option<String>,
    pub gaia_source_id: Option<u64>,
    pub ra_deg_j2016: f64,
    pub dec_deg_j2016: f64,
    pub hp_mag: f64,
    pub bt_mag: Option<f64>,
    pub vt_mag: Option<f64>,
    pub gaia_g_mag: Option<f64>,
    pub match_separation_arcsec: Option<f64>,
    pub proper_motion_displacement_arcsec: f64,
    pub class: SupplementClass,
    pub spectral_route: String,
    pub flux_336_650_ph_m2_s: f64,
    pub flux_300_336_ph_m2_s: Option<f64>,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_independent_uncertainty_ph_m2_s: f64,
    pub reason: String,
}

#[allow(clippy::too_many_arguments)]
pub fn build_experimental_artifact(
    nside: u32,
    build_commit: &str,
    inputs: Vec<BrightStarInputProvenance>,
    hipparcos: &[Hipparcos2Record],
    tycho_by_hip: &BTreeMap<u32, Tycho2Photometry>,
    identity_matches: &BTreeMap<u32, Vec<HipGaiaIdentityMatch>>,
    gaia_quality: &BTreeMap<u64, GaiaMatchRow>,
    positional_search_rows: &[GaiaMatchRow],
    spectra: &BTreeMap<u32, SpectralEstimate>,
    population_policy: BrightStarPopulationPolicy,
    precedence_policy: BrightStarPrecedencePolicy,
) -> Result<(BrightStarArtifact, BrightStarBuildDiagnostics)> {
    population_policy.validate()?;
    precedence_policy.validate()?;
    let mut sources = Vec::new();
    let mut diagnostics = Vec::new();
    let mut displacements = Vec::new();
    let mut spectral_failed = 0_u64;
    let mut matched_gaia = 0_u64;

    for hip in hipparcos {
        if !population_policy.admits_hp_or_vt(
            Some(hip.hp_mag),
            tycho_by_hip.get(&hip.astrometry.hip).and_then(|t| t.vt_mag),
        ) {
            continue;
        }
        let propagated = propagate_hipparcos_to_j2016(&hip.astrometry)?;
        displacements.push((hip.astrometry.hip, propagated.angular_displacement_arcsec));
        let radius = (5.0 * propagated.positional_uncertainty_arcsec).max(1.0);
        let official = identity_matches.get(&hip.astrometry.hip);
        let candidates = if let Some(official) = official {
            official
                .iter()
                .map(|identity| {
                    let quality = gaia_quality.get(&identity.gaia_source_id);
                    MatchCandidate {
                        gaia_source_id: identity.gaia_source_id,
                        separation_arcsec: identity.angular_distance_arcsec,
                        gaia_g_mag: quality.and_then(|row| row.gaia_g_mag),
                        gaia_xp_usable: quality.is_some_and(|row| row.gaia_xp_usable),
                        gaia_photometric_usable: quality
                            .is_some_and(|row| row.gaia_photometric_usable),
                    }
                })
                .collect::<Vec<_>>()
        } else {
            positional_match_candidates(&propagated, positional_search_rows, radius)?
        };
        let classification_radius = official
            .map(|rows| {
                rows.iter()
                    .map(|row| row.angular_distance_arcsec)
                    .fold(radius, f64::max)
                    + f64::EPSILON
            })
            .unwrap_or(radius);
        let mut decision = classify_match(&precedence_policy, &candidates, classification_radius)?;
        if official.is_some_and(|rows| rows.len() != 1 || rows[0].number_of_neighbours != 1) {
            decision.class = SupplementClass::AmbiguousManualReview;
            decision.gaia_source_id = None;
            decision.reason = "official_crossmatch_not_unique".into();
        }
        if hip.components > 1 {
            decision.class = SupplementClass::AmbiguousManualReview;
            decision.gaia_source_id = None;
            decision.reason = "multiple_system_requires_resolved_flux_policy".into();
        }
        if decision.gaia_source_id.is_some() {
            matched_gaia += 1;
        }
        let estimate = spectra.get(&hip.astrometry.hip);
        if estimate.is_none() {
            spectral_failed += 1;
            decision.class = SupplementClass::AmbiguousManualReview;
            decision.gaia_source_id = None;
            decision.reason = "spectral_reconstruction_failed".into();
        }
        let (flux, stat, sys, groups, route) = estimate.map_or(
            (0.0, 0.0, 0.0, Vec::new(), "unavailable".to_string()),
            |estimate| {
                (
                    estimate.flux_336_650_ph_m2_s,
                    estimate.statistical_uncertainty_ph_m2_s,
                    estimate.systematic_independent_uncertainty_ph_m2_s,
                    estimate.systematic_catalogue_correlated.clone(),
                    estimate.route.clone(),
                )
            },
        );
        if estimate.is_some()
            && (!flux.is_finite()
                || flux <= 0.0
                || !stat.is_finite()
                || stat < 0.0
                || !sys.is_finite()
                || sys < 0.0)
        {
            bail!("invalid spectral estimate for HIP {}", hip.astrometry.hip);
        }
        let source_id = format!("HIP {}", hip.astrometry.hip);
        sources.push(BrightStarSourceRecord {
            source_id: source_id.clone(),
            origin_catalogue: "Hipparcos-2 I/311".into(),
            class: decision.class,
            gaia_source_id: decision.gaia_source_id,
            ra_deg_j2016: propagated.ra_deg_j2016,
            dec_deg_j2016: propagated.dec_deg_j2016,
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_independent_uncertainty_ph_m2_s: sys,
            systematic_catalogue_correlated: groups,
            spectral_route: route.clone(),
            classification_reason: decision.reason.clone(),
        });
        let tycho = tycho_by_hip.get(&hip.astrometry.hip);
        let selected_match = decision.gaia_source_id.and_then(|id| {
            candidates
                .iter()
                .find(|candidate| candidate.gaia_source_id == id)
        });
        diagnostics.push(BrightStarSourceDiagnostic {
            supplement_source_id: source_id,
            hip: hip.astrometry.hip,
            tycho_id: tycho.map(|t| t.tycho_id.clone()),
            gaia_source_id: decision.gaia_source_id,
            ra_deg_j2016: propagated.ra_deg_j2016,
            dec_deg_j2016: propagated.dec_deg_j2016,
            hp_mag: hip.hp_mag,
            bt_mag: tycho.and_then(|t| t.bt_mag),
            vt_mag: tycho.and_then(|t| t.vt_mag),
            gaia_g_mag: selected_match.and_then(|m| m.gaia_g_mag),
            match_separation_arcsec: selected_match.map(|m| m.separation_arcsec),
            proper_motion_displacement_arcsec: propagated.angular_displacement_arcsec,
            class: decision.class,
            spectral_route: route,
            flux_336_650_ph_m2_s: flux,
            flux_300_336_ph_m2_s: None,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_independent_uncertainty_ph_m2_s: sys,
            reason: decision.reason,
        });
    }

    let artifact = BrightStarArtifact::from_sources(
        nside,
        build_commit,
        inputs,
        sources,
        population_policy,
        precedence_policy,
    )?;
    displacements.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let median = if displacements.is_empty() {
        0.0
    } else {
        displacements[(displacements.len() - 1) / 2].1
    };
    let largest = displacements
        .iter()
        .rev()
        .take(20)
        .map(|(hip, displacement)| ProperMotionDiagnostic {
            hip: *hip,
            displacement_arcsec: *displacement,
        })
        .collect();
    diagnostics.sort_by(|a, b| {
        b.flux_336_650_ph_m2_s
            .total_cmp(&a.flux_336_650_ph_m2_s)
            .then(a.hip.cmp(&b.hip))
    });
    let summary = BrightStarBuildDiagnostics {
        schema_version: 1,
        input_hipparcos: hipparcos.len() as u64,
        passes_population_cut: artifact.counts.input_stars,
        matched_gaia,
        supplement_only: artifact.counts.supplement_only,
        replacement: artifact.counts.matched_and_replaces_primary,
        duplicate_rejected: artifact.counts.matched_and_rejected_as_duplicate,
        ambiguous: artifact.counts.ambiguous,
        spectral_reconstruction_failed: spectral_failed,
        final_admitted: artifact.counts.final_admitted,
        max_angular_displacement_arcsec: displacements.last().map_or(0.0, |item| item.1),
        median_angular_displacement_arcsec: median,
        displacement_over_1_arcsec: displacements.iter().filter(|item| item.1 > 1.0).count() as u64,
        displacement_over_10_arcsec: displacements.iter().filter(|item| item.1 > 10.0).count()
            as u64,
        nside_pixel_scale_arcsec: (std::f64::consts::PI / (3.0 * f64::from(nside).powi(2)))
            .sqrt()
            .to_degrees()
            * 3600.0,
        largest_proper_motion_sources: largest,
        sources: diagnostics,
    };
    Ok((artifact, summary))
}
