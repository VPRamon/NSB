use super::config::StarlightMode;
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
            return Ok(Some(configured_partitions(config)));
        };
        if starlight.mode == StarlightMode::Snapshot {
            return Ok(Some(configured_partitions(config)));
        }
        inventory::production_partition_ids(&config.workspace.root, &starlight.gaia_products)
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &[
            "starlight_nside128.csv",
            "starlight_nside64.csv",
            "starlight_nside256.csv",
            "merge_report.json",
        ]
    }

    fn expected_outputs_for(&self, config: &RunConfig) -> &'static [&'static str] {
        if config
            .starlight
            .as_ref()
            .is_some_and(|starlight| starlight.mode == StarlightMode::Production)
        {
            self.expected_outputs()
        } else {
            &["starlight_manual_seed_v1.csv"]
        }
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        Ok(source_name)
    }

    fn update(&self, config: &RunConfig, partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        let Some(starlight) = &config.starlight else {
            return Ok(None);
        };
        if starlight.mode == StarlightMode::Snapshot {
            return Ok(None);
        }
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
        if starlight.mode == StarlightMode::Snapshot {
            return Ok(None);
        }
        let artifacts = super::worker::build_partitions(
            &config.workspace.root,
            &starlight.gaia_products,
            partitions,
            config.execution.concurrency,
        )?;
        super::worker::write_artifact_index(&config.workspace.root, &artifacts)?;
        Ok(Some(artifacts))
    }

    fn finalize(&self, config: &RunConfig) -> Result<Option<Vec<Artifact>>> {
        if config
            .starlight
            .as_ref()
            .is_some_and(|starlight| starlight.mode == StarlightMode::Production)
        {
            let expected = self
                .available_partitions(config)?
                .ok_or_else(|| anyhow::anyhow!("Starlight inventories are missing"))?;
            return Ok(Some(super::map::product::emit_maps(
                &config.workspace.root,
                &expected,
            )?));
        }
        Ok(None)
    }

    fn validation_gates(
        &self,
        config: &RunConfig,
        _artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        if config
            .starlight
            .as_ref()
            .is_some_and(|starlight| starlight.mode == StarlightMode::Production)
        {
            return super::map::product::scientific_gates(&config.workspace.root);
        }
        Ok(Vec::new())
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        if name == "merge_report.json" {
            return super::map::product::validate_report(path);
        }
        let expected_nside = match name {
            "starlight_nside64.csv" => Some(64),
            "starlight_nside128.csv" => Some(128),
            "starlight_nside256.csv" => Some(256),
            _ => None,
        };
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
        if starlight.map.target_nside != 128 {
            bail!("production Starlight target_nside must be 128");
        }
        for required in [64, 128, 256, 512] {
            if !starlight.map.sweep_nsides.contains(&required) {
                bail!("Starlight resolution sweep is missing nside={required}");
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
        if starlight.mode == StarlightMode::Production {
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
        }
        Ok(())
    }
}

fn configured_partitions(config: &RunConfig) -> Vec<String> {
    let mut partitions: Vec<String> = config
        .sources
        .iter()
        .filter_map(|source| source.partition.clone())
        .collect();
    partitions.sort();
    partitions.dedup();
    partitions
}
