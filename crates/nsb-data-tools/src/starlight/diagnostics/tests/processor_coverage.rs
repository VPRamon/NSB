// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Deterministic unit coverage for Starlight diagnostic accounting and gates.

use super::*;
use crate::starlight::healpix::IcrsSkyPosition;
use crate::starlight::worker::gaia_source::GaiaSourceEntry;

fn source() -> GaiaSourceEntry {
    GaiaSourceEntry {
        source_id: 42,
        icrs: IcrsSkyPosition::new(10.0, 20.0).unwrap(),
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

fn outcome(pixel: u32, admitted: bool, branch: Option<&str>) -> SourceOutcome {
    SourceOutcome {
        admitted,
        exclusion_reason: None,
        population_branch: branch.map(str::to_owned),
        xp_available: branch == Some("xp_continuous"),
        galactic_pixel: pixel,
        selection_healpix: None,
        raw_flux_336_650_ph_m2_s: 6.0,
        raw_flux_300_650_ph_m2_s: 10.0,
        uv_flux_300_336_ph_m2_s: 4.0,
        weighted_flux_300_650_ph_m2_s: 30.0,
        selection_weight: 3.0,
        selection_completeness: 0.25,
        selection_capped: true,
        g_mag: Some(15.0),
        bp_rp: Some(1.0),
    }
}

#[test]
fn accumulator_counts_admitted_xp_and_photometric_branches() {
    let source = source();
    let mut grid = DiagnosticAccumulator::default();
    grid.record_outcome(
        1,
        &source,
        &outcome(0, true, Some("xp_continuous")),
        AblationStage::E,
    );
    let mut fallback = outcome(0, true, Some("photometric_g_only"));
    fallback.xp_available = false;
    fallback.selection_weight = 1.1;
    fallback.selection_capped = false;
    grid.record_outcome(2, &source, &fallback, AblationStage::B);
    let cell = &grid.pixels[&0];
    assert_eq!((cell.observed, cell.admitted), (2, 2));
    assert_eq!((cell.xp_admitted, cell.photometric_admitted), (1, 1));
    assert_eq!(
        (cell.raw_flux_336_650_ph_m2_s, cell.raw_flux_300_650_ph_m2_s),
        (12.0, 20.0)
    );
    assert_eq!(
        (
            cell.weighted_flux_300_650_ph_m2_s,
            cell.uv_flux_300_336_ph_m2_s
        ),
        (60.0, 8.0)
    );
    assert_eq!(cell.sum_selection_weight, 4.1);
    assert_eq!(cell.sum_completeness, 0.5);
    assert_eq!(
        (cell.weight_gt_1_1, cell.weight_gt_2, cell.weight_capped),
        (1, 1, 1)
    );
    assert_eq!(cell.branch_counts["xp_continuous"], 1);
    assert_eq!(cell.branch_counts["photometric_g_only"], 1);
}

#[test]
fn accumulator_records_exclusion_without_contributing_flux() {
    let source = source();
    let mut grid = DiagnosticAccumulator::default();
    let mut denied = outcome(5, false, Some("xp_continuous"));
    denied.exclusion_reason = Some("invalid_uv_predictors".into());
    grid.record_outcome(1, &source, &denied, AblationStage::E);
    grid.record_outcome(2, &source, &denied, AblationStage::E);
    let unclassified = outcome(5, false, None);
    grid.record_outcome(3, &source, &unclassified, AblationStage::E);
    let cell = &grid.pixels[&5];
    assert_eq!((cell.observed, cell.admitted), (3, 0));
    assert_eq!(cell.exclusion_counts["invalid_uv_predictors"], 2);
    assert_eq!(cell.raw_flux_300_650_ph_m2_s, 0.0);
    assert_eq!(cell.weighted_flux_300_650_ph_m2_s, 0.0);
    assert!(cell.branch_counts.is_empty());
}

#[test]
fn diagnostic_candidate_maps_skip_excluded_pixels_and_preserve_counts() -> Result<()> {
    let source = source();
    let mut grid = DiagnosticAccumulator::default();
    let mut excluded = outcome(7, false, None);
    excluded.exclusion_reason = Some("excluded".into());
    grid.record_outcome(1, &source, &excluded, AblationStage::E);
    grid.record_outcome(
        2,
        &source,
        &outcome(8, true, Some("xp_continuous")),
        AblationStage::E,
    );
    grid.record_outcome(3, &source, &outcome(8, false, None), AblationStage::E);

    let weighted = grid.to_candidate_map(128)?;
    let raw = grid.raw_candidate_map(128)?;
    assert_eq!(weighted.schema, "diagnostic");
    assert_eq!(raw.schema, "diagnostic-raw");
    assert_eq!(weighted.flux_unit, "ph/m2/s");
    assert!(!weighted.pixels.contains_key(&7));
    assert_eq!(weighted.pixels[&8].flux_ph_m2_s, 30.0);
    assert_eq!(raw.pixels[&8].flux_ph_m2_s, 10.0);
    assert_eq!(weighted.pixels[&8].admitted_sources, 1);
    assert_eq!(weighted.pixels[&8].excluded_sources, 1);
    assert_eq!(raw.pixels[&8].total_uncertainty_ph_m2_s, 0.0);
    Ok(())
}

#[test]
fn parent_summaries_aggregate_flux_exclusions_and_guard_zero_denominators() -> Result<()> {
    let mut grid = DiagnosticAccumulator::default();
    let mut first = PixelDiagnosticCell {
        observed: 5,
        admitted: 2,
        raw_flux_300_650_ph_m2_s: 20.0,
        weighted_flux_300_650_ph_m2_s: 40.0,
        xp_admitted: 1,
        sum_selection_weight: 6.0,
        ..Default::default()
    };
    first
        .exclusion_counts
        .insert("invalid_uv_predictors".into(), 2);
    grid.pixels.insert(13 * 4096, first);
    grid.pixels.insert(
        13 * 4096 + 1,
        PixelDiagnosticCell {
            observed: 3,
            admitted: 1,
            raw_flux_300_650_ph_m2_s: 10.0,
            weighted_flux_300_650_ph_m2_s: 20.0,
            xp_admitted: 1,
            sum_selection_weight: 3.0,
            ..Default::default()
        },
    );
    let summaries = summarize_parents(&grid, 2, &[13, 14])?;
    let populated = &summaries[0];
    assert_eq!((populated.parent, populated.parent_nside), (13, 2));
    assert_eq!(
        (populated.observed, populated.admitted, populated.excluded),
        (8, 3, 5)
    );
    assert_eq!(populated.invalid_uv_predictors, 2);
    assert_eq!(populated.raw_flux_per_admitted, 10.0);
    assert_eq!(populated.weighted_flux_per_admitted, 20.0);
    assert!((populated.xp_fraction - 2.0 / 3.0).abs() < 1e-12);
    assert_eq!(populated.mean_selection_weight, 3.0);
    assert!((populated.exclusion_fraction_of_observed - 5.0 / 8.0).abs() < 1e-12);
    let empty = &summaries[1];
    assert_eq!((empty.observed, empty.admitted, empty.excluded), (0, 0, 0));
    assert_eq!(empty.xp_fraction, 0.0);
    assert_eq!(empty.exclusion_fraction_of_observed, 0.0);
    assert_eq!(empty.raw_flux_per_admitted, 0.0);
    Ok(())
}

#[test]
fn traces_are_limited_per_pixel_and_classified_by_parent() {
    let source = source();
    let mut grid = DiagnosticAccumulator::default();
    let mut traced = HashSet::new();
    let control = outcome(0, true, Some("xp_continuous"));
    for id in 0..(MAX_TRACES_PER_PIXEL as u64 + 3) {
        maybe_trace(&mut grid, id, &source, &control, 128, &mut traced);
    }
    assert_eq!(grid.traces.len(), MAX_TRACES_PER_PIXEL);
    assert_eq!(grid.trace_budget[&0], MAX_TRACES_PER_PIXEL);
    assert_eq!(grid.traces[0].trace_region, "control_parent");
    assert_eq!(grid.traces[0].parent_nside2, 0);
    assert!(traced.contains(&0));

    maybe_trace(
        &mut grid,
        99,
        &source,
        &outcome(13 * 4096, true, None),
        128,
        &mut traced,
    );
    assert_eq!(grid.traces.last().unwrap().trace_region, "anomalous_parent");
    assert_eq!(grid.traces.last().unwrap().parent_nside2, 13);
    let previous_count = grid.traces.len();
    maybe_trace(
        &mut grid,
        100,
        &source,
        &outcome(4 * 4096, true, None),
        128,
        &mut traced,
    );
    assert_eq!(grid.traces.len(), previous_count);
}

fn parent_summary(xp_fraction: f64, exclusion_fraction: f64, invalid_uv: u64) -> ParentSummary {
    ParentSummary {
        parent: 13,
        parent_nside: 2,
        observed: 100,
        admitted: 50,
        excluded: 50,
        invalid_uv_predictors: invalid_uv,
        raw_flux_per_admitted: 1.0,
        weighted_flux_per_admitted: 1.0,
        xp_fraction,
        mean_selection_weight: 1.0,
        exclusion_fraction_of_observed: exclusion_fraction,
    }
}

#[test]
fn exclusion_gate_distinguishes_correlated_exclusions_and_uv_failures() {
    let control = [parent_summary(0.5, 0.1, 1)];
    assert!(
        evaluate_exclusion_gate(&[parent_summary(0.5, 0.5, 1)], &control)
            .starts_with("A_strong_spatial_correlation")
    );
    assert!(
        evaluate_exclusion_gate(&[parent_summary(0.5, 0.1, 30)], &control)
            .starts_with("A_strong_spatial_correlation")
    );
    assert!(
        evaluate_exclusion_gate(&[parent_summary(0.5, 0.1, 1)], &control)
            .starts_with("B_no_meaningful_spatial_correlation")
    );
    assert!(evaluate_exclusion_gate(&[], &[]).starts_with("B_no_meaningful_spatial_correlation"));
}

#[test]
fn branch_gate_detects_material_xp_population_differences() {
    let control = [parent_summary(0.5, 0.1, 0)];
    assert!(
        evaluate_branch_gate(&[parent_summary(0.8, 0.1, 0)], &control)
            .starts_with("A_branch_mix_differs")
    );
    assert!(
        evaluate_branch_gate(&[parent_summary(0.52, 0.1, 0)], &control)
            .starts_with("B_branch_mix_similar")
    );
    assert!(evaluate_branch_gate(&[], &[]).starts_with("B_branch_mix_similar"));
}

#[test]
fn weighting_gate_handles_empty_sparse_maps() {
    let mut stages = BTreeMap::new();
    stages.insert(AblationStage::C, DiagnosticAccumulator::default());
    stages.insert(AblationStage::D, DiagnosticAccumulator::default());
    assert_eq!(
        evaluate_weighting_gate(&stages),
        "B_selection_weighting_not_primary_patch_driver_on_smoke"
    );
}

#[test]
fn arithmetic_mean_handles_empty_singleton_and_multiple_values() {
    assert_eq!(mean(std::iter::empty()), 0.0);
    assert_eq!(mean([7.0].into_iter()), 7.0);
    assert_eq!(mean([2.0, 4.0, 9.0].into_iter()), 5.0);
}

#[test]
fn unconfigured_corrections_are_optional_but_bad_pins_fail_closed() {
    assert!(load_uv(None).unwrap().is_none());
    assert!(load_photometric(None, None).unwrap().is_none());
    assert!(load_selection(None).unwrap().is_none());
    let invalid = ArtifactPinConfig {
        artifact_path: std::path::PathBuf::from("/nonexistent/nsb-test-correction.json"),
        sha256: "0".repeat(64),
    };
    assert!(load_uv(Some(&invalid)).is_err());
    assert!(load_photometric(Some(&invalid), None).is_err());
    assert!(load_selection(Some(&invalid)).is_err());
    assert!(load_photometric(
        None,
        Some(PhotometricArtifactOverride {
            path: invalid.artifact_path.clone(),
            sha256: invalid.sha256.clone(),
        })
    )
    .is_err());
}

#[test]
fn diagnostic_suite_rejects_missing_or_invalid_config_before_loading_sources() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("absent.toml");
    let output = directory.path().join("output");
    assert!(run_diagnostic_suite(
        directory.path(),
        &missing,
        directory.path(),
        "commit",
        &output,
        None,
    )
    .is_err());
    let malformed = directory.path().join("invalid.toml");
    std::fs::write(&malformed, b"= bad TOML")?;
    assert!(run_diagnostic_suite(
        directory.path(),
        &malformed,
        directory.path(),
        "commit",
        &output,
        None,
    )
    .is_err());
    Ok(())
}


#[test]
fn empty_offline_diagnostic_run_produces_all_ablation_stages_and_fails_scientifically() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let repository = directory.path().join("repo");
    let partitions = repository.join(
        crate::starlight::diagnostics::baseline::SMOKE_PARTITIONS_PATH,
    );
    std::fs::create_dir_all(partitions.parent().unwrap())?;
    std::fs::write(&partitions, "# deliberately empty diagnostic control\n")?;
    let config = directory.path().join("config.toml");
    std::fs::write(
        &config,
        include_str!("../../../../config/starlight-production.toml"),
    )?;
    let workspace = directory.path().join("workspace");
    let output = directory.path().join("results");
    let report = run_diagnostic_suite(
        &repository,
        &config,
        &workspace,
        "synthetic-no-data",
        &output,
        None,
    )?;
    assert!(report.partitions.is_empty());
    assert_eq!(report.ablation_stages.len(), AblationStage::all().len());
    assert_eq!(report.phase2_parent_summaries.len(), TRACE_PARENTS_SMOKE.len());
    assert_eq!(report.phase2_control_parent_summaries.len(), CONTROL_PARENTS.len());
    assert!(report.source_traces.is_empty());
    assert!(report.phase2_exclusion_gate.starts_with("B_no_meaningful"));
    assert!(report.phase3_branch_gate.starts_with("B_branch_mix"));
    assert_eq!(
        report.phase4_weighting_gate,
        "B_selection_weighting_not_primary_patch_driver_on_smoke",
    );
    let json: DiagnosticSuiteReport =
        serde_json::from_slice(&std::fs::read(output.join("diagnostic-suite.json"))?)?;
    assert_eq!(json.commit, "synthetic-no-data");
    assert_eq!(json.ablation_stages.len(), report.ablation_stages.len());
    Ok(())
}
