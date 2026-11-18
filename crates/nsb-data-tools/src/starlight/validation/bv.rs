// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Independent comparisons of a verified monochromatic Starlight candidate.

use super::metrics::{self, MetricsSummary};
use super::regions::{RegionEngine, RegionsDocument};
use crate::platform::{artifact_store, checksum_io};
use crate::starlight::{pack, xp::calibrate::BV_REFERENCE_NM};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    schema: String,
    source: String,
    upstream_sha256: Vec<String>,
    operator: String,
    nside: u32,
    ordering: String,
    frame: String,
    domain: String,
    reference_nm: [f64; 2],
    unit: String,
    pixels: Vec<ReferencePixel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferencePixel {
    flux: [f64; 2],
    sigma: [f64; 2],
}

#[derive(Debug, Serialize)]
struct RegionResult {
    region: String,
    wavelength_nm: f64,
    metrics: MetricsSummary,
    passed: bool,
}

/// The existing NSB bias, pixel-error and interval-coverage thresholds,
/// applied independently to B and V on the preregistered sky regions.
fn passes(region: &str, metrics: &MetricsSummary) -> bool {
    (region != "all-sky" || metrics.relative_bias.abs() <= 0.03)
        && metrics.relative_error_p50 <= 0.05
        && metrics.relative_error_p95 <= 0.10
        && (0.63..=0.73).contains(&metrics.coverage_68)
        && (0.90..=0.98).contains(&metrics.coverage_95)
}

pub(crate) fn run(
    workspace: &Path,
    manifest_sha256: &str,
    reference: &Path,
    reference_sha256: &str,
    regions: &Path,
    regions_sha256: &str,
    output: &Path,
) -> Result<bool> {
    let (candidate, evidence) = pack::verify_bv_candidate(workspace, manifest_sha256)?;
    let reference: Reference = read_pinned(reference, reference_sha256)?;
    let regions: RegionsDocument = read_pinned(regions, regions_sha256)?;
    regions.validate()?;
    let engine = RegionEngine::build(candidate.nside)?;
    if reference.schema != "nsb-starlight-bv-reference-v1"
        || reference.nside != candidate.nside
        || reference.ordering != "nested"
        || reference.frame != "galactic"
        || reference.domain != "top-of-atmosphere-stellar-only"
        || reference.reference_nm != BV_REFERENCE_NM
        || reference.unit != "W m-2 nm-1"
        || reference.source.trim().is_empty()
        || reference.operator.trim().is_empty()
        || reference.upstream_sha256.is_empty()
        || reference
            .upstream_sha256
            .iter()
            .any(|hash| !crate::starlight::bv::is_sha256(hash))
        || reference.pixels.len() as u64 != engine.domain_size()
        || reference.pixels.iter().any(|p| {
            p.flux.iter().any(|v| !v.is_finite() || *v <= 0.0)
                || p.sigma.iter().any(|v| !v.is_finite() || *v < 0.0)
        })
    {
        bail!("incompatible B/V reference. Supply a pinned, dense Galactic NESTED stellar-only TOA reference with positive fluxes and valid uncertainties");
    }

    let mut results = Vec::new();
    for (region, pixels) in engine.resolve(&regions, &candidate.admitted_sources())? {
        for (band, wavelength_nm) in BV_REFERENCE_NM.into_iter().enumerate() {
            let mut flux = Vec::with_capacity(pixels.len());
            let mut observed = Vec::with_capacity(pixels.len());
            let mut sigma = Vec::with_capacity(pixels.len());
            for pixel in &pixels {
                let datum = &reference.pixels[*pixel as usize];
                let (value, model_sigma) = evidence.pixels.get(pixel).map_or((0.0, 0.0), |p| {
                    let sigma = p.shared_systematic_sigma.values().fold(
                        (p.statistical_covariance[band]
                            + p.independent_systematic_covariance[band])
                            .sqrt(),
                        |combined, group| combined.hypot(group[band]),
                    );
                    (p.flux_w_m2_nm[band], sigma)
                });
                flux.push(value);
                observed.push(datum.flux[band]);
                sigma.push(model_sigma.hypot(datum.sigma[band]));
            }
            let metrics = metrics::compute(&flux, &observed, &sigma)
                .map_err(|error| anyhow::anyhow!("{region} at {wavelength_nm} nm: {error}"))?;
            results.push(RegionResult {
                passed: passes(&region, &metrics),
                region: region.clone(),
                wavelength_nm,
                metrics,
            });
        }
    }
    let passed = results.iter().all(|result| result.passed);
    let report = serde_json::json!({
        "schema": "nsb-starlight-bv-comparison-v1",
        "candidate_manifest_sha256": manifest_sha256,
        "reference_sha256": reference_sha256,
        "regions_sha256": regions_sha256,
        "reference_source": reference.source,
        "reference_operator": reference.operator,
        "numerical_gates_passed": passed,
        "production_admission": "review-required",
        "regions": results,
    });
    artifact_store::atomic_write(output, &serde_json::to_vec_pretty(&report)?)?;
    Ok(passed)
}

fn read_pinned<T: serde::de::DeserializeOwned>(path: &Path, sha256: &str) -> Result<T> {
    let bytes = fs::read(path)?;
    if checksum_io::sha256_bytes(&bytes) != sha256 {
        bail!("SHA-256 mismatch for {}", path.display());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::starlight::pack::BvEvidence;

    pub(crate) fn check_comparison(
        workspace: &Path,
        manifest_sha: &str,
        evidence: &BvEvidence,
    ) -> Result<()> {
        let reference = workspace.join("reference.json");
        let regions = workspace.join("regions.json");
        let output = workspace.join("comparison.json");
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "nside": 1, "ordering": "nested", "coordinate_frame": "galactic",
            "regions": [{"id":"all-sky", "description":"synthetic all-sky", "selector":{"kind":"all"}}]
        }))?;
        fs::write(&regions, &bytes)?;
        let regions_sha = checksum_io::sha256_bytes(&bytes);
        let execute = |reference_sha| {
            run(
                workspace,
                manifest_sha,
                &reference,
                reference_sha,
                &regions,
                &regions_sha,
                &output,
            )
        };
        let mut sample = Reference {
            schema: "nsb-starlight-bv-reference-v1".into(),
            source: "synthetic comparison fixture".into(),
            upstream_sha256: vec!["a".repeat(64)],
            operator: "whole HEALPix cell flux".into(),
            nside: 1,
            ordering: "nested".into(),
            frame: "galactic".into(),
            domain: "top-of-atmosphere-stellar-only".into(),
            reference_nm: BV_REFERENCE_NM,
            unit: "W m-2 nm-1".into(),
            pixels: (0..12)
                .map(|index| ReferencePixel {
                    flux: evidence
                        .pixels
                        .get(&index)
                        .map_or([1e-30; 2], |p| p.flux_w_m2_nm),
                    sigma: [1e-25; 2],
                })
                .collect(),
        };
        let write = |sample: &Reference| -> Result<String> {
            let bytes = serde_json::to_vec(sample)?;
            fs::write(&reference, &bytes)?;
            Ok(checksum_io::sha256_bytes(&bytes))
        };
        let reference_sha = write(&sample)?;
        assert!(execute("").is_err());
        assert!(!execute(&reference_sha)?);
        let report: serde_json::Value = serde_json::from_slice(&fs::read(output.clone())?)?;
        assert_eq!(report["production_admission"], "review-required");
        assert_eq!(report["regions"][0]["wavelength_nm"], 445.0);
        assert_eq!(report["regions"][1]["wavelength_nm"], 551.0);
        assert_eq!(report["regions"][1]["metrics"]["sample_count"], 12);
        sample.domain = "ground-level".into();
        assert!(execute(&write(&sample)?)
            .unwrap_err()
            .to_string()
            .contains("incompatible"));
        Ok(())
    }

    #[test]
    fn flags_undercoverage_overcoverage_and_bias_independently() -> Result<()> {
        let reference = [100.0; 12];
        let sigma = [1.0; 12];
        let mut flux = [100.5; 12];
        flux[8..11].fill(101.5);
        flux[11] = 102.5;
        let mut summary = metrics::compute(&flux, &reference, &sigma)?;
        assert!(passes("all-sky", &summary));
        summary.relative_bias = 0.031;
        assert!(!passes("all-sky", &summary));
        assert!(passes("plane", &summary));
        summary.coverage_95 = 0.99;
        assert!(!passes("plane", &summary));
        summary.coverage_95 = 0.89;
        assert!(!passes("plane", &summary));
        Ok(())
    }
}
