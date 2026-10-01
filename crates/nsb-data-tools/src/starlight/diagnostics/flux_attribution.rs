//! Flux-weighted exclusion accounting for issue #182.
//!
//! Source counts alone cannot explain the Starlight vs nsb2 radiometric deficit.
//! This diagnostic re-evaluates Gaia/XP partitions with the production scientific
//! policy and records both source counts and the measured 336–650 nm flux that is
//! discarded for each exclusion reason.

use super::baseline::load_smoke_partitions;
use super::processor::{load_photometric, load_selection, load_uv, PhotometricArtifactOverride};
use crate::dataset::RunConfig;
use crate::starlight::healpix::{self, galactic_nested_pixel_from_icrs_position};
use crate::starlight::photometric::{PhotometricCorrection, PhotometricFeatures, RouteDecision};
use crate::starlight::selection::SelectionCorrection;
use crate::starlight::sources::acquisition;
use crate::starlight::uv::{
    EvaluationDecision, MeasuredBandInput, UvCorrection, UvEvaluationInput,
};
use crate::starlight::worker::gaia_source::{load_gaia_sources, GaiaSourceEntry};
use crate::starlight::worker::processing::{population_branch_reason, scientific_exclusion_reason};
use crate::starlight::xp::{integrate_photon_flux, GaiaXpContinuousCalibrator, XpProduct};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One row of flux-weighted exclusion / admission accounting.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FluxAttributionRow {
    pub source_count: u64,
    pub sum_raw_flux_336_650_ph_m2_s: f64,
    pub sum_selection_weighted_flux_336_650_ph_m2_s: f64,
    pub sum_weighted_flux_300_650_ph_m2_s: f64,
    pub sum_uv_flux_300_336_ph_m2_s: f64,
    /// Finer classification for `invalid_uv_predictors` only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub predictor_failure_detail: BTreeMap<String, u64>,
}

impl FluxAttributionRow {
    fn absorb(&mut self, other: &Self) {
        self.source_count += other.source_count;
        self.sum_raw_flux_336_650_ph_m2_s += other.sum_raw_flux_336_650_ph_m2_s;
        self.sum_selection_weighted_flux_336_650_ph_m2_s +=
            other.sum_selection_weighted_flux_336_650_ph_m2_s;
        self.sum_weighted_flux_300_650_ph_m2_s += other.sum_weighted_flux_300_650_ph_m2_s;
        self.sum_uv_flux_300_336_ph_m2_s += other.sum_uv_flux_300_336_ph_m2_s;
        for (key, count) in &other.predictor_failure_detail {
            *self
                .predictor_failure_detail
                .entry(key.clone())
                .or_default() += count;
        }
    }
}

/// Machine-readable flux attribution report for issue #182 Experiment C.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluxAttributionReport {
    pub schema_version: u32,
    pub issue: String,
    pub experiment: String,
    pub commit: String,
    pub workspace: String,
    pub config_path: String,
    pub partitions: Vec<String>,
    pub partition_count: usize,
    pub observed_sources: u64,
    pub admitted_sources: u64,
    pub excluded_sources: u64,
    pub admitted: FluxAttributionRow,
    pub by_exclusion_reason: BTreeMap<String, FluxAttributionRow>,
    pub totals: FluxAttributionTotals,
    pub interpretation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FluxAttributionTotals {
    pub admitted_weighted_flux_300_650_ph_m2_s: f64,
    pub admitted_weighted_flux_336_650_ph_m2_s: f64,
    pub admitted_uv_flux_300_336_ph_m2_s: f64,
    pub excluded_with_measured_336_650_source_count: u64,
    pub excluded_selection_weighted_flux_336_650_ph_m2_s: f64,
    pub invalid_uv_predictors_selection_weighted_flux_336_650_ph_m2_s: f64,
    /// Counterfactual: admitted 300–650 + selection-weighted 336–650 of UV-predictor failures.
    pub counterfactual_retain_invalid_uv_measured_only_flux_ph_m2_s: f64,
    /// Fraction of currently admitted combined flux that the discarded UV-failed measured flux represents.
    pub invalid_uv_lost_flux_over_admitted_combined: f64,
    /// Approximate contribution to a −24.4% deficit if nsb2 ≈ admitted / 0.756.
    pub invalid_uv_lost_flux_over_estimated_nsb2_total: f64,
}

#[derive(Debug, Clone)]
struct AttributionOutcome {
    admitted: bool,
    exclusion_reason: Option<String>,
    predictor_failure_detail: Option<String>,
    raw_flux_336_650_ph_m2_s: f64,
    selection_weighted_flux_336_650_ph_m2_s: f64,
    weighted_flux_300_650_ph_m2_s: f64,
    uv_flux_300_336_ph_m2_s: f64,
}

/// Run flux-weighted exclusion accounting over the requested partitions.
pub fn run_flux_attribution(
    repo_root: &Path,
    config_path: &Path,
    workspace: &Path,
    commit: &str,
    partitions: Option<&[String]>,
    output_path: &Path,
    photometric_override: Option<PhotometricArtifactOverride>,
) -> Result<FluxAttributionReport> {
    let config_bytes = std::fs::read(config_path)?;
    let config: RunConfig = toml::from_slice(&config_bytes)
        .with_context(|| format!("parse {}", config_path.display()))?;
    let starlight = config
        .starlight
        .as_ref()
        .context("config is not a Starlight run")?;
    let selected_partitions = match partitions {
        Some(list) if !list.is_empty() => list.to_vec(),
        _ => load_smoke_partitions(repo_root)?,
    };

    let products = &starlight.gaia_products;
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;
    let ultraviolet = load_uv(starlight.ultraviolet_correction.as_ref())?;
    let photometric = load_photometric(
        starlight.photometric_inference.as_ref(),
        photometric_override,
    )?;
    let selection = load_selection(starlight.selection_function.as_ref())?;
    let nside = starlight.map.canonical_nside;
    let predictor_names = ultraviolet
        .as_ref()
        .map(|uv| {
            uv.artifact()
                .predictors
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut observed = 0_u64;
    let mut admitted = FluxAttributionRow::default();
    let mut by_reason: BTreeMap<String, FluxAttributionRow> = BTreeMap::new();

    for partition in &selected_partitions {
        let gaia_path = acquisition::verified_object_for_partition(
            workspace,
            products,
            "gaia-source",
            partition,
        )?;
        let xp_path = acquisition::verified_object_for_partition(
            workspace,
            products,
            "xp-continuous",
            partition,
        )?;
        let gaia_sources = load_gaia_sources(&gaia_path, &predictor_names)?;
        let mut xp_by_source = BTreeMap::new();
        let mut stream = crate::starlight::xp::stream_bulk_ecsv_gz(&xp_path)?;
        while let Some(record) = stream.next_record()? {
            let source_id = record.source_id.parse::<u64>()?;
            if let Ok(product) = calibrator.calibrate(&record) {
                xp_by_source.insert(source_id, product);
            }
        }
        for (source_id, gaia_source) in &gaia_sources {
            let outcome = evaluate_source_for_flux_attribution(
                gaia_source,
                xp_by_source.get(source_id),
                nside,
                ultraviolet.as_ref(),
                photometric.as_ref(),
                selection.as_ref(),
            );
            observed += 1;
            record_outcome(&mut admitted, &mut by_reason, &outcome);
        }
    }

    let excluded_sources = by_reason.values().map(|row| row.source_count).sum::<u64>();
    let admitted_sources = admitted.source_count;
    let invalid_uv = by_reason
        .get("invalid_uv_predictors")
        .cloned()
        .unwrap_or_default();
    let excluded_with_measured = by_reason
        .values()
        .filter(|row| row.sum_raw_flux_336_650_ph_m2_s > 0.0)
        .map(|row| row.source_count)
        .sum::<u64>();
    let excluded_weighted_336 = by_reason
        .values()
        .map(|row| row.sum_selection_weighted_flux_336_650_ph_m2_s)
        .sum::<f64>();
    let counterfactual = admitted.sum_weighted_flux_300_650_ph_m2_s
        + invalid_uv.sum_selection_weighted_flux_336_650_ph_m2_s;
    let admitted_combined = admitted.sum_weighted_flux_300_650_ph_m2_s.max(1e-300);
    // Issue #182 baseline integrated relative bias ≈ −24.4% ⇒ NSB/nsb2 ≈ 0.756.
    let estimated_nsb2 = admitted_combined / 0.756;
    let totals = FluxAttributionTotals {
        admitted_weighted_flux_300_650_ph_m2_s: admitted.sum_weighted_flux_300_650_ph_m2_s,
        admitted_weighted_flux_336_650_ph_m2_s: admitted
            .sum_selection_weighted_flux_336_650_ph_m2_s,
        admitted_uv_flux_300_336_ph_m2_s: admitted.sum_uv_flux_300_336_ph_m2_s,
        excluded_with_measured_336_650_source_count: excluded_with_measured,
        excluded_selection_weighted_flux_336_650_ph_m2_s: excluded_weighted_336,
        invalid_uv_predictors_selection_weighted_flux_336_650_ph_m2_s: invalid_uv
            .sum_selection_weighted_flux_336_650_ph_m2_s,
        counterfactual_retain_invalid_uv_measured_only_flux_ph_m2_s: counterfactual,
        invalid_uv_lost_flux_over_admitted_combined: invalid_uv
            .sum_selection_weighted_flux_336_650_ph_m2_s
            / admitted_combined,
        invalid_uv_lost_flux_over_estimated_nsb2_total: invalid_uv
            .sum_selection_weighted_flux_336_650_ph_m2_s
            / estimated_nsb2,
    };

    let report = FluxAttributionReport {
        schema_version: 1,
        issue: "182".to_string(),
        experiment: "C_exclusion_flux_attribution".to_string(),
        commit: commit.to_string(),
        workspace: workspace.display().to_string(),
        config_path: config_path.display().to_string(),
        partitions: selected_partitions.clone(),
        partition_count: selected_partitions.len(),
        observed_sources: observed,
        admitted_sources,
        excluded_sources,
        admitted,
        by_exclusion_reason: by_reason,
        totals,
        interpretation: vec![
            "selection-weighted 336-650 flux for invalid_uv_predictors is the measured Gaia contribution discarded by the pre-#182 whole-source exclusion policy".to_string(),
            "counterfactual_retain_invalid_uv_measured_only adds that discarded measured flux without inventing a 300-336 nm correction".to_string(),
            "production admission after #182 retains measured 336-650 for these sources with ApplicabilityStatus::Unavailable; this diagnostic still applies the legacy exclusion for quantification".to_string(),
            "invalid_uv_lost_flux_over_estimated_nsb2_total uses the #182 baseline ratio NSB/nsb2≈0.756 applied to this sample's admitted combined flux".to_string(),
            "UV predictors for the production artifact are bp_rp and phot_g_mean_mag; predictor_failure_detail separates missing colour vs magnitude".to_string(),
        ],
    };

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(&report)?;
    std::fs::write(output_path, payload)
        .with_context(|| format!("write {}", output_path.display()))?;
    Ok(report)
}

fn record_outcome(
    admitted: &mut FluxAttributionRow,
    by_reason: &mut BTreeMap<String, FluxAttributionRow>,
    outcome: &AttributionOutcome,
) {
    let mut row = FluxAttributionRow {
        source_count: 1,
        sum_raw_flux_336_650_ph_m2_s: outcome.raw_flux_336_650_ph_m2_s,
        sum_selection_weighted_flux_336_650_ph_m2_s: outcome
            .selection_weighted_flux_336_650_ph_m2_s,
        sum_weighted_flux_300_650_ph_m2_s: outcome.weighted_flux_300_650_ph_m2_s,
        sum_uv_flux_300_336_ph_m2_s: outcome.uv_flux_300_336_ph_m2_s,
        predictor_failure_detail: BTreeMap::new(),
    };
    if let Some(detail) = &outcome.predictor_failure_detail {
        row.predictor_failure_detail.insert(detail.clone(), 1);
    }
    if outcome.admitted {
        admitted.absorb(&row);
    } else if let Some(reason) = &outcome.exclusion_reason {
        by_reason.entry(reason.clone()).or_default().absorb(&row);
    }
}

fn evaluate_source_for_flux_attribution(
    gaia_source: &GaiaSourceEntry,
    xp_product: Option<&XpProduct>,
    nside: u32,
    ultraviolet_correction: Option<&UvCorrection>,
    photometric_correction: Option<&PhotometricCorrection>,
    selection_correction: Option<&SelectionCorrection>,
) -> AttributionOutcome {
    let _ = galactic_nested_pixel_from_icrs_position(
        gaia_source.icrs.ra_deg,
        gaia_source.icrs.dec_deg,
        nside,
    );
    let mut outcome = AttributionOutcome {
        admitted: false,
        exclusion_reason: None,
        predictor_failure_detail: None,
        raw_flux_336_650_ph_m2_s: 0.0,
        selection_weighted_flux_336_650_ph_m2_s: 0.0,
        weighted_flux_300_650_ph_m2_s: 0.0,
        uv_flux_300_336_ph_m2_s: 0.0,
    };

    if let Some(reason) = scientific_exclusion_reason(gaia_source) {
        outcome.exclusion_reason = Some(reason.to_string());
        return outcome;
    }

    let raw_flux_336_650 = if let Some(product) = xp_product {
        match integrate_photon_flux(product) {
            Ok(flux) if flux.is_finite() && flux > 0.0 => flux,
            _ => {
                outcome.exclusion_reason = Some("invalid_flux".to_string());
                return outcome;
            }
        }
    } else {
        let Some(photometric) = photometric_correction else {
            outcome.exclusion_reason = Some("no_xp_spectrum".to_string());
            return outcome;
        };
        let route = match photometric.route_and_evaluate(PhotometricFeatures {
            phot_g_mean_mag: gaia_source.phot_g_mean_mag,
            phot_bp_mean_mag: gaia_source.phot_bp_mean_mag,
            phot_rp_mean_mag: gaia_source.phot_rp_mean_mag,
            bp_rp: gaia_source.bp_rp,
            quality_flag: true,
        }) {
            Ok(route) => route,
            Err(_) => {
                outcome.exclusion_reason = Some("photometric_evaluation_failed".to_string());
                return outcome;
            }
        };
        let RouteDecision { branch, flux } = route;
        let Some(estimate) = flux else {
            outcome.exclusion_reason = Some(population_branch_reason(branch).to_string());
            return outcome;
        };
        estimate.flux_336_650_ph_m2_s
    };

    outcome.raw_flux_336_650_ph_m2_s = raw_flux_336_650;

    // Match production admit_weighted_source: selection weight, then UV.
    let weight = match selection_weight(selection_correction, gaia_source) {
        Ok(weight) => weight,
        Err(reason) => {
            outcome.exclusion_reason = Some(reason.to_string());
            return outcome;
        }
    };
    let weighted_336 = weight * raw_flux_336_650;
    outcome.selection_weighted_flux_336_650_ph_m2_s = weighted_336;

    let Some(correction) = ultraviolet_correction else {
        outcome.exclusion_reason = Some("uv_correction_missing".to_string());
        return outcome;
    };
    let Some(predictors) = &gaia_source.predictors else {
        // Experiment C intentionally reproduces the pre-#182 exclusion so the
        // discarded measured 336–650 nm flux can be quantified. Production
        // admission now retains that flux via retain_measured_when_uv_unavailable.
        outcome.exclusion_reason = Some("invalid_uv_predictors".to_string());
        outcome.predictor_failure_detail = Some(classify_predictor_failure(gaia_source));
        return outcome;
    };
    let evaluation = match correction.evaluate(UvEvaluationInput {
        predictors,
        measured_band: Some(MeasuredBandInput {
            flux_336_650_ph_m2_s: weighted_336,
            statistical_uncertainty_336_650_ph_m2_s: 0.0,
        }),
    }) {
        Ok(evaluation) => evaluation,
        Err(_) => {
            outcome.exclusion_reason = Some("uv_evaluation_failed".to_string());
            return outcome;
        }
    };
    if evaluation.decision == EvaluationDecision::Rejected {
        outcome.exclusion_reason = Some("uv_out_of_domain".to_string());
        return outcome;
    }
    match correction.combine_with_measured(weighted_336, 0.0, &evaluation) {
        Ok(combined) => {
            outcome.uv_flux_300_336_ph_m2_s = combined.flux_300_336_ph_m2_s;
            outcome.weighted_flux_300_650_ph_m2_s = combined.flux_300_650_ph_m2_s;
            outcome.admitted = true;
            outcome
        }
        Err(_) => {
            outcome.exclusion_reason = Some("uv_evaluation_failed".to_string());
            outcome
        }
    }
}

fn selection_weight(
    selection: Option<&SelectionCorrection>,
    gaia_source: &GaiaSourceEntry,
) -> Result<f64, &'static str> {
    let Some(selection) = selection else {
        return Ok(1.0);
    };
    let Some(g_mag) = gaia_source.phot_g_mean_mag else {
        return Err("selection_missing_g_magnitude");
    };
    let healpix = healpix::icrs_equatorial_nested_pixel(
        gaia_source.icrs.ra_deg,
        gaia_source.icrs.dec_deg,
        selection.artifact().healpix_nside,
    )
    .map_err(|_| "selection_healpix_failed")?;
    let evaluation = selection
        .evaluate(healpix, g_mag, gaia_source.bp_rp)
        .map_err(|_| "selection_evaluation_failed")?;
    Ok(evaluation.weight)
}

fn classify_predictor_failure(gaia_source: &GaiaSourceEntry) -> String {
    let missing_g = gaia_source
        .phot_g_mean_mag
        .filter(|value| value.is_finite())
        .is_none();
    let missing_bp_rp = gaia_source
        .bp_rp
        .filter(|value| value.is_finite())
        .is_none();
    match (missing_g, missing_bp_rp) {
        (true, true) => "missing_phot_g_mean_mag_and_bp_rp".to_string(),
        (true, false) => "missing_phot_g_mean_mag".to_string(),
        (false, true) => "missing_bp_rp".to_string(),
        (false, false) => "non_finite_or_unparsed_predictor_column".to_string(),
    }
}

/// Optional helper for callers constructing partition lists from a file.
pub fn load_partition_list(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read partition list {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

pub fn default_output_path(output_dir: &Path) -> PathBuf {
    output_dir.join("flux-attribution-report.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predictor_failure_classifies_missing_colour() {
        let source = GaiaSourceEntry {
            source_id: 1,
            icrs: crate::starlight::healpix::IcrsSkyPosition::new(10.0, 20.0).unwrap(),
            phot_g_mean_mag: Some(15.0),
            phot_bp_mean_mag: None,
            phot_rp_mean_mag: None,
            bp_rp: None,
            duplicated_source: false,
            in_qso_candidates: false,
            in_galaxy_candidates: false,
            predictors: None,
        };
        assert_eq!(classify_predictor_failure(&source), "missing_bp_rp");
    }

    #[test]
    fn attribution_row_absorb_sums_flux_and_detail() {
        let mut left = FluxAttributionRow {
            source_count: 1,
            sum_raw_flux_336_650_ph_m2_s: 10.0,
            sum_selection_weighted_flux_336_650_ph_m2_s: 12.0,
            sum_weighted_flux_300_650_ph_m2_s: 0.0,
            sum_uv_flux_300_336_ph_m2_s: 0.0,
            predictor_failure_detail: BTreeMap::from([("missing_bp_rp".into(), 1)]),
        };
        let right = FluxAttributionRow {
            source_count: 2,
            sum_raw_flux_336_650_ph_m2_s: 5.0,
            sum_selection_weighted_flux_336_650_ph_m2_s: 6.0,
            sum_weighted_flux_300_650_ph_m2_s: 0.0,
            sum_uv_flux_300_336_ph_m2_s: 0.0,
            predictor_failure_detail: BTreeMap::from([("missing_bp_rp".into(), 2)]),
        };
        left.absorb(&right);
        assert_eq!(left.source_count, 3);
        assert_eq!(left.sum_raw_flux_336_650_ph_m2_s, 15.0);
        assert_eq!(left.predictor_failure_detail["missing_bp_rp"], 3);
    }
}
