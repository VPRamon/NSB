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
    let actual_build_commit = repository_head_commit()?;
    verify_build_commit(expected_build_commit, &actual_build_commit)?;
    run_experimental_build_at_commit(
        config_path,
        expected_config_sha256,
        &actual_build_commit,
        output_directory,
    )
}

/// Build using an already-resolved, verified full Git SHA.
///
/// Production callers must go through [`run_experimental_build`], which
/// resolves and equality-checks a clean repository `HEAD`.
pub(crate) fn run_experimental_build_at_commit(
    config_path: &Path,
    expected_config_sha256: &str,
    build_commit: &str,
    output_directory: &Path,
) -> Result<BrightStarBuildRunManifest> {
    if !valid_commit_identity(build_commit) {
        bail!("bright-star build commits must be full 40-character lowercase Git SHAs");
    }
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
        build_commit,
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
        build_commit: build_commit.to_owned(),
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
    super::artifact::is_full_git_sha(value)
}

fn verify_build_commit(expected: &str, actual: &str) -> Result<()> {
    if !valid_commit_identity(expected) || !valid_commit_identity(actual) {
        bail!("bright-star build commits must be full 40-character lowercase Git SHAs");
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
    use super::{run_experimental_build_at_commit, valid_commit_identity, verify_build_commit};
    use crate::platform::checksum_io;
    use crate::starlight::bright_stars::reconstruction::{
        SpectralReconstructionModel, TemplateAssignment, UnsupportedSpectralAssignment,
        SPECTRAL_RECONSTRUCTION_MODEL_ID_V1,
    };
    use crate::starlight::bright_stars::{
        PhotometricBandCalibration, PhotometricBandResponse, SpectralTemplate,
    };
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    const FULL_SHA: &str = "33fe9047f5f57eae6717131b5d35edd65976d5b8";
    const OTHER_FULL_SHA: &str = "ceba696914b3dc9c73f2787b58ef55c2c02adf41";

    #[test]
    fn software_identity_requires_full_lowercase_sha() {
        assert!(valid_commit_identity(FULL_SHA));
        assert!(!valid_commit_identity(""));
        assert!(!valid_commit_identity("123456"));
        assert!(!valid_commit_identity("33fe904"));
        assert!(!valid_commit_identity(
            "33FE9047f5f57eae6717131b5d35edd65976d5b8"
        ));
        assert!(!valid_commit_identity(
            "not-a-commit-identity-value-xxxxxxxxx"
        ));
        assert!(!valid_commit_identity(&format!("{FULL_SHA}a")));
        assert!(verify_build_commit(FULL_SHA, FULL_SHA).is_ok());
        assert!(verify_build_commit(FULL_SHA, OTHER_FULL_SHA).is_err());
        assert!(verify_build_commit("33fe904", FULL_SHA).is_err());
        assert!(verify_build_commit(FULL_SHA, "33fe904").is_err());
        assert!(verify_build_commit("", FULL_SHA).is_err());
        assert!(verify_build_commit(FULL_SHA, "").is_err());
    }

    fn put(record: &mut [u8], start: usize, end: usize, value: &str) {
        record[start..end].fill(b' ');
        record[start..start + value.len()].copy_from_slice(value.as_bytes());
    }

    fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) -> (String, String) {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        (
            path.display().to_string(),
            checksum_io::sha256_file(&path).unwrap(),
        )
    }

    fn hipparcos_line(hip: u32) -> Vec<u8> {
        let mut line = vec![b' '; 164];
        put(&mut line, 0, 6, &hip.to_string());
        put(&mut line, 7, 10, "5");
        put(&mut line, 13, 14, "1");
        put(&mut line, 15, 28, "0.100000000");
        put(&mut line, 29, 42, "-0.200000000");
        put(&mut line, 43, 50, "10.0");
        put(&mut line, 51, 59, "0.0");
        put(&mut line, 60, 68, "0.0");
        put(&mut line, 69, 75, "0.2");
        put(&mut line, 76, 82, "0.3");
        put(&mut line, 90, 96, "0.4");
        put(&mut line, 97, 103, "0.5");
        put(&mut line, 129, 136, "2.50");
        put(&mut line, 137, 143, "0.01");
        line.push(b'\n');
        line
    }

    fn xhip_line(hip: u32) -> Vec<u8> {
        let mut line = vec![b' '; 283];
        put(&mut line, 0, 6, &hip.to_string());
        put(&mut line, 236, 262, "G2V");
        put(&mut line, 263, 266, "50");
        put(&mut line, 267, 268, "5");
        put(&mut line, 269, 276, "0.0");
        put(&mut line, 277, 283, "1.0");
        line.push(b'\n');
        line
    }

    fn spectral_model_bytes() -> Vec<u8> {
        let model = SpectralReconstructionModel {
            model_id: SPECTRAL_RECONSTRUCTION_MODEL_ID_V1.into(),
            builder_software_commit: FULL_SHA.into(),
            assignments: vec![TemplateAssignment {
                temperature_code: 50,
                luminosity_class_code: 5,
                template_id: "ck04-t6000-g45".into(),
            }],
            templates: vec![SpectralTemplate {
                template_id: "ck04-t6000-g45".into(),
                wavelengths_m: vec![300e-9, 336e-9, 500e-9, 650e-9, 900e-9],
                f_lambda_si: vec![1.0; 5],
                provenance: "fixture".into(),
            }],
            hp_response: PhotometricBandResponse {
                band_id: "Hipparcos/Hipparcos.Hp_bes".into(),
                wavelengths_m: vec![350e-9, 500e-9, 850e-9],
                throughput: vec![0.0, 1.0, 0.0],
                detector_convention: "photon_counting".into(),
                citation: "Bessell 2000".into(),
                sha256: "a".repeat(64),
            },
            hp_calibration: PhotometricBandCalibration {
                system: "Hipparcos Vega".into(),
                band_id: "Hipparcos/Hipparcos.Hp_bes".into(),
                zero_point_convention: "Vega/Pogson".into(),
                reference_spectrum: "CALSPEC alpha_lyr".into(),
                zero_point_photon_flux_ph_m2_s: 1.0e10,
                citation: "Bessell 2000; CALSPEC".into(),
            },
            template_mismatch_fraction: 0.1,
            spectral_type_mapping_fraction: 0.2,
            hp_zero_point_fraction: 0.01,
            uncertainty_calibration_status: "provisional-uncalibrated".into(),
            uncertainty_calibration_sha256: "b".repeat(64),
            spectral_mapping_status: "experimental-provisional".into(),
            spectral_mapping_sha256: "c".repeat(64),
            unsupported_assignments: vec![UnsupportedSpectralAssignment {
                temperature_code: 50,
                luminosity_class_code: 6,
                reason: "unsupported_spectral_mapping".into(),
            }],
            template_library_citation: "Castelli and Kurucz 2004".into(),
            spectral_type_mapping_citation: "fixture mapping".into(),
        };
        serde_json::to_vec_pretty(&model).unwrap()
    }

    fn provenance_toml(role: &str, source_id: &str, sha: &str) -> String {
        format!(
            r#"
role = "{role}"
source_id = "{source_id}"
release = "fixture-v1"
sha256 = "{sha}"
retrieval_url = "https://example.invalid/{source_id}"
license_or_terms_url = "https://example.invalid/terms"
"#
        )
    }

    #[test]
    fn experimental_build_at_commit_is_checksum_and_commit_fail_closed() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let (hip_path, hip_sha) = write_bytes(root, "hip2.dat", &hipparcos_line(42));
        let (xhip_path, xhip_sha) = write_bytes(root, "xhip.dat", &xhip_line(42));
        let (cross_path, cross_sha) = write_bytes(
            root,
            "cross.csv",
            b"source_id,original_ext_source_id,angular_distance,number_of_neighbours\n",
        );
        let (quality_path, quality_sha) = write_bytes(
            root,
            "quality.csv",
            b"source_id,ra,dec,phot_g_mean_mag,has_xp_continuous,photometric_usable\n",
        );
        let (positional_path, positional_sha) = write_bytes(
            root,
            "positional.csv",
            b"source_id,ra,dec,phot_g_mean_mag,has_xp_continuous,photometric_usable\n",
        );
        let (model_path, model_sha) = write_bytes(root, "model.json", &spectral_model_bytes());
        let (extra_path, extra_sha) = write_bytes(root, "extra.bin", b"opaque-input");
        let (tycho_path, tycho_sha) = write_bytes(root, "tycho.dat", &{
            let mut tycho = vec![b' '; 151];
            put(&mut tycho, 0, 4, "1");
            put(&mut tycho, 5, 10, "2");
            put(&mut tycho, 11, 12, "3");
            put(&mut tycho, 110, 116, "3.10");
            put(&mut tycho, 117, 122, "0.02");
            put(&mut tycho, 123, 129, "2.90");
            put(&mut tycho, 130, 135, "0.03");
            put(&mut tycho, 142, 148, "42");
            tycho.push(b'\n');
            tycho
        });
        let (tycho2_path, tycho2_sha) = write_bytes(root, "tycho2.dat", &{
            // Same HIP in a second segment marks the colour association ambiguous.
            let mut tycho = vec![b' '; 151];
            put(&mut tycho, 0, 4, "4");
            put(&mut tycho, 5, 10, "5");
            put(&mut tycho, 11, 12, "6");
            put(&mut tycho, 110, 116, "3.20");
            put(&mut tycho, 117, 122, "0.02");
            put(&mut tycho, 123, 129, "2.80");
            put(&mut tycho, 130, 135, "0.03");
            put(&mut tycho, 142, 148, "42");
            tycho.push(b'\n');
            tycho
        });

        let config = format!(
            r#"
schema_version = 1
nside = 1
config_source_id = "fixture-config"
config_release = "fixture-v1"
config_retrieval_url = "https://example.invalid/config"
config_terms_url = "https://example.invalid/terms"
spectral_model_path = "{model_path}"
spectral_model_sha256 = "{model_sha}"

[hipparcos2]
path = "{hip_path}"
[hipparcos2.provenance]
{hip_prov}

[xhip]
path = "{xhip_path}"
[xhip.provenance]
{xhip_prov}

[hip_gaia_crossmatch]
path = "{cross_path}"
[hip_gaia_crossmatch.provenance]
{cross_prov}

[gaia_quality]
path = "{quality_path}"
[gaia_quality.provenance]
{quality_prov}

[gaia_positional_fallback]
path = "{positional_path}"
[gaia_positional_fallback.provenance]
{positional_prov}

[spectral_model_provenance]
{model_prov}

[[tycho2]]
path = "{tycho_path}"
[tycho2.provenance]
{tycho_prov}

[[tycho2]]
path = "{tycho2_path}"
[tycho2.provenance]
{tycho2_prov}

[[additional_checksum_pinned_inputs]]
path = "{extra_path}"
[additional_checksum_pinned_inputs.provenance]
{extra_prov}
"#,
            hip_prov = provenance_toml("hipparcos2", "fixture-hip2", &hip_sha),
            xhip_prov = provenance_toml("spectral_type_catalogue", "fixture-xhip", &xhip_sha),
            cross_prov = provenance_toml("hip_gaia_crossmatch", "fixture-cross", &cross_sha),
            quality_prov =
                provenance_toml("gaia_dr3_quality_extract", "fixture-quality", &quality_sha),
            positional_prov = provenance_toml(
                "gaia_dr3_quality_extract",
                "fixture-positional",
                &positional_sha,
            ),
            model_prov = provenance_toml("build_config", "fixture-spectral-model", &model_sha),
            tycho_prov = provenance_toml("tycho2", "fixture-tycho", &tycho_sha),
            tycho2_prov = provenance_toml("tycho2", "fixture-tycho-b", &tycho2_sha),
            extra_prov =
                provenance_toml("spectral_template_library", "fixture-templates", &extra_sha),
        );
        let config_path = root.join("build.toml");
        fs::write(&config_path, &config).unwrap();
        let config_sha = checksum_io::sha256_file(&config_path).unwrap();
        let output = root.join("out");

        let manifest =
            run_experimental_build_at_commit(&config_path, &config_sha, FULL_SHA, &output).unwrap();
        assert_eq!(manifest.build_commit, FULL_SHA);
        assert_eq!(manifest.build_config_sha256, config_sha);
        assert!(manifest.artifact_path.exists());
        assert!(output
            .join("starlight-bright-stars-v1-build-manifest.json")
            .exists());
        assert_eq!(manifest.diagnostics.input_hipparcos, 1);
        assert!(manifest.diagnostics.passes_population_cut >= 1);

        assert!(run_experimental_build_at_commit(
            &config_path,
            &"0".repeat(64),
            FULL_SHA,
            &output.join("bad-sha"),
        )
        .is_err());
        assert!(run_experimental_build_at_commit(
            &config_path,
            &config_sha,
            "33fe904",
            &output.join("short-commit"),
        )
        .is_err());

        let mut bad_config = fs::File::create(root.join("bad.toml")).unwrap();
        writeln!(
            bad_config,
            "schema_version = 1\nnside = 1\nconfig_source_id = \"\"\nconfig_release = \"x\""
        )
        .unwrap();
        let bad_sha = checksum_io::sha256_file(&root.join("bad.toml")).unwrap();
        assert!(run_experimental_build_at_commit(
            &root.join("bad.toml"),
            &bad_sha,
            FULL_SHA,
            &output.join("bad-config"),
        )
        .is_err());
    }

    #[test]
    fn run_experimental_build_resolves_repository_head_before_equality_check() {
        use super::run_experimental_build;
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("empty.toml");
        fs::write(&config_path, "schema_version = 1\n").unwrap();
        let sha = checksum_io::sha256_file(&config_path).unwrap();
        // Wrong expected commit exercises HEAD resolution + fail-closed equality
        // (or the dirty-worktree gate, which is also production behaviour).
        let err = run_experimental_build(
            &config_path,
            &sha,
            "0000000000000000000000000000000000000000",
            temp.path(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("build commit mismatch")
                || err.contains("clean tracked NSB worktree")
                || err.contains("malformed")
        );
    }
}
