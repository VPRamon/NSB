//! Dataset-specific behavior behind the common lifecycle.

use super::{Artifact, DatasetName, RunConfig, ValidationGate};
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Domain behavior required by the generic dataset engine.
///
/// Implementations own scientific transformation and validation. They must not
/// own scheduling, persistence, CLI parsing, or publication.
pub trait DatasetPipeline: Sync {
    /// Dataset implemented by this pipeline.
    fn dataset(&self) -> DatasetName;

    /// Whether this dataset may be split into independently processed inputs.
    fn supports_partitions(&self) -> bool {
        false
    }

    /// Discover the complete, deterministic partition set for this dataset.
    ///
    /// `None` means that a prerequisite inventory has not been created yet.
    /// Implementations must distinguish that state from a malformed inventory,
    /// which must fail closed.
    fn available_partitions(&self, config: &RunConfig) -> Result<Option<Vec<String>>> {
        let mut partitions: Vec<String> = config
            .sources
            .iter()
            .filter_map(|source| source.partition.clone())
            .collect();
        partitions.sort();
        partitions.dedup();
        Ok(Some(partitions))
    }

    /// Complete artifact names for non-partitioned datasets.
    fn expected_outputs(&self) -> &'static [&'static str];

    /// Configuration-specific artifact set when outputs depend on run policy.
    fn expected_outputs_for(&self, _config: &RunConfig) -> Vec<String> {
        self.expected_outputs()
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    /// Whether a configured source produces a runtime artifact during build.
    /// Validation-only reference sources return `false`.
    fn is_build_source(&self, _source_name: &str) -> bool {
        true
    }

    /// Validate dataset-specific configuration before any state is written.
    fn validate_config(&self, _config: &RunConfig) -> Result<()> {
        Ok(())
    }

    /// Optionally own source update for a specialized dataset.
    ///
    /// `None` delegates to the common file/HTTPS source updater.
    fn update(&self, _config: &RunConfig, _partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        Ok(None)
    }

    /// Optionally own the complete build for a specialized dataset.
    ///
    /// `None` delegates to the common one-source/one-artifact transformer.
    fn build(&self, _config: &RunConfig, _partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        Ok(None)
    }

    /// Optionally reconcile partition artifacts into final dataset products.
    fn finalize(&self, _config: &RunConfig) -> Result<Option<Vec<Artifact>>> {
        Ok(None)
    }

    /// Dataset-wide validation gates that cannot be checked per artifact.
    fn validation_gates(
        &self,
        _config: &RunConfig,
        _artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        Ok(Vec::new())
    }

    /// Map one configured source name to its deterministic artifact name.
    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str>;

    /// Transform one verified source into its output artifact.
    fn transform(&self, source_name: &str, input: &Path, output: &Path) -> Result<()> {
        let bytes = fs::read(input)?;
        if bytes.contains(&0) {
            bail!("source {source_name:?} contains NUL bytes");
        }
        self.validate_artifact(source_name, input)?;
        crate::dataset::engine::atomic_write(output, &bytes)
    }

    /// Validate one generated artifact against its domain schema.
    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()>;
}

struct AirglowPipeline;
struct SolarPipeline;
struct MoonlightPipeline;

static AIRGLOW: AirglowPipeline = AirglowPipeline;
static SOLAR: SolarPipeline = SolarPipeline;
static MOONLIGHT: MoonlightPipeline = MoonlightPipeline;

pub(crate) fn pipeline_for(dataset: DatasetName) -> &'static dyn DatasetPipeline {
    match dataset {
        DatasetName::AirglowContinuum => &AIRGLOW,
        DatasetName::SolarSpectrum => &SOLAR,
        DatasetName::MoonlightScattering => &MOONLIGHT,
        DatasetName::Starlight => &crate::starlight::PIPELINE,
    }
}

impl DatasetPipeline for AirglowPipeline {
    fn dataset(&self) -> DatasetName {
        DatasetName::AirglowContinuum
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &[super::airglow_palace::OUTPUT_NAME]
    }

    fn output_name<'a>(&self, _source_name: &'a str) -> Result<&'a str> {
        bail!("PALACE owns its complete build")
    }

    fn validate_config(&self, config: &RunConfig) -> Result<()> {
        super::airglow_palace::validate_config(config)
    }

    fn build(&self, config: &RunConfig, _partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        Ok(Some(super::airglow_palace::build(config)?))
    }

    fn validate_artifact(&self, _name: &str, path: &Path) -> Result<()> {
        super::airglow_palace::validate_artifact(path)
    }

    fn validation_gates(
        &self,
        _config: &RunConfig,
        artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        super::airglow_palace::validation_gates(artifacts)
    }
}

impl DatasetPipeline for SolarPipeline {
    fn dataset(&self) -> DatasetName {
        DatasetName::SolarSpectrum
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &["solar_spectrum.dat"]
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        super::solar_spectrum::output_name(source_name)
    }

    fn is_build_source(&self, source_name: &str) -> bool {
        source_name == super::solar_spectrum::SOURCE_NAME
    }

    fn validate_config(&self, config: &RunConfig) -> Result<()> {
        super::solar_spectrum::validate_config(config)
    }

    fn transform(&self, source_name: &str, input: &Path, output: &Path) -> Result<()> {
        super::solar_spectrum::transform(source_name, input, output)
    }

    fn validation_gates(
        &self,
        config: &RunConfig,
        artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        super::solar_spectrum::validation_gates(config, artifacts)
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        super::solar_spectrum::validate_artifact(name, path)
    }
}

impl DatasetPipeline for MoonlightPipeline {
    fn dataset(&self) -> DatasetName {
        DatasetName::MoonlightScattering
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &[super::moonlight_mie::OUTPUT, super::moonlight_mie::SSCAT]
    }

    fn is_build_source(&self, source_name: &str) -> bool {
        source_name != super::moonlight_mie::MODEL_SOURCE
    }

    fn validate_config(&self, config: &RunConfig) -> Result<()> {
        super::moonlight_mie::validate_config(config)
    }

    fn build(&self, config: &RunConfig, _partitions: &[String]) -> Result<Option<Vec<Artifact>>> {
        Ok(Some(super::moonlight_mie::build(config)?))
    }

    fn validation_gates(
        &self,
        config: &RunConfig,
        artifacts: &[Artifact],
    ) -> Result<Vec<ValidationGate>> {
        super::moonlight_mie::validation_gates(config, artifacts)
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        match source_name {
            super::moonlight_mie::MODEL_SOURCE => Ok(super::moonlight_mie::OUTPUT),
            super::moonlight_mie::SSCAT => Ok(super::moonlight_mie::SSCAT),
            _ => bail!("unexpected moonlight source {source_name:?}"),
        }
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        super::moonlight_mie::validate_artifact(name, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::moonlight_mie;

    #[test]
    fn moonlight_pipeline_routes_model_and_runtime_artifacts() {
        let pipeline = pipeline_for(DatasetName::MoonlightScattering);
        assert_eq!(pipeline.dataset(), DatasetName::MoonlightScattering);
        assert_eq!(
            pipeline.expected_outputs(),
            &[moonlight_mie::OUTPUT, moonlight_mie::SSCAT]
        );
        assert!(!pipeline.is_build_source(moonlight_mie::MODEL_SOURCE));
        assert!(pipeline.is_build_source(moonlight_mie::SSCAT));
        assert_eq!(
            pipeline.output_name(moonlight_mie::MODEL_SOURCE).unwrap(),
            moonlight_mie::OUTPUT
        );
        assert_eq!(
            pipeline.output_name(moonlight_mie::SSCAT).unwrap(),
            moonlight_mie::SSCAT
        );
        assert!(pipeline.output_name("unexpected").is_err());

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(moonlight_mie::SSCAT);
        fs::write(&path, "1\n2\n").unwrap();
        pipeline
            .validate_artifact(moonlight_mie::SSCAT, &path)
            .unwrap();
    }
}
