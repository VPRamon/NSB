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
use crate::starlight::map::accumulator::StableSum;
use crate::starlight::photometric::{PhotometricCorrection, PhotometricFeatures, RouteDecision};
use crate::starlight::selection::SelectionCorrection;
use crate::starlight::sources::acquisition;
use crate::starlight::uv::{
    EvaluationDecision, MeasuredBandInput, UvCorrection, UvEvaluationInput,
};
use crate::starlight::worker::gaia_source::{load_gaia_sources, GaiaSourceEntry};
use crate::starlight::worker::processing::{
    measured_xp_flux_and_uncertainty, population_branch_reason, scientific_exclusion_reason,
};
use crate::starlight::xp::{GaiaXpContinuousCalibrator, XpProduct};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
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

#[derive(Default)]
struct FluxAttributionAccumulator {
    source_count: u64,
    raw_336_650: StableSum,
    weighted_336_650: StableSum,
    weighted_300_650: StableSum,
    uv_300_336: StableSum,
    predictor_failure_detail: BTreeMap<String, u64>,
}

impl FluxAttributionAccumulator {
    fn absorb_outcome(&mut self, outcome: &AttributionOutcome) -> Result<()> {
        self.source_count += 1;
        self.raw_336_650.add(outcome.raw_flux_336_650_ph_m2_s)?;
        self.weighted_336_650
            .add(outcome.selection_weighted_flux_336_650_ph_m2_s)?;
        self.weighted_300_650
            .add(outcome.weighted_flux_300_650_ph_m2_s)?;
        self.uv_300_336.add(outcome.uv_flux_300_336_ph_m2_s)?;
        if let Some(detail) = &outcome.predictor_failure_detail {
            *self
                .predictor_failure_detail
                .entry(detail.clone())
                .or_default() += 1;
        }
        Ok(())
    }

    fn into_row(self) -> FluxAttributionRow {
        FluxAttributionRow {
            source_count: self.source_count,
            sum_raw_flux_336_650_ph_m2_s: self.raw_336_650.value(),
            sum_selection_weighted_flux_336_650_ph_m2_s: self.weighted_336_650.value(),
            sum_weighted_flux_300_650_ph_m2_s: self.weighted_300_650.value(),
            sum_uv_flux_300_336_ph_m2_s: self.uv_300_336.value(),
            predictor_failure_detail: self.predictor_failure_detail,
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
    let mut admitted_acc = FluxAttributionAccumulator::default();
    let mut by_reason_acc: BTreeMap<String, FluxAttributionAccumulator> = BTreeMap::new();

    let mut selected_partitions = selected_partitions;
    selected_partitions.sort();

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
        // Mirror production worker routing: XP stream first (file order), mark
        // processed including calibration failures, then remaining Gaia sources
        // in sorted source_id order for photometric fallback.
        let mut processed = HashSet::new();
        let mut stream = crate::starlight::xp::stream_bulk_ecsv_gz(&xp_path)?;
        while let Some(record) = stream.next_record()? {
            let source_id = record.source_id.parse::<u64>().with_context(|| {
                format!(
                    "invalid XP source_id {} in partition {partition}",
                    record.source_id
                )
            })?;
            processed.insert(source_id);
            let Some(gaia_source) = gaia_sources.get(&source_id) else {
                // XP row without a matching GaiaSource row has no authoritative
                // ICRS position; skip rather than invent coordinates (production).
                continue;
            };
            observed += 1;
            // Match production ordering: scientific exclusion before calibration.
            if let Some(reason) = scientific_exclusion_reason(gaia_source) {
                let outcome = AttributionOutcome {
                    admitted: false,
                    exclusion_reason: Some(reason.to_string()),
                    predictor_failure_detail: None,
                    raw_flux_336_650_ph_m2_s: 0.0,
                    selection_weighted_flux_336_650_ph_m2_s: 0.0,
                    weighted_flux_300_650_ph_m2_s: 0.0,
                    uv_flux_300_336_ph_m2_s: 0.0,
                };
                record_outcome(&mut admitted_acc, &mut by_reason_acc, &outcome)?;
                continue;
            }
            let outcome = match calibrator.calibrate(&record) {
                Ok(product) => outcome_for_calibrated_xp(
                    gaia_source,
                    &product,
                    nside,
                    ultraviolet.as_ref(),
                    photometric.as_ref(),
                    selection.as_ref(),
                ),
                Err(_) => evaluate_source_for_flux_attribution(
                    gaia_source,
                    XpRoute::CalibrationFailed,
                    nside,
                    ultraviolet.as_ref(),
                    photometric.as_ref(),
                    selection.as_ref(),
                ),
            };
            record_outcome(&mut admitted_acc, &mut by_reason_acc, &outcome)?;
        }

        let mut remaining: Vec<_> = gaia_sources
            .iter()
            .filter(|(source_id, _)| !processed.contains(source_id))
            .collect();
        remaining.sort_by_key(|(source_id, _)| *source_id);
        for (_source_id, gaia_source) in remaining {
            observed += 1;
            let outcome = evaluate_source_for_flux_attribution(
                gaia_source,
                XpRoute::NoXpRecord,
                nside,
                ultraviolet.as_ref(),
                photometric.as_ref(),
                selection.as_ref(),
            );
            record_outcome(&mut admitted_acc, &mut by_reason_acc, &outcome)?;
        }
    }

    let admitted = admitted_acc.into_row();
    let by_reason: BTreeMap<String, FluxAttributionRow> = by_reason_acc
        .into_iter()
        .map(|(reason, acc)| (reason, acc.into_row()))
        .collect();

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
            "selection-weighted 336-650 flux for invalid_uv_predictors is the measured Gaia contribution discarded by the production whole-source exclusion policy (sources lacking UV predictors are not admitted into the canonical 300-650 map)".to_string(),
            "counterfactual_retain_invalid_uv_measured_only adds that discarded measured flux without inventing a 300-336 nm correction; it is diagnostic only and is not a production admission path".to_string(),
            "XP calibration failures are recorded as calibration_failed and never fall through to photometric inference, matching production worker routing".to_string(),
            "invalid_uv_lost_flux_over_estimated_nsb2_total uses the #182 baseline ratio NSB/nsb2≈0.756 applied to this sample's admitted combined flux".to_string(),
            "UV predictors for the production artifact are bp_rp and phot_g_mean_mag; predictor_failure_detail separates missing colour vs magnitude".to_string(),
            "flux totals are accumulated with StableSum over partitions sorted by name and remaining Gaia sources sorted by source_id, so identical inputs yield byte-identical reports".to_string(),
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

/// XP routing for one Gaia source, matching production worker semantics.
#[derive(Debug, PartialEq)]
enum XpRoute {
    /// Successfully calibrated XP spectrum with validated flux and uncertainty.
    CalibratedMeasured {
        flux_336_650_ph_m2_s: f64,
        statistical_uncertainty_336_650_ph_m2_s: f64,
    },
    /// XP record existed but calibration failed — never photometric fallback.
    CalibrationFailed,
    /// No XP record for this source — photometric fallback may apply.
    NoXpRecord,
}

/// Match production worker.rs: flux gate, then uncertainty gate, before admission.
fn measured_flux_route(product: &XpProduct) -> Result<XpRoute, &'static str> {
    let (flux, uncertainty) = measured_xp_flux_and_uncertainty(product)?;
    Ok(XpRoute::CalibratedMeasured {
        flux_336_650_ph_m2_s: flux,
        statistical_uncertainty_336_650_ph_m2_s: uncertainty,
    })
}

fn outcome_for_calibrated_xp(
    gaia_source: &GaiaSourceEntry,
    product: &XpProduct,
    nside: u32,
    ultraviolet_correction: Option<&UvCorrection>,
    photometric_correction: Option<&PhotometricCorrection>,
    selection_correction: Option<&SelectionCorrection>,
) -> AttributionOutcome {
    match measured_flux_route(product) {
        Ok(route) => evaluate_source_for_flux_attribution(
            gaia_source,
            route,
            nside,
            ultraviolet_correction,
            photometric_correction,
            selection_correction,
        ),
        Err(reason) => AttributionOutcome {
            admitted: false,
            exclusion_reason: Some(reason.to_string()),
            predictor_failure_detail: None,
            raw_flux_336_650_ph_m2_s: 0.0,
            selection_weighted_flux_336_650_ph_m2_s: 0.0,
            weighted_flux_300_650_ph_m2_s: 0.0,
            uv_flux_300_336_ph_m2_s: 0.0,
        },
    }
}

fn record_outcome(
    admitted: &mut FluxAttributionAccumulator,
    by_reason: &mut BTreeMap<String, FluxAttributionAccumulator>,
    outcome: &AttributionOutcome,
) -> Result<()> {
    if outcome.admitted {
        admitted.absorb_outcome(outcome)?;
    } else if let Some(reason) = &outcome.exclusion_reason {
        by_reason
            .entry(reason.clone())
            .or_default()
            .absorb_outcome(outcome)?;
    }
    Ok(())
}

fn evaluate_source_for_flux_attribution(
    gaia_source: &GaiaSourceEntry,
    xp_route: XpRoute,
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

    if matches!(xp_route, XpRoute::CalibrationFailed) {
        outcome.exclusion_reason = Some("calibration_failed".to_string());
        return outcome;
    }

    let (raw_flux_336_650, statistical_336_650) = match xp_route {
        XpRoute::CalibratedMeasured {
            flux_336_650_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s,
        } => (
            flux_336_650_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s,
        ),
        XpRoute::NoXpRecord => {
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
            (
                estimate.flux_336_650_ph_m2_s,
                estimate.statistical_uncertainty_336_650_ph_m2_s,
            )
        }
        XpRoute::CalibrationFailed => unreachable!("handled above"),
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
    let weighted_statistical = weight * statistical_336_650;
    outcome.selection_weighted_flux_336_650_ph_m2_s = weighted_336;

    let Some(correction) = ultraviolet_correction else {
        outcome.exclusion_reason = Some("uv_correction_missing".to_string());
        return outcome;
    };
    let Some(predictors) = &gaia_source.predictors else {
        // Production excludes sources lacking UV predictors from the canonical
        // 300–650 map. Experiment C quantifies the discarded measured 336–650
        // flux under that honest exclusion policy.
        outcome.exclusion_reason = Some("invalid_uv_predictors".to_string());
        outcome.predictor_failure_detail = Some(classify_predictor_failure(gaia_source));
        return outcome;
    };
    let evaluation = match correction.evaluate(UvEvaluationInput {
        predictors,
        measured_band: Some(MeasuredBandInput {
            flux_336_650_ph_m2_s: weighted_336,
            statistical_uncertainty_336_650_ph_m2_s: weighted_statistical,
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
    match correction.combine_with_measured(weighted_336, weighted_statistical, &evaluation) {
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
    use crate::platform::{artifact_store, checksum_io};
    use crate::starlight::config::{GaiaProductConfig, OfficialChecksumAlgorithm};
    use crate::starlight::photometric::PhotometricCorrection;
    use crate::starlight::selection::{
        ColourMarginalisation, CompletenessEntry, FaintTailModel, SelectionArtifact,
        SelectionCorrection, SelectionReferenceDataset, SelectionReferenceFile,
    };
    use crate::starlight::sources::acquisition::AcquisitionReceipt;
    use crate::starlight::sources::inventory::{SourceInventory, SourceInventoryEntry};
    use crate::starlight::uv::{
        CalibrationStatus, OutOfDomainPolicy, UvCalibrationArtifact, UvCorrection,
    };
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn sample_source(source_id: u64) -> GaiaSourceEntry {
        GaiaSourceEntry {
            source_id,
            icrs: crate::starlight::healpix::IcrsSkyPosition::new(10.0, 20.0).unwrap(),
            phot_g_mean_mag: Some(15.0),
            phot_bp_mean_mag: Some(15.5),
            phot_rp_mean_mag: Some(14.5),
            bp_rp: Some(1.0),
            duplicated_source: false,
            in_qso_candidates: false,
            in_galaxy_candidates: false,
            predictors: None,
        }
    }

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    fn load_uv_fixture() -> UvCorrection {
        let path = fixture_root().join("uv_synthetic_non_production/artifact.json");
        let sha256 = checksum_io::sha256_file(&path).unwrap();
        UvCorrection::load(&path, &sha256).unwrap()
    }

    fn load_uv_reject_fixture() -> (TempDir, UvCorrection) {
        let path = fixture_root().join("uv_synthetic_non_production/artifact.json");
        let mut artifact: UvCalibrationArtifact =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        artifact.out_of_domain_policy = OutOfDomainPolicy::Reject;
        let temporary = TempDir::new().unwrap();
        let out = temporary.path().join("uv-reject.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        fs::write(&out, &bytes).unwrap();
        let correction = UvCorrection::load(&out, &checksum_io::sha256_bytes(&bytes)).unwrap();
        (temporary, correction)
    }

    fn load_photometric_fixture() -> PhotometricCorrection {
        let path = fixture_root().join("photometric_xp_anchored_v1/artifact.json");
        let pin: Value = serde_json::from_slice(
            &fs::read(fixture_root().join("photometric_xp_anchored_v1/production-pin.json"))
                .unwrap(),
        )
        .unwrap();
        let sha256 = pin["fixture_sha256"].as_str().unwrap();
        PhotometricCorrection::load(&path, sha256).unwrap()
    }

    fn load_selection_fixture(healpix: u32) -> (TempDir, SelectionCorrection) {
        let artifact = SelectionArtifact {
            schema_version: crate::starlight::selection::SELECTION_ARTIFACT_SCHEMA_VERSION,
            model_id: "fixture-selection".to_string(),
            calibration_status: CalibrationStatus::Candidate,
            reference_dataset: SelectionReferenceDataset {
                name: "fixture-selection-dataset".to_string(),
                release: "fixture".to_string(),
                licence: "CC-BY-4.0".to_string(),
                doi: "10.0000/fixture".to_string(),
                files: vec![SelectionReferenceFile {
                    name: "completeness.parquet".to_string(),
                    sha256: "a".repeat(64),
                }],
            },
            weight_cap: 5.0,
            magnitude_bins: vec![10.0, 15.0, 20.0],
            colour_bins: vec![0.0, 1.0, 2.0],
            healpix_nside: 1,
            coordinate_frame: crate::starlight::healpix::HealpixCoordinateFrame::Equatorial,
            ordering: crate::starlight::healpix::HealpixOrderingScheme::Nested,
            table_spatial_nside: None,
            completeness_table: vec![CompletenessEntry {
                healpix,
                magnitude_bin: 1,
                colour_bin: 0,
                completeness: 0.5,
            }],
            m10_map: Vec::new(),
            colour_marginalisation: ColourMarginalisation::MarginaliseUniform,
            faint_tail: FaintTailModel {
                enabled: false,
                magnitude_limit_g: 20.0,
                residual_fraction_per_pixel: 0.0,
                systematic_fraction: 0.0,
            },
            training_command: "fixture-generated, not trained".to_string(),
            software_version: "nsb-data-tools-test-fixture".to_string(),
        };
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("selection.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        fs::write(&path, &bytes).unwrap();
        let correction =
            SelectionCorrection::load(&path, &checksum_io::sha256_bytes(&bytes)).unwrap();
        (temporary, correction)
    }

    fn sample_xp_product(flux: f64, errors: Option<Vec<f64>>) -> XpProduct {
        XpProduct {
            source_id: "1".to_string(),
            wavelengths_nm: vec![336.0, 650.0],
            flux_w_m2_nm: vec![flux, flux],
            flux_error_w_m2_nm: errors,
        }
    }

    #[test]
    fn predictor_failure_classifies_missing_colour() {
        let source = sample_source(1);
        let source = GaiaSourceEntry {
            bp_rp: None,
            predictors: None,
            ..source
        };
        assert_eq!(classify_predictor_failure(&source), "missing_bp_rp");
    }

    #[test]
    fn predictor_failure_classifies_missing_g_and_both() {
        assert_eq!(
            classify_predictor_failure(&GaiaSourceEntry {
                phot_g_mean_mag: None,
                bp_rp: Some(1.0),
                ..sample_source(2)
            }),
            "missing_phot_g_mean_mag"
        );
        assert_eq!(
            classify_predictor_failure(&GaiaSourceEntry {
                phot_g_mean_mag: None,
                bp_rp: None,
                ..sample_source(3)
            }),
            "missing_phot_g_mean_mag_and_bp_rp"
        );
        assert_eq!(
            classify_predictor_failure(&GaiaSourceEntry {
                predictors: None,
                ..sample_source(4)
            }),
            "non_finite_or_unparsed_predictor_column"
        );
    }

    #[test]
    fn calibration_failed_never_falls_through_to_photometric() {
        let source = sample_source(42);
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibrationFailed,
            128,
            None,
            None,
            None,
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("calibration_failed")
        );
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 0.0);
    }

    #[test]
    fn scientific_exclusion_precedes_calibration_failed_route() {
        let source = GaiaSourceEntry {
            in_qso_candidates: true,
            ..sample_source(7)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibrationFailed,
            128,
            None,
            None,
            None,
        );
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("scientific_exclusion_nonstellar")
        );
    }

    #[test]
    fn duplicated_source_is_excluded() {
        let source = GaiaSourceEntry {
            duplicated_source: true,
            ..sample_source(8)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::NoXpRecord,
            128,
            None,
            None,
            None,
        );
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("duplicated_source")
        );
    }

    #[test]
    fn no_xp_without_photometric_artifact_is_no_xp_spectrum() {
        let source = sample_source(9);
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::NoXpRecord,
            128,
            None,
            None,
            None,
        );
        assert_eq!(outcome.exclusion_reason.as_deref(), Some("no_xp_spectrum"));
    }

    #[test]
    fn measured_flux_route_rejects_non_positive_flux() {
        let product = sample_xp_product(0.0, Some(vec![1.0, 1.0]));
        assert_eq!(measured_flux_route(&product), Err("invalid_flux"));
    }

    #[test]
    fn measured_flux_route_rejects_missing_uncertainty() {
        let product = sample_xp_product(1.0e-15, None);
        assert_eq!(measured_flux_route(&product), Err("invalid_uncertainty"));
    }

    #[test]
    fn outcome_for_calibrated_xp_records_invalid_uncertainty() {
        let source = sample_source(20);
        let product = sample_xp_product(1.0e-15, None);
        let outcome = outcome_for_calibrated_xp(&source, &product, 128, None, None, None);
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("invalid_uncertainty")
        );
    }

    #[test]
    fn outcome_for_calibrated_xp_records_invalid_flux() {
        let source = sample_source(21);
        let product = sample_xp_product(0.0, Some(vec![1.0, 1.0]));
        let outcome = outcome_for_calibrated_xp(&source, &product, 128, None, None, None);
        assert_eq!(outcome.exclusion_reason.as_deref(), Some("invalid_flux"));
    }

    #[test]
    fn calibrated_measured_without_uv_artifact_is_uv_correction_missing() {
        let source = sample_source(11);
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 1.0,
                statistical_uncertainty_336_650_ph_m2_s: 0.1,
            },
            128,
            None,
            None,
            None,
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("uv_correction_missing")
        );
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 1.0);
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 1.0);
    }

    #[test]
    fn calibrated_measured_records_selection_weighted_flux_before_uv_gate() {
        let source = sample_source(12);
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 2.5,
                statistical_uncertainty_336_650_ph_m2_s: 0.25,
            },
            128,
            None,
            None,
            None,
        );
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 2.5);
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 2.5);
        assert_eq!(outcome.weighted_flux_300_650_ph_m2_s, 0.0);
        assert!(!outcome.admitted);
    }

    #[test]
    fn invalid_uv_predictors_exclude_measured_flux_honestly() {
        let uv = load_uv_fixture();
        let source = sample_source(30);
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            },
            128,
            Some(&uv),
            None,
            None,
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("invalid_uv_predictors")
        );
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 100.0);
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 100.0);
        assert_eq!(
            outcome.predictor_failure_detail.as_deref(),
            Some("non_finite_or_unparsed_predictor_column")
        );
        assert_eq!(outcome.weighted_flux_300_650_ph_m2_s, 0.0);
    }

    #[test]
    fn valid_uv_predictors_admit_combined_flux() {
        let uv = load_uv_fixture();
        let source = GaiaSourceEntry {
            predictors: Some(BTreeMap::from([("x".to_string(), 5.0)])),
            ..sample_source(31)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            },
            128,
            Some(&uv),
            None,
            None,
        );
        assert!(outcome.admitted);
        assert!(outcome.exclusion_reason.is_none());
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 100.0);
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 100.0);
        assert_eq!(outcome.uv_flux_300_336_ph_m2_s, 20.0);
        assert_eq!(outcome.weighted_flux_300_650_ph_m2_s, 120.0);
    }

    #[test]
    fn uv_out_of_domain_is_excluded_under_reject_policy() {
        let (_tmp, uv) = load_uv_reject_fixture();
        let source = GaiaSourceEntry {
            predictors: Some(BTreeMap::from([("x".to_string(), 50.0)])),
            ..sample_source(32)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            },
            128,
            Some(&uv),
            None,
            None,
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("uv_out_of_domain")
        );
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 100.0);
    }

    #[test]
    fn selection_missing_g_magnitude_fails_before_uv() {
        let healpix =
            crate::starlight::healpix::icrs_equatorial_nested_pixel(10.0, 20.0, 1).unwrap();
        let (_tmp, selection) = load_selection_fixture(healpix);
        let source = GaiaSourceEntry {
            phot_g_mean_mag: None,
            predictors: Some(BTreeMap::from([("x".to_string(), 5.0)])),
            ..sample_source(33)
        };
        let uv = load_uv_fixture();
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::CalibratedMeasured {
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_336_650_ph_m2_s: 4.0,
            },
            128,
            Some(&uv),
            None,
            Some(&selection),
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("selection_missing_g_magnitude")
        );
        assert_eq!(outcome.raw_flux_336_650_ph_m2_s, 100.0);
        assert_eq!(outcome.selection_weighted_flux_336_650_ph_m2_s, 0.0);
    }

    #[test]
    fn photometric_no_xp_route_admits_with_uv_predictors() {
        let photometric = load_photometric_fixture();
        let uv = load_uv_fixture();
        let source = GaiaSourceEntry {
            predictors: Some(BTreeMap::from([("x".to_string(), 5.0)])),
            ..sample_source(34)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::NoXpRecord,
            128,
            Some(&uv),
            Some(&photometric),
            None,
        );
        assert!(
            outcome.admitted,
            "expected photometric+UV admission, got {:?}",
            outcome.exclusion_reason
        );
        assert!(outcome.raw_flux_336_650_ph_m2_s > 0.0);
        assert!(outcome.weighted_flux_300_650_ph_m2_s > outcome.raw_flux_336_650_ph_m2_s);
        assert!(outcome.uv_flux_300_336_ph_m2_s > 0.0);
    }

    #[test]
    fn photometric_no_usable_photometry_is_excluded() {
        let photometric = load_photometric_fixture();
        let source = GaiaSourceEntry {
            phot_g_mean_mag: None,
            phot_bp_mean_mag: None,
            phot_rp_mean_mag: None,
            bp_rp: None,
            ..sample_source(35)
        };
        let outcome = evaluate_source_for_flux_attribution(
            &source,
            XpRoute::NoXpRecord,
            128,
            None,
            Some(&photometric),
            None,
        );
        assert!(!outcome.admitted);
        assert_eq!(
            outcome.exclusion_reason.as_deref(),
            Some("no_usable_photometry")
        );
    }

    #[test]
    fn flux_attribution_accumulator_is_order_independent() {
        let outcomes = [
            AttributionOutcome {
                admitted: true,
                exclusion_reason: None,
                predictor_failure_detail: None,
                raw_flux_336_650_ph_m2_s: 1.0,
                selection_weighted_flux_336_650_ph_m2_s: 1.0,
                weighted_flux_300_650_ph_m2_s: 1.05,
                uv_flux_300_336_ph_m2_s: 0.05,
            },
            AttributionOutcome {
                admitted: false,
                exclusion_reason: Some("invalid_uv_predictors".into()),
                predictor_failure_detail: Some("missing_bp_rp".into()),
                raw_flux_336_650_ph_m2_s: 1e12,
                selection_weighted_flux_336_650_ph_m2_s: 1e12,
                weighted_flux_300_650_ph_m2_s: 0.0,
                uv_flux_300_336_ph_m2_s: 0.0,
            },
            AttributionOutcome {
                admitted: true,
                exclusion_reason: None,
                predictor_failure_detail: None,
                raw_flux_336_650_ph_m2_s: 2.0,
                selection_weighted_flux_336_650_ph_m2_s: 2.0,
                weighted_flux_300_650_ph_m2_s: 2.1,
                uv_flux_300_336_ph_m2_s: 0.1,
            },
        ];

        let mut forward_admitted = FluxAttributionAccumulator::default();
        let mut forward_reasons = BTreeMap::new();
        for outcome in &outcomes {
            record_outcome(&mut forward_admitted, &mut forward_reasons, outcome).unwrap();
        }
        let forward = (
            forward_admitted.into_row(),
            forward_reasons
                .into_iter()
                .map(|(k, v)| (k, v.into_row()))
                .collect::<BTreeMap<_, _>>(),
        );

        let mut reverse_admitted = FluxAttributionAccumulator::default();
        let mut reverse_reasons = BTreeMap::new();
        for outcome in outcomes.iter().rev() {
            record_outcome(&mut reverse_admitted, &mut reverse_reasons, outcome).unwrap();
        }
        let reverse = (
            reverse_admitted.into_row(),
            reverse_reasons
                .into_iter()
                .map(|(k, v)| (k, v.into_row()))
                .collect::<BTreeMap<_, _>>(),
        );

        let forward_bytes = serde_json::to_vec(&forward).unwrap();
        let reverse_bytes = serde_json::to_vec(&reverse).unwrap();
        assert_eq!(
            forward_bytes, reverse_bytes,
            "StableSum accumulation must yield byte-identical serialized rows regardless of HashMap-like insertion order"
        );
    }

    #[test]
    fn flux_attribution_report_serialization_is_deterministic() {
        let report = FluxAttributionReport {
            schema_version: 1,
            issue: "182".to_string(),
            experiment: "C_exclusion_flux_attribution".to_string(),
            commit: "deadbeef".to_string(),
            workspace: "/tmp/ws".to_string(),
            config_path: "/tmp/cfg.toml".to_string(),
            partitions: vec!["a".into(), "b".into()],
            partition_count: 2,
            observed_sources: 3,
            admitted_sources: 1,
            excluded_sources: 2,
            admitted: FluxAttributionRow {
                source_count: 1,
                sum_raw_flux_336_650_ph_m2_s: 1.0,
                sum_selection_weighted_flux_336_650_ph_m2_s: 1.0,
                sum_weighted_flux_300_650_ph_m2_s: 1.2,
                sum_uv_flux_300_336_ph_m2_s: 0.2,
                predictor_failure_detail: BTreeMap::new(),
            },
            by_exclusion_reason: BTreeMap::from([(
                "invalid_uv_predictors".to_string(),
                FluxAttributionRow {
                    source_count: 2,
                    sum_raw_flux_336_650_ph_m2_s: 10.0,
                    sum_selection_weighted_flux_336_650_ph_m2_s: 10.0,
                    sum_weighted_flux_300_650_ph_m2_s: 0.0,
                    sum_uv_flux_300_336_ph_m2_s: 0.0,
                    predictor_failure_detail: BTreeMap::from([("missing_bp_rp".into(), 2)]),
                },
            )]),
            totals: FluxAttributionTotals {
                admitted_weighted_flux_300_650_ph_m2_s: 1.2,
                admitted_weighted_flux_336_650_ph_m2_s: 1.0,
                admitted_uv_flux_300_336_ph_m2_s: 0.2,
                excluded_with_measured_336_650_source_count: 2,
                excluded_selection_weighted_flux_336_650_ph_m2_s: 10.0,
                invalid_uv_predictors_selection_weighted_flux_336_650_ph_m2_s: 10.0,
                counterfactual_retain_invalid_uv_measured_only_flux_ph_m2_s: 11.2,
                invalid_uv_lost_flux_over_admitted_combined: 10.0 / 1.2,
                invalid_uv_lost_flux_over_estimated_nsb2_total: 10.0 / (1.2 / 0.756),
            },
            interpretation: vec!["deterministic".into()],
        };
        let first = serde_json::to_vec_pretty(&report).unwrap();
        let second = serde_json::to_vec_pretty(&report).unwrap();
        assert_eq!(first, second);
        let roundtrip: FluxAttributionReport = serde_json::from_slice(&first).unwrap();
        assert_eq!(roundtrip, report);
    }

    #[test]
    fn load_partition_list_skips_comments_and_blank_lines() {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("partitions.txt");
        fs::write(&path, "# comment\n\n000001-000002\n000003-000004\n").unwrap();
        let list = load_partition_list(&path).unwrap();
        assert_eq!(list, vec!["000001-000002", "000003-000004"]);
        assert_eq!(
            default_output_path(temporary.path()),
            temporary.path().join("flux-attribution-report.json")
        );
    }

    fn gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes)?;
        Ok(encoder.finish()?)
    }

    fn product(id: &str, prefix: &str) -> GaiaProductConfig {
        GaiaProductConfig {
            id: id.to_string(),
            base_url: format!("https://example.test/{id}/"),
            checksum_manifest_url: format!("https://example.test/{id}/checksums"),
            checksum_manifest_sha256: "a".repeat(64),
            checksum_algorithm: OfficialChecksumAlgorithm::Md5,
            expected_partitions: Some(1),
            filename_prefix: prefix.to_string(),
            filename_suffix: ".csv.gz".to_string(),
        }
    }

    fn install_object_and_receipt(
        workspace: &Path,
        product: &GaiaProductConfig,
        partition: &str,
        bytes: &[u8],
        filename: &str,
    ) -> Result<()> {
        let object_sha = checksum_io::sha256_bytes(bytes);
        let object_path = workspace.join("cache/objects/sha256").join(&object_sha);
        artifact_store::atomic_write(&object_path, bytes)?;
        let entry = SourceInventoryEntry {
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            url: format!("{}{filename}", product.base_url),
            official_checksum: "0".repeat(32),
        };
        let inventory = SourceInventory {
            schema_version: 1,
            product_id: product.id.clone(),
            base_url: product.base_url.clone(),
            checksum_manifest_url: product.checksum_manifest_url.clone(),
            checksum_manifest_sha256: product.checksum_manifest_sha256.clone(),
            official_checksum_algorithm: product.checksum_algorithm,
            entries: vec![entry.clone()],
        };
        artifact_store::atomic_write(
            &workspace
                .join("inventories")
                .join(format!("{}.inventory.json", product.id)),
            &serde_json::to_vec_pretty(&inventory)?,
        )?;
        let receipt = AcquisitionReceipt {
            schema_version: 1,
            product_id: product.id.clone(),
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            source_url: entry.url,
            official_checksum_algorithm: product.checksum_algorithm,
            official_checksum: entry.official_checksum,
            sha256: object_sha,
            bytes: bytes.len() as u64,
            object_path,
        };
        artifact_store::atomic_write(
            &workspace
                .join("cache/receipts")
                .join(&product.id)
                .join(format!("{partition}.json")),
            &serde_json::to_vec_pretty(&receipt)?,
        )
    }

    fn write_minimal_starlight_config(
        path: &Path,
        workspace: &Path,
        products: &[GaiaProductConfig],
    ) -> Result<()> {
        let mut products_toml = String::new();
        for product in products {
            products_toml.push_str(&format!(
                r#"
[[starlight.gaia_products]]
id = "{id}"
base_url = "{base_url}"
checksum_manifest_url = "{checksum_manifest_url}"
checksum_manifest_sha256 = "{checksum_manifest_sha256}"
checksum_algorithm = "md5"
expected_partitions = 1
filename_prefix = "{filename_prefix}"
filename_suffix = "{filename_suffix}"
"#,
                id = product.id,
                base_url = product.base_url,
                checksum_manifest_url = product.checksum_manifest_url,
                checksum_manifest_sha256 = product.checksum_manifest_sha256,
                filename_prefix = product.filename_prefix,
                filename_suffix = product.filename_suffix,
            ));
        }
        let text = format!(
            r#"schema_version = 1
dataset = "starlight"

[workspace]
root = "{workspace}"

[starlight]
product_band = "combined-300-650"

[starlight.map]
canonical_nside = 128
{products_toml}
"#,
            workspace = workspace.display(),
        );
        fs::write(path, text)?;
        Ok(())
    }

    #[test]
    fn run_flux_attribution_smoke_one_partition() -> Result<()> {
        let temporary = TempDir::new()?;
        let workspace = temporary.path();
        let partition = "000000-003111";
        let oracle: Value = serde_json::from_slice(&fs::read(
            fixture_root().join("gaiaxpy_oracle/record-01.json"),
        )?)?;
        let source_id = oracle["source_id"].as_str().context("oracle source_id")?;
        let gaia_only = "999";
        let gaia_bytes = gzip_bytes(
            format!(
                "source_id,ra,dec,in_galaxy_candidates\n{source_id},45.0,20.0,false\n{gaia_only},50.0,10.0,true\n"
            )
            .as_bytes(),
        )?;
        let correlations = vec![0.0; 55 * 54 / 2];
        let arrays = |name: &str| serde_json::to_string(&oracle[name]).unwrap();
        let mut xp_csv = csv::Writer::from_writer(Vec::new());
        xp_csv.write_record([
            "source_id",
            "bp_n_parameters",
            "bp_standard_deviation",
            "rp_n_parameters",
            "rp_standard_deviation",
            "bp_coefficients",
            "bp_coefficient_errors",
            "bp_coefficient_correlations",
            "rp_coefficients",
            "rp_coefficient_errors",
            "rp_coefficient_correlations",
            "bp_n_relevant_bases",
            "rp_n_relevant_bases",
        ])?;
        xp_csv.write_record([
            source_id,
            "55",
            oracle["bp_standard_deviation"]
                .as_f64()
                .context("bp standard deviation")?
                .to_string()
                .as_str(),
            "55",
            oracle["rp_standard_deviation"]
                .as_f64()
                .context("rp standard deviation")?
                .to_string()
                .as_str(),
            &arrays("bp_coefficients"),
            &arrays("bp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            &arrays("rp_coefficients"),
            &arrays("rp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            "55",
            "55",
        ])?;
        let xp_bytes = gzip_bytes(&xp_csv.into_inner()?)?;
        let products = vec![
            product("gaia-source", "GaiaSource_"),
            product("xp-continuous", "XpContinuousMeanSpectrum_"),
        ];
        install_object_and_receipt(
            workspace,
            &products[0],
            partition,
            &gaia_bytes,
            "GaiaSource_000000-003111.csv.gz",
        )?;
        install_object_and_receipt(
            workspace,
            &products[1],
            partition,
            &xp_bytes,
            "XpContinuousMeanSpectrum_000000-003111.csv.gz",
        )?;

        let config_path = workspace.join("config.toml");
        write_minimal_starlight_config(&config_path, workspace, &products)?;
        let output = workspace.join("flux-attribution-report.json");
        let report = run_flux_attribution(
            workspace,
            &config_path,
            workspace,
            "test-commit",
            Some(&[partition.to_string()]),
            &output,
            None,
        )?;
        assert!(output.is_file());
        assert_eq!(report.observed_sources, 2);
        assert_eq!(report.partition_count, 1);
        assert!(
            report
                .by_exclusion_reason
                .contains_key("uv_correction_missing")
                || report
                    .by_exclusion_reason
                    .contains_key("scientific_exclusion_nonstellar")
                || report.by_exclusion_reason.contains_key("no_xp_spectrum"),
            "expected production exclusion reasons, got {:?}",
            report.by_exclusion_reason.keys().collect::<Vec<_>>()
        );
        // Deterministic re-run yields byte-identical report payload (commit/workspace fixed).
        let again = run_flux_attribution(
            workspace,
            &config_path,
            workspace,
            "test-commit",
            Some(&[partition.to_string()]),
            &workspace.join("flux-attribution-report-2.json"),
            None,
        )?;
        assert_eq!(
            serde_json::to_vec(&report)?,
            serde_json::to_vec(&again)?,
            "identical inputs must serialize identically"
        );
        Ok(())
    }
}
