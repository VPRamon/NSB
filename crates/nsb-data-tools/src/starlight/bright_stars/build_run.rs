//! Reproducible orchestration for the external bright-star experiment.

use super::artifact::{BrightStarInputProvenance, BrightStarInputRole};
use super::builder::{build_experimental_artifact, BrightStarBuildDiagnostics};
use super::catalogue::{
    ingest_gaia_quality_extract, ingest_hip_gaia_crossmatch, ingest_hipparcos2, ingest_tycho2,
    ingest_xhip, PinnedCatalogueInput, Tycho2Photometry,
};
use super::policy::{BrightStarPopulationPolicy, BrightStarPrecedencePolicy};
use super::reconstruction::{load_spectral_reconstruction_model, reconstruct_spectral_estimates};
use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarBuildRunConfig {
    pub schema_version: u32,
    pub nside: u32,
    pub config_source_id: String,
    pub config_release: String,
    pub config_retrieval_url: String,
    pub config_terms_url: String,
    pub hipparcos2: PinnedCatalogueInput,
    pub tycho2: Vec<PinnedCatalogueInput>,
    pub xhip: PinnedCatalogueInput,
    pub hip_gaia_crossmatch: PinnedCatalogueInput,
    pub gaia_quality: PinnedCatalogueInput,
    pub gaia_positional_fallback: PinnedCatalogueInput,
    pub spectral_model_path: PathBuf,
    pub spectral_model_sha256: String,
    pub spectral_model_provenance: BrightStarInputProvenance,
    #[serde(default)]
    pub additional_checksum_pinned_inputs: Vec<PinnedCatalogueInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrightStarBuildRunManifest {
    pub schema_version: u32,
    pub model_id: String,
    pub build_commit: String,
    pub build_config_sha256: String,
    pub artifact_path: PathBuf,
    pub artifact_sha256: String,
    pub diagnostics_path: PathBuf,
    pub diagnostics_sha256: String,
    pub inputs: Vec<BrightStarInputProvenance>,
    pub diagnostics: BrightStarBuildDiagnostics,
}

pub fn run_experimental_build(
    config_path: &Path,
    expected_config_sha256: &str,
    expected_build_commit: &str,
    output_directory: &Path,
) -> Result<BrightStarBuildRunManifest> {
    let actual_config_sha = checksum_io::sha256_file(config_path)?;
    if actual_config_sha != expected_config_sha256 {
        bail!("bright-star build config checksum mismatch");
    }
    let config: BrightStarBuildRunConfig = toml::from_str(&fs::read_to_string(config_path)?)?;
    if config.schema_version != 1
        || config.config_source_id.trim().is_empty()
        || config.config_release.trim().is_empty()
    {
        bail!("invalid bright-star build run config identity");
    }
    let actual_build_commit = repository_head_commit()?;
    verify_build_commit(expected_build_commit, &actual_build_commit)?;
    let hipparcos = ingest_hipparcos2(&config.hipparcos2)?;
    let mut tycho_by_hip: BTreeMap<u32, Tycho2Photometry> = BTreeMap::new();
    let mut ambiguous_tycho_hips = BTreeSet::new();
    for input in &config.tycho2 {
        let ingested = ingest_tycho2(input)?;
        for hip in ingested.ambiguous_hip_ids {
            tycho_by_hip.remove(&hip);
            ambiguous_tycho_hips.insert(hip);
        }
        for (hip, photometry) in ingested.by_hip {
            if ambiguous_tycho_hips.contains(&hip) {
                continue;
            }
            if let std::collections::btree_map::Entry::Occupied(entry) = tycho_by_hip.entry(hip) {
                // The same conservative rule applies across fixed-width
                // segments: an overlapping HIP is an unresolved component
                // association, so do not use its Tycho colour.
                entry.remove();
                ambiguous_tycho_hips.insert(hip);
            } else {
                tycho_by_hip.insert(hip, photometry);
            }
        }
    }
    let xhip = ingest_xhip(&config.xhip)?;
    let identities = ingest_hip_gaia_crossmatch(&config.hip_gaia_crossmatch)?;
    let gaia_quality = ingest_gaia_quality_extract(&config.gaia_quality)?;
    let positional = ingest_gaia_quality_extract(&config.gaia_positional_fallback)?;
    let spectral_model = load_spectral_reconstruction_model(
        &config.spectral_model_path,
        &config.spectral_model_sha256,
    )?;
    let unsupported_spectral_codes = spectral_model
        .unsupported_assignments
        .iter()
        .map(|assignment| {
            (
                assignment.temperature_code,
                assignment.luminosity_class_code,
            )
        })
        .collect::<BTreeSet<_>>();
    let spectra = reconstruct_spectral_estimates(&hipparcos, &xhip, &spectral_model)?;

    let mut inputs = vec![
        config.hipparcos2.provenance.clone(),
        config.xhip.provenance.clone(),
        config.hip_gaia_crossmatch.provenance.clone(),
        config.gaia_quality.provenance.clone(),
        config.gaia_positional_fallback.provenance.clone(),
        config.spectral_model_provenance.clone(),
        BrightStarInputProvenance {
            role: BrightStarInputRole::BuildConfig,
            source_id: config.config_source_id.clone(),
            release: config.config_release.clone(),
            sha256: actual_config_sha.clone(),
            retrieval_url: config.config_retrieval_url.clone(),
            license_or_terms_url: config.config_terms_url.clone(),
        },
    ];
    inputs.extend(config.tycho2.iter().map(|input| input.provenance.clone()));
    for input in &config.additional_checksum_pinned_inputs {
        verify_opaque_input(input)?;
        inputs.push(input.provenance.clone());
    }
    inputs.sort_by(|a, b| a.role.cmp(&b.role).then(a.source_id.cmp(&b.source_id)));
    let (artifact, diagnostics) = build_experimental_artifact(
        config.nside,
        &actual_build_commit,
        inputs.clone(),
        &hipparcos,
        &tycho_by_hip,
        ambiguous_tycho_hips.len() as u64,
        &xhip,
        &identities,
        &gaia_quality,
        &positional.into_values().collect::<Vec<_>>(),
        &spectra,
        &unsupported_spectral_codes,
        BrightStarPopulationPolicy::v1(),
        BrightStarPrecedencePolicy::v1(),
    )?;

    fs::create_dir_all(output_directory)?;
    let artifact_path = output_directory.join("starlight-bright-stars-v1.json");
    let diagnostics_path = output_directory.join("starlight-bright-stars-v1-diagnostics.json");
    fs::write(&artifact_path, artifact.to_json_pretty()?)?;
    fs::write(&diagnostics_path, serde_json::to_vec_pretty(&diagnostics)?)?;
    let artifact_sha256 = checksum_io::sha256_file(&artifact_path)?;
    let diagnostics_sha256 = checksum_io::sha256_file(&diagnostics_path)?;
    let manifest = BrightStarBuildRunManifest {
        schema_version: 1,
        model_id: artifact.model_id,
        build_commit: actual_build_commit,
        build_config_sha256: actual_config_sha,
        artifact_path,
        artifact_sha256,
        diagnostics_path,
        diagnostics_sha256,
        inputs,
        diagnostics,
    };
    let manifest_path = output_directory.join("starlight-bright-stars-v1-build-manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(manifest)
}

fn valid_commit_identity(value: &str) -> bool {
    (7..=40).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn verify_build_commit(expected: &str, actual: &str) -> Result<()> {
    if !valid_commit_identity(expected) || !valid_commit_identity(actual) {
        bail!("bright-star build commits must be 7-40 lowercase hexadecimal characters");
    }
    if actual != expected {
        bail!("bright-star build commit mismatch: actual {actual} != expected {expected}");
    }
    Ok(())
}

fn repository_head_commit() -> Result<String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .current_dir(&repository)
        .output()
        .context("inspect NSB build worktree")?;
    if !status.status.success() || !status.stdout.is_empty() {
        bail!("bright-star builds require a clean tracked NSB worktree");
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .context("resolve NSB build commit")?;
    if !output.status.success() {
        bail!("could not resolve NSB build commit");
    }
    let commit = String::from_utf8(output.stdout)
        .context("NSB build commit is not UTF-8")?
        .trim()
        .to_owned();
    if !valid_commit_identity(&commit) {
        bail!("resolved NSB build commit is malformed");
    }
    Ok(commit)
}

fn verify_opaque_input(input: &PinnedCatalogueInput) -> Result<()> {
    let actual = checksum_io::sha256_file(&input.path)?;
    if actual != input.provenance.sha256 {
        bail!("checksum mismatch for input {}", input.path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{valid_commit_identity, verify_build_commit};

    #[test]
    fn software_identity_is_strict() {
        assert!(valid_commit_identity("33fe904"));
        assert!(valid_commit_identity(
            "33fe9047f5f57eae6717131b5d35edd65976d5b8"
        ));
        assert!(!valid_commit_identity(""));
        assert!(!valid_commit_identity("123456"));
        assert!(!valid_commit_identity("33FE904"));
        assert!(!valid_commit_identity("not-a-commit"));
        assert!(verify_build_commit("33fe904", "33fe904").is_ok());
        assert!(verify_build_commit("33fe904", "f43f951").is_err());
        assert!(verify_build_commit("", "33fe904").is_err());
        assert!(verify_build_commit("33fe904", "").is_err());
    }
}
