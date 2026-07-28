//! Dataset-specific behavior behind the common lifecycle.

use super::{Artifact, DatasetName, RunConfig, ValidationGate};
use anyhow::{bail, Result};
use std::fs;
use std::io::{BufRead, BufReader};
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

    /// Configuration-specific artifact set for pipelines with snapshot and production modes.
    fn expected_outputs_for(&self, _config: &RunConfig) -> Vec<String> {
        self.expected_outputs()
            .iter()
            .map(|name| (*name).to_string())
            .collect()
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
        &["airglow_cont.dat"]
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        require_expected(self, source_name)
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        require_minimum_rows(name, path, 2)
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
        require_expected(self, source_name)
    }

    fn validate_artifact(&self, _name: &str, path: &Path) -> Result<()> {
        let rows = data_rows(path)?;
        if rows.is_empty()
            || rows.iter().any(|line| {
                let mut fields = line.split(',');
                fields
                    .next()
                    .and_then(|value| value.trim().parse::<f64>().ok())
                    .is_none()
                    || fields
                        .next()
                        .and_then(|value| value.trim().parse::<f64>().ok())
                        .is_none()
            })
        {
            bail!("solar spectrum requires two numeric CSV columns");
        }
        Ok(())
    }
}

impl DatasetPipeline for MoonlightPipeline {
    fn dataset(&self) -> DatasetName {
        DatasetName::MoonlightScattering
    }

    fn expected_outputs(&self) -> &'static [&'static str] {
        &["mie_m15s1.dat", "sscatcor_m15s1.dat"]
    }

    fn output_name<'a>(&self, source_name: &'a str) -> Result<&'a str> {
        require_expected(self, source_name)
    }

    fn validate_artifact(&self, name: &str, path: &Path) -> Result<()> {
        require_minimum_rows(name, path, 2)
    }
}

fn require_expected<'a>(pipeline: &dyn DatasetPipeline, source_name: &'a str) -> Result<&'a str> {
    if pipeline.expected_outputs().contains(&source_name) {
        Ok(source_name)
    } else {
        bail!(
            "unexpected source name {source_name:?} for {}",
            pipeline.dataset()
        )
    }
}

fn require_minimum_rows(name: &str, path: &Path, minimum: usize) -> Result<()> {
    if data_rows(path)?.len() < minimum {
        bail!("{name} contains too few data rows");
    }
    Ok(())
}

fn data_rows(path: &Path) -> Result<Vec<String>> {
    Ok(data_rows_from(&read_lines(path)?)
        .into_iter()
        .map(str::to_string)
        .collect())
}

fn data_rows_from(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .map(String::as_str)
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect()
}

fn read_lines(path: &Path) -> Result<Vec<String>> {
    Ok(BufReader::new(fs::File::open(path)?)
        .lines()
        .collect::<std::io::Result<_>>()?)
}
