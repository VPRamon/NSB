//! Source-level diagnostic accounting and ablation for issue #116.

use super::baseline::load_smoke_partitions;
use super::{analyse_candidate_map, boundary_discontinuity_report, HealpixAnomalyReport, BoundaryDiscontinuityReport};
use crate::dataset::RunConfig;
use crate::starlight::config::ArtifactPinConfig;
use crate::starlight::healpix::nested_parent_at_coarser_nside;
use crate::starlight::photometric::PhotometricCorrection;
use crate::starlight::selection::SelectionCorrection;
use crate::starlight::sources::acquisition;
use crate::starlight::uv::UvCorrection;
use crate::starlight::validation::candidate_map::{CandidateMap, CandidatePixel};
use crate::starlight::worker::gaia_source::load_gaia_sources;
use crate::starlight::worker::processing::{evaluate_source_for_diagnostic, AblationStage, SourceOutcome};
use crate::starlight::xp::GaiaXpContinuousCalibrator;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub const TRACE_PARENTS_SMOKE: [u32; 7] = [13, 24, 32, 33, 36, 37, 43];
const CONTROL_PARENTS: [u32; 4] = [0, 1, 2, 3];
const MAX_TRACES_PER_PIXEL: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PixelDiagnosticCell {
    pub observed: u64,
    pub admitted: u64,
    pub exclusion_counts: BTreeMap<String, u64>,
    pub raw_flux_336_650_ph_m2_s: f64,
    pub raw_flux_300_650_ph_m2_s: f64,
    pub weighted_flux_300_650_ph_m2_s: f64,
    pub uv_flux_300_336_ph_m2_s: f64,
    pub sum_selection_weight: f64,
    pub xp_admitted: u64,
    pub photometric_admitted: u64,
    pub branch_counts: BTreeMap<String, u64>,
    pub weight_gt_1_1: u64,
    pub weight_gt_2: u64,
    pub weight_capped: u64,
    pub sum_completeness: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentSummary {
    pub parent: u32,
    pub parent_nside: u32,
    pub observed: u64,
    pub admitted: u64,
    pub excluded: u64,
    pub invalid_uv_predictors: u64,
    pub raw_flux_per_admitted: f64,
    pub weighted_flux_per_admitted: f64,
    pub xp_fraction: f64,
    pub mean_selection_weight: f64,
    pub exclusion_fraction_of_observed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AblationStageReport {
    pub stage: String,
    pub anomaly_report: HealpixAnomalyReport,
    pub boundary_report: BoundaryDiscontinuityReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTrace {
    pub source_id: u64,
    pub ra_deg: f64,
    pub dec_deg: f64,
    pub galactic_pixel: u32,
    pub parent_nside2: u32,
    pub trace_region: String,
    pub outcome: SourceOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSuiteReport {
    pub commit: String,
    pub workspace: String,
    pub partitions: Vec<String>,
    pub ablation_stages: Vec<AblationStageReport>,
    pub phase2_exclusion_gate: String,
    pub phase2_parent_summaries: Vec<ParentSummary>,
    pub phase2_control_parent_summaries: Vec<ParentSummary>,
    pub phase3_branch_gate: String,
    pub phase4_weighting_gate: String,
    pub source_traces: Vec<SourceTrace>,
}

pub type MergedDiagnosticReport = DiagnosticSuiteReport;

#[derive(Default)]
struct DiagnosticAccumulator {
    pixels: BTreeMap<u32, PixelDiagnosticCell>,
    traces: Vec<SourceTrace>,
    trace_budget: BTreeMap<u32, usize>,
}

pub fn run_diagnostic_suite(
    repo_root: &Path,
    config_path: &Path,
    workspace: &Path,
    commit: &str,
    output_dir: &Path,
) -> Result<DiagnosticSuiteReport> {
    let config_bytes = std::fs::read(config_path)?;
    let config: RunConfig =
        toml::from_slice(&config_bytes).with_context(|| format!("parse {}", config_path.display()))?;
    let partitions = load_smoke_partitions(repo_root)?;
    std::fs::create_dir_all(output_dir)?;

    let nside = config
        .starlight
        .as_ref()
        .context("config is not a Starlight run")?
        .map
        .canonical_nside;
    let stage_maps = process_partitions(&config, workspace, &partitions)?;
    let mut ablation_stages = Vec::new();
    for (stage, grid) in &stage_maps {
        let candidate = grid.to_candidate_map(nside)?;
        let mut anomaly = analyse_candidate_map(&candidate)?;
        anomaly.detect_anomalous_parents(5.0);
        let boundary = boundary_discontinuity_report(&candidate, 2)?;
        ablation_stages.push(AblationStageReport {
            stage: stage.label().to_string(),
            anomaly_report: anomaly,
            boundary_report: boundary,
        });
    }

    let full_grid = stage_maps
        .get(&AblationStage::E)
        .context("full production ablation stage missing")?;
    let phase2_parents = summarize_parents(full_grid, 2, &TRACE_PARENTS_SMOKE)?;
    let phase2_controls = summarize_parents(full_grid, 2, &CONTROL_PARENTS)?;
    let phase2_gate = evaluate_exclusion_gate(&phase2_parents, &phase2_controls);
    let phase3_gate = evaluate_branch_gate(&phase2_parents, &phase2_controls);
    let phase4_gate = evaluate_weighting_gate(&stage_maps);

    let report = DiagnosticSuiteReport {
        commit: commit.to_string(),
        workspace: workspace.display().to_string(),
        partitions,
        ablation_stages,
        phase2_exclusion_gate: phase2_gate,
        phase2_parent_summaries: phase2_parents,
        phase2_control_parent_summaries: phase2_controls,
        phase3_branch_gate: phase3_gate,
        phase4_weighting_gate: phase4_gate,
        source_traces: full_grid.traces.clone(),
    };
    std::fs::write(
        output_dir.join("diagnostic-suite.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

fn process_partitions(
    config: &RunConfig,
    workspace: &Path,
    partitions: &[String],
) -> Result<BTreeMap<AblationStage, DiagnosticAccumulator>> {
    let starlight = config
        .starlight
        .as_ref()
        .context("config is not a Starlight run")?;
    let products = &starlight.gaia_products;
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;
    let ultraviolet = load_uv(starlight.ultraviolet_correction.as_ref())?;
    let photometric = load_photometric(starlight.photometric_inference.as_ref())?;
    let selection = load_selection(starlight.selection_function.as_ref())?;
    let nside = starlight.map.canonical_nside;
    let product_band = starlight.product_band;
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

    let mut stage_maps: BTreeMap<AblationStage, DiagnosticAccumulator> = BTreeMap::new();
    for stage in AblationStage::all() {
        stage_maps.insert(stage, DiagnosticAccumulator::default());
    }

    let mut traced_pixels = HashSet::new();
    for partition in partitions {
        let gaia_path = acquisition::verified_object_for_partition(
            workspace, products, "gaia-source", partition,
        )?;
        let xp_path = acquisition::verified_object_for_partition(
            workspace, products, "xp-continuous", partition,
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
        for stage in AblationStage::all() {
            let grid = stage_maps.get_mut(&stage).unwrap();
            for (source_id, gaia_source) in &gaia_sources {
                let xp_product = xp_by_source.get(source_id);
                let outcome = evaluate_source_for_diagnostic(
                    gaia_source,
                    xp_product,
                    stage,
                    nside,
                    product_band,
                    ultraviolet.as_ref(),
                    photometric.as_ref(),
                    selection.as_ref(),
                );
                grid.record_outcome(*source_id, gaia_source, &outcome, stage);
                if stage == AblationStage::E {
                    maybe_trace(grid, *source_id, gaia_source, &outcome, nside, &mut traced_pixels);
                }
            }
        }
    }
    Ok(stage_maps)
}

impl DiagnosticAccumulator {
    fn record_outcome(
        &mut self,
        _source_id: u64,
        _gaia_source: &crate::starlight::worker::gaia_source::GaiaSourceEntry,
        outcome: &SourceOutcome,
        stage: AblationStage,
    ) {
        let pixel = outcome.galactic_pixel;
        let cell = self.pixels.entry(pixel).or_default();
        cell.observed += 1;
        if outcome.admitted {
            cell.admitted += 1;
            cell.raw_flux_336_650_ph_m2_s += outcome.raw_flux_336_650_ph_m2_s;
            cell.raw_flux_300_650_ph_m2_s += outcome.raw_flux_300_650_ph_m2_s;
            cell.weighted_flux_300_650_ph_m2_s += outcome.weighted_flux_300_650_ph_m2_s;
            cell.uv_flux_300_336_ph_m2_s += outcome.uv_flux_300_336_ph_m2_s;
            cell.sum_selection_weight += outcome.selection_weight;
            cell.sum_completeness += outcome.selection_completeness;
            if outcome.selection_weight > 1.1 {
                cell.weight_gt_1_1 += 1;
            }
            if outcome.selection_weight > 2.0 {
                cell.weight_gt_2 += 1;
            }
            if outcome.selection_capped {
                cell.weight_capped += 1;
            }
            if outcome.xp_available && outcome.population_branch.as_deref() == Some("xp_continuous") {
                cell.xp_admitted += 1;
            } else if outcome.admitted {
                cell.photometric_admitted += 1;
            }
            if let Some(branch) = &outcome.population_branch {
                *cell.branch_counts.entry(branch.clone()).or_default() += 1;
            }
        } else if let Some(reason) = &outcome.exclusion_reason {
            *cell.exclusion_counts.entry(reason.clone()).or_default() += 1;
        }
        let _ = stage;
    }

    fn to_candidate_map(&self, nside: u32) -> Result<CandidateMap> {
        let mut pixels = BTreeMap::new();
        for (pixel, cell) in &self.pixels {
            if cell.admitted == 0 {
                continue;
            }
            pixels.insert(
                *pixel,
                CandidatePixel {
                    flux_ph_m2_s: cell.weighted_flux_300_650_ph_m2_s,
                    statistical_uncertainty_ph_m2_s: 0.0,
                    systematic_uncertainty_ph_m2_s: 0.0,
                    total_uncertainty_ph_m2_s: 0.0,
                    admitted_sources: cell.admitted,
                    excluded_sources: cell.observed.saturating_sub(cell.admitted),
                },
            );
        }
        Ok(CandidateMap {
            nside,
            schema: "diagnostic".to_string(),
            flux_unit: "ph/m2/s".to_string(),
            sha256: String::new(),
            pixels,
        })
    }

    fn raw_candidate_map(&self, nside: u32) -> Result<CandidateMap> {
        let mut pixels = BTreeMap::new();
        for (pixel, cell) in &self.pixels {
            if cell.admitted == 0 {
                continue;
            }
            pixels.insert(
                *pixel,
                CandidatePixel {
                    flux_ph_m2_s: cell.raw_flux_300_650_ph_m2_s,
                    statistical_uncertainty_ph_m2_s: 0.0,
                    systematic_uncertainty_ph_m2_s: 0.0,
                    total_uncertainty_ph_m2_s: 0.0,
                    admitted_sources: cell.admitted,
                    excluded_sources: cell.observed.saturating_sub(cell.admitted),
                },
            );
        }
        Ok(CandidateMap {
            nside,
            schema: "diagnostic-raw".to_string(),
            flux_unit: "ph/m2/s".to_string(),
            sha256: String::new(),
            pixels,
        })
    }
}

fn maybe_trace(
    grid: &mut DiagnosticAccumulator,
    source_id: u64,
    gaia_source: &crate::starlight::worker::gaia_source::GaiaSourceEntry,
    outcome: &SourceOutcome,
    nside: u32,
    traced_pixels: &mut HashSet<u32>,
) {
    let parent = nested_parent_at_coarser_nside(outcome.galactic_pixel, nside, 2).unwrap_or(0);
    let region = if TRACE_PARENTS_SMOKE.contains(&parent) {
        "anomalous_parent"
    } else if CONTROL_PARENTS.contains(&parent) {
        "control_parent"
    } else {
        return;
    };
    let budget = grid.trace_budget.entry(outcome.galactic_pixel).or_insert(0);
    if *budget >= MAX_TRACES_PER_PIXEL {
        return;
    }
    *budget += 1;
    traced_pixels.insert(outcome.galactic_pixel);
    grid.traces.push(SourceTrace {
        source_id,
        ra_deg: gaia_source.icrs.ra_deg,
        dec_deg: gaia_source.icrs.dec_deg,
        galactic_pixel: outcome.galactic_pixel,
        parent_nside2: parent,
        trace_region: region.to_string(),
        outcome: outcome.clone(),
    });
}

fn summarize_parents(
    grid: &DiagnosticAccumulator,
    parent_nside: u32,
    parents: &[u32],
) -> Result<Vec<ParentSummary>> {
    let nside = 128_u32;
    let mut out = Vec::new();
    for &parent in parents {
        let mut observed = 0_u64;
        let mut admitted = 0_u64;
        let mut invalid_uv = 0_u64;
        let mut raw_flux = 0.0;
        let mut weighted_flux = 0.0;
        let mut xp = 0_u64;
        let mut sum_weight = 0.0;
        for (pixel, cell) in &grid.pixels {
            if nested_parent_at_coarser_nside(*pixel, nside, parent_nside)? != parent {
                continue;
            }
            observed += cell.observed;
            admitted += cell.admitted;
            raw_flux += cell.raw_flux_300_650_ph_m2_s;
            weighted_flux += cell.weighted_flux_300_650_ph_m2_s;
            xp += cell.xp_admitted;
            sum_weight += cell.sum_selection_weight;
            invalid_uv += cell
                .exclusion_counts
                .get("invalid_uv_predictors")
                .copied()
                .unwrap_or(0);
        }
        let excluded = observed.saturating_sub(admitted);
        out.push(ParentSummary {
            parent,
            parent_nside,
            observed,
            admitted,
            excluded,
            invalid_uv_predictors: invalid_uv,
            raw_flux_per_admitted: if admitted > 0 {
                raw_flux / admitted as f64
            } else {
                0.0
            },
            weighted_flux_per_admitted: if admitted > 0 {
                weighted_flux / admitted as f64
            } else {
                0.0
            },
            xp_fraction: if admitted > 0 {
                xp as f64 / admitted as f64
            } else {
                0.0
            },
            mean_selection_weight: if admitted > 0 {
                sum_weight / admitted as f64
            } else {
                0.0
            },
            exclusion_fraction_of_observed: if observed > 0 {
                excluded as f64 / observed as f64
            } else {
                0.0
            },
        });
    }
    Ok(out)
}

fn evaluate_exclusion_gate(anomalous: &[ParentSummary], control: &[ParentSummary]) -> String {
    let anom_excl = mean(anomalous.iter().map(|p| p.exclusion_fraction_of_observed));
    let ctrl_excl = mean(control.iter().map(|p| p.exclusion_fraction_of_observed));
    let anom_uv = mean(
        anomalous
            .iter()
            .map(|p| p.invalid_uv_predictors as f64 / p.observed.max(1) as f64),
    );
    let ctrl_uv = mean(
        control
            .iter()
            .map(|p| p.invalid_uv_predictors as f64 / p.observed.max(1) as f64),
    );
    if anom_excl > ctrl_excl * 1.25 || anom_uv > ctrl_uv * 1.25 {
        format!(
            "A_strong_spatial_correlation: anomalous exclusion_frac={anom_excl:.4} vs control={ctrl_excl:.4}; invalid_uv_frac={anom_uv:.4} vs {ctrl_uv:.4}"
        )
    } else {
        format!(
            "B_no_meaningful_spatial_correlation: anomalous exclusion_frac={anom_excl:.4} vs control={ctrl_excl:.4}; invalid_uv_frac={anom_uv:.4} vs {ctrl_uv:.4}"
        )
    }
}

fn evaluate_branch_gate(anomalous: &[ParentSummary], control: &[ParentSummary]) -> String {
    let anom_xp = mean(anomalous.iter().map(|p| p.xp_fraction));
    let ctrl_xp = mean(control.iter().map(|p| p.xp_fraction));
    if (anom_xp - ctrl_xp).abs() > 0.05 {
        format!("A_branch_mix_differs: anomalous xp_fraction={anom_xp:.4} vs control={ctrl_xp:.4}")
    } else {
        format!("B_branch_mix_similar: anomalous xp_fraction={anom_xp:.4} vs control={ctrl_xp:.4}")
    }
}

fn evaluate_weighting_gate(stage_maps: &BTreeMap<AblationStage, DiagnosticAccumulator>) -> String {
    let d_grid = stage_maps.get(&AblationStage::D).unwrap();
    let c_grid = stage_maps.get(&AblationStage::C).unwrap();
    let weighted = d_grid.to_candidate_map(128).ok();
    let raw = c_grid.raw_candidate_map(128).ok();
    if let (Some(weighted), Some(raw)) = (weighted, raw) {
        if let (Ok(w), Ok(r)) = (analyse_candidate_map(&weighted), analyse_candidate_map(&raw)) {
            let w_count = w.anomalous_parents.len();
            let r_count = r.anomalous_parents.len();
            if w_count > r_count.saturating_add(1) {
                return format!(
                    "A_patches_appear_with_selection_weighting: weighted_anomalous={w_count} pre_selection_anomalous={r_count}"
                );
            }
            if w_count + 2 < r_count {
                return format!(
                    "B_patches_exist_before_selection_weighting: pre_selection_anomalous={r_count} weighted_anomalous={w_count}"
                );
            }
        }
    }
    "B_selection_weighting_not_primary_patch_driver_on_smoke".to_string()
}

fn mean<I: Iterator<Item = f64>>(values: I) -> f64 {
    let mut sum = 0.0;
    let mut count = 0_u64;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn load_uv(pin: Option<&ArtifactPinConfig>) -> Result<Option<UvCorrection>> {
    pin.map(|config| {
        let correction = UvCorrection::load(&config.artifact_path, &config.sha256)?;
        correction.require_production_status()?;
        Ok(correction)
    })
    .transpose()
}

fn load_photometric(pin: Option<&ArtifactPinConfig>) -> Result<Option<PhotometricCorrection>> {
    pin.map(|config| {
        let correction = PhotometricCorrection::load(&config.artifact_path, &config.sha256)?;
        correction.require_production_status()?;
        Ok(correction)
    })
    .transpose()
}

fn load_selection(pin: Option<&ArtifactPinConfig>) -> Result<Option<SelectionCorrection>> {
    pin.map(|config| {
        let correction = SelectionCorrection::load(&config.artifact_path, &config.sha256)?;
        correction.require_production_status()?;
        Ok(correction)
    })
    .transpose()
}
