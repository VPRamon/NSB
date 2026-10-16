// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::sources::{acquisition, inventory};
use crate::dataset::{Artifact, DatasetName, DatasetPipeline, RunConfig, ValidationGate};
use anyhow::{bail, Result};
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
        if config
            .starlight
            .as_ref()
            .is_some_and(|s| s.ingestion_audit.is_some())
        {
            return vec![super::ingestion_audit::NAME.into()];
        }
        let canonical_nside = config
            .starlight
            .as_ref()
            .map(|starlight| starlight.map.canonical_nside)
            .unwrap_or(128);
        if config.starlight.as_ref().is_some_and(|s| {
            s.product_band == super::config::StarlightProductBand::Measured360To570
        }) {
            return vec![
                super::restricted::MAP_NAME.into(),
                super::restricted::REPORT_NAME.into(),
            ];
        }
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
        if starlight.ingestion_audit.is_some() {
            return Ok(Some(super::ingestion_audit::build(
                &config.workspace.root,
                starlight,
                partitions,
            )?));
        }
        if starlight.product_band == super::config::StarlightProductBand::Measured360To570 {
            let artifacts =
                super::restricted::build(&config.workspace.root, starlight, partitions)?;
            super::worker::write_artifact_index(&config.workspace.root, &artifacts)?;
            return Ok(Some(artifacts));
        }
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
        if starlight.ingestion_audit.is_some() {
            return Ok(Some(super::ingestion_audit::finalize(
                &config.workspace.root,
                starlight,
                &expected,
            )?));
        }
        if starlight.product_band == super::config::StarlightProductBand::Measured360To570 {
            return Ok(Some(super::restricted::finalize(
                &config.workspace.root,
                starlight,
                &expected,
            )?));
        }
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
        if let Some(pin) = &starlight.bright_star_supplement {
            let artifact =
                super::bright_stars::load_bright_star_artifact(&pin.artifact_path, &pin.sha256)?;
            if !super::bright_stars::artifact_compatible_with_product_band(
                &artifact.product_band,
                starlight.product_band,
            ) {
                bail!(
                    "bright-star supplement spectral coverage is incompatible with configured Starlight product band"
                );
            }
            let ultraviolet = starlight
                .ultraviolet_correction
                .as_ref()
                .map(|pin| super::uv::UvCorrection::load(&pin.artifact_path, &pin.sha256))
                .transpose()?;
            let shard = super::worker::bright_star_supplement_shard(
                &artifact,
                &pin.sha256,
                starlight.map.canonical_nside,
                starlight.product_band,
                ultraviolet.as_ref(),
            )?;
            let shard_path = config
                .workspace
                .root
                .join("outputs/shards/bright-star-supplement.json");
            shard.write(&shard_path)?;
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
        if starlight.ingestion_audit.is_some() {
            return super::ingestion_audit::gates(&config.workspace.root);
        }
        if starlight.product_band == super::config::StarlightProductBand::Measured360To570 {
            return super::restricted::gates(&config.workspace.root);
        }
        super::map::product::scientific_gates(&config.workspace.root, starlight.map.canonical_nside)
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        if name == super::ingestion_audit::NAME {
            return super::ingestion_audit::validate(path);
        }
        if name == super::restricted::MAP_NAME {
            let sha256 = crate::platform::checksum_io::sha256_file(path)?;
            nsb::StarlightMap::from_json_file(path, &sha256)?;
            return Ok(());
        }
        if name == super::restricted::REPORT_NAME {
            let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
            if value["schema"] != "nsb-starlight-interval-validation-v1" {
                bail!("invalid interval validation schema");
            }
            return Ok(());
        }
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
        if let Some(audit) = &starlight.ingestion_audit {
            if audit.expected_rows == 0
                || starlight.product_band != super::config::StarlightProductBand::Measured360To570
            {
                bail!("XP ingestion audit requires a positive declared row count and the explicit interval candidate configuration");
            }
        }
        if starlight.product_band == super::config::StarlightProductBand::Measured360To570 {
            super::restricted::validate_config(starlight)?;
        }
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
            if !super::bright_stars::artifact_compatible_with_product_band(
                &artifact.product_band,
                starlight.product_band,
            ) {
                bail!("bright-star supplement spectral coverage is incompatible with configured Starlight product band");
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::RunConfig;
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
            ingestion_audit: None,
        }
    }

    #[test]
    fn starlight_pipeline_entry_points_handle_missing_configuration() {
        let config = base_config(None);
        assert_eq!(PIPELINE.dataset(), DatasetName::Starlight);
        assert!(PIPELINE.supports_partitions());
        assert_eq!(
            PIPELINE.expected_outputs(),
            &["starlight_nside128.csv", "merge_report.json"]
        );
        assert_eq!(
            PIPELINE.expected_outputs_for(&config),
            vec!["starlight_nside128.csv", "merge_report.json"]
        );
        assert_eq!(
            PIPELINE.output_name("arbitrary.dat").unwrap(),
            "arbitrary.dat"
        );
        assert!(PIPELINE.available_partitions(&config).is_err());
        assert!(PIPELINE.build(&config, &[]).unwrap().is_none());
        assert!(PIPELINE.finalize(&config).unwrap().is_none());
        assert!(PIPELINE.validation_gates(&config, &[]).unwrap().is_empty());
        PIPELINE.validate_config(&config).unwrap();
    }

    #[test]
    fn starlight_pipeline_generic_artifact_validation_requires_map_headers_and_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy-map.csv");
        assert!(PIPELINE.validate_artifact("legacy-map.csv", &path).is_err());
        fs::write(&path, "# map_type=healpix\n# nside=4\n1,2\n2,3\n").unwrap();
        let err = PIPELINE
            .validate_artifact("legacy-map.csv", &path)
            .unwrap_err();
        assert!(err.to_string().contains("coordinate_frame"));
        fs::write(
            &path,
            "# map_type=healpix\n# coordinate_frame=galactic\n# nside=4\n1,2\n",
        )
        .unwrap();
        let err = PIPELINE
            .validate_artifact("legacy-map.csv", &path)
            .unwrap_err();
        assert!(err.to_string().contains("no map rows"));
        fs::write(
            &path,
            "# map_type=healpix\n# coordinate_frame=galactic\n# nside=4\n1,2\n2,3\n",
        )
        .unwrap();
        PIPELINE.validate_artifact("legacy-map.csv", &path).unwrap();
        assert!(PIPELINE
            .validate_artifact("merge_report.json", &path)
            .is_err());
        assert!(PIPELINE
            .validate_artifact("starlight_nside4.csv", &path)
            .is_err());
    }

    #[test]
    fn starlight_pipeline_rejects_invalid_gaia_source_fields() {
        let original = measured_config();
        let mut config = original.clone();
        config.gaia_products[0].id.clear();
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        let mut config = original.clone();
        config.gaia_products[0].checksum_manifest_sha256 = "X".repeat(64);
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        let mut config = original.clone();
        config.gaia_products[0].base_url = "http://insecure.test".into();
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        let mut config = original.clone();
        config.gaia_products[0].checksum_manifest_url = "file:///wrong".into();
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        let mut config = original.clone();
        config.acquisition.connect_timeout_seconds = 0;
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        let mut config = original.clone();
        config.acquisition.request_timeout_seconds = 0;
        assert!(PIPELINE
            .validate_config(&base_config(Some(config)))
            .is_err());
        PIPELINE
            .validate_config(&base_config(Some(original)))
            .unwrap();
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
}
