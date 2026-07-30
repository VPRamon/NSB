//! Versioned, fail-closed contract for the Starlight 300–336 nm correction.
//!
//! This module defines ingestion and evaluation contracts only. The repository
//! intentionally contains no production UV calibration coefficients.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const UV_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const REFERENCE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const PARTITION_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const VALIDATION_REPORT_SCHEMA_VERSION: u32 = 1;
pub const UV_BAND_NM: [u16; 2] = [300, 336];
pub const MEASURED_BAND_NM: [u16; 2] = [336, 650];
pub const PHOTON_FLUX_UNIT: &str = "ph_m-2_s-1";

/// Calibration readiness asserted by the artifact authors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalibrationStatus {
    TestOnly,
    Candidate,
    Validated,
    Rejected,
}

/// Immutable reference-dataset identity embedded in an artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceDataset {
    pub name: String,
    pub release: String,
    pub licence: String,
    pub files: Vec<ReferenceFile>,
    pub wavelength_band_nm: [u16; 2],
    pub spectral_flux_unit: String,
    pub transformations: Vec<String>,
    pub quality_cuts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceFile {
    pub name: String,
    pub sha256: String,
}

/// Pre-registered source/sky partition evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionEvidence {
    pub assignment_algorithm: String,
    pub seed: u64,
    pub manifest_sha256: String,
    pub training: PartitionSummary,
    pub validation: PartitionSummary,
    pub test: PartitionSummary,
    pub source_disjoint: bool,
    pub sky_disjoint: bool,
    pub disjointness_evidence_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionSummary {
    pub partition_id: String,
    pub source_count: u64,
    pub source_ids_sha256: String,
    pub sky_regions: Vec<String>,
}

/// One explicitly named and ordered model input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predictor {
    pub name: String,
    pub unit: String,
    pub transformation: PredictorTransformation,
    pub domain: PredictorDomain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PredictorTransformation {
    Identity,
    Log10,
    Standardize { mean: f64, scale: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictorDomain {
    pub minimum: f64,
    pub maximum: f64,
}

/// Supported, intentionally simple evaluation families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CorrectionModel {
    /// Intercept followed by coefficients in `predictors` order.
    Linear {
        parameters: Vec<f64>,
        covariance: Vec<Vec<f64>>,
    },
}

/// Physical quantity predicted by the registered model score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ModelResponse {
    AbsoluteUvPhotonFlux,
    NaturalLogUvToMeasuredFluxRatio { denominator_band_nm: [u16; 2] },
}

/// Explicit action for inputs outside the registered applicability domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "kebab-case", deny_unknown_fields)]
pub enum OutOfDomainPolicy {
    Reject,
    /// Evaluate at the nearest registered boundary and inflate systematic error.
    ClampWithSystematicInflation {
        factor: f64,
    },
}

/// Source-to-source correlation semantics for correction systematics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SystematicCorrelation {
    IndependentBetweenSources,
    FullyCorrelatedBetweenSources,
}

/// Residual and systematic uncertainty contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UncertaintyModel {
    /// Absolute residual floor in `ph m^-2 s^-1`. Required for absolute-flux
    /// responses; must be exactly `0` for log-ratio responses (use
    /// [`Self::statistical_floor_log_ratio`] instead).
    pub statistical_floor_ph_m2_s: f64,
    /// Absolute systematic floor in `ph m^-2 s^-1`. Required for absolute-flux
    /// responses; must be exactly `0` for log-ratio responses (use
    /// [`Self::systematic_floor_log_ratio`] instead).
    pub systematic_floor_ph_m2_s: f64,
    pub systematic_fraction: f64,
    /// Dimensionless residual floor on `ln(F_UV / F_meas)`. Used only by
    /// log-ratio responses; defaults to `0` for absolute-flux artifacts.
    #[serde(default)]
    pub statistical_floor_log_ratio: f64,
    /// Dimensionless systematic floor on `ln(F_UV / F_meas)`. Used only by
    /// log-ratio responses; defaults to `0` for absolute-flux artifacts.
    #[serde(default)]
    pub systematic_floor_log_ratio: f64,
    /// Correlation between the measured XP statistical error and the conditional
    /// UV-model residual. For log-ratio responses this excludes the structural
    /// dependence of corrected UV flux on the measured XP flux.
    pub measured_conditional_residual_statistical_correlation: f64,
    pub systematic_correlation: SystematicCorrelation,
}

/// Holdout evidence carried by the artifact, stratified by required dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationMetric {
    pub kind: ValidationMetricKind,
    pub value: f64,
    pub unit: String,
    pub sample_count: u64,
    pub stratum: ValidationStratum,
}

/// Closed metric vocabulary with value-domain semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationMetricKind {
    Bias,
    MeanResidual,
    MedianResidual,
    Mae,
    Rmse,
    Scatter,
    StandardDeviation,
    Uncertainty,
    IntervalCoverage,
}

impl ValidationMetricKind {
    fn value_domain(self) -> MetricValueDomain {
        match self {
            Self::Bias | Self::MeanResidual | Self::MedianResidual => MetricValueDomain::Signed,
            Self::Mae
            | Self::Rmse
            | Self::Scatter
            | Self::StandardDeviation
            | Self::Uncertainty => MetricValueDomain::NonNegative,
            Self::IntervalCoverage => MetricValueDomain::Fraction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetricValueDomain {
    Signed,
    NonNegative,
    Fraction,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationStratum {
    pub colour: String,
    pub magnitude: String,
    pub extinction_proxy: String,
    pub quality: String,
    pub sky_region: String,
    pub extrapolation_status: String,
}

/// Complete immutable 300–336 nm correction artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvCalibrationArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub calibration_status: CalibrationStatus,
    pub correction_band_nm: [u16; 2],
    pub flux_unit: String,
    pub statistical_uncertainty_unit: String,
    pub systematic_uncertainty_unit: String,
    pub reference_dataset: ReferenceDataset,
    pub partitions: PartitionEvidence,
    pub predictors: Vec<Predictor>,
    pub model: CorrectionModel,
    pub response: ModelResponse,
    pub uncertainty_model: UncertaintyModel,
    pub out_of_domain_policy: OutOfDomainPolicy,
    pub validation_metrics: Vec<ValidationMetric>,
    pub training_command: String,
    pub software_version: String,
}

/// Domain classification preserved for every evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicabilityStatus {
    InDomain,
    Boundary,
    OutOfDomain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvaluationDecision {
    Applied,
    Rejected,
    Clamped,
}

/// Measured XP value required by response families that are relative to it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredBandInput {
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
}

/// Explicit inputs to one UV model evaluation.
#[derive(Debug, Clone, Copy)]
pub struct UvEvaluationInput<'a> {
    pub predictors: &'a BTreeMap<String, f64>,
    pub measured_band: Option<MeasuredBandInput>,
}

/// Result of evaluating explicitly named predictor values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvCorrectionEvaluation {
    pub flux_300_336_ph_m2_s: Option<f64>,
    pub statistical_uncertainty_300_336_ph_m2_s: Option<f64>,
    pub systematic_uncertainty_300_336_ph_m2_s: Option<f64>,
    pub applicability_status: ApplicabilityStatus,
    pub decision: EvaluationDecision,
    pub model_id: String,
    pub artifact_sha256: String,
    pub response: ModelResponse,
    pub measured_band: Option<MeasuredBandInput>,
    /// Total statistical covariance between measured XP flux and corrected UV
    /// flux. Log-ratio responses include their structural shared-flux term.
    pub measured_correction_statistical_covariance_ph2_m4_s2: Option<f64>,
}

/// Measured XP and corrected UV components retained for source diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedBandFlux {
    pub flux_300_336_ph_m2_s: f64,
    pub flux_336_650_ph_m2_s: f64,
    pub flux_300_650_ph_m2_s: f64,
    pub statistical_uncertainty_300_336_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_300_650_ph_m2_s: f64,
    pub systematic_uncertainty_300_336_ph_m2_s: f64,
    pub systematic_uncertainty_300_650_ph_m2_s: f64,
    pub applicability_status: ApplicabilityStatus,
    pub decision: EvaluationDecision,
    pub model_id: String,
    pub artifact_sha256: String,
    pub systematic_correlation: SystematicCorrelation,
}

/// Validated artifact paired with the digest of its exact serialized bytes.
#[derive(Debug, Clone)]
pub struct UvCorrection {
    artifact: UvCalibrationArtifact,
    artifact_sha256: String,
}

impl UvCalibrationArtifact {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != UV_ARTIFACT_SCHEMA_VERSION {
            bail!(
                "unsupported UV artifact schema_version {}",
                self.schema_version
            );
        }
        require_text("model_id", &self.model_id)?;
        if self.correction_band_nm != UV_BAND_NM {
            bail!("UV correction band must be exactly 300–336 nm");
        }
        for (label, unit) in [
            ("flux", &self.flux_unit),
            (
                "statistical uncertainty",
                &self.statistical_uncertainty_unit,
            ),
            ("systematic uncertainty", &self.systematic_uncertainty_unit),
        ] {
            if unit != PHOTON_FLUX_UNIT {
                bail!("UV {label} unit must be {PHOTON_FLUX_UNIT}");
            }
        }
        self.reference_dataset.validate()?;
        self.partitions.validate()?;
        if self.predictors.is_empty() {
            bail!("UV artifact requires at least one predictor");
        }
        let mut names = BTreeSet::new();
        for predictor in &self.predictors {
            predictor.validate()?;
            if !names.insert(predictor.name.as_str()) {
                bail!(
                    "UV artifact contains duplicate predictor {}",
                    predictor.name
                );
            }
        }
        self.validate_model()?;
        self.response.validate()?;
        self.uncertainty_model
            .validate_for_response(&self.response)?;
        match self.out_of_domain_policy {
            OutOfDomainPolicy::Reject => {}
            OutOfDomainPolicy::ClampWithSystematicInflation { factor }
                if factor.is_finite() && factor >= 1.0 => {}
            OutOfDomainPolicy::ClampWithSystematicInflation { .. } => {
                bail!("out-of-domain systematic inflation must be finite and >= 1")
            }
        }
        if self.validation_metrics.is_empty() {
            bail!("UV artifact requires validation metrics");
        }
        for metric in &self.validation_metrics {
            metric.validate()?;
        }
        require_text("training_command", &self.training_command)?;
        require_text("software_version", &self.software_version)?;
        Ok(())
    }

    fn validate_model(&self) -> Result<()> {
        let dimension = self.predictors.len() + 1;
        match &self.model {
            CorrectionModel::Linear {
                parameters,
                covariance,
            } => {
                if parameters.len() != dimension
                    || parameters.iter().any(|value| !value.is_finite())
                {
                    bail!("linear UV model requires {} finite parameters", dimension);
                }
                if covariance.len() != dimension
                    || covariance.iter().any(|row| row.len() != dimension)
                {
                    bail!("UV model covariance dimensions do not match parameters");
                }
                for row in covariance {
                    if row.iter().any(|value| !value.is_finite()) {
                        bail!("UV model covariance contains non-finite values");
                    }
                }
                for (index, row) in covariance.iter().enumerate() {
                    if row[index] < 0.0 {
                        bail!("UV model covariance has negative diagonal variance");
                    }
                    for (other, other_row) in covariance.iter().enumerate() {
                        let scale = row[other].abs().max(other_row[index].abs()).max(1.0);
                        if (row[other] - other_row[index]).abs() > 1.0e-12 * scale {
                            bail!("UV model covariance must be symmetric");
                        }
                    }
                }
                validate_positive_semidefinite(covariance)?;
            }
        }
        Ok(())
    }
}

impl ReferenceDataset {
    pub fn validate(&self) -> Result<()> {
        require_text("reference dataset name", &self.name)?;
        require_text("reference dataset release", &self.release)?;
        require_text("reference dataset licence", &self.licence)?;
        if self.files.is_empty() {
            bail!("reference dataset requires immutable files");
        }
        let mut names = BTreeSet::new();
        for file in &self.files {
            require_safe_relative_path("reference file name", &file.name)?;
            require_sha256("reference file", &file.sha256)?;
            if !names.insert(file.name.as_str()) {
                bail!("duplicate reference file {}", file.name);
            }
        }
        if self.wavelength_band_nm[0] > UV_BAND_NM[0] || self.wavelength_band_nm[1] <= UV_BAND_NM[1]
        {
            bail!("reference dataset must cover both sides of 300–336 nm");
        }
        require_text("reference spectral flux unit", &self.spectral_flux_unit)?;
        require_nonempty_texts("reference transformations", &self.transformations)?;
        require_nonempty_texts("reference quality cuts", &self.quality_cuts)
    }
}

impl PartitionEvidence {
    pub fn validate(&self) -> Result<()> {
        require_text("partition assignment algorithm", &self.assignment_algorithm)?;
        require_sha256("partition manifest", &self.manifest_sha256)?;
        require_sha256(
            "partition disjointness evidence",
            &self.disjointness_evidence_sha256,
        )?;
        if !self.source_disjoint || !self.sky_disjoint {
            bail!("UV partitions require source-disjoint and sky-disjoint evidence");
        }
        self.training.validate()?;
        self.validation.validate()?;
        self.test.validate()?;
        let partitions = [&self.training, &self.validation, &self.test];
        if partitions
            .iter()
            .map(|partition| partition.partition_id.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            != 3
        {
            bail!("training, validation, and test partition IDs must differ");
        }
        let mut sky_regions = BTreeSet::new();
        for partition in partitions {
            for region in &partition.sky_regions {
                if !sky_regions.insert(region) {
                    bail!("UV partition sky regions are not disjoint");
                }
            }
        }
        Ok(())
    }
}

impl PartitionSummary {
    fn validate(&self) -> Result<()> {
        require_text("partition_id", &self.partition_id)?;
        if self.source_count == 0 || self.sky_regions.is_empty() {
            bail!("UV partitions must contain sources and sky regions");
        }
        require_sha256("partition source IDs", &self.source_ids_sha256)?;
        require_nonempty_texts("partition sky regions", &self.sky_regions)
    }
}

impl Predictor {
    fn validate(&self) -> Result<()> {
        require_identifier("predictor name", &self.name)?;
        require_text("predictor unit", &self.unit)?;
        if !self.domain.minimum.is_finite()
            || !self.domain.maximum.is_finite()
            || self.domain.minimum >= self.domain.maximum
        {
            bail!("predictor {} has an invalid domain", self.name);
        }
        match self.transformation {
            PredictorTransformation::Identity => {}
            PredictorTransformation::Log10 if self.domain.minimum > 0.0 => {}
            PredictorTransformation::Log10 => {
                bail!("log10 predictor {} requires a positive domain", self.name)
            }
            PredictorTransformation::Standardize { mean, scale }
                if mean.is_finite() && scale.is_finite() && scale > 0.0 => {}
            PredictorTransformation::Standardize { .. } => {
                bail!(
                    "standardized predictor {} has invalid parameters",
                    self.name
                )
            }
        }
        Ok(())
    }
}

impl UncertaintyModel {
    fn validate(&self) -> Result<()> {
        for (label, value) in [
            ("statistical floor", self.statistical_floor_ph_m2_s),
            ("systematic floor", self.systematic_floor_ph_m2_s),
            ("systematic fraction", self.systematic_fraction),
            (
                "statistical floor log-ratio",
                self.statistical_floor_log_ratio,
            ),
            (
                "systematic floor log-ratio",
                self.systematic_floor_log_ratio,
            ),
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!("UV {label} must be finite and non-negative");
            }
        }
        if !self
            .measured_conditional_residual_statistical_correlation
            .is_finite()
            || !(-1.0..=1.0).contains(&self.measured_conditional_residual_statistical_correlation)
        {
            bail!("measured/conditional-residual statistical correlation must be in [-1, 1]");
        }
        Ok(())
    }

    fn validate_for_response(&self, response: &ModelResponse) -> Result<()> {
        self.validate()?;
        match response {
            ModelResponse::AbsoluteUvPhotonFlux => {
                if self.statistical_floor_log_ratio != 0.0 || self.systematic_floor_log_ratio != 0.0
                {
                    bail!(
                        "absolute UV artifacts must leave log-ratio floors at 0; \
                         use statistical_floor_ph_m2_s / systematic_floor_ph_m2_s"
                    );
                }
            }
            ModelResponse::NaturalLogUvToMeasuredFluxRatio { .. } => {
                if self.statistical_floor_ph_m2_s != 0.0 || self.systematic_floor_ph_m2_s != 0.0 {
                    bail!(
                        "log-ratio UV artifacts must set absolute ph m^-2 s^-1 floors to 0; \
                         bright-star absolute RMSE floors do not transfer to Gaia sources. \
                         Use statistical_floor_log_ratio / systematic_floor_log_ratio"
                    );
                }
            }
        }
        Ok(())
    }
}

impl ModelResponse {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::AbsoluteUvPhotonFlux => Ok(()),
            Self::NaturalLogUvToMeasuredFluxRatio {
                denominator_band_nm,
            } if *denominator_band_nm == MEASURED_BAND_NM => Ok(()),
            Self::NaturalLogUvToMeasuredFluxRatio { .. } => {
                bail!("UV log-ratio response denominator must be exactly 336–650 nm")
            }
        }
    }
}

impl ValidationMetric {
    fn validate(&self) -> Result<()> {
        require_text("validation metric unit", &self.unit)?;
        if !self.value.is_finite() || self.sample_count == 0 {
            bail!("validation metrics require finite values and non-zero sample counts");
        }
        match self.kind.value_domain() {
            MetricValueDomain::Signed => {}
            MetricValueDomain::NonNegative if self.value >= 0.0 => {}
            MetricValueDomain::NonNegative => {
                bail!("non-negative validation metric has a negative value")
            }
            MetricValueDomain::Fraction if (0.0..=1.0).contains(&self.value) => {}
            MetricValueDomain::Fraction => {
                bail!("fraction validation metric must be in [0, 1]")
            }
        }
        for (label, value) in [
            ("colour", &self.stratum.colour),
            ("magnitude", &self.stratum.magnitude),
            ("extinction proxy", &self.stratum.extinction_proxy),
            ("quality", &self.stratum.quality),
            ("sky region", &self.stratum.sky_region),
            ("extrapolation status", &self.stratum.extrapolation_status),
        ] {
            require_text(label, value)?;
        }
        Ok(())
    }
}

impl UvCorrection {
    /// Load exact artifact bytes, verify their pinned digest, and validate them.
    pub fn load(path: &Path, pinned_sha256: &str) -> Result<Self> {
        require_sha256("configured UV artifact", pinned_sha256)?;
        let bytes = fs::read(path)
            .with_context(|| format!("read UV calibration artifact {}", path.display()))?;
        let actual = checksum_io::sha256_bytes(&bytes);
        if actual != pinned_sha256 {
            bail!(
                "UV artifact checksum mismatch for {}: expected {}, actual {}",
                path.display(),
                pinned_sha256,
                actual
            );
        }
        let artifact: UvCalibrationArtifact = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse UV calibration artifact {}", path.display()))?;
        artifact.validate()?;
        Ok(Self {
            artifact,
            artifact_sha256: actual,
        })
    }

    /// Refuse artifacts that have not completed the independent validation gate.
    pub fn require_production_status(&self) -> Result<()> {
        if self.artifact.calibration_status != CalibrationStatus::Validated {
            bail!(
                "UV artifact {} has status {:?}, not validated",
                self.artifact.model_id,
                self.artifact.calibration_status
            );
        }
        Ok(())
    }

    pub fn artifact(&self) -> &UvCalibrationArtifact {
        &self.artifact
    }

    pub fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    /// Evaluate a correction without spectral-edge extrapolation.
    pub fn evaluate(&self, input: UvEvaluationInput<'_>) -> Result<UvCorrectionEvaluation> {
        let expected = self
            .artifact
            .predictors
            .iter()
            .map(|predictor| predictor.name.as_str())
            .collect::<BTreeSet<_>>();
        let supplied = input
            .predictors
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if supplied != expected {
            bail!(
                "UV predictor names do not match artifact: expected {:?}, found {:?}",
                expected,
                supplied
            );
        }
        if let Some(measured) = input.measured_band {
            validate_measured_band(measured)?;
        }
        if matches!(
            self.artifact.response,
            ModelResponse::NaturalLogUvToMeasuredFluxRatio { .. }
        ) {
            let measured = input.measured_band.context(
                "UV log-ratio response requires measured 336–650 nm flux and uncertainty",
            )?;
            if measured.flux_336_650_ph_m2_s <= 0.0 {
                bail!("UV log-ratio response requires positive measured 336–650 nm flux");
            }
        }

        let mut status = ApplicabilityStatus::InDomain;
        let mut transformed = Vec::with_capacity(self.artifact.predictors.len() + 1);
        transformed.push(1.0);
        for predictor in &self.artifact.predictors {
            let original = input.predictors[&predictor.name];
            if !original.is_finite() {
                bail!("UV predictor {} is not finite", predictor.name);
            }
            let outside =
                original < predictor.domain.minimum || original > predictor.domain.maximum;
            let boundary =
                original == predictor.domain.minimum || original == predictor.domain.maximum;
            if outside {
                status = ApplicabilityStatus::OutOfDomain;
            } else if boundary && status == ApplicabilityStatus::InDomain {
                status = ApplicabilityStatus::Boundary;
            }
            let evaluated = if outside {
                match self.artifact.out_of_domain_policy {
                    OutOfDomainPolicy::Reject => {
                        return Ok(self.rejected_evaluation(input.measured_band));
                    }
                    OutOfDomainPolicy::ClampWithSystematicInflation { .. } => {
                        original.clamp(predictor.domain.minimum, predictor.domain.maximum)
                    }
                }
            } else {
                original
            };
            transformed.push(transform(&predictor.transformation, evaluated)?);
        }

        let (parameters, covariance) = match &self.artifact.model {
            CorrectionModel::Linear {
                parameters,
                covariance,
            } => (parameters, covariance),
        };
        let score = dot(parameters, &transformed);
        if !score.is_finite() {
            bail!("UV correction model produced a non-finite score");
        }
        let score_variance = quadratic_form(covariance, &transformed)?;
        let statistical_floor_abs = self.artifact.uncertainty_model.statistical_floor_ph_m2_s;
        let statistical_floor_ln = self.artifact.uncertainty_model.statistical_floor_log_ratio;
        let measured_residual_correlation = self
            .artifact
            .uncertainty_model
            .measured_conditional_residual_statistical_correlation;
        let (flux, statistical, measured_covariance) = match &self.artifact.response {
            ModelResponse::AbsoluteUvPhotonFlux => {
                if score < 0.0 {
                    bail!("absolute UV response produced a negative flux");
                }
                let correction_statistical = score_variance.sqrt().hypot(statistical_floor_abs);
                let covariance = input.measured_band.map(|measured| {
                    measured_residual_correlation
                        * measured.statistical_uncertainty_336_650_ph_m2_s
                        * correction_statistical
                });
                (score, correction_statistical, covariance)
            }
            ModelResponse::NaturalLogUvToMeasuredFluxRatio { .. } => {
                let measured = input.measured_band.context(
                    "UV log-ratio response requires measured 336–650 nm flux and uncertainty",
                )?;
                let ratio = score.exp();
                if !ratio.is_finite() {
                    bail!("UV log-ratio response exponent overflow");
                }
                let flux = measured.flux_336_650_ph_m2_s * ratio;
                if !flux.is_finite() {
                    bail!("UV log-ratio response produced non-finite flux");
                }
                // Conditional residual lives in ln-ratio space: convert to absolute
                // flux units with the local Jacobian `dF/d(ln r) = F`.
                let conditional_statistical =
                    flux * score_variance.sqrt().hypot(statistical_floor_ln);
                let measured_sigma = measured.statistical_uncertainty_336_650_ph_m2_s;
                let measured_contribution = ratio * measured_sigma;
                let correction_variance = conditional_statistical.powi(2)
                    + measured_contribution.powi(2)
                    + 2.0
                        * measured_residual_correlation
                        * conditional_statistical
                        * measured_contribution;
                if !correction_variance.is_finite() || correction_variance < -1.0e-12 {
                    bail!("UV log-ratio uncertainty propagation produced invalid variance");
                }
                let measured_covariance = ratio * measured_sigma.powi(2)
                    + measured_residual_correlation * measured_sigma * conditional_statistical;
                (
                    flux,
                    correction_variance.max(0.0).sqrt(),
                    Some(measured_covariance),
                )
            }
        };
        let mut systematic = match &self.artifact.response {
            ModelResponse::AbsoluteUvPhotonFlux => self
                .artifact
                .uncertainty_model
                .systematic_floor_ph_m2_s
                .hypot(flux * self.artifact.uncertainty_model.systematic_fraction),
            ModelResponse::NaturalLogUvToMeasuredFluxRatio { .. } => {
                let relative = self
                    .artifact
                    .uncertainty_model
                    .systematic_floor_log_ratio
                    .hypot(self.artifact.uncertainty_model.systematic_fraction);
                flux * relative
            }
        };
        let decision = if status == ApplicabilityStatus::OutOfDomain {
            let OutOfDomainPolicy::ClampWithSystematicInflation { factor } =
                self.artifact.out_of_domain_policy
            else {
                unreachable!("rejection returned before evaluation");
            };
            systematic *= factor;
            EvaluationDecision::Clamped
        } else {
            EvaluationDecision::Applied
        };
        if !statistical.is_finite() || !systematic.is_finite() {
            bail!("UV correction model produced non-finite uncertainty");
        }
        Ok(UvCorrectionEvaluation {
            flux_300_336_ph_m2_s: Some(flux),
            statistical_uncertainty_300_336_ph_m2_s: Some(statistical),
            systematic_uncertainty_300_336_ph_m2_s: Some(systematic),
            applicability_status: status,
            decision,
            model_id: self.artifact.model_id.clone(),
            artifact_sha256: self.artifact_sha256.clone(),
            response: self.artifact.response.clone(),
            measured_band: input.measured_band,
            measured_correction_statistical_covariance_ph2_m4_s2: measured_covariance,
        })
    }

    /// Combine a successful correction with the unchanged measured XP value.
    ///
    /// The evaluation's total measured/correction covariance is used for
    /// statistical propagation. The artifact correlation applies only to the
    /// conditional model residual; the correction systematic remains separate.
    pub fn combine_with_measured(
        &self,
        flux_336_650_ph_m2_s: f64,
        statistical_uncertainty_336_650_ph_m2_s: f64,
        correction: &UvCorrectionEvaluation,
    ) -> Result<CombinedBandFlux> {
        validate_measured_band(MeasuredBandInput {
            flux_336_650_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s,
        })?;
        if correction.model_id != self.artifact.model_id
            || correction.artifact_sha256 != self.artifact_sha256
            || correction.response != self.artifact.response
        {
            bail!("UV correction evaluation does not belong to this artifact");
        }
        if let Some(measured) = correction.measured_band {
            if measured
                != (MeasuredBandInput {
                    flux_336_650_ph_m2_s,
                    statistical_uncertainty_336_650_ph_m2_s,
                })
            {
                bail!("measured XP value does not match UV evaluation context");
            }
        }
        let uv_flux = correction
            .flux_300_336_ph_m2_s
            .context("cannot combine a rejected UV correction")?;
        let uv_statistical = correction
            .statistical_uncertainty_300_336_ph_m2_s
            .context("rejected UV correction has no statistical uncertainty")?;
        let uv_systematic = correction
            .systematic_uncertainty_300_336_ph_m2_s
            .context("rejected UV correction has no systematic uncertainty")?;
        let combined_variance = if let Some(covariance) =
            correction.measured_correction_statistical_covariance_ph2_m4_s2
        {
            statistical_uncertainty_336_650_ph_m2_s.powi(2)
                + uv_statistical.powi(2)
                + 2.0 * covariance
        } else {
            let measured_residual_correlation = self
                .artifact
                .uncertainty_model
                .measured_conditional_residual_statistical_correlation;
            statistical_uncertainty_336_650_ph_m2_s.powi(2)
                + uv_statistical.powi(2)
                + 2.0
                    * measured_residual_correlation
                    * statistical_uncertainty_336_650_ph_m2_s
                    * uv_statistical
        };
        if !combined_variance.is_finite() || combined_variance < -1.0e-12 {
            bail!("UV/XP statistical covariance produced invalid combined variance");
        }
        let combined_flux = flux_336_650_ph_m2_s + uv_flux;
        if !combined_flux.is_finite() {
            bail!("combined 300–650 nm flux is not finite");
        }
        Ok(CombinedBandFlux {
            flux_300_336_ph_m2_s: uv_flux,
            flux_336_650_ph_m2_s,
            flux_300_650_ph_m2_s: combined_flux,
            statistical_uncertainty_300_336_ph_m2_s: uv_statistical,
            statistical_uncertainty_336_650_ph_m2_s,
            statistical_uncertainty_300_650_ph_m2_s: combined_variance.max(0.0).sqrt(),
            systematic_uncertainty_300_336_ph_m2_s: uv_systematic,
            systematic_uncertainty_300_650_ph_m2_s: uv_systematic,
            applicability_status: correction.applicability_status,
            decision: correction.decision,
            model_id: correction.model_id.clone(),
            artifact_sha256: correction.artifact_sha256.clone(),
            systematic_correlation: self.artifact.uncertainty_model.systematic_correlation,
        })
    }

    fn rejected_evaluation(
        &self,
        measured_band: Option<MeasuredBandInput>,
    ) -> UvCorrectionEvaluation {
        UvCorrectionEvaluation {
            flux_300_336_ph_m2_s: None,
            statistical_uncertainty_300_336_ph_m2_s: None,
            systematic_uncertainty_300_336_ph_m2_s: None,
            applicability_status: ApplicabilityStatus::OutOfDomain,
            decision: EvaluationDecision::Rejected,
            model_id: self.artifact.model_id.clone(),
            artifact_sha256: self.artifact_sha256.clone(),
            response: self.artifact.response.clone(),
            measured_band,
            measured_correction_statistical_covariance_ph2_m4_s2: None,
        }
    }
}

/// Reference-dataset manifest consumed by reproducibility tooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceDatasetManifest {
    pub schema_version: u32,
    pub dataset: ReferenceDataset,
    pub source_table_file: String,
    pub source_table_sha256: String,
    pub source_id_column: String,
    pub sky_region_column: String,
}

impl ReferenceDatasetManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != REFERENCE_MANIFEST_SCHEMA_VERSION {
            bail!(
                "unsupported UV reference manifest schema_version {}",
                self.schema_version
            );
        }
        self.dataset.validate()?;
        require_safe_relative_path("reference source table file", &self.source_table_file)?;
        require_sha256("reference source table", &self.source_table_sha256)?;
        if !self.dataset.files.iter().any(|file| {
            file.name == self.source_table_file && file.sha256 == self.source_table_sha256
        }) {
            bail!("reference source table identity is not present in dataset files");
        }
        require_identifier("reference source ID column", &self.source_id_column)?;
        require_identifier("reference sky-region column", &self.sky_region_column)?;
        if self.source_id_column == self.sky_region_column {
            bail!("reference source-ID and sky-region columns must be distinct");
        }
        Ok(())
    }
}

/// One deterministic source/sky assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionAssignment {
    pub source_id: String,
    pub sky_region: String,
    pub partition: PartitionRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PartitionRole {
    Training,
    Validation,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionManifest {
    pub schema_version: u32,
    pub assignment_algorithm: String,
    pub seed: u64,
    pub assignments: Vec<PartitionAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartitionIndexEntry {
    sky_region: String,
    role: PartitionRole,
}

type PartitionIndex = BTreeMap<String, PartitionIndexEntry>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldoutMetrics {
    pub rows: u64,
    pub evaluated_rows: u64,
    pub mean_residual_ph_m2_s: Option<f64>,
    pub rmse_ph_m2_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvValidationReport {
    pub schema_version: u32,
    pub reference_dataset_name: String,
    pub reference_dataset_release: String,
    pub reference_manifest_sha256: String,
    pub partition_manifest_sha256: String,
    pub artifact_model_id: String,
    pub artifact_sha256: String,
    pub calibration_status: CalibrationStatus,
    pub response: ModelResponse,
    pub holdout_sha256: String,
    pub holdout_rows: u64,
    pub by_colour: BTreeMap<String, HoldoutMetrics>,
    pub by_magnitude: BTreeMap<String, HoldoutMetrics>,
    pub by_extinction_proxy: BTreeMap<String, HoldoutMetrics>,
    pub by_quality: BTreeMap<String, HoldoutMetrics>,
    pub by_sky_region: BTreeMap<String, HoldoutMetrics>,
    pub by_extrapolation_status: BTreeMap<String, HoldoutMetrics>,
}

#[derive(Debug, Clone)]
struct HoldoutObservation {
    expected_flux: f64,
    residual: Option<f64>,
    colour: String,
    magnitude: String,
    extinction_proxy: String,
    quality: String,
    sky_region: String,
    extrapolation_status: String,
}

#[derive(Debug, Default)]
struct MetricsAccumulator {
    rows: u64,
    evaluated_rows: u64,
    residual_sum: f64,
    squared_residual_sum: f64,
}

impl MetricsAccumulator {
    fn add(&mut self, residual: Option<f64>) -> Result<()> {
        self.rows = self.rows.checked_add(1).context("holdout row overflow")?;
        if let Some(residual) = residual {
            self.evaluated_rows = self
                .evaluated_rows
                .checked_add(1)
                .context("evaluated holdout row overflow")?;
            self.residual_sum += residual;
            self.squared_residual_sum += residual.powi(2);
            if !self.residual_sum.is_finite() || !self.squared_residual_sum.is_finite() {
                bail!("holdout residual accumulation overflow");
            }
        }
        Ok(())
    }

    fn finish(self) -> HoldoutMetrics {
        let count = self.evaluated_rows as f64;
        HoldoutMetrics {
            rows: self.rows,
            evaluated_rows: self.evaluated_rows,
            mean_residual_ph_m2_s: (self.evaluated_rows > 0).then_some(self.residual_sum / count),
            rmse_ph_m2_s: (self.evaluated_rows > 0)
                .then_some((self.squared_residual_sum / count).sqrt()),
        }
    }
}

/// Inputs for the deterministic `nsb-data starlight-uv validate` workflow.
#[derive(Debug, Clone)]
pub struct ReproducibilityInputs {
    pub reference_manifest: PathBuf,
    pub partition_manifest: PathBuf,
    pub artifact: PathBuf,
    pub artifact_sha256: String,
    pub holdout: PathBuf,
    pub materialize_partitions: Option<PathBuf>,
    pub output: PathBuf,
}

/// Validate all calibration inputs, evaluate the holdout, and write canonical JSON.
pub fn run_reproducibility_validation(
    inputs: &ReproducibilityInputs,
) -> Result<UvValidationReport> {
    let reference_bytes = fs::read(&inputs.reference_manifest).with_context(|| {
        format!(
            "read UV reference manifest {}",
            inputs.reference_manifest.display()
        )
    })?;
    let reference: ReferenceDatasetManifest = serde_json::from_slice(&reference_bytes)
        .with_context(|| {
            format!(
                "parse UV reference manifest {}",
                inputs.reference_manifest.display()
            )
        })?;
    reference.validate()?;
    let reference_root = inputs
        .reference_manifest
        .parent()
        .context("UV reference manifest has no parent directory")?;
    for file in &reference.dataset.files {
        let path = reference_root.join(&file.name);
        let actual = checksum_io::sha256_file(&path)
            .with_context(|| format!("verify UV reference file {}", path.display()))?;
        if actual != file.sha256 {
            bail!(
                "UV reference file checksum mismatch for {}: expected {}, actual {}",
                path.display(),
                file.sha256,
                actual
            );
        }
    }
    let reference_sources = load_reference_sources(reference_root, &reference)?;

    let partition_bytes = fs::read(&inputs.partition_manifest).with_context(|| {
        format!(
            "read UV partition manifest {}",
            inputs.partition_manifest.display()
        )
    })?;
    let partitions: PartitionManifest =
        serde_json::from_slice(&partition_bytes).with_context(|| {
            format!(
                "parse UV partition manifest {}",
                inputs.partition_manifest.display()
            )
        })?;
    let canonical_partitions = partitions.canonicalized()?;
    let partition_index = build_partition_index(&canonical_partitions);
    bind_reference_sources_to_partitions(&reference_sources, &partition_index)?;
    if let Some(path) = &inputs.materialize_partitions {
        crate::platform::artifact_store::atomic_write(
            path,
            &serde_json::to_vec_pretty(&canonical_partitions)?,
        )?;
    }

    let partition_sha256 = checksum_io::sha256_bytes(&partition_bytes);
    let correction = UvCorrection::load(&inputs.artifact, &inputs.artifact_sha256)?;
    if correction.artifact.reference_dataset != reference.dataset {
        bail!("UV artifact reference dataset does not match reference manifest");
    }
    if correction.artifact.partitions.manifest_sha256 != partition_sha256 {
        bail!("UV artifact partition checksum does not match partition manifest");
    }
    validate_partition_evidence(
        &canonical_partitions,
        &correction.artifact.partitions,
        &partition_sha256,
    )?;

    let holdout_bytes = fs::read(&inputs.holdout)
        .with_context(|| format!("read UV holdout {}", inputs.holdout.display()))?;
    let observations = evaluate_holdout(&holdout_bytes, &correction, &partition_index)?;
    let report = UvValidationReport {
        schema_version: VALIDATION_REPORT_SCHEMA_VERSION,
        reference_dataset_name: reference.dataset.name,
        reference_dataset_release: reference.dataset.release,
        reference_manifest_sha256: checksum_io::sha256_bytes(&reference_bytes),
        partition_manifest_sha256: partition_sha256,
        artifact_model_id: correction.artifact.model_id.clone(),
        artifact_sha256: correction.artifact_sha256.clone(),
        calibration_status: correction.artifact.calibration_status,
        response: correction.artifact.response.clone(),
        holdout_sha256: checksum_io::sha256_bytes(&holdout_bytes),
        holdout_rows: u64::try_from(observations.len()).context("holdout row count overflow")?,
        by_colour: aggregate_holdout(&observations, |row| &row.colour)?,
        by_magnitude: aggregate_holdout(&observations, |row| &row.magnitude)?,
        by_extinction_proxy: aggregate_holdout(&observations, |row| &row.extinction_proxy)?,
        by_quality: aggregate_holdout(&observations, |row| &row.quality)?,
        by_sky_region: aggregate_holdout(&observations, |row| &row.sky_region)?,
        by_extrapolation_status: aggregate_holdout(&observations, |row| &row.extrapolation_status)?,
    };
    crate::platform::artifact_store::atomic_write(
        &inputs.output,
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

impl PartitionManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PARTITION_MANIFEST_SCHEMA_VERSION {
            bail!(
                "unsupported UV partition manifest schema_version {}",
                self.schema_version
            );
        }
        require_text("partition assignment algorithm", &self.assignment_algorithm)?;
        if self.assignments.is_empty() {
            bail!("UV partition manifest contains no assignments");
        }
        let mut sources = BTreeSet::new();
        let mut sky_to_role = BTreeMap::new();
        let mut roles = BTreeSet::new();
        for assignment in &self.assignments {
            require_text("partition source_id", &assignment.source_id)?;
            require_text("partition sky_region", &assignment.sky_region)?;
            if !sources.insert(assignment.source_id.as_str()) {
                bail!(
                    "UV partition manifest repeats source {}",
                    assignment.source_id
                );
            }
            if sky_to_role
                .insert(assignment.sky_region.as_str(), assignment.partition)
                .is_some_and(|role| role != assignment.partition)
            {
                bail!(
                    "UV partition sky region {} appears in multiple roles",
                    assignment.sky_region
                );
            }
            roles.insert(assignment.partition);
        }
        if roles
            != BTreeSet::from([
                PartitionRole::Training,
                PartitionRole::Validation,
                PartitionRole::Test,
            ])
        {
            bail!("UV partition manifest requires non-empty training, validation, and test roles");
        }
        Ok(())
    }

    /// Canonical ordering used for deterministic validation and materialization.
    pub fn canonicalized(&self) -> Result<Self> {
        self.validate()?;
        let mut result = self.clone();
        result.assignments.sort_by(|left, right| {
            left.partition
                .cmp(&right.partition)
                .then_with(|| left.sky_region.cmp(&right.sky_region))
                .then_with(|| left.source_id.cmp(&right.source_id))
        });
        Ok(result)
    }
}

fn load_reference_sources(
    reference_root: &Path,
    manifest: &ReferenceDatasetManifest,
) -> Result<BTreeMap<String, String>> {
    let path = reference_root.join(&manifest.source_table_file);
    let bytes = fs::read(&path)
        .with_context(|| format!("read UV reference source table {}", path.display()))?;
    let actual = checksum_io::sha256_bytes(&bytes);
    if actual != manifest.source_table_sha256 {
        bail!(
            "UV reference source table checksum mismatch for {}: expected {}, actual {}",
            path.display(),
            manifest.source_table_sha256,
            actual
        );
    }
    let mut reader = csv::ReaderBuilder::new().from_reader(bytes.as_slice());
    let headers = reader
        .headers()
        .with_context(|| format!("read UV reference source table headers {}", path.display()))?
        .clone();
    let duplicate_headers = headers
        .iter()
        .filter(|header| {
            headers
                .iter()
                .filter(|candidate| *candidate == *header)
                .count()
                > 1
        })
        .collect::<BTreeSet<_>>();
    if !duplicate_headers.is_empty() {
        bail!(
            "UV reference source table has duplicate columns {:?}",
            duplicate_headers
        );
    }
    let source_index = headers
        .iter()
        .position(|header| header == manifest.source_id_column)
        .with_context(|| {
            format!(
                "UV reference source table has no configured source-ID column {}",
                manifest.source_id_column
            )
        })?;
    let sky_index = headers
        .iter()
        .position(|header| header == manifest.sky_region_column)
        .with_context(|| {
            format!(
                "UV reference source table has no configured sky-region column {}",
                manifest.sky_region_column
            )
        })?;
    let mut sources = BTreeMap::new();
    for (row_index, record) in reader.records().enumerate() {
        let row = row_index + 2;
        let record = record.with_context(|| {
            format!(
                "read UV reference source table row {row} in {}",
                path.display()
            )
        })?;
        let source_id = record.get(source_index).with_context(|| {
            format!("UV reference source table row {row} has no source-ID field")
        })?;
        let sky_region = record.get(sky_index).with_context(|| {
            format!("UV reference source table row {row} has no sky-region field")
        })?;
        require_text("reference source ID", source_id)
            .with_context(|| format!("invalid UV reference source table row {row}"))?;
        require_text("reference sky region", sky_region)
            .with_context(|| format!("invalid UV reference source table row {row}"))?;
        if sources
            .insert(source_id.to_string(), sky_region.to_string())
            .is_some()
        {
            bail!("UV reference source table repeats source ID {source_id}");
        }
    }
    if sources.is_empty() {
        bail!("UV reference source table contains no sources");
    }
    Ok(sources)
}

fn build_partition_index(manifest: &PartitionManifest) -> PartitionIndex {
    manifest
        .assignments
        .iter()
        .map(|assignment| {
            (
                assignment.source_id.clone(),
                PartitionIndexEntry {
                    sky_region: assignment.sky_region.clone(),
                    role: assignment.partition,
                },
            )
        })
        .collect()
}

fn bind_reference_sources_to_partitions(
    sources: &BTreeMap<String, String>,
    partitions: &PartitionIndex,
) -> Result<()> {
    let source_ids = sources.keys().cloned().collect::<BTreeSet<_>>();
    let partition_ids = partitions.keys().cloned().collect::<BTreeSet<_>>();
    if source_ids != partition_ids {
        let unpartitioned = source_ids
            .difference(&partition_ids)
            .cloned()
            .collect::<Vec<_>>();
        let absent_from_reference = partition_ids
            .difference(&source_ids)
            .cloned()
            .collect::<Vec<_>>();
        bail!(
            "UV reference/partition source coverage mismatch: unpartitioned reference sources {:?}, partition sources absent from reference {:?}",
            unpartitioned,
            absent_from_reference
        );
    }
    for (source_id, sky_region) in sources {
        let partition_sky = &partitions[source_id].sky_region;
        if sky_region != partition_sky {
            bail!(
                "UV reference/partition sky mismatch for source {source_id}: reference {sky_region}, partition {partition_sky}"
            );
        }
    }
    Ok(())
}

fn evaluate_holdout(
    bytes: &[u8],
    correction: &UvCorrection,
    partitions: &PartitionIndex,
) -> Result<Vec<HoldoutObservation>> {
    let mut reader = csv::ReaderBuilder::new().from_reader(bytes);
    let headers = reader.headers()?.clone();
    let required = [
        "source_id",
        "expected_flux_300_336_ph_m2_s",
        "measured_flux_336_650_ph_m2_s",
        "measured_statistical_uncertainty_336_650_ph_m2_s",
        "colour",
        "magnitude",
        "extinction_proxy",
        "quality",
        "sky_region",
    ];
    let indexes = required
        .iter()
        .map(|name| {
            headers
                .iter()
                .position(|header| header == *name)
                .with_context(|| format!("UV holdout has no {name} column"))
        })
        .collect::<Result<Vec<_>>>()?;
    let predictor_indexes = correction
        .artifact
        .predictors
        .iter()
        .map(|predictor| {
            headers
                .iter()
                .position(|header| header == predictor.name)
                .with_context(|| format!("UV holdout has no predictor column {}", predictor.name))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut source_ids = BTreeSet::new();
    let mut observations = Vec::new();
    for (row_index, record) in reader.records().enumerate() {
        let record = record.with_context(|| format!("read UV holdout row {}", row_index + 2))?;
        let source_id = required_field(&record, indexes[0], "source_id")?;
        require_text("holdout source_id", source_id)?;
        if !source_ids.insert(source_id.to_string()) {
            bail!("UV holdout repeats source_id {source_id}");
        }
        let assignment = partitions
            .get(source_id)
            .with_context(|| format!("UV holdout source {source_id} is not partitioned"))?;
        if assignment.role != PartitionRole::Test {
            bail!(
                "UV holdout source {source_id} belongs to {:?}, not the test partition",
                assignment.role
            );
        }
        let expected_flux = parse_nonnegative(
            required_field(&record, indexes[1], "expected_flux_300_336_ph_m2_s")?,
            "expected UV holdout flux",
        )?;
        let measured_band = MeasuredBandInput {
            flux_336_650_ph_m2_s: parse_nonnegative(
                required_field(&record, indexes[2], "measured_flux_336_650_ph_m2_s")?,
                "measured holdout flux",
            )?,
            statistical_uncertainty_336_650_ph_m2_s: parse_nonnegative(
                required_field(
                    &record,
                    indexes[3],
                    "measured_statistical_uncertainty_336_650_ph_m2_s",
                )?,
                "measured holdout statistical uncertainty",
            )?,
        };
        let mut values = BTreeMap::new();
        for (predictor, index) in correction
            .artifact
            .predictors
            .iter()
            .zip(&predictor_indexes)
        {
            let raw = required_field(&record, *index, &predictor.name)?;
            let value = raw.parse::<f64>().with_context(|| {
                format!(
                    "UV holdout predictor {} is not a number at row {}",
                    predictor.name,
                    row_index + 2
                )
            })?;
            values.insert(predictor.name.clone(), value);
        }
        let sky_region = required_field(&record, indexes[8], "sky_region")?;
        if sky_region != assignment.sky_region {
            bail!(
                "UV holdout source {source_id} has sky region {sky_region}, expected {}",
                assignment.sky_region
            );
        }
        let evaluation = correction.evaluate(UvEvaluationInput {
            predictors: &values,
            measured_band: Some(measured_band),
        })?;
        let residual = evaluation
            .flux_300_336_ph_m2_s
            .map(|prediction| prediction - expected_flux);
        let status = match evaluation.applicability_status {
            ApplicabilityStatus::InDomain => "in-domain",
            ApplicabilityStatus::Boundary => "boundary",
            ApplicabilityStatus::OutOfDomain => "out-of-domain",
        };
        let strata = [
            required_field(&record, indexes[4], "colour")?,
            required_field(&record, indexes[5], "magnitude")?,
            required_field(&record, indexes[6], "extinction_proxy")?,
            required_field(&record, indexes[7], "quality")?,
            sky_region,
        ];
        for value in strata {
            require_text("holdout stratum", value)?;
        }
        observations.push(HoldoutObservation {
            expected_flux,
            residual,
            colour: strata[0].to_string(),
            magnitude: strata[1].to_string(),
            extinction_proxy: strata[2].to_string(),
            quality: strata[3].to_string(),
            sky_region: strata[4].to_string(),
            extrapolation_status: status.to_string(),
        });
    }
    if observations.is_empty() {
        bail!("UV holdout contains no rows");
    }
    let expected_test_sources = partitions
        .iter()
        .filter(|(_, assignment)| assignment.role == PartitionRole::Test)
        .map(|(source_id, _)| source_id.clone())
        .collect::<BTreeSet<_>>();
    if source_ids != expected_test_sources {
        let missing = expected_test_sources
            .difference(&source_ids)
            .cloned()
            .collect::<Vec<_>>();
        let additional = source_ids
            .difference(&expected_test_sources)
            .cloned()
            .collect::<Vec<_>>();
        bail!(
            "UV holdout source set does not equal the test partition: missing {:?}, additional {:?}",
            missing,
            additional
        );
    }
    if observations
        .iter()
        .any(|observation| !observation.expected_flux.is_finite())
    {
        bail!("UV holdout expected flux is not finite");
    }
    Ok(observations)
}

fn validate_partition_evidence(
    manifest: &PartitionManifest,
    evidence: &PartitionEvidence,
    manifest_sha256: &str,
) -> Result<()> {
    if evidence.assignment_algorithm != manifest.assignment_algorithm
        || evidence.seed != manifest.seed
        || evidence.disjointness_evidence_sha256 != manifest_sha256
    {
        bail!("UV artifact partition evidence does not match partition manifest");
    }
    for (role, summary) in [
        (PartitionRole::Training, &evidence.training),
        (PartitionRole::Validation, &evidence.validation),
        (PartitionRole::Test, &evidence.test),
    ] {
        let mut source_ids = manifest
            .assignments
            .iter()
            .filter(|assignment| assignment.partition == role)
            .map(|assignment| assignment.source_id.as_str())
            .collect::<Vec<_>>();
        source_ids.sort_unstable();
        let source_bytes = source_ids
            .iter()
            .map(|source_id| format!("{source_id}\n"))
            .collect::<String>();
        let sky_regions = manifest
            .assignments
            .iter()
            .filter(|assignment| assignment.partition == role)
            .map(|assignment| assignment.sky_region.clone())
            .collect::<BTreeSet<_>>();
        if summary.source_count != source_ids.len() as u64
            || summary.source_ids_sha256 != checksum_io::sha256_bytes(source_bytes.as_bytes())
            || summary.sky_regions.iter().cloned().collect::<BTreeSet<_>>() != sky_regions
        {
            bail!("UV artifact partition summary does not match manifest assignments");
        }
    }
    Ok(())
}

fn aggregate_holdout<F>(
    observations: &[HoldoutObservation],
    key: F,
) -> Result<BTreeMap<String, HoldoutMetrics>>
where
    F: Fn(&HoldoutObservation) -> &str,
{
    let mut accumulators: BTreeMap<String, MetricsAccumulator> = BTreeMap::new();
    for observation in observations {
        accumulators
            .entry(key(observation).to_string())
            .or_default()
            .add(observation.residual)?;
    }
    Ok(accumulators
        .into_iter()
        .map(|(key, accumulator)| (key, accumulator.finish()))
        .collect())
}

fn required_field<'a>(record: &'a csv::StringRecord, index: usize, label: &str) -> Result<&'a str> {
    record
        .get(index)
        .with_context(|| format!("UV holdout row has no {label} field"))
}

fn parse_nonnegative(raw: &str, label: &str) -> Result<f64> {
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("{label} is not a number"))?;
    if !value.is_finite() || value < 0.0 {
        bail!("{label} must be finite and non-negative");
    }
    Ok(value)
}

fn validate_measured_band(measured: MeasuredBandInput) -> Result<()> {
    if !measured.flux_336_650_ph_m2_s.is_finite()
        || measured.flux_336_650_ph_m2_s < 0.0
        || !measured.statistical_uncertainty_336_650_ph_m2_s.is_finite()
        || measured.statistical_uncertainty_336_650_ph_m2_s < 0.0
    {
        bail!("measured 336–650 nm flux and uncertainty must be finite and non-negative");
    }
    Ok(())
}

fn transform(transformation: &PredictorTransformation, value: f64) -> Result<f64> {
    let transformed = match transformation {
        PredictorTransformation::Identity => value,
        PredictorTransformation::Log10 => value.log10(),
        PredictorTransformation::Standardize { mean, scale } => (value - mean) / scale,
    };
    if !transformed.is_finite() {
        bail!("UV predictor transformation produced a non-finite value");
    }
    Ok(transformed)
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn quadratic_form(matrix: &[Vec<f64>], vector: &[f64]) -> Result<f64> {
    let mut value = 0.0;
    for (row, left) in matrix.iter().zip(vector) {
        value += left * dot(row, vector);
    }
    let tolerance = 1.0e-12 * matrix.len() as f64;
    if !value.is_finite() || value < -tolerance {
        bail!("UV covariance produced a negative or non-finite variance");
    }
    Ok(value.max(0.0))
}

fn validate_positive_semidefinite(matrix: &[Vec<f64>]) -> Result<()> {
    let dimension = matrix.len();
    let scale = matrix
        .iter()
        .flatten()
        .fold(1.0_f64, |maximum, value| maximum.max(value.abs()));
    let tolerance = scale * dimension as f64 * 1.0e-12;
    let mut lower = vec![vec![0.0; dimension]; dimension];
    for row in 0..dimension {
        for column in 0..=row {
            let remainder = matrix[row][column]
                - (0..column)
                    .map(|index| lower[row][index] * lower[column][index])
                    .sum::<f64>();
            if row == column {
                if remainder < -tolerance {
                    bail!("UV model covariance is not positive semidefinite");
                }
                lower[row][column] = remainder.max(0.0).sqrt();
            } else if lower[column][column] > tolerance.sqrt() {
                lower[row][column] = remainder / lower[column][column];
            } else if remainder.abs() > tolerance {
                bail!("UV model covariance is not positive semidefinite");
            }
        }
    }
    Ok(())
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["placeholder", "todo", "tbd", "unknown", "unspecified"]
            .iter()
            .any(|marker| normalized == *marker || normalized.contains(&format!("<{marker}>")))
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

fn require_identifier(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("{label} must be an ASCII identifier");
    }
    Ok(())
}

fn require_safe_relative_path(label: &str, value: &str) -> Result<()> {
    require_text(label, value)?;
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("{label} must be a safe relative path");
    }
    Ok(())
}

fn require_nonempty_texts(label: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        bail!("{label} must not be empty");
    }
    for value in values {
        require_text(label, value)?;
    }
    Ok(())
}

fn require_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} SHA-256 must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}
