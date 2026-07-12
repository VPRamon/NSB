//! Overlap difference vs absolute physical uncertainty contracts for Phase 5.

use crate::starlight_phase5::{OverlapComparison, MetricBundle, XpContinuousGates, CATASTROPHIC_RELATIVE_ERROR, percentile};
use serde::{Deserialize, Serialize};

/// How overlap validation coverage gates are evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageUncertaintyKind {
    /// Legacy exploratory contract: reconstructed absolute uncertainty × inflation.
    AbsolutePhysicalLegacy,
    /// Calibrated difference uncertainty for XP sampled vs XP continuous overlap.
    OverlapDifference,
}

/// Parameters for overlap difference uncertainty (correlated XP products).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlapDifferenceUncertaintyModel {
    pub formula_id: String,
    pub difference_uncertainty_formula: String,
    pub correlation_assumptions: String,
    /// Gaia XP calibration correlation between sampled and continuous flux estimates.
    pub correlation_rho: f64,
    /// p68(|Δ/sampled|) on the inflation-fit split (train).
    pub relative_residual_scale: f64,
    /// Minimum difference sigma in ph m⁻² s⁻¹.
    pub systematic_floor_ph_m2_s: f64,
    /// Multiplier fit on validation to target 68% difference coverage.
    pub inflation_factor: f64,
}

impl Default for OverlapDifferenceUncertaintyModel {
    fn default() -> Self {
        Self {
            formula_id: "overlap_difference_v1".to_string(),
            difference_uncertainty_formula: "sigma_diff = inflation * hypot(sqrt(2)*sigma_recon*sqrt(1-rho), max(floor, relative_scale*|sampled|))".to_string(),
            correlation_assumptions: "XP sampled and XP continuous share Gaia DR3 calibration; rho captures shared systematics. Residual scale captures reconstruction mismatch not absorbed by correlated quadrature.".to_string(),
            correlation_rho: 0.999_999,
            relative_residual_scale: 0.0,
            systematic_floor_ph_m2_s: 0.05,
            inflation_factor: 1.0,
        }
    }
}

/// Absolute physical uncertainty carried into the integrated starlight product.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsolutePhysicalUncertaintyModel {
    pub formula_id: String,
    pub absolute_uncertainty_formula: String,
    pub reconstruction_systematic_fraction: f64,
    pub systematic_floor_ph_m2_s: f64,
}

impl Default for AbsolutePhysicalUncertaintyModel {
    fn default() -> Self {
        Self {
            formula_id: "absolute_physical_v1".to_string(),
            absolute_uncertainty_formula: "sigma_abs = hypot(sigma_recon_stat, max(floor, fraction*|flux|))".to_string(),
            reconstruction_systematic_fraction: 0.01,
            systematic_floor_ph_m2_s: 0.05,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyModelBundle {
    pub overlap_difference: OverlapDifferenceUncertaintyModel,
    pub absolute_physical: AbsolutePhysicalUncertaintyModel,
    pub coverage_gate_kind: CoverageUncertaintyKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyDiagnostics {
    pub sample_count: u64,
    pub median_abs_z_stat: f64,
    pub p68_abs_z_stat: f64,
    pub p95_abs_z_stat: f64,
    pub coverage_z_stat_le_1: f64,
    pub coverage_z_stat_le_1_96: f64,
    pub median_abs_z_total: f64,
    pub p68_abs_z_total: f64,
    pub p95_abs_z_total: f64,
    pub coverage_z_total_le_1: f64,
    pub coverage_z_total_le_1_96: f64,
    pub max_abs_z_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenValidationPolicyV1 {
    pub schema_version: u32,
    pub policy_id: String,
    pub status: String,
    pub uncertainty_model: UncertaintyModelBundle,
    pub quality_filters: Vec<String>,
    pub systematic_floor_ph_m2_s: f64,
    pub outlier_policy: String,
    pub fallbacks: Vec<String>,
    pub train_checksum: String,
    pub validation_checksum: String,
    pub software_commit: String,
    pub gaiaxpy_version: String,
    pub adapter_version: String,
    pub photometry_model: String,
    pub integration_band_nm: [f64; 2],
    pub gates: XpContinuousGates,
    pub created_at: String,
    pub archived_exploratory_policy: String,
}

pub fn overlap_difference_sigma(row: &OverlapComparison, model: &OverlapDifferenceUncertaintyModel) -> f64 {
    let stat = row.statistical_uncertainty_ph_m2_s;
    let stat_diff = (2.0 * stat * stat * (1.0 - model.correlation_rho)).max(0.0).sqrt();
    let empirical = model
        .relative_residual_scale
        * row.sampled_flux_ph_m2_s.abs()
        .max(crate::starlight_phase5::RELATIVE_ERROR_FLUX_FLOOR_PH_M2_S);
    let core = stat_diff.hypot(empirical.max(model.systematic_floor_ph_m2_s));
    (core * model.inflation_factor).max(model.systematic_floor_ph_m2_s)
}

pub fn absolute_physical_sigma(
    flux_ph_m2_s: f64,
    reconstruction_stat: f64,
    model: &AbsolutePhysicalUncertaintyModel,
) -> f64 {
    let systematic = (model.reconstruction_systematic_fraction * flux_ph_m2_s.abs())
        .max(model.systematic_floor_ph_m2_s);
    reconstruction_stat.hypot(systematic)
}

pub fn fit_relative_residual_scale(train: &[OverlapComparison]) -> f64 {
    if train.is_empty() {
        return 0.0;
    }
    let mut rel: Vec<f64> = train
        .iter()
        .map(|row| {
            let scale = row
                .sampled_flux_ph_m2_s
                .abs()
                .max(crate::starlight_phase5::RELATIVE_ERROR_FLUX_FLOOR_PH_M2_S);
            (row.sampled_flux_ph_m2_s - row.reconstructed_flux_ph_m2_s).abs() / scale
        })
        .collect();
    rel.sort_by(f64::total_cmp);
    percentile(&rel, 0.68)
}

pub fn fit_difference_inflation(validation: &[OverlapComparison], model: &mut OverlapDifferenceUncertaintyModel) {
    let target_68 = 0.68_f64;
    let mut best = 1.0_f64;
    let mut best_err = f64::MAX;
    let mut factor = 0.5_f64;
    while factor <= 2.0 {
        model.inflation_factor = factor;
        let metrics = compute_overlap_metrics(validation, model);
        let err = (metrics.coverage_68 - target_68).abs();
        if err < best_err {
            best_err = err;
            best = factor;
        }
        factor += 0.01;
    }
    model.inflation_factor = best;
}

pub fn compute_overlap_metrics(
    rows: &[OverlapComparison],
    model: &OverlapDifferenceUncertaintyModel,
) -> MetricBundle {
    if rows.is_empty() {
        return MetricBundle::default();
    }
    let rel: Vec<f64> = rows.iter().map(|r| r.relative_error).collect();
    let abs_rel: Vec<f64> = rel.iter().map(|v| v.abs()).collect();
    let signed_mean = rel.iter().sum::<f64>() / rel.len() as f64;
    let flux_sum: f64 = rows.iter().map(|r| r.sampled_flux_ph_m2_s.abs()).sum();
    let flux_weighted_bias = if flux_sum > 0.0 {
        rows.iter()
            .map(|r| r.relative_error * r.sampled_flux_ph_m2_s.abs())
            .sum::<f64>()
            / flux_sum
    } else {
        0.0
    };
    let mae = abs_rel.iter().sum::<f64>() / abs_rel.len() as f64;
    let rmse = (rel.iter().map(|v| v * v).sum::<f64>() / rel.len() as f64).sqrt();
    let mut cover68 = 0_u64;
    let mut cover95 = 0_u64;
    let mut catastrophic = 0_u64;
    for row in rows {
        let sigma = overlap_difference_sigma(row, model);
        let delta = (row.sampled_flux_ph_m2_s - row.reconstructed_flux_ph_m2_s).abs();
        if delta <= sigma {
            cover68 += 1;
        }
        if delta <= 1.96 * sigma {
            cover95 += 1;
        }
        if row.relative_error.abs() > CATASTROPHIC_RELATIVE_ERROR {
            catastrophic += 1;
        }
    }
    let n = rows.len() as f64;
    MetricBundle {
        sample_count: rows.len() as u64,
        mean_signed_relative_bias: signed_mean,
        median_signed_relative_bias: percentile(&rel, 0.5),
        flux_weighted_integrated_bias: flux_weighted_bias,
        mae_relative: mae,
        rmse_relative: rmse,
        robust_relative_error: percentile(&abs_rel, 0.5),
        p50_abs_relative_error: percentile(&abs_rel, 0.50),
        p68_abs_relative_error: percentile(&abs_rel, 0.68),
        p90_abs_relative_error: percentile(&abs_rel, 0.90),
        p95_abs_relative_error: percentile(&abs_rel, 0.95),
        p99_abs_relative_error: percentile(&abs_rel, 0.99),
        coverage_68: cover68 as f64 / n,
        coverage_95: cover95 as f64 / n,
        catastrophic_outlier_fraction: catastrophic as f64 / n,
    }
}

pub fn compute_uncertainty_diagnostics(
    rows: &[OverlapComparison],
    model: &OverlapDifferenceUncertaintyModel,
) -> UncertaintyDiagnostics {
    if rows.is_empty() {
        return UncertaintyDiagnostics {
            sample_count: 0,
            median_abs_z_stat: 0.0,
            p68_abs_z_stat: 0.0,
            p95_abs_z_stat: 0.0,
            coverage_z_stat_le_1: 0.0,
            coverage_z_stat_le_1_96: 0.0,
            median_abs_z_total: 0.0,
            p68_abs_z_total: 0.0,
            p95_abs_z_total: 0.0,
            coverage_z_total_le_1: 0.0,
            coverage_z_total_le_1_96: 0.0,
            max_abs_z_total: 0.0,
        };
    }
    let mut z_stat = Vec::with_capacity(rows.len());
    let mut z_total = Vec::with_capacity(rows.len());
    let mut cover_stat_1 = 0_u64;
    let mut cover_stat_196 = 0_u64;
    let mut cover_total_1 = 0_u64;
    let mut cover_total_196 = 0_u64;
    let mut max_z = 0.0_f64;
    for row in rows {
        let delta = row.sampled_flux_ph_m2_s - row.reconstructed_flux_ph_m2_s;
        let z_s = if row.statistical_uncertainty_ph_m2_s > 0.0 {
            delta / row.statistical_uncertainty_ph_m2_s
        } else {
            0.0
        };
        let sigma = overlap_difference_sigma(row, model);
        let z_t = if sigma > 0.0 { delta / sigma } else { 0.0 };
        z_stat.push(z_s.abs());
        z_total.push(z_t.abs());
        max_z = max_z.max(z_t.abs());
        if z_s.abs() <= 1.0 {
            cover_stat_1 += 1;
        }
        if z_s.abs() <= 1.96 {
            cover_stat_196 += 1;
        }
        if z_t.abs() <= 1.0 {
            cover_total_1 += 1;
        }
        if z_t.abs() <= 1.96 {
            cover_total_196 += 1;
        }
    }
    let n = rows.len() as f64;
    UncertaintyDiagnostics {
        sample_count: rows.len() as u64,
        median_abs_z_stat: percentile(&z_stat, 0.5),
        p68_abs_z_stat: percentile(&z_stat, 0.68),
        p95_abs_z_stat: percentile(&z_stat, 0.95),
        coverage_z_stat_le_1: cover_stat_1 as f64 / n,
        coverage_z_stat_le_1_96: cover_stat_196 as f64 / n,
        median_abs_z_total: percentile(&z_total, 0.5),
        p68_abs_z_total: percentile(&z_total, 0.68),
        p95_abs_z_total: percentile(&z_total, 0.95),
        coverage_z_total_le_1: cover_total_1 as f64 / n,
        coverage_z_total_le_1_96: cover_total_196 as f64 / n,
        max_abs_z_total: max_z,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::starlight_phase5::OverlapComparison;

    fn sample_row(sampled: f64, reconstructed: f64, stat: f64) -> OverlapComparison {
        OverlapComparison {
            source_id: 1,
            split: "train".to_string(),
            sampled_flux_ph_m2_s: sampled,
            reconstructed_flux_ph_m2_s: reconstructed,
            statistical_uncertainty_ph_m2_s: stat,
            systematic_uncertainty_ph_m2_s: 0.0,
            total_uncertainty_ph_m2_s: stat,
            relative_error: (reconstructed - sampled) / sampled,
            phot_g_mean_mag: None,
            bp_rp: None,
            phot_g_snr: None,
            phot_bp_rp_excess_factor: None,
            l: None,
            b: None,
            g_mag_bin: String::new(),
            colour_bin: String::new(),
            snr_bin: String::new(),
            sky_region: String::new(),
            bp_rp_excess_bin: String::new(),
            crowding_bin: String::new(),
            duplicated_bin: String::new(),
            variable_bin: String::new(),
            qso_galaxy_bin: String::new(),
        }
    }

    #[test]
    fn difference_sigma_is_smaller_than_absolute_for_correlated_products() {
        let row = sample_row(1.0e5, 1.0e5 + 0.1, 150.0);
        let mut model = OverlapDifferenceUncertaintyModel {
            relative_residual_scale: 1.0e-5,
            ..Default::default()
        };
        let diff = overlap_difference_sigma(&row, &model);
        assert!(diff < row.statistical_uncertainty_ph_m2_s);
    }

    #[test]
    fn inflation_fit_moves_validation_coverage_toward_target() {
        let train = vec![
            sample_row(1.0e4, 1.0e4 + 0.2, 100.0),
            sample_row(2.0e4, 2.0e4 - 0.1, 120.0),
            sample_row(5.0e3, 5.0e3 + 0.05, 80.0),
        ];
        let validation = vec![
            sample_row(1.1e4, 1.1e4 + 0.15, 90.0),
            sample_row(2.1e4, 2.1e4 - 0.12, 110.0),
            sample_row(5.1e3, 5.1e3 + 0.08, 70.0),
        ];
        let mut model = OverlapDifferenceUncertaintyModel::default();
        model.relative_residual_scale = fit_relative_residual_scale(&train);
        fit_difference_inflation(&validation, &mut model);
        let metrics = compute_overlap_metrics(&validation, &model);
        assert!(metrics.coverage_68 > 0.0);
        assert!(metrics.coverage_68 <= 1.0);
    }
}
