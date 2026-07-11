//! Auditable, offline models used to assemble the integrated Starlight product.
//!
//! This module deliberately contains no built-in scientific coefficients. A
//! release must load a checksum-pinned model artifact trained and validated by
//! the maintainer pipeline. Unit tests use synthetic artifacts only.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Lower wavelength bound of the NSB integrated-radiance contract.
pub const STARLIGHT_BAND_MIN_NM: f64 = 300.0;
/// Boundary between the externally modelled UV term and Gaia XP sampled data.
pub const STARLIGHT_UV_MAX_NM: f64 = 336.0;
/// Upper wavelength bound of the NSB integrated-radiance contract.
pub const STARLIGHT_BAND_MAX_NM: f64 = 650.0;
/// Current model-artifact schema.
pub const MODEL_SCHEMA_VERSION: u32 = 1;

/// Available information for one Gaia source or aggregated source bin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotometryFeatures {
    pub g_flux_e_s: Option<f64>,
    pub bp_flux_e_s: Option<f64>,
    pub rp_flux_e_s: Option<f64>,
    pub g_mag: Option<f64>,
    pub bp_rp: Option<f64>,
    pub bp_rp_excess: Option<f64>,
    pub g_flux_over_error: Option<f64>,
    pub bp_flux_over_error: Option<f64>,
    pub rp_flux_over_error: Option<f64>,
    pub galactic_lon_deg: f64,
    pub galactic_lat_deg: f64,
    pub extinction_proxy_mag: Option<f64>,
    pub crowding_proxy: Option<f64>,
}

impl PhotometryFeatures {
    /// Select the most informative admissible inference branch.
    pub fn branch(&self) -> PhotometryBranch {
        match (
            positive(self.g_flux_e_s),
            positive(self.bp_flux_e_s),
            positive(self.rp_flux_e_s),
            finite(self.bp_rp),
        ) {
            (true, true, true, true) => PhotometryBranch::GBpRpColour,
            (true, _, true, _) | (true, true, _, _) => PhotometryBranch::PartialColour,
            (true, _, _, _) => PhotometryBranch::GOnly,
            _ => PhotometryBranch::NoUsablePhotometry,
        }
    }

    fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("galactic_lon_deg", self.galactic_lon_deg),
            ("galactic_lat_deg", self.galactic_lat_deg),
        ] {
            if !value.is_finite() {
                bail!("photometry feature {name} must be finite");
            }
        }
        if !(0.0..360.0).contains(&self.galactic_lon_deg) {
            bail!("galactic_lon_deg must be in [0,360)");
        }
        if !(-90.0..=90.0).contains(&self.galactic_lat_deg) {
            bail!("galactic_lat_deg must be in [-90,90]");
        }
        for (name, value) in self.optional_values() {
            if value.is_some_and(|entry| !entry.is_finite()) {
                bail!("optional photometry feature {name} must be finite when present");
            }
        }
        Ok(())
    }

    fn optional_values(&self) -> [(&'static str, Option<f64>); 11] {
        [
            ("g_flux_e_s", self.g_flux_e_s),
            ("bp_flux_e_s", self.bp_flux_e_s),
            ("rp_flux_e_s", self.rp_flux_e_s),
            ("g_mag", self.g_mag),
            ("bp_rp", self.bp_rp),
            ("bp_rp_excess", self.bp_rp_excess),
            ("g_flux_over_error", self.g_flux_over_error),
            ("bp_flux_over_error", self.bp_flux_over_error),
            ("rp_flux_over_error", self.rp_flux_over_error),
            ("extinction_proxy_mag", self.extinction_proxy_mag),
            ("crowding_proxy", self.crowding_proxy),
        ]
    }

    fn feature(&self, name: &str) -> Result<f64> {
        let value = match name {
            "ln_g_flux" => positive_log(self.g_flux_e_s),
            "ln_bp_flux" => positive_log(self.bp_flux_e_s),
            "ln_rp_flux" => positive_log(self.rp_flux_e_s),
            "g_mag" => finite_value(self.g_mag),
            "bp_rp" => finite_value(self.bp_rp),
            "bp_rp_excess" => finite_value(self.bp_rp_excess),
            "ln_g_snr" => positive_log(self.g_flux_over_error),
            "ln_bp_snr" => positive_log(self.bp_flux_over_error),
            "ln_rp_snr" => positive_log(self.rp_flux_over_error),
            "galactic_lon_sin" => Some(self.galactic_lon_deg.to_radians().sin()),
            "galactic_lon_cos" => Some(self.galactic_lon_deg.to_radians().cos()),
            "abs_galactic_lat" => Some(self.galactic_lat_deg.abs()),
            "extinction_proxy" => finite_value(self.extinction_proxy_mag),
            "crowding_proxy" => finite_value(self.crowding_proxy),
            other => bail!("unsupported Starlight model feature {other:?}"),
        };
        value.with_context(|| format!("required Starlight model feature {name:?} is unavailable"))
    }
}

/// Explicit photometric fallback branches, ordered by information content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhotometryBranch {
    GBpRpColour,
    PartialColour,
    GOnly,
    NoUsablePhotometry,
}

/// Domain of one standardized regression feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureTransform {
    pub name: String,
    pub center: f64,
    pub scale: f64,
    pub valid_min: f64,
    pub valid_max: f64,
}

impl FeatureTransform {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("model feature name must not be empty");
        }
        if !self.center.is_finite()
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || !self.valid_min.is_finite()
            || !self.valid_max.is_finite()
            || self.valid_min >= self.valid_max
        {
            bail!("invalid transform/domain for feature {:?}", self.name);
        }
        Ok(())
    }
}

/// Log-linear photon-flux model for one inference branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchModel {
    pub branch: PhotometryBranch,
    pub features: Vec<FeatureTransform>,
    /// Intercept followed by one coefficient per feature.
    pub coefficients: Vec<f64>,
    pub residual_fractional_sigma: f64,
    pub systematic_fractional_sigma: f64,
    pub upper_bound_sigma_multiplier: f64,
    pub validation: ValidationMetrics,
}

impl BranchModel {
    fn validate(&self) -> Result<()> {
        if self.branch == PhotometryBranch::NoUsablePhotometry {
            bail!("no-usable-photometry must not have a point-estimate regression");
        }
        if self.coefficients.len() != self.features.len() + 1
            || self.coefficients.iter().any(|value| !value.is_finite())
        {
            bail!(
                "branch {:?} has invalid regression coefficients",
                self.branch
            );
        }
        for feature in &self.features {
            feature.validate()?;
        }
        if !valid_nonnegative(self.residual_fractional_sigma)
            || !valid_nonnegative(self.systematic_fractional_sigma)
            || !self.upper_bound_sigma_multiplier.is_finite()
            || self.upper_bound_sigma_multiplier < 1.0
        {
            bail!(
                "branch {:?} has invalid uncertainty parameters",
                self.branch
            );
        }
        self.validation.validate()?;
        Ok(())
    }

    fn predict(&self, input: &PhotometryFeatures) -> Result<RegressionPrediction> {
        if input.branch() != self.branch {
            bail!(
                "branch model {:?} cannot evaluate features assigned to {:?}",
                self.branch,
                input.branch()
            );
        }
        let mut log_flux = self.coefficients[0];
        let mut extrapolated = false;
        for (index, feature) in self.features.iter().enumerate() {
            let value = input.feature(&feature.name)?;
            extrapolated |= value < feature.valid_min || value > feature.valid_max;
            let standardized = (value - feature.center) / feature.scale;
            log_flux += self.coefficients[index + 1] * standardized;
        }
        if !log_flux.is_finite() {
            bail!("branch {:?} produced non-finite log flux", self.branch);
        }
        let flux = log_flux.exp();
        if !valid_nonnegative(flux) {
            bail!("branch {:?} produced invalid photon flux", self.branch);
        }
        let inflation = if extrapolated { 2.0 } else { 1.0 };
        let statistical = flux * self.residual_fractional_sigma * inflation;
        let systematic = flux * self.systematic_fractional_sigma * inflation;
        let total = statistical.hypot(systematic);
        Ok(RegressionPrediction {
            flux_336_650_ph_m2_s: flux,
            statistical_uncertainty_ph_m2_s: statistical,
            systematic_uncertainty_ph_m2_s: systematic,
            total_uncertainty_ph_m2_s: total,
            upper_bound_ph_m2_s: flux + self.upper_bound_sigma_multiplier * total,
            extrapolated,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RegressionPrediction {
    flux_336_650_ph_m2_s: f64,
    statistical_uncertainty_ph_m2_s: f64,
    systematic_uncertainty_ph_m2_s: f64,
    total_uncertainty_ph_m2_s: f64,
    upper_bound_ph_m2_s: f64,
    extrapolated: bool,
}

/// Deterministic training row for one log-linear source model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelFitSample {
    pub source_id: u64,
    pub spatial_cell: u64,
    pub features: PhotometryFeatures,
    pub target: f64,
    pub target_one_sigma: f64,
}

/// Fit one photometric branch with ridge-regularised weighted least squares.
///
/// `target` is the held-out physical 336--650 nm photon flux. Feature
/// standardisation is learned from `training` only; validation rows never
/// influence coefficients or transforms.
pub fn fit_branch_model(
    training: &[ModelFitSample],
    validation: &[ModelFitSample],
    branch: PhotometryBranch,
    feature_names: &[String],
    ridge: f64,
    systematic_fractional_sigma: f64,
    upper_bound_sigma_multiplier: f64,
) -> Result<BranchModel> {
    if branch == PhotometryBranch::NoUsablePhotometry {
        bail!("cannot fit a point estimate for no-usable-photometry");
    }
    validate_fit_inputs(training, validation, branch, feature_names, ridge)?;
    if !valid_nonnegative(systematic_fractional_sigma)
        || !upper_bound_sigma_multiplier.is_finite()
        || upper_bound_sigma_multiplier < 1.0
    {
        bail!("invalid fitted-branch uncertainty settings");
    }
    let (features, coefficients) = fit_log_linear(training, feature_names, ridge)?;
    let provisional = BranchModel {
        branch,
        features,
        coefficients,
        residual_fractional_sigma: 0.0,
        systematic_fractional_sigma,
        upper_bound_sigma_multiplier,
        validation: ValidationMetrics::default(),
    };
    let residual_sigma = validation_residual_scale(&provisional, validation)?;
    let mut fitted = BranchModel {
        residual_fractional_sigma: residual_sigma,
        ..provisional
    };
    fitted.validation = metrics_for_branch(&fitted, validation)?;
    fitted.validate()?;
    Ok(fitted)
}

/// Fit the 300--336 / 336--650 photon-flux ratio for one information branch.
/// `ModelFitSample::target` is the positive ratio derived from independent
/// calibrated spectra.
pub fn fit_uv_correction_model(
    training: &[ModelFitSample],
    validation: &[ModelFitSample],
    branch: PhotometryBranch,
    feature_names: &[String],
    ridge: f64,
    maximum_ratio: f64,
    systematic_fractional_sigma: f64,
) -> Result<UvCorrectionModel> {
    if branch == PhotometryBranch::NoUsablePhotometry {
        bail!("cannot fit a UV point estimate for no-usable-photometry");
    }
    validate_fit_inputs(training, validation, branch, feature_names, ridge)?;
    if !maximum_ratio.is_finite()
        || maximum_ratio <= 0.0
        || !valid_nonnegative(systematic_fractional_sigma)
    {
        bail!("invalid UV fit bound or systematic uncertainty");
    }
    let (features, coefficients) = fit_log_linear(training, feature_names, ridge)?;
    let mut predicted = Vec::with_capacity(validation.len());
    let provisional = UvCorrectionModel {
        branch,
        features,
        coefficients,
        maximum_ratio,
        residual_fractional_sigma: 0.0,
        systematic_fractional_sigma,
        validation: ValidationMetrics::default(),
    };
    let mut absolute_residuals = Vec::with_capacity(validation.len());
    for sample in validation {
        let ratio = provisional.predict_ratio(&sample.features)?.ratio;
        absolute_residuals.push((ratio - sample.target).abs() / sample.target.max(1.0e-30));
    }
    absolute_residuals.sort_by(f64::total_cmp);
    let residual_fractional_sigma = percentile_sorted(&absolute_residuals, 0.68).max(1.0e-12);
    let mut fitted = UvCorrectionModel {
        residual_fractional_sigma,
        ..provisional
    };
    for sample in validation {
        let ratio = fitted.predict_ratio(&sample.features)?;
        let sigma = ratio.ratio
            * ratio
                .statistical_fractional_sigma
                .hypot(ratio.systematic_fractional_sigma);
        predicted.push(ResidualSample {
            observed_photon_flux: sample.target,
            predicted_photon_flux: ratio.ratio,
            predicted_one_sigma: sigma,
        });
    }
    let floor = validation
        .iter()
        .map(|sample| sample.target)
        .fold(f64::INFINITY, f64::min)
        .max(1.0e-30);
    fitted.validation = compute_validation_metrics(&predicted, floor)?;
    fitted.validate()?;
    Ok(fitted)
}

fn validate_fit_inputs(
    training: &[ModelFitSample],
    validation: &[ModelFitSample],
    branch: PhotometryBranch,
    feature_names: &[String],
    ridge: f64,
) -> Result<()> {
    if feature_names.is_empty() || feature_names.iter().any(|name| name.trim().is_empty()) {
        bail!("model fitting requires named features");
    }
    if !ridge.is_finite() || ridge < 0.0 {
        bail!("ridge penalty must be finite and non-negative");
    }
    if training.len() < feature_names.len() + 2 || validation.is_empty() {
        bail!("insufficient independent training or validation rows");
    }
    let mut train_sources = BTreeSet::new();
    let mut train_cells = BTreeSet::new();
    for sample in training {
        validate_fit_sample(sample, branch, feature_names)?;
        if !train_sources.insert(sample.source_id) {
            bail!(
                "duplicate source_id {} in training partition",
                sample.source_id
            );
        }
        train_cells.insert(sample.spatial_cell);
    }
    let mut validation_sources = BTreeSet::new();
    for sample in validation {
        validate_fit_sample(sample, branch, feature_names)?;
        if train_sources.contains(&sample.source_id)
            || train_cells.contains(&sample.spatial_cell)
            || !validation_sources.insert(sample.source_id)
        {
            bail!("training/validation leakage for source or spatial cell");
        }
    }
    Ok(())
}

fn validate_fit_sample(
    sample: &ModelFitSample,
    branch: PhotometryBranch,
    feature_names: &[String],
) -> Result<()> {
    sample.features.validate()?;
    if sample.features.branch() != branch {
        bail!(
            "fit sample {} belongs to {:?}, expected {:?}",
            sample.source_id,
            sample.features.branch(),
            branch
        );
    }
    if !sample.target.is_finite()
        || sample.target <= 0.0
        || !valid_nonnegative(sample.target_one_sigma)
    {
        bail!("fit sample target and uncertainty must be finite and physical");
    }
    for name in feature_names {
        sample.features.feature(name)?;
    }
    Ok(())
}

fn fit_log_linear(
    training: &[ModelFitSample],
    feature_names: &[String],
    ridge: f64,
) -> Result<(Vec<FeatureTransform>, Vec<f64>)> {
    let mut transforms = Vec::with_capacity(feature_names.len());
    for name in feature_names {
        let values: Vec<f64> = training
            .iter()
            .map(|sample| sample.features.feature(name))
            .collect::<Result<_>>()?;
        let center = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values
            .iter()
            .map(|value| (value - center).powi(2))
            .sum::<f64>()
            / values.len() as f64;
        let scale = variance.sqrt();
        let valid_min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let valid_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !scale.is_finite() || scale <= f64::EPSILON || valid_min >= valid_max {
            bail!("training feature {name:?} has no usable variance");
        }
        transforms.push(FeatureTransform {
            name: name.clone(),
            center,
            scale,
            valid_min,
            valid_max,
        });
    }

    let dimension = transforms.len() + 1;
    let mut normal = vec![vec![0.0; dimension]; dimension];
    let mut rhs = vec![0.0; dimension];
    for sample in training {
        let mut row = Vec::with_capacity(dimension);
        row.push(1.0);
        for transform in &transforms {
            let value = sample.features.feature(&transform.name)?;
            row.push((value - transform.center) / transform.scale);
        }
        let target_log = sample.target.ln();
        let relative_sigma = (sample.target_one_sigma / sample.target).max(1.0e-6);
        let weight = 1.0 / (relative_sigma * relative_sigma);
        for i in 0..dimension {
            rhs[i] += weight * row[i] * target_log;
            for j in 0..dimension {
                normal[i][j] += weight * row[i] * row[j];
            }
        }
    }
    for (index, row) in normal.iter_mut().enumerate().skip(1) {
        row[index] += ridge;
    }
    let coefficients = solve_linear_system(normal, rhs)?;
    Ok((transforms, coefficients))
}

fn solve_linear_system(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Result<Vec<f64>> {
    let dimension = rhs.len();
    for pivot in 0..dimension {
        let best = (pivot..dimension)
            .max_by(|a, b| matrix[*a][pivot].abs().total_cmp(&matrix[*b][pivot].abs()))
            .expect("nonempty pivot range");
        if matrix[best][pivot].abs() <= 1.0e-14 {
            bail!("model normal matrix is singular");
        }
        matrix.swap(pivot, best);
        rhs.swap(pivot, best);
        let divisor = matrix[pivot][pivot];
        for value in &mut matrix[pivot][pivot..] {
            *value /= divisor;
        }
        rhs[pivot] /= divisor;
        for row in 0..dimension {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            for column in pivot..dimension {
                matrix[row][column] -= factor * matrix[pivot][column];
            }
            rhs[row] -= factor * rhs[pivot];
        }
    }
    if rhs.iter().any(|value| !value.is_finite()) {
        bail!("model fit produced non-finite coefficients");
    }
    Ok(rhs)
}

fn validation_residual_scale(model: &BranchModel, validation: &[ModelFitSample]) -> Result<f64> {
    let mut residuals = Vec::with_capacity(validation.len());
    for sample in validation {
        let prediction = model.predict(&sample.features)?;
        residuals.push(
            (prediction.flux_336_650_ph_m2_s - sample.target).abs() / sample.target.max(1.0e-30),
        );
    }
    residuals.sort_by(f64::total_cmp);
    Ok(percentile_sorted(&residuals, 0.68).max(1.0e-12))
}

fn metrics_for_branch(
    model: &BranchModel,
    validation: &[ModelFitSample],
) -> Result<ValidationMetrics> {
    let residuals = validation
        .iter()
        .map(|sample| {
            let prediction = model.predict(&sample.features)?;
            Ok(ResidualSample {
                observed_photon_flux: sample.target,
                predicted_photon_flux: prediction.flux_336_650_ph_m2_s,
                predicted_one_sigma: prediction.total_uncertainty_ph_m2_s,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let floor = validation
        .iter()
        .map(|sample| sample.target)
        .fold(f64::INFINITY, f64::min)
        .max(1.0e-30);
    compute_validation_metrics(&residuals, floor)
}

/// Independently calibrated model of the 300--336 nm / 336--650 nm ratio.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvCorrectionModel {
    pub branch: PhotometryBranch,
    pub features: Vec<FeatureTransform>,
    /// Intercept followed by one coefficient per feature; predicts ln(ratio).
    pub coefficients: Vec<f64>,
    pub maximum_ratio: f64,
    pub residual_fractional_sigma: f64,
    pub systematic_fractional_sigma: f64,
    pub validation: ValidationMetrics,
}

impl UvCorrectionModel {
    fn validate(&self) -> Result<()> {
        if self.branch == PhotometryBranch::NoUsablePhotometry {
            bail!("no-usable-photometry must not have a UV point-estimate model");
        }
        if self.coefficients.len() != self.features.len() + 1
            || self.coefficients.iter().any(|value| !value.is_finite())
        {
            bail!("UV correction has invalid coefficients");
        }
        for feature in &self.features {
            feature.validate()?;
        }
        if !self.maximum_ratio.is_finite()
            || self.maximum_ratio <= 0.0
            || !valid_nonnegative(self.residual_fractional_sigma)
            || !valid_nonnegative(self.systematic_fractional_sigma)
        {
            bail!("UV correction has invalid bounds or uncertainty");
        }
        self.validation.validate()?;
        Ok(())
    }

    fn predict_ratio(&self, input: &PhotometryFeatures) -> Result<UvPrediction> {
        if input.branch() != self.branch {
            bail!(
                "UV model {:?} cannot evaluate features assigned to {:?}",
                self.branch,
                input.branch()
            );
        }
        let mut log_ratio = self.coefficients[0];
        let mut extrapolated = false;
        for (index, feature) in self.features.iter().enumerate() {
            let value = input.feature(&feature.name)?;
            extrapolated |= value < feature.valid_min || value > feature.valid_max;
            log_ratio += self.coefficients[index + 1] * (value - feature.center) / feature.scale;
        }
        let unbounded = log_ratio.exp();
        if !unbounded.is_finite() || unbounded < 0.0 {
            bail!("UV correction produced an invalid ratio");
        }
        let bounded = unbounded.min(self.maximum_ratio);
        let bounded_by_cap = unbounded > self.maximum_ratio;
        let inflation = if extrapolated || bounded_by_cap {
            2.0
        } else {
            1.0
        };
        Ok(UvPrediction {
            ratio: bounded,
            statistical_fractional_sigma: self.residual_fractional_sigma * inflation,
            systematic_fractional_sigma: self.systematic_fractional_sigma * inflation,
            extrapolated: extrapolated || bounded_by_cap,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UvPrediction {
    ratio: f64,
    statistical_fractional_sigma: f64,
    systematic_fractional_sigma: f64,
    extrapolated: bool,
}

/// Spatially and photometrically resolved Gaia completeness cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletenessCell {
    pub healpix_index: u64,
    pub g_min: f64,
    pub g_max: f64,
    pub colour_min: Option<f64>,
    pub colour_max: Option<f64>,
    pub completeness: f64,
    pub statistical_sigma: f64,
    pub systematic_sigma: f64,
    /// Fraction of represented flux additionally expected beyond the modelled
    /// catalogue faint limit.
    pub faint_tail_fraction: f64,
    pub crowding_flag: bool,
}

impl CompletenessCell {
    fn validate(&self) -> Result<()> {
        if !self.g_min.is_finite() || !self.g_max.is_finite() || self.g_min >= self.g_max {
            bail!("invalid magnitude domain in completeness cell");
        }
        match (self.colour_min, self.colour_max) {
            (Some(min), Some(max)) if min.is_finite() && max.is_finite() && min < max => {}
            (None, None) => {}
            _ => bail!("completeness colour bounds must be a valid pair or both absent"),
        }
        if !self.completeness.is_finite()
            || !(0.0..=1.0).contains(&self.completeness)
            || self.completeness == 0.0
            || !valid_nonnegative(self.statistical_sigma)
            || !valid_nonnegative(self.systematic_sigma)
            || !valid_nonnegative(self.faint_tail_fraction)
        {
            bail!("invalid completeness estimate or uncertainty");
        }
        Ok(())
    }

    fn matches(&self, healpix_index: u64, g_mag: f64, colour: Option<f64>) -> bool {
        if self.healpix_index != healpix_index || !(self.g_min..self.g_max).contains(&g_mag) {
            return false;
        }
        match (self.colour_min, self.colour_max, colour) {
            (Some(min), Some(max), Some(value)) => (min..max).contains(&value),
            (None, None, _) => true,
            _ => false,
        }
    }
}

/// Bounded selection-function representation used only during map generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionFunctionModel {
    pub healpix_nside: u32,
    pub maximum_inverse_completeness_weight: f64,
    pub cells: Vec<CompletenessCell>,
    pub reference: String,
    pub reference_version: String,
    pub license: String,
}

impl SelectionFunctionModel {
    fn validate(&self) -> Result<()> {
        if self.healpix_nside == 0 || !self.healpix_nside.is_power_of_two() {
            bail!("selection-function HEALPix nside must be a positive power of two");
        }
        if !self.maximum_inverse_completeness_weight.is_finite()
            || self.maximum_inverse_completeness_weight < 1.0
        {
            bail!("selection-function weight cap must be finite and at least one");
        }
        if self.cells.is_empty() {
            bail!("selection-function model must contain spatial cells");
        }
        for value in [&self.reference, &self.reference_version, &self.license] {
            if value.trim().is_empty() {
                bail!("selection-function provenance must not be empty");
            }
        }
        let mut sky_cells = BTreeSet::new();
        for cell in &self.cells {
            cell.validate()?;
            sky_cells.insert(cell.healpix_index);
        }
        if sky_cells.len() < 2 {
            bail!("selection function must be spatial; a single global cell is forbidden");
        }
        Ok(())
    }

    /// Correct an represented photon flux without permitting negative or
    /// unbounded corrections.
    pub fn correct_flux(
        &self,
        healpix_index: u64,
        g_mag: f64,
        colour: Option<f64>,
        represented_flux_ph_m2_s: f64,
    ) -> Result<CompletenessPrediction> {
        if !g_mag.is_finite() || !valid_nonnegative(represented_flux_ph_m2_s) {
            bail!("selection-function inputs must be finite and non-negative");
        }
        let cell = self
            .cells
            .iter()
            .find(|cell| cell.matches(healpix_index, g_mag, colour))
            .with_context(|| {
                format!(
                    "selection-function domain has no cell for pixel {healpix_index}, G={g_mag}, colour={colour:?}"
                )
            })?;
        let raw_weight = 1.0 / cell.completeness;
        let weight = raw_weight.min(self.maximum_inverse_completeness_weight);
        let capped = raw_weight > weight;
        let catalogue_missing = represented_flux_ph_m2_s * (weight - 1.0);
        let faint_tail = represented_flux_ph_m2_s * cell.faint_tail_fraction;
        let correction = catalogue_missing + faint_tail;
        let derivative = represented_flux_ph_m2_s / (cell.completeness * cell.completeness);
        let statistical = derivative * cell.statistical_sigma;
        let systematic = derivative * cell.systematic_sigma
            + if capped {
                represented_flux_ph_m2_s * (raw_weight - weight)
            } else {
                0.0
            }
            + faint_tail;
        Ok(CompletenessPrediction {
            correction_ph_m2_s: correction,
            statistical_uncertainty_ph_m2_s: statistical,
            systematic_uncertainty_ph_m2_s: systematic,
            total_uncertainty_ph_m2_s: statistical.hypot(systematic),
            completeness: cell.completeness,
            inverse_weight: weight,
            capped,
            crowding_flag: cell.crowding_flag,
        })
    }
}

/// Missing-population contribution for an aggregate source bin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CompletenessPrediction {
    pub correction_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_uncertainty_ph_m2_s: f64,
    pub total_uncertainty_ph_m2_s: f64,
    pub completeness: f64,
    pub inverse_weight: f64,
    pub capped: bool,
    pub crowding_flag: bool,
}

/// Validation statistics carried by each fitted model branch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationMetrics {
    pub samples: u64,
    pub mean_bias_fraction: f64,
    pub median_bias_fraction: f64,
    pub rmse_fraction: f64,
    pub mae_fraction: f64,
    pub robust_relative_error_fraction: f64,
    pub absolute_error_percentiles: BTreeMap<String, f64>,
    pub interval_68_coverage: f64,
    pub interval_95_coverage: f64,
}

impl ValidationMetrics {
    fn validate(&self) -> Result<()> {
        if self.samples == 0 {
            bail!("validation metrics require at least one held-out sample");
        }
        for value in [
            self.mean_bias_fraction,
            self.median_bias_fraction,
            self.rmse_fraction,
            self.mae_fraction,
            self.robust_relative_error_fraction,
            self.interval_68_coverage,
            self.interval_95_coverage,
        ] {
            if !value.is_finite() {
                bail!("validation metric must be finite");
            }
        }
        for percentile in ["p50", "p68", "p90", "p95", "p99"] {
            if !self
                .absolute_error_percentiles
                .get(percentile)
                .is_some_and(|value| valid_nonnegative(*value))
            {
                bail!("validation metrics are missing finite {percentile}");
            }
        }
        if !(0.0..=1.0).contains(&self.interval_68_coverage)
            || !(0.0..=1.0).contains(&self.interval_95_coverage)
        {
            bail!("validation coverage must be in [0,1]");
        }
        Ok(())
    }

    /// Apply the preregistered per-source production thresholds.
    pub fn passes_source_gates(&self) -> bool {
        self.mean_bias_fraction.abs() <= 0.03
            && self.median_bias_fraction.abs() <= 0.05
            && self
                .absolute_error_percentiles
                .get("p95")
                .is_some_and(|value| *value <= 0.10)
            && (0.63..=0.73).contains(&self.interval_68_coverage)
            && (0.90..=0.98).contains(&self.interval_95_coverage)
    }
}

/// One held-out target used to compute validation metrics.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidualSample {
    pub observed_photon_flux: f64,
    pub predicted_photon_flux: f64,
    pub predicted_one_sigma: f64,
}

/// Compute the complete source-level metric set without discarding outliers.
pub fn compute_validation_metrics(
    samples: &[ResidualSample],
    relative_error_floor: f64,
) -> Result<ValidationMetrics> {
    if samples.is_empty() {
        bail!("validation metrics require held-out samples");
    }
    if !relative_error_floor.is_finite() || relative_error_floor <= 0.0 {
        bail!("relative-error floor must be finite and positive");
    }
    let mut signed = Vec::with_capacity(samples.len());
    let mut absolute = Vec::with_capacity(samples.len());
    let mut squared = 0.0;
    let mut within_one = 0_u64;
    let mut within_two = 0_u64;
    for sample in samples {
        if !valid_nonnegative(sample.observed_photon_flux)
            || !valid_nonnegative(sample.predicted_photon_flux)
            || !valid_nonnegative(sample.predicted_one_sigma)
        {
            bail!("validation samples must contain finite non-negative values");
        }
        let scale = sample.observed_photon_flux.abs().max(relative_error_floor);
        let residual = (sample.predicted_photon_flux - sample.observed_photon_flux) / scale;
        signed.push(residual);
        absolute.push(residual.abs());
        squared += residual * residual;
        let absolute_flux_error =
            (sample.predicted_photon_flux - sample.observed_photon_flux).abs();
        within_one += u64::from(absolute_flux_error <= sample.predicted_one_sigma);
        within_two +=
            u64::from(absolute_flux_error <= 1.959_963_984_540_054 * sample.predicted_one_sigma);
    }
    signed.sort_by(f64::total_cmp);
    absolute.sort_by(f64::total_cmp);
    let count = samples.len() as f64;
    let mean_bias = signed.iter().sum::<f64>() / count;
    let median_bias = percentile_sorted(&signed, 0.5);
    let mae = absolute.iter().sum::<f64>() / count;
    let rmse = (squared / count).sqrt();
    let percentiles = [
        ("p50", 0.50),
        ("p68", 0.68),
        ("p90", 0.90),
        ("p95", 0.95),
        ("p99", 0.99),
    ]
    .into_iter()
    .map(|(name, probability)| (name.to_string(), percentile_sorted(&absolute, probability)))
    .collect();
    Ok(ValidationMetrics {
        samples: u64::try_from(samples.len()).context("validation sample count exceeds u64")?,
        mean_bias_fraction: mean_bias,
        median_bias_fraction: median_bias,
        rmse_fraction: rmse,
        mae_fraction: mae,
        robust_relative_error_fraction: percentile_sorted(&absolute, 0.5),
        absolute_error_percentiles: percentiles,
        interval_68_coverage: within_one as f64 / count,
        interval_95_coverage: within_two as f64 / count,
    })
}

fn percentile_sorted(values: &[f64], probability: f64) -> f64 {
    debug_assert!(!values.is_empty());
    if values.len() == 1 {
        return values[0];
    }
    let position = probability.clamp(0.0, 1.0) * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    let fraction = position - lower as f64;
    values[lower] * (1.0 - fraction) + values[upper] * fraction
}

/// Complete, serialized inference model. It is not an approval artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StarlightModelArtifact {
    pub schema_version: u32,
    pub model_id: String,
    pub release_id: String,
    pub band_nm: [f64; 2],
    pub training_data_sha256: String,
    pub calibration_inputs_sha256: Vec<String>,
    pub split: SpatialSplitSpec,
    pub branches: Vec<BranchModel>,
    pub uv_corrections: Vec<UvCorrectionModel>,
    pub selection_function: SelectionFunctionModel,
    pub absolute_calibration_fractional_uncertainty: f64,
}

impl StarlightModelArtifact {
    /// Validate schema, physical bounds, branch coverage, and held-out metrics.
    pub fn validate(&self, require_production_metrics: bool) -> Result<()> {
        if self.schema_version != MODEL_SCHEMA_VERSION {
            bail!(
                "unsupported Starlight model schema {}; expected {MODEL_SCHEMA_VERSION}",
                self.schema_version
            );
        }
        if self.model_id.trim().is_empty() || self.release_id.trim().is_empty() {
            bail!("Starlight model and release identifiers must not be empty");
        }
        if self.band_nm.map(f64::to_bits)
            != [STARLIGHT_BAND_MIN_NM, STARLIGHT_BAND_MAX_NM].map(f64::to_bits)
        {
            bail!("Starlight model must use the exact 300-650 nm contract");
        }
        validate_sha256(&self.training_data_sha256, "training_data_sha256")?;
        if self.calibration_inputs_sha256.is_empty() {
            bail!("Starlight model must checksum its calibration inputs");
        }
        for checksum in &self.calibration_inputs_sha256 {
            validate_sha256(checksum, "calibration input")?;
        }
        self.split.validate()?;
        let mut branches = BTreeSet::new();
        for branch in &self.branches {
            branch.validate()?;
            if !branches.insert(branch.branch) {
                bail!("duplicate model for photometry branch {:?}", branch.branch);
            }
            if require_production_metrics && !branch.validation.passes_source_gates() {
                bail!("photometry branch {:?} fails source gates", branch.branch);
            }
        }
        for required in [
            PhotometryBranch::GBpRpColour,
            PhotometryBranch::PartialColour,
            PhotometryBranch::GOnly,
        ] {
            if !branches.contains(&required) {
                bail!("Starlight model lacks required branch {required:?}");
            }
        }
        let mut uv_branches = BTreeSet::new();
        for correction in &self.uv_corrections {
            correction.validate()?;
            if !uv_branches.insert(correction.branch) {
                bail!("duplicate UV correction for branch {:?}", correction.branch);
            }
            if require_production_metrics && !correction.validation.passes_source_gates() {
                bail!(
                    "300-336 nm correction for branch {:?} fails source gates",
                    correction.branch
                );
            }
        }
        for required in [
            PhotometryBranch::GBpRpColour,
            PhotometryBranch::PartialColour,
            PhotometryBranch::GOnly,
        ] {
            if !uv_branches.contains(&required) {
                bail!("Starlight model lacks required UV branch {required:?}");
            }
        }
        self.selection_function.validate()?;
        if !valid_nonnegative(self.absolute_calibration_fractional_uncertainty) {
            bail!("absolute calibration uncertainty must be finite and non-negative");
        }
        Ok(())
    }

    /// Infer the complete 300--650 nm photon flux for one source.
    pub fn predict_photometric_source(
        &self,
        input: &PhotometryFeatures,
    ) -> Result<SourceFluxPrediction> {
        input.validate()?;
        let branch = input.branch();
        if branch == PhotometryBranch::NoUsablePhotometry {
            return Ok(SourceFluxPrediction::no_photometry());
        }
        let model = self
            .branches
            .iter()
            .find(|entry| entry.branch == branch)
            .with_context(|| format!("model artifact has no branch {branch:?}"))?;
        let optical = model.predict(input)?;
        let uv = self.uv_model(branch)?.predict_ratio(input)?;
        combine_optical_and_uv(
            branch,
            optical,
            uv,
            self.absolute_calibration_fractional_uncertainty,
        )
    }

    /// Add the calibrated 300--336 nm term to a measured/reconstructed Gaia XP
    /// 336--650 nm photon flux.
    pub fn extend_xp_flux(
        &self,
        input: &PhotometryFeatures,
        branch: SourceMeasurementBranch,
        flux_336_650_ph_m2_s: f64,
        statistical_uncertainty_336_650_ph_m2_s: f64,
        systematic_uncertainty_336_650_ph_m2_s: f64,
    ) -> Result<SourceFluxPrediction> {
        input.validate()?;
        for value in [
            flux_336_650_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s,
            systematic_uncertainty_336_650_ph_m2_s,
        ] {
            if !valid_nonnegative(value) {
                bail!("XP flux and uncertainties must be finite and non-negative");
            }
        }
        let uv = self.uv_model(input.branch())?.predict_ratio(input)?;
        let optical = RegressionPrediction {
            flux_336_650_ph_m2_s,
            statistical_uncertainty_ph_m2_s: statistical_uncertainty_336_650_ph_m2_s,
            systematic_uncertainty_ph_m2_s: systematic_uncertainty_336_650_ph_m2_s,
            total_uncertainty_ph_m2_s: statistical_uncertainty_336_650_ph_m2_s
                .hypot(systematic_uncertainty_336_650_ph_m2_s),
            upper_bound_ph_m2_s: flux_336_650_ph_m2_s
                + 2.0
                    * statistical_uncertainty_336_650_ph_m2_s
                        .hypot(systematic_uncertainty_336_650_ph_m2_s),
            extrapolated: false,
        };
        let mut prediction = combine_optical_and_uv(
            PhotometryBranch::GBpRpColour,
            optical,
            uv,
            self.absolute_calibration_fractional_uncertainty,
        )?;
        prediction.measurement_branch = Some(branch);
        Ok(prediction)
    }

    fn uv_model(&self, branch: PhotometryBranch) -> Result<&UvCorrectionModel> {
        self.uv_corrections
            .iter()
            .find(|entry| entry.branch == branch)
            .with_context(|| format!("model artifact has no 300-336 nm branch {branch:?}"))
    }
}

/// Direct spectral-information branch, separate from photometric fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMeasurementBranch {
    XpSampledMeasured,
    XpContinuousReconstructed,
}

/// Auditable source-level prediction and uncertainty.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceFluxPrediction {
    pub photometry_branch: PhotometryBranch,
    pub measurement_branch: Option<SourceMeasurementBranch>,
    pub flux_300_336_ph_m2_s: Option<f64>,
    pub flux_336_650_ph_m2_s: Option<f64>,
    pub flux_300_650_ph_m2_s: Option<f64>,
    pub statistical_uncertainty_ph_m2_s: Option<f64>,
    pub systematic_uncertainty_ph_m2_s: Option<f64>,
    pub total_uncertainty_ph_m2_s: Option<f64>,
    pub upper_bound_ph_m2_s: Option<f64>,
    pub extrapolated: bool,
}

impl SourceFluxPrediction {
    fn no_photometry() -> Self {
        Self {
            photometry_branch: PhotometryBranch::NoUsablePhotometry,
            measurement_branch: None,
            flux_300_336_ph_m2_s: None,
            flux_336_650_ph_m2_s: None,
            flux_300_650_ph_m2_s: None,
            statistical_uncertainty_ph_m2_s: None,
            systematic_uncertainty_ph_m2_s: None,
            total_uncertainty_ph_m2_s: None,
            upper_bound_ph_m2_s: None,
            extrapolated: true,
        }
    }
}

fn combine_optical_and_uv(
    branch: PhotometryBranch,
    optical: RegressionPrediction,
    uv: UvPrediction,
    absolute_calibration_fractional_uncertainty: f64,
) -> Result<SourceFluxPrediction> {
    let uv_flux = optical.flux_336_650_ph_m2_s * uv.ratio;
    let total_flux = optical.flux_336_650_ph_m2_s + uv_flux;
    let uv_statistical = uv_flux * uv.statistical_fractional_sigma;
    let statistical = optical
        .statistical_uncertainty_ph_m2_s
        .hypot(uv_statistical);

    // UV-model, branch-model, and absolute-calibration terms are treated as
    // fully correlated systematics for one source and therefore add linearly.
    let systematic = optical.systematic_uncertainty_ph_m2_s
        + uv_flux * uv.systematic_fractional_sigma
        + total_flux * absolute_calibration_fractional_uncertainty;
    let total = statistical.hypot(systematic);
    for value in [uv_flux, total_flux, statistical, systematic, total] {
        if !valid_nonnegative(value) {
            bail!("combined Starlight prediction is non-finite or negative");
        }
    }
    Ok(SourceFluxPrediction {
        photometry_branch: branch,
        measurement_branch: None,
        flux_300_336_ph_m2_s: Some(uv_flux),
        flux_336_650_ph_m2_s: Some(optical.flux_336_650_ph_m2_s),
        flux_300_650_ph_m2_s: Some(total_flux),
        statistical_uncertainty_ph_m2_s: Some(statistical),
        systematic_uncertainty_ph_m2_s: Some(systematic),
        total_uncertainty_ph_m2_s: Some(total),
        upper_bound_ph_m2_s: Some(
            (optical.upper_bound_ph_m2_s * (1.0 + uv.ratio)).max(total_flux + 2.0 * total),
        ),
        extrapolated: optical.extrapolated || uv.extrapolated,
    })
}

/// Deterministic spatial split that keeps nearby sources out of different
/// train/validation/test partitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialSplitSpec {
    pub algorithm: String,
    pub seed: u64,
    pub spatial_nside: u32,
    pub train_buckets: Vec<u8>,
    pub validation_buckets: Vec<u8>,
    pub test_buckets: Vec<u8>,
    pub bucket_modulus: u8,
}

impl SpatialSplitSpec {
    fn validate(&self) -> Result<()> {
        if self.algorithm != "splitmix64_spatial_cell_v1"
            || self.spatial_nside == 0
            || !self.spatial_nside.is_power_of_two()
            || self.bucket_modulus < 3
        {
            bail!("invalid spatial split definition");
        }
        let mut used = BTreeSet::new();
        for bucket in self
            .train_buckets
            .iter()
            .chain(&self.validation_buckets)
            .chain(&self.test_buckets)
        {
            if *bucket >= self.bucket_modulus || !used.insert(*bucket) {
                bail!("spatial split buckets must be unique and in range");
            }
        }
        if self.train_buckets.is_empty()
            || self.validation_buckets.is_empty()
            || self.test_buckets.is_empty()
            || used.len() != usize::from(self.bucket_modulus)
        {
            bail!("spatial split must assign every bucket exactly once");
        }
        Ok(())
    }

    /// Assign all sources in one spatial cell to the same partition.
    pub fn partition(&self, spatial_cell: u64) -> Result<DataPartition> {
        self.validate()?;
        let bucket = (splitmix64(spatial_cell ^ self.seed) % u64::from(self.bucket_modulus)) as u8;
        if self.train_buckets.contains(&bucket) {
            Ok(DataPartition::Train)
        } else if self.validation_buckets.contains(&bucket) {
            Ok(DataPartition::Validation)
        } else if self.test_buckets.contains(&bucket) {
            Ok(DataPartition::Test)
        } else {
            bail!("spatial split left bucket {bucket} unassigned")
        }
    }
}

/// Independent data partitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPartition {
    Train,
    Validation,
    Test,
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn validate_sha256(value: &str, name: &str) -> Result<()> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{name} must be a 64-digit SHA-256");
    }
    Ok(())
}

fn finite(value: Option<f64>) -> bool {
    value.is_some_and(f64::is_finite)
}

fn finite_value(value: Option<f64>) -> Option<f64> {
    value.filter(|entry| entry.is_finite())
}

fn positive(value: Option<f64>) -> bool {
    value.is_some_and(|entry| entry.is_finite() && entry > 0.0)
}

fn positive_log(value: Option<f64>) -> Option<f64> {
    value
        .filter(|entry| entry.is_finite() && *entry > 0.0)
        .map(f64::ln)
}

fn valid_nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics() -> ValidationMetrics {
        ValidationMetrics {
            samples: 100,
            mean_bias_fraction: 0.01,
            median_bias_fraction: 0.01,
            rmse_fraction: 0.04,
            mae_fraction: 0.03,
            robust_relative_error_fraction: 0.03,
            absolute_error_percentiles: [
                ("p50".to_string(), 0.02),
                ("p68".to_string(), 0.03),
                ("p90".to_string(), 0.06),
                ("p95".to_string(), 0.08),
                ("p99".to_string(), 0.15),
            ]
            .into_iter()
            .collect(),
            interval_68_coverage: 0.68,
            interval_95_coverage: 0.95,
        }
    }

    fn transform(name: &str, min: f64, max: f64) -> FeatureTransform {
        FeatureTransform {
            name: name.to_string(),
            center: 0.0,
            scale: 1.0,
            valid_min: min,
            valid_max: max,
        }
    }

    fn branch(branch: PhotometryBranch, features: Vec<FeatureTransform>) -> BranchModel {
        BranchModel {
            branch,
            coefficients: std::iter::once(5.0)
                .chain(std::iter::repeat_n(0.0, features.len()))
                .collect(),
            features,
            residual_fractional_sigma: 0.04,
            systematic_fractional_sigma: 0.02,
            upper_bound_sigma_multiplier: 2.0,
            validation: metrics(),
        }
    }

    fn artifact() -> StarlightModelArtifact {
        StarlightModelArtifact {
            schema_version: MODEL_SCHEMA_VERSION,
            model_id: "synthetic-test-model".to_string(),
            release_id: "synthetic-test-release".to_string(),
            band_nm: [STARLIGHT_BAND_MIN_NM, STARLIGHT_BAND_MAX_NM],
            training_data_sha256: "1".repeat(64),
            calibration_inputs_sha256: vec!["2".repeat(64)],
            split: SpatialSplitSpec {
                algorithm: "splitmix64_spatial_cell_v1".to_string(),
                seed: 42,
                spatial_nside: 8,
                train_buckets: vec![0, 1, 2, 3, 4, 5],
                validation_buckets: vec![6, 7],
                test_buckets: vec![8, 9],
                bucket_modulus: 10,
            },
            branches: vec![
                branch(
                    PhotometryBranch::GBpRpColour,
                    vec![
                        transform("ln_g_flux", -10.0, 30.0),
                        transform("bp_rp", -2.0, 8.0),
                    ],
                ),
                branch(
                    PhotometryBranch::PartialColour,
                    vec![transform("ln_g_flux", -10.0, 30.0)],
                ),
                branch(
                    PhotometryBranch::GOnly,
                    vec![transform("ln_g_flux", -10.0, 30.0)],
                ),
            ],
            uv_corrections: vec![
                UvCorrectionModel {
                    branch: PhotometryBranch::GBpRpColour,
                    features: vec![transform("bp_rp", -2.0, 8.0)],
                    coefficients: vec![-2.0, 0.0],
                    maximum_ratio: 0.8,
                    residual_fractional_sigma: 0.10,
                    systematic_fractional_sigma: 0.05,
                    validation: metrics(),
                },
                UvCorrectionModel {
                    branch: PhotometryBranch::PartialColour,
                    features: vec![transform("ln_g_flux", -10.0, 30.0)],
                    coefficients: vec![-2.0, 0.0],
                    maximum_ratio: 0.8,
                    residual_fractional_sigma: 0.15,
                    systematic_fractional_sigma: 0.10,
                    validation: metrics(),
                },
                UvCorrectionModel {
                    branch: PhotometryBranch::GOnly,
                    features: vec![transform("ln_g_flux", -10.0, 30.0)],
                    coefficients: vec![-2.0, 0.0],
                    maximum_ratio: 0.8,
                    residual_fractional_sigma: 0.25,
                    systematic_fractional_sigma: 0.20,
                    validation: metrics(),
                },
            ],
            selection_function: SelectionFunctionModel {
                healpix_nside: 1,
                maximum_inverse_completeness_weight: 5.0,
                cells: vec![
                    CompletenessCell {
                        healpix_index: 0,
                        g_min: 0.0,
                        g_max: 25.0,
                        colour_min: None,
                        colour_max: None,
                        completeness: 0.8,
                        statistical_sigma: 0.01,
                        systematic_sigma: 0.02,
                        faint_tail_fraction: 0.03,
                        crowding_flag: false,
                    },
                    CompletenessCell {
                        healpix_index: 1,
                        g_min: 0.0,
                        g_max: 25.0,
                        colour_min: None,
                        colour_max: None,
                        completeness: 0.4,
                        statistical_sigma: 0.02,
                        systematic_sigma: 0.05,
                        faint_tail_fraction: 0.10,
                        crowding_flag: true,
                    },
                ],
                reference: "synthetic test reference".to_string(),
                reference_version: "v1".to_string(),
                license: "test-only".to_string(),
            },
            absolute_calibration_fractional_uncertainty: 0.01,
        }
    }

    fn full_features() -> PhotometryFeatures {
        PhotometryFeatures {
            g_flux_e_s: Some(1000.0),
            bp_flux_e_s: Some(800.0),
            rp_flux_e_s: Some(1200.0),
            g_mag: Some(15.0),
            bp_rp: Some(1.0),
            bp_rp_excess: Some(1.2),
            g_flux_over_error: Some(100.0),
            bp_flux_over_error: Some(80.0),
            rp_flux_over_error: Some(90.0),
            galactic_lon_deg: 10.0,
            galactic_lat_deg: -20.0,
            extinction_proxy_mag: Some(0.3),
            crowding_proxy: Some(0.1),
        }
    }

    #[test]
    fn branch_selection_is_explicit_and_degrades_information() {
        let mut features = full_features();
        assert_eq!(features.branch(), PhotometryBranch::GBpRpColour);
        features.bp_flux_e_s = None;
        features.bp_rp = None;
        assert_eq!(features.branch(), PhotometryBranch::PartialColour);
        features.rp_flux_e_s = None;
        assert_eq!(features.branch(), PhotometryBranch::GOnly);
        features.g_flux_e_s = None;
        assert_eq!(features.branch(), PhotometryBranch::NoUsablePhotometry);
    }

    #[test]
    fn exact_band_and_all_branches_are_required() {
        let mut model = artifact();
        model.validate(true).unwrap();
        model.band_nm[0] = 336.0;
        assert!(model
            .validate(false)
            .unwrap_err()
            .to_string()
            .contains("300-650"));
        let mut model = artifact();
        model.branches.pop();
        assert!(model
            .validate(false)
            .unwrap_err()
            .to_string()
            .contains("required branch"));
    }

    #[test]
    fn photometric_prediction_separates_uv_and_correlated_systematics() {
        let model = artifact();
        let prediction = model.predict_photometric_source(&full_features()).unwrap();
        assert_eq!(prediction.photometry_branch, PhotometryBranch::GBpRpColour);
        assert!(prediction.flux_300_336_ph_m2_s.unwrap() > 0.0);
        assert!(
            prediction.flux_300_650_ph_m2_s.unwrap() > prediction.flux_336_650_ph_m2_s.unwrap()
        );
        assert!(
            prediction.total_uncertainty_ph_m2_s.unwrap()
                >= prediction.statistical_uncertainty_ph_m2_s.unwrap()
        );
        assert!(
            prediction.upper_bound_ph_m2_s.unwrap() >= prediction.flux_300_650_ph_m2_s.unwrap()
        );
    }

    #[test]
    fn no_photometry_has_no_false_point_estimate() {
        let model = artifact();
        let mut features = full_features();
        features.g_flux_e_s = None;
        features.bp_flux_e_s = None;
        features.rp_flux_e_s = None;
        features.bp_rp = None;
        let prediction = model.predict_photometric_source(&features).unwrap();
        assert_eq!(
            prediction.photometry_branch,
            PhotometryBranch::NoUsablePhotometry
        );
        assert!(prediction.flux_300_650_ph_m2_s.is_none());
        assert!(prediction.upper_bound_ph_m2_s.is_none());
        assert!(prediction.extrapolated);
    }

    #[test]
    fn completeness_is_spatial_bounded_and_nonnegative() {
        let selection = &artifact().selection_function;
        selection.validate().unwrap();
        let sparse = selection.correct_flux(1, 18.0, Some(1.0), 100.0).unwrap();
        let clear = selection.correct_flux(0, 18.0, Some(1.0), 100.0).unwrap();
        assert!(sparse.correction_ph_m2_s > clear.correction_ph_m2_s);
        assert!(sparse.inverse_weight <= selection.maximum_inverse_completeness_weight);
        assert!(sparse.total_uncertainty_ph_m2_s >= sparse.systematic_uncertainty_ph_m2_s);
        assert!(sparse.crowding_flag);

        let mut global = selection.clone();
        global.cells.truncate(1);
        assert!(global
            .validate()
            .unwrap_err()
            .to_string()
            .contains("single global"));
    }

    #[test]
    fn spatial_split_never_leaks_one_cell_between_partitions() {
        let split = &artifact().split;
        for cell in 0..1000 {
            assert_eq!(
                split.partition(cell).unwrap(),
                split.partition(cell).unwrap()
            );
        }
        let represented: BTreeSet<_> = (0..10_000)
            .map(|cell| split.partition(cell).unwrap())
            .map(|partition| format!("{partition:?}"))
            .collect();
        assert_eq!(represented.len(), 3);
    }

    #[test]
    fn failed_held_out_metrics_block_production_validation() {
        let mut model = artifact();
        model.branches[0]
            .validation
            .absolute_error_percentiles
            .insert("p95".to_string(), 0.2);
        assert!(model
            .validate(true)
            .unwrap_err()
            .to_string()
            .contains("fails source gates"));
        model.validate(false).unwrap();
    }

    #[test]
    fn validation_metrics_keep_outliers_and_measure_coverage() {
        let samples = vec![
            ResidualSample {
                observed_photon_flux: 100.0,
                predicted_photon_flux: 101.0,
                predicted_one_sigma: 2.0,
            },
            ResidualSample {
                observed_photon_flux: 100.0,
                predicted_photon_flux: 99.0,
                predicted_one_sigma: 2.0,
            },
            ResidualSample {
                observed_photon_flux: 100.0,
                predicted_photon_flux: 150.0,
                predicted_one_sigma: 2.0,
            },
        ];
        let result = compute_validation_metrics(&samples, 1.0).unwrap();
        assert_eq!(result.samples, 3);
        assert_eq!(result.absolute_error_percentiles.len(), 5);
        assert!(result.absolute_error_percentiles["p99"] > 0.45);
        assert!(result.rmse_fraction > result.mae_fraction);
        assert_eq!(result.interval_68_coverage, 2.0 / 3.0);
        assert_eq!(result.interval_95_coverage, 2.0 / 3.0);
    }

    #[test]
    fn deterministic_fit_uses_disjoint_spatial_validation() {
        let make = |source_id: u64, spatial_cell: u64, g_flux: f64| {
            let mut features = full_features();
            features.g_flux_e_s = Some(g_flux);
            let target = (2.0 + 0.5 * g_flux.ln()).exp();
            ModelFitSample {
                source_id,
                spatial_cell,
                features,
                target,
                target_one_sigma: target * 0.02,
            }
        };
        let training: Vec<_> = (0..16)
            .map(|index| make(index + 1, index + 100, 10.0 + index as f64))
            .collect();
        let validation: Vec<_> = (0..6)
            .map(|index| make(index + 1000, index + 1000, 11.5 + index as f64 * 2.0))
            .collect();
        let fitted = fit_branch_model(
            &training,
            &validation,
            PhotometryBranch::GBpRpColour,
            &["ln_g_flux".to_string()],
            1.0e-8,
            0.01,
            2.0,
        )
        .unwrap();
        assert!(fitted.validation.mean_bias_fraction.abs() < 1.0e-6);
        assert!(fitted.validation.absolute_error_percentiles["p95"] < 1.0e-6);

        let mut leaked = validation.clone();
        leaked[0].spatial_cell = training[0].spatial_cell;
        assert!(fit_branch_model(
            &training,
            &leaked,
            PhotometryBranch::GBpRpColour,
            &["ln_g_flux".to_string()],
            1.0e-8,
            0.01,
            2.0,
        )
        .unwrap_err()
        .to_string()
        .contains("leakage"));
    }
}
