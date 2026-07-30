//! Versioned, fail-closed contract for non-XP photometric 336–650 nm inference.
//!
//! This module defines ingestion, source routing, and linear evaluation only.
//! The repository intentionally contains no production photometric coefficients.

use crate::platform::checksum_io;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::uv::{
    ApplicabilityStatus, CalibrationStatus, EvaluationDecision, OutOfDomainPolicy,
    PartitionEvidence, PHOTON_FLUX_UNIT,
};

pub const PHOTOMETRIC_ARTIFACT_SCHEMA_VERSION: u32 = 1;
pub const MEASURED_BAND_NM: [u16; 2] = [336, 650];

/// Ordered photometric fallback branch identity embedded in the artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhotometricBranchId {
    PhotometricGBpRp,
    PhotometricPartial,
    PhotometricGOnly,
}

/// Normative population accounting branch for every Gaia row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopulationBranch {
    XpContinuous,
    PhotometricGBpRp,
    PhotometricPartial,
    PhotometricGOnly,
    NoUsablePhotometry,
    ScientificExclusion,
}

impl From<PhotometricBranchId> for PopulationBranch {
    fn from(value: PhotometricBranchId) -> Self {
        match value {
            PhotometricBranchId::PhotometricGBpRp => Self::PhotometricGBpRp,
            PhotometricBranchId::PhotometricPartial => Self::PhotometricPartial,
            PhotometricBranchId::PhotometricGOnly => Self::PhotometricGOnly,
        }
    }
}

/// One calibrated linear branch with explicit feature support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricBranchModel {
    pub branch: PhotometricBranchId,
    pub required_features: Vec<String>,
    pub predictors: Vec<PhotometricPredictor>,
    pub parameters: Vec<f64>,
    pub covariance: Vec<Vec<f64>>,
    pub statistical_floor_ph_m2_s: f64,
    pub systematic_floor_ph_m2_s: f64,
    pub systematic_fraction: f64,
}

/// Named predictor with a registered applicability domain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricPredictor {
    pub name: String,
    pub unit: String,
    pub domain: PredictorDomain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PredictorDomain {
    pub minimum: f64,
    pub maximum: f64,
}

/// Physical quantity predicted by the photometric linear score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PhotometricResponse {
    /// Linear score is the 336–650 nm photon flux directly.
    AbsolutePhotonFlux,
    /// Linear score is ln(336–650 nm photon flux); evaluation exponentiates.
    NaturalLogPhotonFlux,
}

/// Complete immutable photometric-inference artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub calibration_status: CalibrationStatus,
    /// Ordered fallback list: full colour, partial colour, then G-only.
    pub branches: Vec<PhotometricBranchModel>,
    pub flux_unit: String,
    pub measured_band_nm: [u16; 2],
    #[serde(default = "default_photometric_response")]
    pub response: PhotometricResponse,
    pub partitions: PartitionEvidence,
    pub out_of_domain_policy: OutOfDomainPolicy,
    pub training_command: String,
    pub software_version: String,
}

fn default_photometric_response() -> PhotometricResponse {
    PhotometricResponse::AbsolutePhotonFlux
}

/// Observed Gaia photometric inputs for one source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotometricFeatures {
    pub phot_g_mean_mag: Option<f64>,
    pub phot_bp_mean_mag: Option<f64>,
    pub phot_rp_mean_mag: Option<f64>,
    pub bp_rp: Option<f64>,
    /// When false the source is scientifically excluded rather than inferred.
    pub quality_flag: bool,
}

/// Point estimate produced by a photometric branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometricFluxEstimate {
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
    pub systematic_uncertainty_336_650_ph_m2_s: f64,
    pub applicability_status: ApplicabilityStatus,
    pub decision: EvaluationDecision,
    pub branch: PhotometricBranchId,
    pub model_id: String,
    pub artifact_sha256: String,
}

/// Result of routing a source and optionally evaluating its photometric branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDecision {
    pub branch: PopulationBranch,
    pub flux: Option<PhotometricFluxEstimate>,
}

/// Validated artifact paired with the digest of its exact serialized bytes.
#[derive(Debug, Clone)]
pub struct PhotometricCorrection {
    artifact: PhotometricArtifact,
    artifact_sha256: String,
}

impl PhotometricArtifact {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PHOTOMETRIC_ARTIFACT_SCHEMA_VERSION {
            bail!(
                "unsupported photometric artifact schema_version {}",
                self.schema_version
            );
        }
        require_text("model_id", &self.model_id)?;
        if self.flux_unit != PHOTON_FLUX_UNIT {
            bail!("photometric flux unit must be {PHOTON_FLUX_UNIT}");
        }
        if self.measured_band_nm != MEASURED_BAND_NM {
            bail!("photometric measured band must be exactly 336–650 nm");
        }
        self.partitions.validate()?;
        match self.out_of_domain_policy {
            OutOfDomainPolicy::Reject => {}
            OutOfDomainPolicy::ClampWithSystematicInflation { factor }
                if factor.is_finite() && factor >= 1.0 => {}
            OutOfDomainPolicy::ClampWithSystematicInflation { .. } => {
                bail!("out-of-domain systematic inflation must be finite and >= 1")
            }
        }
        require_text("training_command", &self.training_command)?;
        require_text("software_version", &self.software_version)?;
        self.validate_branches()?;
        Ok(())
    }

    fn validate_branches(&self) -> Result<()> {
        let expected = [
            PhotometricBranchId::PhotometricGBpRp,
            PhotometricBranchId::PhotometricPartial,
            PhotometricBranchId::PhotometricGOnly,
        ];
        if self.branches.len() != expected.len() {
            bail!(
                "photometric artifact requires exactly {} ordered branches",
                expected.len()
            );
        }
        let mut seen = BTreeSet::new();
        for (index, branch) in self.branches.iter().enumerate() {
            if branch.branch != expected[index] {
                bail!(
                    "photometric branches must be ordered as photometric_g_bp_rp, photometric_partial, photometric_g_only"
                );
            }
            if !seen.insert(branch.branch) {
                bail!("photometric artifact contains a duplicate branch");
            }
            branch.validate()?;
        }
        Ok(())
    }
}

impl PhotometricBranchModel {
    fn validate(&self) -> Result<()> {
        if self.required_features.is_empty() {
            bail!(
                "{:?} branch requires at least one feature name",
                self.branch
            );
        }
        let mut required = BTreeSet::new();
        for name in &self.required_features {
            require_identifier("required feature", name)?;
            if !required.insert(name.as_str()) {
                bail!(
                    "{:?} branch has duplicate required feature {}",
                    self.branch,
                    name
                );
            }
        }
        if self.predictors.is_empty() {
            bail!("{:?} branch requires at least one predictor", self.branch);
        }
        let mut predictor_names = BTreeSet::new();
        for predictor in &self.predictors {
            require_identifier("predictor name", &predictor.name)?;
            require_text("predictor unit", &predictor.unit)?;
            if !predictor.domain.minimum.is_finite()
                || !predictor.domain.maximum.is_finite()
                || predictor.domain.minimum >= predictor.domain.maximum
            {
                bail!("predictor {} has an invalid domain", predictor.name);
            }
            if !predictor_names.insert(predictor.name.as_str()) {
                bail!(
                    "{:?} branch has duplicate predictor {}",
                    self.branch,
                    predictor.name
                );
            }
            if !required.contains(predictor.name.as_str()) {
                bail!(
                    "predictor {} is not listed in required_features for {:?}",
                    predictor.name,
                    self.branch
                );
            }
        }
        for name in &required {
            if !predictor_names.contains(name) {
                bail!(
                    "required feature {name} has no predictor domain for {:?}",
                    self.branch
                );
            }
        }
        let dimension = self.predictors.len() + 1;
        if self.parameters.len() != dimension
            || self.parameters.iter().any(|value| !value.is_finite())
        {
            bail!(
                "{:?} linear model requires {dimension} finite parameters",
                self.branch
            );
        }
        if self.covariance.len() != dimension
            || self.covariance.iter().any(|row| row.len() != dimension)
        {
            bail!(
                "{:?} covariance dimensions do not match parameters",
                self.branch
            );
        }
        for row in &self.covariance {
            if row.iter().any(|value| !value.is_finite()) {
                bail!("{:?} covariance contains non-finite values", self.branch);
            }
        }
        for (index, row) in self.covariance.iter().enumerate() {
            if row[index] < 0.0 {
                bail!(
                    "{:?} covariance has negative diagonal variance",
                    self.branch
                );
            }
            for (other, other_row) in self.covariance.iter().enumerate() {
                let scale = row[other].abs().max(other_row[index].abs()).max(1.0);
                if (row[other] - other_row[index]).abs() > 1.0e-12 * scale {
                    bail!("{:?} covariance must be symmetric", self.branch);
                }
            }
        }
        validate_positive_semidefinite(&self.covariance)?;
        for (label, value) in [
            ("statistical floor", self.statistical_floor_ph_m2_s),
            ("systematic floor", self.systematic_floor_ph_m2_s),
            ("systematic fraction", self.systematic_fraction),
        ] {
            if !value.is_finite() || value < 0.0 {
                bail!("{:?} {label} must be finite and non-negative", self.branch);
            }
        }
        Ok(())
    }
}

impl PhotometricCorrection {
    /// Load exact artifact bytes, verify their pinned digest, and validate them.
    pub fn load(path: &Path, pinned_sha256: &str) -> Result<Self> {
        require_sha256("configured photometric artifact", pinned_sha256)?;
        let bytes = fs::read(path)
            .with_context(|| format!("read photometric artifact {}", path.display()))?;
        let actual = checksum_io::sha256_bytes(&bytes);
        if actual != pinned_sha256 {
            bail!(
                "photometric artifact checksum mismatch for {}: expected {}, actual {}",
                path.display(),
                pinned_sha256,
                actual
            );
        }
        let artifact: PhotometricArtifact = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse photometric artifact {}", path.display()))?;
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
                "photometric artifact {} has status {:?}, not validated",
                self.artifact.model_id,
                self.artifact.calibration_status
            );
        }
        Ok(())
    }

    pub fn artifact(&self) -> &PhotometricArtifact {
        &self.artifact
    }

    pub fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    /// Route a source into exactly one population branch and evaluate when possible.
    pub fn route_and_evaluate(&self, features: PhotometricFeatures) -> Result<RouteDecision> {
        if !features.quality_flag {
            return Ok(RouteDecision {
                branch: PopulationBranch::ScientificExclusion,
                flux: None,
            });
        }

        let feature_map = feature_map(features);
        let Some(start) = select_branch_index(&feature_map) else {
            return Ok(RouteDecision {
                branch: PopulationBranch::NoUsablePhotometry,
                flux: None,
            });
        };

        // Walk the ordered fallback list from the selected tier downward until a
        // branch has every required feature present.
        for model in &self.artifact.branches[start..] {
            if !model
                .required_features
                .iter()
                .all(|name| feature_map.contains_key(name.as_str()))
            {
                continue;
            }
            return match self.evaluate_branch(model, &feature_map)? {
                Some(flux) => Ok(RouteDecision {
                    branch: model.branch.into(),
                    flux: Some(flux),
                }),
                None => Ok(RouteDecision {
                    branch: model.branch.into(),
                    flux: None,
                }),
            };
        }

        Ok(RouteDecision {
            branch: PopulationBranch::NoUsablePhotometry,
            flux: None,
        })
    }

    fn evaluate_branch(
        &self,
        model: &PhotometricBranchModel,
        features: &BTreeMap<&str, f64>,
    ) -> Result<Option<PhotometricFluxEstimate>> {
        let mut status = ApplicabilityStatus::InDomain;
        let mut design = Vec::with_capacity(model.predictors.len() + 1);
        design.push(1.0);
        for predictor in &model.predictors {
            let original = *features.get(predictor.name.as_str()).with_context(|| {
                format!(
                    "missing photometric predictor {} for {:?}",
                    predictor.name, model.branch
                )
            })?;
            if !original.is_finite() {
                bail!("photometric predictor {} is not finite", predictor.name);
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
                    OutOfDomainPolicy::Reject => return Ok(None),
                    OutOfDomainPolicy::ClampWithSystematicInflation { .. } => {
                        original.clamp(predictor.domain.minimum, predictor.domain.maximum)
                    }
                }
            } else {
                original
            };
            design.push(evaluated);
        }

        let score = dot(&model.parameters, &design);
        if !score.is_finite() {
            bail!("{:?} model produced a non-finite score", model.branch);
        }
        let flux = match self.artifact.response {
            PhotometricResponse::AbsolutePhotonFlux => score,
            PhotometricResponse::NaturalLogPhotonFlux => score.exp(),
        };
        if !flux.is_finite() || flux <= 0.0 {
            bail!("{:?} model produced a non-positive flux", model.branch);
        }

        // Parameter variances come from the covariance diagonal; design weights
        // propagate them, then floors close the uncertainty budget.
        let mut variance = 0.0;
        for (index, weight) in design.iter().enumerate() {
            let diagonal = model.covariance[index][index];
            variance += diagonal * weight * weight;
        }
        if !variance.is_finite() || variance < 0.0 {
            bail!(
                "{:?} covariance diagonal produced invalid variance",
                model.branch
            );
        }
        let score_sigma = variance.sqrt();
        let statistical = match self.artifact.response {
            PhotometricResponse::AbsolutePhotonFlux => {
                score_sigma.hypot(model.statistical_floor_ph_m2_s)
            }
            PhotometricResponse::NaturalLogPhotonFlux => {
                (flux * score_sigma).hypot(model.statistical_floor_ph_m2_s)
            }
        };
        let mut systematic = model
            .systematic_floor_ph_m2_s
            .hypot(flux * model.systematic_fraction);
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
            bail!("{:?} model produced non-finite uncertainty", model.branch);
        }
        Ok(Some(PhotometricFluxEstimate {
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_336_650_ph_m2_s: statistical,
            systematic_uncertainty_336_650_ph_m2_s: systematic,
            applicability_status: status,
            decision,
            branch: model.branch,
            model_id: self.artifact.model_id.clone(),
            artifact_sha256: self.artifact_sha256.clone(),
        }))
    }
}

fn feature_map(features: PhotometricFeatures) -> BTreeMap<&'static str, f64> {
    let mut map = BTreeMap::new();
    if let Some(value) = features.phot_g_mean_mag.filter(|value| value.is_finite()) {
        map.insert("phot_g_mean_mag", value);
    }
    if let Some(value) = features.phot_bp_mean_mag.filter(|value| value.is_finite()) {
        map.insert("phot_bp_mean_mag", value);
    }
    if let Some(value) = features.phot_rp_mean_mag.filter(|value| value.is_finite()) {
        map.insert("phot_rp_mean_mag", value);
    }
    let colour = features
        .bp_rp
        .filter(|value| value.is_finite())
        .or_else(
            || match (features.phot_bp_mean_mag, features.phot_rp_mean_mag) {
                (Some(bp), Some(rp)) if bp.is_finite() && rp.is_finite() => Some(bp - rp),
                _ => None,
            },
        );
    if let Some(value) = colour {
        map.insert("bp_rp", value);
    }
    map
}

fn select_branch_index(features: &BTreeMap<&str, f64>) -> Option<usize> {
    let g = features.contains_key("phot_g_mean_mag");
    let bp = features.contains_key("phot_bp_mean_mag");
    let rp = features.contains_key("phot_rp_mean_mag");
    let colour = features.contains_key("bp_rp");
    if g && bp && rp {
        Some(0)
    } else if g && (colour || bp || rp) {
        Some(1)
    } else if g {
        Some(2)
    } else {
        None
    }
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
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
                    bail!("photometric model covariance is not positive semidefinite");
                }
                lower[row][column] = remainder.max(0.0).sqrt();
            } else if lower[column][column] > tolerance.sqrt() {
                lower[row][column] = remainder / lower[column][column];
            } else if remainder.abs() > tolerance {
                bail!("photometric model covariance is not positive semidefinite");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use tempfile::TempDir;

    fn partition_summary(
        id: &str,
        count: u64,
        sha: &str,
        sky: &str,
    ) -> super::super::uv::PartitionSummary {
        super::super::uv::PartitionSummary {
            partition_id: id.to_string(),
            source_count: count,
            source_ids_sha256: sha.repeat(32),
            sky_regions: vec![sky.to_string()],
        }
    }

    fn sample_partitions() -> PartitionEvidence {
        PartitionEvidence {
            assignment_algorithm: "synthetic-photometric-test-v1".to_string(),
            seed: 1,
            manifest_sha256: "aa".repeat(32),
            training: partition_summary("training", 10, "bb", "train-sky"),
            validation: partition_summary("validation", 5, "cc", "validation-sky"),
            test: partition_summary("test", 5, "dd", "test-sky"),
            source_disjoint: true,
            sky_disjoint: true,
            disjointness_evidence_sha256: "ee".repeat(32),
        }
    }

    fn branch(
        id: PhotometricBranchId,
        features: &[&str],
        parameters: Vec<f64>,
        covariance: Vec<Vec<f64>>,
        domains: &[(f64, f64)],
    ) -> PhotometricBranchModel {
        PhotometricBranchModel {
            branch: id,
            required_features: features.iter().map(|name| (*name).to_string()).collect(),
            predictors: features
                .iter()
                .zip(domains)
                .map(|(name, (minimum, maximum))| PhotometricPredictor {
                    name: (*name).to_string(),
                    unit: "mag".to_string(),
                    domain: PredictorDomain {
                        minimum: *minimum,
                        maximum: *maximum,
                    },
                })
                .collect(),
            parameters,
            covariance,
            statistical_floor_ph_m2_s: 0.5,
            systematic_floor_ph_m2_s: 1.0,
            systematic_fraction: 0.1,
        }
    }

    fn sample_artifact() -> PhotometricArtifact {
        PhotometricArtifact {
            schema_version: 1,
            model_id: "SYNTHETIC-NON-PRODUCTION-PHOTOMETRIC-V1".to_string(),
            calibration_status: CalibrationStatus::TestOnly,
            branches: vec![
                branch(
                    PhotometricBranchId::PhotometricGBpRp,
                    &["phot_g_mean_mag", "bp_rp"],
                    vec![100.0, -5.0, 2.0],
                    vec![
                        vec![1.0, 0.0, 0.0],
                        vec![0.0, 0.25, 0.0],
                        vec![0.0, 0.0, 0.04],
                    ],
                    &[(5.0, 20.0), (-1.0, 4.0)],
                ),
                branch(
                    PhotometricBranchId::PhotometricPartial,
                    &["phot_g_mean_mag", "bp_rp"],
                    vec![90.0, -4.0, 1.5],
                    vec![
                        vec![4.0, 0.0, 0.0],
                        vec![0.0, 1.0, 0.0],
                        vec![0.0, 0.0, 0.25],
                    ],
                    &[(5.0, 20.0), (-1.0, 4.0)],
                ),
                branch(
                    PhotometricBranchId::PhotometricGOnly,
                    &["phot_g_mean_mag"],
                    vec![80.0, -3.0],
                    vec![vec![9.0, 0.0], vec![0.0, 1.0]],
                    &[(5.0, 20.0)],
                ),
            ],
            flux_unit: PHOTON_FLUX_UNIT.to_string(),
            measured_band_nm: MEASURED_BAND_NM,
            response: PhotometricResponse::AbsolutePhotonFlux,
            partitions: sample_partitions(),
            out_of_domain_policy: OutOfDomainPolicy::Reject,
            training_command: "cargo test -p nsb-data-tools photometric".to_string(),
            software_version: "nsb-data-tools-test".to_string(),
        }
    }

    fn correction_with(artifact: PhotometricArtifact) -> (TempDir, PhotometricCorrection) {
        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("artifact.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        fs::write(&path, &bytes).unwrap();
        let sha256 = checksum_io::sha256_bytes(&bytes);
        let correction = PhotometricCorrection::load(&path, &sha256).unwrap();
        (temporary, correction)
    }

    #[test]
    fn routes_full_partial_g_only_and_unusable() {
        let (_tmp, correction) = correction_with(sample_artifact());

        let full = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(12.0),
                phot_bp_mean_mag: Some(12.5),
                phot_rp_mean_mag: Some(11.5),
                bp_rp: None,
                quality_flag: true,
            })
            .unwrap();
        assert_eq!(full.branch, PopulationBranch::PhotometricGBpRp);
        assert!(full.flux.is_some());

        let partial = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(12.0),
                phot_bp_mean_mag: None,
                phot_rp_mean_mag: Some(11.5),
                bp_rp: Some(1.0),
                quality_flag: true,
            })
            .unwrap();
        assert_eq!(partial.branch, PopulationBranch::PhotometricPartial);
        assert!(partial.flux.is_some());

        let g_only = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(12.0),
                phot_bp_mean_mag: None,
                phot_rp_mean_mag: None,
                bp_rp: None,
                quality_flag: true,
            })
            .unwrap();
        assert_eq!(g_only.branch, PopulationBranch::PhotometricGOnly);
        assert!(g_only.flux.is_some());

        let unusable = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: None,
                phot_bp_mean_mag: Some(12.5),
                phot_rp_mean_mag: Some(11.5),
                bp_rp: Some(1.0),
                quality_flag: true,
            })
            .unwrap();
        assert_eq!(unusable.branch, PopulationBranch::NoUsablePhotometry);
        assert!(unusable.flux.is_none());

        let excluded = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(12.0),
                phot_bp_mean_mag: Some(12.5),
                phot_rp_mean_mag: Some(11.5),
                bp_rp: Some(1.0),
                quality_flag: false,
            })
            .unwrap();
        assert_eq!(excluded.branch, PopulationBranch::ScientificExclusion);
        assert!(excluded.flux.is_none());
    }

    #[test]
    fn linear_evaluation_matches_intercept_and_coefficients() {
        let (_tmp, correction) = correction_with(sample_artifact());
        let decision = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(10.0),
                phot_bp_mean_mag: Some(10.5),
                phot_rp_mean_mag: Some(9.5),
                bp_rp: Some(1.0),
                quality_flag: true,
            })
            .unwrap();
        let flux = decision.flux.expect("full branch should evaluate");
        // 100 + (-5)*10 + 2*1 = 52
        assert!((flux.flux_336_650_ph_m2_s - 52.0).abs() < 1.0e-12);
        // sqrt(1 + 0.25*100 + 0.04*1) hypot 0.5
        let expected_stat = (1.0 + 25.0 + 0.04_f64).sqrt().hypot(0.5);
        assert!((flux.statistical_uncertainty_336_650_ph_m2_s - expected_stat).abs() < 1.0e-12);
        let expected_sys = 1.0_f64.hypot(52.0 * 0.1);
        assert!((flux.systematic_uncertainty_336_650_ph_m2_s - expected_sys).abs() < 1.0e-12);
        assert_eq!(flux.decision, EvaluationDecision::Applied);
        assert_eq!(flux.applicability_status, ApplicabilityStatus::InDomain);
    }

    #[test]
    fn out_of_domain_reject_returns_branch_without_flux() {
        let (_tmp, correction) = correction_with(sample_artifact());
        let decision = correction
            .route_and_evaluate(PhotometricFeatures {
                phot_g_mean_mag: Some(25.0),
                phot_bp_mean_mag: Some(25.5),
                phot_rp_mean_mag: Some(24.5),
                bp_rp: Some(1.0),
                quality_flag: true,
            })
            .unwrap();
        assert_eq!(decision.branch, PopulationBranch::PhotometricGBpRp);
        assert!(decision.flux.is_none());
    }

    #[test]
    fn production_status_and_checksum_fail_closed() {
        let artifact = sample_artifact();
        let (_tmp, correction) = correction_with(artifact.clone());
        assert!(correction.require_production_status().is_err());

        let temporary = TempDir::new().unwrap();
        let path = temporary.path().join("artifact.json");
        let bytes = serde_json::to_vec_pretty(&artifact).unwrap();
        fs::write(&path, &bytes).unwrap();
        let error = PhotometricCorrection::load(&path, &"0".repeat(64))
            .unwrap_err()
            .to_string();
        assert!(error.contains("checksum mismatch"));
    }
}
