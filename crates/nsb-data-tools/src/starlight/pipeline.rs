use super::sources::{acquisition, inventory};
use crate::dataset::{Artifact, DatasetName, DatasetPipeline, RunConfig, ValidationGate};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

pub(crate) static PIPELINE: StarlightPipeline = StarlightPipeline;

pub(crate) struct StarlightPipeline;

impl DatasetPipeline for StarlightPipeline {
    fn dataset(&self) -> DatasetName {
        DatasetName::Starlight
    }

    fn supports_partitions(&self) -> bool {
        true
    }

    fn available_partitions(&self, config: &RunConfig) -> Result<Option<Vec<String>>> {
        let Some(starlight) = &config.starlight else {
            bail!("Starlight production configuration is missing");
        };
        inventory::production_partition_ids(&config.workspace.root, &starlight.gaia_products)
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &["starlight_nside128.csv", "merge_report.json"]
    }

    fn expected_outputs_for(&self, config: &RunConfig) -> Vec<String> {
        let canonical_nside = config
            .starlight
            .as_ref()
            .map(|starlight| starlight.map.canonical_nside)
            .unwrap_or(128);
        production_output_names(canonical_nside)
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        Ok(source_name)
    }

    fn update(&self, config: &RunConfig, partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        let Some(starlight) = &config.starlight else {
            return Ok(None);
        };
        if starlight.gaia_products.is_empty() {
            bail!("production Starlight requires at least one Gaia product inventory");
        }
        if partitions.is_empty() {
            return Ok(Some(inventory::update_inventories(
                &config.workspace.root,
                &starlight.gaia_products,
            )?));
        }
        let mut artifacts = Vec::with_capacity(partitions.len() * starlight.gaia_products.len());
        for partition in partitions {
            artifacts.extend(acquisition::acquire_partition(
                &config.workspace.root,
                &starlight.gaia_products,
                &starlight.acquisition,
                partition,
            )?);
        }
        artifacts.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Some(artifacts))
    }

    fn build(&self, config: &RunConfig, partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        let Some(starlight) = &config.starlight else {
            return Ok(None);
        };
        let artifacts = super::worker::build_partitions(
            &config.workspace.root,
            &starlight.gaia_products,
            partitions,
            config.execution.concurrency,
            starlight.map.canonical_nside,
            starlight.product_band,
            starlight.ultraviolet_correction.as_ref(),
            starlight.photometric_inference.as_ref(),
            starlight.selection_function.as_ref(),
            starlight.bright_star_supplement.as_ref(),
        )?;
        super::worker::write_artifact_index(&config.workspace.root, &artifacts)?;
        Ok(Some(artifacts))
    }

    fn finalize(&self, config: &RunConfig) -> Result<Option<Vec<Artifact>>> {
        let Some(starlight) = &config.starlight else {
            return Ok(None);
        };
        let expected = self
            .available_partitions(config)?
            .ok_or_else(|| anyhow::anyhow!("Starlight inventories are missing"))?;
        let selection_population = starlight
            .selection_function
            .as_ref()
            .map(|pin| -> Result<_> {
                let correction =
                    super::selection::SelectionCorrection::load(&pin.artifact_path, &pin.sha256)?;
                correction.require_production_status()?;
                Ok(super::map::product::SelectionPopulationPolicy {
                    model_id: correction.artifact().model_id.clone(),
                    weight_cap: correction.artifact().weight_cap,
                    residual_faint_tail_estimated: correction.artifact().faint_tail.enabled,
                })
            })
            .transpose()?;
        let mut expected = expected;
        if prepare_bright_star_supplement_for_finalize(starlight, &config.workspace.root)? {
            expected.push("bright-star-supplement".to_string());
            expected.sort();
        }
        Ok(Some(super::map::product::emit_maps(
            &config.workspace.root,
            &expected,
            starlight.map.canonical_nside,
            starlight.product_band,
            starlight
                .ultraviolet_correction
                .as_ref()
                .map(|ultraviolet| ultraviolet.sha256.as_str()),
            selection_population,
        )?))
    }

    fn validation_gates(
        &self,
        config: &RunConfig,
        _artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        let Some(starlight) = &config.starlight else {
            return Ok(Vec::new());
        };
        super::map::product::scientific_gates(&config.workspace.root, starlight.map.canonical_nside)
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        if name == "merge_report.json" {
            return super::map::product::validate_report(path);
        }
        let expected_nside = name
            .strip_prefix("starlight_nside")
            .and_then(|suffix| suffix.strip_suffix(".csv"))
            .and_then(|nside| nside.parse::<u32>().ok());
        if let Some(nside) = expected_nside {
            return super::map::product::validate_map(path, nside);
        }
        let text = fs::read_to_string(path)?;
        for header in [
            "# map_type=healpix",
            "# coordinate_frame=galactic",
            "# nside=",
        ] {
            if !text.contains(header) {
                bail!("{name} is missing header {header}");
            }
        }
        let data_rows = text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .count();
        if data_rows < 2 {
            bail!("{name} contains no map rows");
        }
        Ok(())
    }

    fn validate_config(&self, config: &RunConfig) -> Result<()> {
        let Some(starlight) = &config.starlight else {
            return Ok(());
        };
        super::config::validate_canonical_nside(starlight.map.canonical_nside)?;
        for (label, pin) in [
            ("UV correction", starlight.ultraviolet_correction.as_ref()),
            (
                "photometric inference",
                starlight.photometric_inference.as_ref(),
            ),
            ("selection function", starlight.selection_function.as_ref()),
            (
                "bright-star supplement",
                starlight.bright_star_supplement.as_ref(),
            ),
        ] {
            if let Some(pin) = pin {
                if pin.sha256.len() != 64
                    || !pin
                        .sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    bail!("{label} SHA-256 must be 64 lowercase hexadecimal characters");
                }
            }
        }
        if (starlight.product_band == super::config::StarlightProductBand::Combined300To650)
            != starlight.ultraviolet_correction.is_some()
        {
            bail!(
                "300–650 nm Starlight product requires a validated UV correction artifact, and measured-only products must not configure one"
            );
        }
        if let Some(pin) = &starlight.bright_star_supplement {
            let artifact =
                super::bright_stars::load_bright_star_artifact(&pin.artifact_path, &pin.sha256)?;
            ensure_bright_star_product_compatible(starlight.product_band, &artifact)?;
        }
        for product in &starlight.gaia_products {
            if product.id.trim().is_empty()
                || product.filename_prefix.is_empty()
                || product.filename_suffix.is_empty()
                || product.checksum_manifest_sha256.len() != 64
                || !product
                    .checksum_manifest_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                bail!(
                    "Gaia product identity, filename boundaries, and manifest SHA-256 must be valid"
                );
            }
            for (field, url) in [
                ("base_url", &product.base_url),
                ("checksum_manifest_url", &product.checksum_manifest_url),
            ] {
                if !url.starts_with("https://") {
                    bail!("Gaia product {} {field} must use HTTPS", product.id);
                }
            }
        }
        let product_ids: std::collections::BTreeSet<_> = starlight
            .gaia_products
            .iter()
            .map(|product| product.id.as_str())
            .collect();
        let required = std::collections::BTreeSet::from(["gaia-source", "xp-continuous"]);
        if product_ids != required {
            bail!(
                "production Starlight requires exactly the gaia-source and xp-continuous products"
            );
        }
        if starlight.acquisition.connect_timeout_seconds == 0
            || starlight.acquisition.request_timeout_seconds == 0
            || starlight.acquisition.max_attempts == 0
        {
            bail!("Starlight acquisition timeouts and max_attempts must be greater than zero");
        }
        Ok(())
    }
}

fn production_output_names(canonical_nside: u32) -> Vec<String> {
    vec![
        format!("starlight_nside{canonical_nside}.csv"),
        "merge_report.json".to_string(),
    ]
}

pub(crate) fn prepare_bright_star_supplement_for_finalize(
    starlight: &super::config::StarlightConfig,
    workspace_root: &Path,
) -> Result<bool> {
    let Some(pin) = &starlight.bright_star_supplement else {
        return Ok(false);
    };
    let artifact = super::bright_stars::load_bright_star_artifact(&pin.artifact_path, &pin.sha256)?;
    ensure_bright_star_product_compatible(starlight.product_band, &artifact)?;
    let ultraviolet_metadata = match starlight.product_band {
        super::config::StarlightProductBand::Measured336To650 => None,
        super::config::StarlightProductBand::Combined300To650 => {
            let uv_pin = starlight
                .ultraviolet_correction
                .as_ref()
                .context("combined bright-star finalize requires ultraviolet_correction pin")?;
            let correction = super::uv::UvCorrection::load(&uv_pin.artifact_path, &uv_pin.sha256)?;
            correction.require_production_status()?;
            Some(uv_correction_shard_metadata(&correction))
        }
    };
    let shard = super::worker::bright_star_supplement_shard(
        &artifact,
        &pin.sha256,
        starlight.map.canonical_nside,
        starlight.product_band,
        ultraviolet_metadata,
    )?;
    let shard_path = workspace_root.join("outputs/shards/bright-star-supplement.json");
    shard.write(&shard_path)?;
    Ok(true)
}

pub(crate) fn ensure_bright_star_product_compatible(
    product_band: super::config::StarlightProductBand,
    artifact: &super::bright_stars::BrightStarArtifact,
) -> Result<()> {
    let artifact_band = artifact.product_band_kind()?;
    let compatible = matches!(
        (product_band, artifact_band),
        (
            super::config::StarlightProductBand::Measured336To650,
            super::bright_stars::BrightStarArtifactProductBand::Measured336To650,
        ) | (
            super::config::StarlightProductBand::Combined300To650,
            super::bright_stars::BrightStarArtifactProductBand::Combined300To650,
        )
    );
    if !compatible {
        bail!(
            "bright-star supplement spectral coverage is incompatible with configured Starlight product band"
        );
    }
    Ok(())
}

pub(crate) fn uv_correction_shard_metadata(
    correction: &super::uv::UvCorrection,
) -> super::map::accumulator::UvCorrectionShardMetadata {
    super::map::accumulator::UvCorrectionShardMetadata::from_correction(correction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::RunConfig;
    use crate::starlight::bright_stars::{BrightStarInputProvenance, BrightStarInputRole};
    use crate::starlight::config::{
        AcquisitionConfig, ArtifactPinConfig, GaiaProductConfig, OfficialChecksumAlgorithm,
        StarlightConfig, StarlightMapConfig, StarlightProductBand,
    };
    use std::path::PathBuf;

    fn valid_gaia_products() -> Vec<GaiaProductConfig> {
        ["gaia-source", "xp-continuous"]
            .into_iter()
            .map(|id| GaiaProductConfig {
                id: id.to_string(),
                base_url: format!("https://example.test/{id}/"),
                checksum_manifest_url: format!("https://example.test/{id}/MD5SUM.txt"),
                checksum_manifest_sha256: "a".repeat(64),
                checksum_algorithm: OfficialChecksumAlgorithm::Md5,
                expected_partitions: Some(1),
                filename_prefix: format!("{id}-"),
                filename_suffix: ".csv.gz".to_string(),
            })
            .collect()
    }

    fn base_config(starlight: Option<StarlightConfig>) -> RunConfig {
        let mut config: RunConfig = toml::from_str(
            r#"
schema_version = 1
dataset = "starlight"

[workspace]
root = "/tmp/nsb-starlight-test"

[execution]
executor = "local"
concurrency = 1
lease_timeout_seconds = 60
"#,
        )
        .expect("minimal starlight run config");
        config.starlight = starlight;
        config
    }

    fn combined_inputs() -> Vec<BrightStarInputProvenance> {
        let input = |role, source_id: &str| BrightStarInputProvenance {
            role,
            source_id: source_id.into(),
            release: "fixture-v1".into(),
            sha256: "a".repeat(64),
            retrieval_url: "https://example.invalid/source".into(),
            license_or_terms_url: "https://example.invalid/terms".into(),
        };
        vec![
            input(BrightStarInputRole::SpectralTypeCatalogue, "xhip-fixture"),
            input(BrightStarInputRole::SpectralTemplateLibrary, "ck04-fixture"),
            input(
                BrightStarInputRole::PhotometricResponseCurve,
                "hp-response-fixture",
            ),
            input(
                BrightStarInputRole::PhotometricZeroPoint,
                "hp-zero-point-fixture",
            ),
            input(
                BrightStarInputRole::BuildConfig,
                "starlight-bright-stars-spectral-model-v1.json",
            ),
        ]
    }

    fn measured_config() -> StarlightConfig {
        StarlightConfig {
            gaia_products: valid_gaia_products(),
            acquisition: AcquisitionConfig::default(),
            map: StarlightMapConfig::default(),
            product_band: StarlightProductBand::Measured336To650,
            ultraviolet_correction: None,
            photometric_inference: None,
            selection_function: None,
            bright_star_supplement: None,
        }
    }

    #[test]
    fn changing_canonical_nside_changes_output_name() {
        assert_eq!(
            production_output_names(128),
            ["starlight_nside128.csv", "merge_report.json"]
        );
        assert_eq!(
            production_output_names(256),
            ["starlight_nside256.csv", "merge_report.json"]
        );
    }

    #[test]
    fn publication_does_not_include_derived_resolution_maps() {
        let outputs = production_output_names(128);
        for retired in [
            "starlight_nside64.csv",
            "starlight_nside256.csv",
            "starlight_nside512.csv",
        ] {
            assert!(!outputs.iter().any(|output| output == retired));
        }
    }

    #[test]
    fn validate_config_accepts_measured_production_policy() {
        PIPELINE
            .validate_config(&base_config(Some(measured_config())))
            .expect("measured config");
    }

    #[test]
    fn validate_config_rejects_combined_band_without_uv_correction() {
        let mut starlight = measured_config();
        starlight.product_band = StarlightProductBand::Combined300To650;
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("combined band requires UV");
        assert!(err
            .to_string()
            .contains("300–650 nm Starlight product requires a validated UV correction"));
    }

    #[test]
    fn validate_config_rejects_measured_band_with_uv_correction() {
        let mut starlight = measured_config();
        starlight.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("uv.toml"),
            sha256: "b".repeat(64),
        });
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("measured band forbids UV");
        assert!(err
            .to_string()
            .contains("300–650 nm Starlight product requires a validated UV correction"));
    }

    #[test]
    fn validate_config_rejects_invalid_artifact_sha_and_zero_acquisition() {
        let mut starlight = measured_config();
        starlight.product_band = StarlightProductBand::Combined300To650;
        starlight.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("uv.toml"),
            sha256: "not-a-sha".to_string(),
        });
        let sha_err = PIPELINE
            .validate_config(&base_config(Some(starlight.clone())))
            .expect_err("invalid sha");
        assert!(sha_err
            .to_string()
            .contains("UV correction SHA-256 must be 64 lowercase hexadecimal"));

        starlight.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("uv.toml"),
            sha256: "c".repeat(64),
        });
        starlight.acquisition.max_attempts = 0;
        let timeout_err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("zero max_attempts");
        assert!(timeout_err
            .to_string()
            .contains("timeouts and max_attempts must be greater than zero"));
    }

    #[test]
    fn validate_config_rejects_incomplete_gaia_product_set() {
        let mut starlight = measured_config();
        starlight.gaia_products.pop();
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("missing xp-continuous");
        assert!(err
            .to_string()
            .contains("exactly the gaia-source and xp-continuous products"));
    }

    #[test]
    fn validate_config_rejects_invalid_bright_star_supplement_pin() {
        let mut starlight = measured_config();
        starlight.bright_star_supplement = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("missing-bright-stars.json"),
            sha256: "not-a-sha".into(),
        });
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("invalid bright-star sha");
        assert!(err
            .to_string()
            .contains("bright-star supplement SHA-256 must be 64 lowercase hexadecimal"));
    }

    #[test]
    fn validate_config_rejects_bright_star_supplement_with_combined_band() {
        let mut starlight = measured_config();
        starlight.product_band = StarlightProductBand::Combined300To650;
        starlight.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("uv.toml"),
            sha256: "d".repeat(64),
        });
        starlight.bright_star_supplement = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("bright.json"),
            sha256: "e".repeat(64),
        });
        // Combined-band UV SHA is accepted structurally, but loading the missing
        // bright-star artifact must still fail closed before any silent merge.
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("bright-star artifact must load");
        assert!(
            err.to_string().contains("bright-star")
                || err.to_string().contains("No such file")
                || err.to_string().contains("failed to read")
                || err.to_string().contains("read ")
        );
    }

    #[test]
    fn update_without_starlight_config_is_inventory_noop() {
        let artifacts = PIPELINE
            .update(&base_config(None), &[])
            .expect("missing config");
        assert!(artifacts.is_none());
    }

    #[test]
    fn update_rejects_empty_gaia_product_inventory() {
        let mut starlight = measured_config();
        starlight.gaia_products.clear();
        let err = PIPELINE
            .update(&base_config(Some(starlight)), &["00000".into()])
            .expect_err("empty products");
        assert!(err
            .to_string()
            .contains("requires at least one Gaia product inventory"));
    }

    #[test]
    fn bright_star_product_compatibility_matrix() {
        use crate::starlight::bright_stars::{
            BrightStarArtifact, BrightStarArtifactProductBand, BrightStarBandComponents,
            BrightStarPopulationPolicy, BrightStarPrecedencePolicy, BrightStarSourceRecord,
            CorrelatedUncertainty, SupplementClass,
        };

        let measured = BrightStarArtifact::from_sources(
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Vec::new(),
            vec![BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 1.0,
                statistical_uncertainty_ph_m2_s: 0.01,
                systematic_independent_uncertainty_ph_m2_s: 0.02,
                systematic_catalogue_correlated: Vec::new(),
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            }],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        assert!(ensure_bright_star_product_compatible(
            StarlightProductBand::Measured336To650,
            &measured
        )
        .is_ok());
        assert!(ensure_bright_star_product_compatible(
            StarlightProductBand::Combined300To650,
            &measured
        )
        .is_err());

        let combined = BrightStarArtifact::from_sources_for_band(
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            combined_inputs(),
            vec![BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 4.0,
                statistical_uncertainty_ph_m2_s: 0.04,
                systematic_independent_uncertainty_ph_m2_s: 0.08,
                systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                    correlation_group_id: "hip2-zero-point".into(),
                    uncertainty_ph_m2_s: 0.12,
                }],
                band_components: Some(BrightStarBandComponents {
                    flux_300_336_ph_m2_s: 1.0,
                    flux_300_650_ph_m2_s: 5.0,
                    statistical_uncertainty_300_336_ph_m2_s: 0.01,
                    statistical_uncertainty_300_650_ph_m2_s: 0.05,
                    systematic_independent_uncertainty_300_336_ph_m2_s: 0.02,
                    systematic_independent_uncertainty_300_650_ph_m2_s: 0.10,
                    systematic_catalogue_correlated_300_336: vec![CorrelatedUncertainty {
                        correlation_group_id: "hip2-zero-point".into(),
                        uncertainty_ph_m2_s: 0.03,
                    }],
                    systematic_catalogue_correlated_300_650: vec![CorrelatedUncertainty {
                        correlation_group_id: "hip2-zero-point".into(),
                        uncertainty_ph_m2_s: 0.15,
                    }],
                }),
                spectral_route: format!(
                    "{}:fixture",
                    crate::starlight::bright_stars::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1
                ),
                classification_reason: "fixture".into(),
            }],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .unwrap();
        assert!(ensure_bright_star_product_compatible(
            StarlightProductBand::Combined300To650,
            &combined
        )
        .is_ok());
        assert!(ensure_bright_star_product_compatible(
            StarlightProductBand::Measured336To650,
            &combined
        )
        .is_err());
    }

    #[test]
    fn validate_config_rejects_measured_artifact_under_combined_product() {
        use crate::platform::checksum_io;
        use crate::starlight::bright_stars::{
            BrightStarArtifact, BrightStarPopulationPolicy, BrightStarPrecedencePolicy,
            BrightStarSourceRecord, SupplementClass,
        };
        use std::fs;

        let temp = tempfile::tempdir().unwrap();
        let artifact = BrightStarArtifact::from_sources(
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Vec::new(),
            vec![BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 1.0,
                statistical_uncertainty_ph_m2_s: 0.01,
                systematic_independent_uncertainty_ph_m2_s: 0.02,
                systematic_catalogue_correlated: Vec::new(),
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            }],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let path = temp.path().join("measured.json");
        fs::write(&path, artifact.to_json_pretty().unwrap()).unwrap();
        let sha = checksum_io::sha256_file(&path).unwrap();

        let mut starlight = measured_config();
        starlight.product_band = StarlightProductBand::Combined300To650;
        starlight.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: PathBuf::from("uv.json"),
            sha256: "d".repeat(64),
        });
        starlight.bright_star_supplement = Some(ArtifactPinConfig {
            artifact_path: path,
            sha256: sha,
        });
        let err = PIPELINE
            .validate_config(&base_config(Some(starlight)))
            .expect_err("measured artifact under combined product");
        assert!(err
            .to_string()
            .contains("incompatible with configured Starlight product band"));
    }

    #[test]
    fn uv_correction_shard_metadata_round_trips_fixture_fields() {
        use crate::starlight::uv::{
            CalibrationStatus, ModelResponse, SystematicCorrelation, UvCorrection,
        };

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/uv_synthetic_non_production/artifact.json");
        let bytes = std::fs::read(&fixture).unwrap();
        let sha = crate::platform::checksum_io::sha256_bytes(&bytes);
        // Load bypasses production status; metadata extraction must still work.
        let correction = UvCorrection::load(&fixture, &sha).unwrap();
        let metadata = uv_correction_shard_metadata(&correction);
        assert_eq!(metadata.model_id, correction.artifact().model_id);
        assert_eq!(metadata.artifact_sha256, sha);
        assert_eq!(
            metadata.calibration_status,
            correction.artifact().calibration_status
        );
        assert_eq!(&metadata.response, &correction.artifact().response);
        assert_eq!(
            metadata.systematic_correlation,
            correction
                .artifact()
                .uncertainty_model
                .systematic_correlation
        );
        let _ = (
            CalibrationStatus::Validated,
            ModelResponse::AbsoluteUvPhotonFlux,
            SystematicCorrelation::IndependentBetweenSources,
        );
    }

    #[test]
    fn prepare_bright_star_supplement_writes_measured_and_combined_shards() {
        use crate::platform::checksum_io;
        use crate::starlight::bright_stars::{
            BrightStarArtifact, BrightStarArtifactProductBand, BrightStarBandComponents,
            BrightStarPopulationPolicy, BrightStarPrecedencePolicy, BrightStarSourceRecord,
            CorrelatedUncertainty, SupplementClass,
        };
        use std::fs;

        let temp = tempfile::tempdir().unwrap();
        let measured = BrightStarArtifact::from_sources(
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Vec::new(),
            vec![BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 1.0,
                statistical_uncertainty_ph_m2_s: 0.01,
                systematic_independent_uncertainty_ph_m2_s: 0.02,
                systematic_catalogue_correlated: Vec::new(),
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            }],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )
        .unwrap();
        let measured_path = temp.path().join("measured.json");
        fs::write(&measured_path, measured.to_json_pretty().unwrap()).unwrap();
        let measured_sha = checksum_io::sha256_file(&measured_path).unwrap();

        let mut measured_cfg = measured_config();
        measured_cfg.map.canonical_nside = 1;
        measured_cfg.bright_star_supplement = Some(ArtifactPinConfig {
            artifact_path: measured_path,
            sha256: measured_sha,
        });
        assert!(prepare_bright_star_supplement_for_finalize(&measured_cfg, temp.path()).unwrap());
        assert!(temp
            .path()
            .join("outputs/shards/bright-star-supplement.json")
            .is_file());

        let combined = BrightStarArtifact::from_sources_for_band(
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            combined_inputs(),
            vec![BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 4.0,
                statistical_uncertainty_ph_m2_s: 0.04,
                systematic_independent_uncertainty_ph_m2_s: 0.08,
                systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                    correlation_group_id: "hip2-zero-point".into(),
                    uncertainty_ph_m2_s: 0.12,
                }],
                band_components: Some(BrightStarBandComponents {
                    flux_300_336_ph_m2_s: 1.0,
                    flux_300_650_ph_m2_s: 5.0,
                    statistical_uncertainty_300_336_ph_m2_s: 0.01,
                    statistical_uncertainty_300_650_ph_m2_s: 0.05,
                    systematic_independent_uncertainty_300_336_ph_m2_s: 0.02,
                    systematic_independent_uncertainty_300_650_ph_m2_s: 0.10,
                    systematic_catalogue_correlated_300_336: vec![CorrelatedUncertainty {
                        correlation_group_id: "hip2-zero-point".into(),
                        uncertainty_ph_m2_s: 0.03,
                    }],
                    systematic_catalogue_correlated_300_650: vec![CorrelatedUncertainty {
                        correlation_group_id: "hip2-zero-point".into(),
                        uncertainty_ph_m2_s: 0.15,
                    }],
                }),
                spectral_route: format!(
                    "{}:fixture",
                    crate::starlight::bright_stars::SPECTRAL_RECONSTRUCTION_MODEL_ID_V1
                ),
                classification_reason: "fixture".into(),
            }],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )
        .unwrap();
        let combined_path = temp.path().join("combined.json");
        fs::write(&combined_path, combined.to_json_pretty().unwrap()).unwrap();
        let combined_sha = checksum_io::sha256_file(&combined_path).unwrap();

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/uv_synthetic_non_production/artifact.json");
        let mut uv_json: serde_json::Value =
            serde_json::from_slice(&fs::read(&fixture).unwrap()).unwrap();
        uv_json["calibration_status"] = serde_json::json!("validated");
        let uv_path = temp.path().join("uv-validated.json");
        fs::write(&uv_path, serde_json::to_vec_pretty(&uv_json).unwrap()).unwrap();
        let uv_sha = checksum_io::sha256_file(&uv_path).unwrap();

        let mut combined_cfg = measured_config();
        combined_cfg.map.canonical_nside = 1;
        combined_cfg.product_band = StarlightProductBand::Combined300To650;
        combined_cfg.ultraviolet_correction = Some(ArtifactPinConfig {
            artifact_path: uv_path,
            sha256: uv_sha,
        });
        combined_cfg.bright_star_supplement = Some(ArtifactPinConfig {
            artifact_path: combined_path,
            sha256: combined_sha,
        });
        let combined_root = temp.path().join("combined-run");
        assert!(
            prepare_bright_star_supplement_for_finalize(&combined_cfg, &combined_root).unwrap()
        );
        assert!(combined_root
            .join("outputs/shards/bright-star-supplement.json")
            .is_file());

        combined_cfg.ultraviolet_correction = None;
        assert!(
            prepare_bright_star_supplement_for_finalize(&combined_cfg, &combined_root).is_err()
        );
        assert!(
            !prepare_bright_star_supplement_for_finalize(&measured_config(), temp.path()).unwrap()
        );
    }
}
