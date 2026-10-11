// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Regression tests for sparse HEALPix anomaly and boundary diagnostics.

use super::*;
use crate::starlight::config::StarlightProductBand;
use std::fs;

fn pixel(flux: f64, admitted: u64, excluded: u64) -> CandidatePixel {
    CandidatePixel {
        flux_ph_m2_s: flux,
        statistical_uncertainty_ph_m2_s: 0.0,
        systematic_uncertainty_ph_m2_s: 0.0,
        total_uncertainty_ph_m2_s: 0.0,
        admitted_sources: admitted,
        excluded_sources: excluded,
    }
}

fn map(nside: u32, pixels: &[(u32, CandidatePixel)]) -> CandidateMap {
    CandidateMap {
        nside,
        schema: candidate_map::EXPECTED_MAP_SCHEMA.to_string(),
        flux_unit: candidate_map::EXPECTED_FLUX_UNIT.to_string(),
        sha256: String::new(),
        pixels: pixels.iter().copied().collect(),
    }
}

#[test]
fn anomaly_report_summarizes_sparse_parents_and_even_pixel_medians() -> Result<()> {
    let candidate = map(
        4,
        &[
            (0, pixel(2.0, 1, 3)),
            (1, pixel(8.0, 2, 1)),
            (4, pixel(30.0, 1, 0)),
            (8, pixel(0.0, 0, 4)),
        ],
    );
    let report = analyse_candidate_map(&candidate)?;
    assert_eq!(report.nside, 4);
    assert_eq!(report.parent_nside, 2);
    assert_eq!(report.pixel_count, 4);
    assert_eq!(report.parent_cells.len(), 3);
    assert_eq!(report.parent_cells[0].parent, 0);
    assert_eq!(report.parent_cells[0].pixel_count, 2);
    assert_eq!(report.parent_cells[0].total_flux_ph_m2_s, 10.0);
    assert_eq!(report.parent_cells[0].admitted_sources, 3);
    assert_eq!(report.parent_cells[0].excluded_sources, 4);
    assert_eq!(report.parent_cells[0].median_flux_per_admitted_source, 3.0);
    assert_eq!(report.parent_cells[1].median_flux_per_admitted_source, 30.0);
    assert_eq!(report.parent_cells[2].median_flux_per_admitted_source, 0.0);
    assert_eq!(report.global_median_flux_per_admitted_source, 30.0);
    assert!(report.anomalous_parents.is_empty());

    let mut report = report;
    report.detect_anomalous_parents(0.5);
    assert_eq!(report.anomalous_parents, vec![1]);
    report.detect_anomalous_parents(20.0);
    assert!(report.anomalous_parents.is_empty());
    Ok(())
}

#[test]
fn anomaly_report_treats_zero_admissions_and_empty_maps_as_zero_medians() -> Result<()> {
    let empty = analyse_candidate_map(&map(4, &[]))?;
    assert_eq!(empty.pixel_count, 0);
    assert_eq!(empty.global_median_flux_per_admitted_source, 0.0);
    assert!(empty.parent_cells.is_empty());

    let candidate = map(4, &[(0, pixel(0.0, 0, 2)), (4, pixel(0.0, 0, 1))]);
    let report = analyse_candidate_map(&candidate)?;
    assert_eq!(report.parent_cells.len(), 2);
    assert!(report
        .parent_cells
        .iter()
        .all(|cell| cell.median_flux_per_admitted_source == 0.0));
    assert_eq!(report.global_median_flux_per_admitted_source, 0.0);
    assert!(analyse_candidate_map(&map(2, &[(0, pixel(1.0, 1, 0))])).is_err());
    Ok(())
}

#[test]
fn anomaly_report_detects_strong_flux_outlier_against_multiple_parents() -> Result<()> {
    let candidate = map(
        4,
        &[
            (0, pixel(2.0, 1, 0)),
            (4, pixel(3.0, 1, 0)),
            (8, pixel(100.0, 1, 0)),
        ],
    );
    let report = analyse_candidate_map(&candidate)?;
    assert_eq!(report.global_median_flux_per_admitted_source, 3.0);
    assert_eq!(report.anomalous_parents, vec![2]);
    Ok(())
}

#[test]
fn boundary_metrics_ignore_nonpositive_or_unadmitted_pixels() -> Result<()> {
    let candidate = map(
        4,
        &[
            (0, pixel(1.0, 1, 0)),
            (1, pixel(0.0, 0, 1)),
            (2, pixel(-2.0, 1, 0)),
            (3, pixel(f64::NAN, 1, 0)),
        ],
    );
    let report = boundary_discontinuity_report(&candidate, 2)?;
    assert_eq!(report.parent_nside, 2);
    assert_eq!(report.median_internal_log_jump, 0.0);
    assert_eq!(report.median_cross_parent_log_jump, 0.0);
    assert_eq!(report.cross_to_internal_ratio, 0.0);
    assert!(boundary_discontinuity_report(&candidate, 4).is_err());
    Ok(())
}

#[test]
fn jump_and_median_helpers_handle_empty_even_and_odd_inputs() {
    assert_eq!(median(&[]), 0.0);
    assert_eq!(median(&[10.0]), 10.0);
    assert_eq!(median(&[8.0, 2.0, 4.0]), 4.0);
    assert_eq!(median(&[1.0, 9.0, 3.0, 7.0]), 7.0);
    assert!((log10_jump(100.0, 1.0) - 2.0).abs() < 1e-12);
    assert_eq!(log10_jump(10.0, 10.0), 0.0);
    assert_eq!(log10_jump(0.0, -2.0), 0.0);
}

#[test]
fn candidate_csv_writer_round_trips_metadata_flux_and_checksum() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("candidate.csv");
    let candidate = map(4, &[(7, pixel(2.5, 2, 1)), (2, pixel(1.25, 1, 3))]);
    write_candidate_map_csv(&candidate, &path)?;
    let sha256 = checksum_io::sha256_file(&path)?;
    let loaded = candidate_map::load(&path, 4, Some(&sha256))?;
    assert_eq!(loaded.pixels.len(), 2);
    assert_eq!(loaded.pixels[&2], pixel(1.25, 1, 3));
    assert_eq!(loaded.pixels[&7], pixel(2.5, 2, 1));
    assert_eq!(
        analyse_candidate_path(&path, 4, Some(&sha256))?.pixel_count,
        2
    );
    assert!(analyse_candidate_path(&path, 4, Some(&"0".repeat(64))).is_err());
    assert!(analyse_candidate_path(&path, 8, None).is_err());
    Ok(())
}

#[test]
fn workspace_anomaly_export_and_shard_loading_fail_closed_on_missing_data() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    assert!(analyse_workspace_shards(root).is_err());
    assert!(export_workspace_candidate_map(root, &root.join("map.csv")).is_err());
    assert!(load_production_shards(root).is_err());

    fs::create_dir_all(root.join("workers/part-a"))?;
    assert!(analyse_workspace_shards(root)
        .unwrap_err()
        .to_string()
        .contains("no shards"));
    assert!(export_workspace_candidate_map(root, &root.join("map.csv")).is_err());
    assert!(load_production_shards(root).is_err());

    fs::write(root.join("workers/part-a/shard.json"), b"not-json")?;
    assert!(analyse_workspace_shards(root).is_err());
    assert!(export_workspace_candidate_map(root, &root.join("map.csv")).is_err());
    assert!(load_production_shards(root).is_err());
    Ok(())
}

#[test]
fn production_shard_loader_prefers_outputs_and_skips_non_json_entries() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let output = root.join("outputs/shards");
    fs::create_dir_all(&output)?;
    assert!(load_production_shards(root)
        .unwrap_err()
        .to_string()
        .contains("no shards"));
    fs::write(output.join("ignored.txt"), "ignored")?;
    assert!(load_production_shards(root).is_err());
    let shard =
        PartitionShard::new_with_policy("part-a", 1, StarlightProductBand::Measured336To650, None)?;
    shard.write(&output.join("part-a.json"))?;
    let loaded = load_production_shards(root)?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].partition_id, "part-a");
    fs::write(output.join("bad.json"), "invalid")?;
    assert!(load_production_shards(root).is_err());
    Ok(())
}

#[test]
fn source_commit_requires_manifest_evidence_for_caller_assertions() -> Result<()> {
    let directory = tempfile::tempdir()?;
    assert_eq!(
        resolve_verified_source_commit(directory.path(), None)?,
        None
    );
    assert!(
        resolve_verified_source_commit(directory.path(), Some("expected"))
            .unwrap_err()
            .to_string()
            .contains("no RunManifest")
    );
    let nested = directory.path().join("runs/subdir");
    fs::create_dir_all(&nested)?;
    fs::write(nested.join("other.txt"), "{}")?;
    assert!(discover_workspace_software_commits(directory.path())?.is_empty());
    fs::write(nested.join("run.json"), b"invalid-json")?;
    assert!(discover_workspace_software_commits(directory.path()).is_err());
    Ok(())
}
