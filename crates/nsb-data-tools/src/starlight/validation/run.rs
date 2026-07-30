//! Orchestration for `nsb-data dataset starlight validation run`.
//!
//! This never sets `scientifically_validated = true` and never fabricates a
//! metric: when no acquired-and-transformed reference data is available for
//! a region, that region is simply absent from `reference_results`, and the
//! overall run is reported with `technical_gates_passed = false`.

use super::candidate_map::{self, CandidateMap};
use super::metrics::{self, MetricsSummary};
use super::preregistration::{Preregistration, Tolerances};
use super::references::{ReferenceStatus, ReferencesDocument};
use super::regions::{RegionEngine, RegionsDocument};
use super::report::{
    ReferenceRunStatus, ReferenceValidationResult, RegionMetricsEntry, ValidationResults,
    VALIDATION_RESULTS_SCHEMA_VERSION,
};
use super::transformed_grid;
use crate::platform::{artifact_store, checksum_io};
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RunInputs {
    pub preregistration: PathBuf,
    pub references: PathBuf,
    pub regions: PathBuf,
    pub candidate_map: PathBuf,
    pub candidate_map_sha256: Option<String>,
    pub references_workspace: PathBuf,
    pub output: PathBuf,
}

pub fn run(inputs: &RunInputs) -> Result<ValidationResults> {
    let preregistration_bytes = fs::read(&inputs.preregistration)
        .with_context(|| format!("read preregistration {}", inputs.preregistration.display()))?;
    let preregistration_sha256 = checksum_io::sha256_bytes(&preregistration_bytes);
    let preregistration: Preregistration = toml::from_str(
        std::str::from_utf8(&preregistration_bytes)
            .with_context(|| format!("{} is not valid UTF-8", inputs.preregistration.display()))?,
    )
    .with_context(|| format!("parse preregistration {}", inputs.preregistration.display()))?;
    preregistration.validate()?;

    let references_bytes = fs::read(&inputs.references)
        .with_context(|| format!("read references {}", inputs.references.display()))?;
    let references_sha256 = checksum_io::sha256_bytes(&references_bytes);
    let references: ReferencesDocument = toml::from_str(
        std::str::from_utf8(&references_bytes)
            .with_context(|| format!("{} is not valid UTF-8", inputs.references.display()))?,
    )
    .with_context(|| format!("parse references {}", inputs.references.display()))?;
    references.validate()?;

    let regions_bytes = fs::read(&inputs.regions)
        .with_context(|| format!("read regions {}", inputs.regions.display()))?;
    let regions_sha256 = checksum_io::sha256_bytes(&regions_bytes);
    let regions: RegionsDocument = serde_json::from_slice(&regions_bytes)
        .with_context(|| format!("parse regions {}", inputs.regions.display()))?;
    regions.validate()?;

    let candidate = candidate_map::load(
        &inputs.candidate_map,
        regions.nside,
        inputs.candidate_map_sha256.as_deref(),
    )?;
    if candidate.schema != preregistration.candidate.map_schema {
        bail!(
            "candidate map schema {} does not match preregistration schema {}",
            candidate.schema,
            preregistration.candidate.map_schema
        );
    }

    let engine = RegionEngine::build(regions.nside)?;
    let region_pixels = engine.resolve(&regions, &candidate.admitted_sources())?;
    let region_ids = regions
        .regions
        .iter()
        .map(|region| region.id.clone())
        .collect::<Vec<_>>();

    let mut reference_statuses = Vec::with_capacity(references.references.len());
    let mut reference_results = Vec::new();
    let mut technical_gate_failures = Vec::new();

    for reference in &references.references {
        if reference.status != ReferenceStatus::Acquired {
            reference_statuses.push(ReferenceRunStatus {
                reference_id: reference.id.clone(),
                status: "pending-acquisition".to_string(),
                detail: reference.acquisition_notes.clone(),
            });
            continue;
        }
        let grid_path = inputs
            .references_workspace
            .join(&reference.id)
            .join("transformed-grid-v1.csv");
        let grid = transformed_grid::load_if_present(&grid_path, regions.nside)?;
        let Some(grid) = grid else {
            reference_statuses.push(ReferenceRunStatus {
                reference_id: reference.id.clone(),
                status: "acquired-awaiting-transform".to_string(),
                detail: format!(
                    "reference is acquired but no transformed grid was found at {}; the reference-specific transformation to the target band/units is out of scope for this scaffolding PR",
                    grid_path.display()
                ),
            });
            continue;
        };

        let mut region_metrics = Vec::new();
        for region_id in &region_ids {
            let pixels = &region_pixels[region_id];
            let (candidate_values, reference_values, sigmas) =
                gather_comparable_pixels(pixels, &candidate, &grid);
            if candidate_values.len() < 2 {
                continue;
            }
            let summary = metrics::compute(&candidate_values, &reference_values, &sigmas)?;
            let tolerance_failures =
                evaluate_tolerances(region_id, &summary, &preregistration.tolerances);
            technical_gate_failures.extend(
                tolerance_failures
                    .iter()
                    .map(|failure| format!("{}/{region_id}: {failure}", reference.id)),
            );
            region_metrics.push(RegionMetricsEntry {
                region_id: region_id.clone(),
                metrics: summary,
                tolerance_failures,
            });
        }
        let evaluated_any = !region_metrics.is_empty();
        reference_statuses.push(ReferenceRunStatus {
            reference_id: reference.id.clone(),
            status: if evaluated_any {
                "evaluated".to_string()
            } else {
                "acquired-insufficient-overlap".to_string()
            },
            detail: format!(
                "grid sha256 {} intersected with candidate map over {} of {} declared regions",
                grid.sha256,
                region_metrics.len(),
                region_ids.len()
            ),
        });
        if evaluated_any {
            reference_results.push(ReferenceValidationResult {
                reference_id: reference.id.clone(),
                region_metrics,
            });
        }
    }

    if reference_results.is_empty() {
        technical_gate_failures.push(
            "no acquired-and-transformed reference data was available; validation is pending acquisition (see #87/#47)"
                .to_string(),
        );
    }
    let technical_gates_passed =
        !reference_results.is_empty() && technical_gate_failures.is_empty();

    let results = ValidationResults {
        schema_version: VALIDATION_RESULTS_SCHEMA_VERSION,
        generated_at_unix_seconds: unix_seconds()?,
        issue: 87,
        preregistration_sha256,
        references_sha256,
        regions_sha256,
        candidate_map_path: inputs.candidate_map.display().to_string(),
        candidate_map_sha256: candidate.sha256.clone(),
        candidate_map_pinned_sha256: inputs.candidate_map_sha256.clone(),
        band_nm: preregistration.band_nm,
        flux_unit: preregistration.flux_unit.clone(),
        region_ids,
        reference_statuses,
        reference_results,
        technical_gates_passed,
        technical_gate_failures,
        scientific_review_status: "pending".to_string(),
        scientifically_validated: false,
        notes: "Technical scaffolding for issue #87. Scientific approval is recorded only in issue #47 and is never inferred from this report.".to_string(),
    };
    results.assert_never_scientifically_validated();

    write_outputs(&inputs.output, &results, inputs)?;
    Ok(results)
}

fn gather_comparable_pixels(
    pixels: &BTreeSet<u32>,
    candidate: &CandidateMap,
    grid: &transformed_grid::TransformedReferenceGrid,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut candidate_values = Vec::new();
    let mut reference_values = Vec::new();
    let mut sigmas = Vec::new();
    for pixel in pixels {
        let (Some(candidate_pixel), Some(grid_pixel)) =
            (candidate.pixels.get(pixel), grid.pixels.get(pixel))
        else {
            continue;
        };
        candidate_values.push(candidate_pixel.flux_ph_m2_s);
        reference_values.push(grid_pixel.value_ph_m2_s);
        sigmas.push(candidate_pixel.total_uncertainty_ph_m2_s);
    }
    (candidate_values, reference_values, sigmas)
}

fn evaluate_tolerances(
    region_id: &str,
    metrics: &MetricsSummary,
    tolerances: &Tolerances,
) -> Vec<String> {
    let mut failures = Vec::new();
    if region_id == "all-sky" && metrics.absolute_bias > tolerances.absolute_all_sky_bias_max {
        failures.push(format!(
            "absolute_all_sky_bias {:.6e} exceeds preregistered maximum {:.6e}",
            metrics.absolute_bias, tolerances.absolute_all_sky_bias_max
        ));
    }
    if metrics.relative_error_p50 > tolerances.median_absolute_regional_relative_error_max {
        failures.push(format!(
            "median relative error {:.6} exceeds preregistered maximum {:.6}",
            metrics.relative_error_p50, tolerances.median_absolute_regional_relative_error_max
        ));
    }
    if metrics.relative_error_p95 > tolerances.regional_relative_error_p95_max {
        failures.push(format!(
            "p95 relative error {:.6} exceeds preregistered maximum {:.6}",
            metrics.relative_error_p95, tolerances.regional_relative_error_p95_max
        ));
    }
    if metrics.coverage_68 < tolerances.coverage_68_min
        || metrics.coverage_68 > tolerances.coverage_68_max
    {
        failures.push(format!(
            "coverage_68 {:.6} is outside preregistered range [{:.6}, {:.6}]",
            metrics.coverage_68, tolerances.coverage_68_min, tolerances.coverage_68_max
        ));
    }
    if metrics.coverage_95 < tolerances.coverage_95_min
        || metrics.coverage_95 > tolerances.coverage_95_max
    {
        failures.push(format!(
            "coverage_95 {:.6} is outside preregistered range [{:.6}, {:.6}]",
            metrics.coverage_95, tolerances.coverage_95_min, tolerances.coverage_95_max
        ));
    }
    failures
}

fn write_outputs(output: &Path, results: &ValidationResults, inputs: &RunInputs) -> Result<()> {
    fs::create_dir_all(output)?;
    let results_path = output.join("validation-results-v1.json");
    let results_bytes = serde_json::to_vec_pretty(results)?;
    artifact_store::atomic_write(&results_path, &results_bytes)?;

    let report_path = output.join("validation-report-v1.md");
    let report_bytes = super::report::render_markdown(results).into_bytes();
    artifact_store::atomic_write(&report_path, &report_bytes)?;

    let manifest_path = output.join("validation-artifact-manifest-v1.toml");
    let artifacts = vec![
        manifest_entry("preregistration-v1.toml", &inputs.preregistration)?,
        manifest_entry("references-v1.toml", &inputs.references)?,
        manifest_entry("regions-v1.json", &inputs.regions)?,
        manifest_entry("candidate-map", &inputs.candidate_map)?,
        manifest_entry("validation-results-v1.json", &results_path)?,
        manifest_entry("validation-report-v1.md", &report_path)?,
    ];
    let manifest = super::ArtifactManifest {
        schema_version: super::report::VALIDATION_MANIFEST_SCHEMA_VERSION,
        generated_at_unix_seconds: results.generated_at_unix_seconds,
        artifacts,
    };
    artifact_store::atomic_write(
        &manifest_path,
        toml::to_string_pretty(&manifest)?.as_bytes(),
    )?;
    Ok(())
}

fn manifest_entry(name: &str, path: &Path) -> Result<crate::dataset::Artifact> {
    Ok(crate::dataset::Artifact {
        name: name.to_string(),
        path: path.to_path_buf(),
        sha256: checksum_io::sha256_file(path)?,
        bytes: path.metadata()?.len(),
    })
}

fn unix_seconds() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn preregistration_toml() -> &'static str {
        r#"
schema_version = 1
issue = 87
title = "Starlight independent validation preregistration"
band_nm = [300, 650]
flux_unit = "ph_m-2_s-1"
metrics = [
  "signed_bias", "absolute_bias", "relative_bias", "mae", "median_absolute_error",
  "rmse", "relative_error_p50", "relative_error_p68", "relative_error_p95",
  "coverage_68", "coverage_95", "outlier_fraction",
]
notes = "technical scaffolding only; scientific review deferred to #47"

[candidate]
map_path = "crates/nsb/data/starlight_nside128.csv"
map_schema = "nsb-healpix-starlight-candidate-v5"
checksum_pinning_status = "pending-regeneration-after-94"
checksum_note = "checksum may change after the #94 uncertainty audit regenerates the map"

[tolerances]
absolute_all_sky_bias_max = 0.03
median_absolute_regional_relative_error_max = 0.05
regional_relative_error_p95_max = 0.10
coverage_68_min = 0.63
coverage_68_max = 0.73
coverage_95_min = 0.90
coverage_95_max = 0.98

[[exclusion_rules]]
id = "placeholder-outlier-rule"
description = "reserved for a future catastrophic-outlier exclusion rule"
status = "placeholder"
"#
    }

    fn references_toml(second_status: &str) -> String {
        format!(
            r#"
schema_version = 1
acquisition_required = {acquisition_required}
notes = "test fixture references"

[[references]]
id = "fixture-a"
citation = "Author A (Year), Journal"
description = "fixture reference A"
coverage = "all-sky"
wavelength_band_nm = [300.0, 650.0]
spectral_quantity = "photon radiance"
transformation_to_target = "identity"
license = "unknown, request from publisher"
status = "pending-acquisition"
filename = "fixture-a.dat"
acquisition_notes = "requires manual literature request"

[[references]]
id = "fixture-b"
citation = "Author B (Year), Journal"
description = "fixture reference B"
coverage = "all-sky"
wavelength_band_nm = [300.0, 650.0]
spectral_quantity = "photon radiance"
transformation_to_target = "identity"
license = "unknown, request from publisher"
status = "{second_status}"
{sha_line}
filename = "fixture-b.dat"
acquisition_notes = "requires manual literature request"
"#,
            acquisition_required = second_status == "pending-acquisition",
            second_status = second_status,
            sha_line = if second_status == "acquired" {
                format!("sha256 = \"{}\"", "a".repeat(64))
            } else {
                String::new()
            },
        )
    }

    fn regions_json() -> &'static str {
        r#"{
  "schema_version": 1,
  "nside": 1,
  "ordering": "nested",
  "coordinate_frame": "galactic",
  "regions": [
    {"id": "all-sky", "description": "test", "selector": {"kind": "all"}}
  ]
}"#
    }

    fn candidate_map_csv() -> String {
        let header = concat!(
            "# schema=nsb-healpix-starlight-candidate-v5\n",
            "# ordering=nested\n",
            "# representation=sparse\n",
            "# nside=1\n",
            "# flux_unit=ph_m-2_s-1\n",
            "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
        );
        let mut body = header.to_string();
        for pixel in 0..12 {
            body.push_str(&format!(
                "{pixel},{:.6},0.1,0.0,0.1,3,0\n",
                10.0 + pixel as f64
            ));
        }
        body
    }

    fn base_inputs(root: &Path, second_status: &str) -> RunInputs {
        let preregistration = root.join("preregistration-v1.toml");
        write(&preregistration, preregistration_toml());
        let references = root.join("references-v1.toml");
        write(&references, &references_toml(second_status));
        let regions = root.join("regions-v1.json");
        write(&regions, regions_json());
        let candidate_map = root.join("starlight_nside1.csv");
        write(&candidate_map, &candidate_map_csv());
        RunInputs {
            preregistration,
            references,
            regions,
            candidate_map,
            candidate_map_sha256: None,
            references_workspace: root.join("references-workspace"),
            output: root.join("output"),
        }
    }

    #[test]
    fn run_with_only_pending_references_fails_closed_without_panicking() {
        let temp = TempDir::new().unwrap();
        let inputs = base_inputs(temp.path(), "pending-acquisition");
        let results = run(&inputs).unwrap();
        assert!(!results.technical_gates_passed);
        assert!(!results.scientifically_validated);
        assert_eq!(results.scientific_review_status, "pending");
        assert!(results.reference_results.is_empty());
        assert!(results
            .technical_gate_failures
            .iter()
            .any(|failure| failure.contains("pending acquisition")));
        assert!(temp
            .path()
            .join("output/validation-results-v1.json")
            .is_file());
        assert!(temp.path().join("output/validation-report-v1.md").is_file());
        assert!(temp
            .path()
            .join("output/validation-artifact-manifest-v1.toml")
            .is_file());
    }

    #[test]
    fn run_with_acquired_but_untransformed_reference_reports_pending_transform() {
        let temp = TempDir::new().unwrap();
        let inputs = base_inputs(temp.path(), "acquired");
        let results = run(&inputs).unwrap();
        assert!(!results.technical_gates_passed);
        let status = results
            .reference_statuses
            .iter()
            .find(|status| status.reference_id == "fixture-b")
            .unwrap();
        assert_eq!(status.status, "acquired-awaiting-transform");
    }

    #[test]
    fn run_computes_metrics_once_a_transformed_grid_is_present() {
        let temp = TempDir::new().unwrap();
        let inputs = base_inputs(temp.path(), "acquired");
        let grid_path = inputs
            .references_workspace
            .join("fixture-b")
            .join("transformed-grid-v1.csv");
        let mut grid = "pixel,value_ph_m2_s,statistical_uncertainty_ph_m2_s\n".to_string();
        for pixel in 0..12 {
            grid.push_str(&format!("{pixel},{:.6},0.1\n", 10.0 + pixel as f64));
        }
        write(&grid_path, &grid);

        let results = run(&inputs).unwrap();
        assert_eq!(results.reference_results.len(), 1);
        let all_sky = &results.reference_results[0].region_metrics[0];
        assert_eq!(all_sky.region_id, "all-sky");
        assert!((all_sky.metrics.signed_bias).abs() < 1.0e-9);
        // Perfect agreement drives coverage_68/coverage_95 to 100%, which is
        // itself outside the preregistered [63,73]/[90,98] bands for a
        // realistic (non-zero-residual) candidate; this fixture only checks
        // that metrics are computed at all once a transform exists, not that
        // this particular synthetic case clears every preregistered gate.
        assert!(!results.scientifically_validated);
        assert_eq!(results.scientific_review_status, "pending");
    }

    #[test]
    fn candidate_map_checksum_mismatch_fails_closed() {
        let temp = TempDir::new().unwrap();
        let mut inputs = base_inputs(temp.path(), "pending-acquisition");
        inputs.candidate_map_sha256 = Some("0".repeat(64));
        assert!(run(&inputs).is_err());
    }
}
