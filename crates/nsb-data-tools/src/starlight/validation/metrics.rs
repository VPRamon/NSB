//! Deterministic, dependency-free comparison metrics for independent validation.
//!
//! These helpers compare a candidate sample against an independently sourced
//! reference sample of equal length and report a fixed, versioned metric
//! vocabulary. They never fabricate values: every function fails closed on
//! empty, mismatched-length, or non-finite input.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// One-sigma and two-sigma-equivalent Gaussian coverage fractions used to
/// classify residuals against declared per-pixel uncertainty.
const COVERAGE_68_Z: f64 = 1.0;
const COVERAGE_95_Z: f64 = 1.959_963_984_540_054;
/// Residuals beyond this many declared sigma are counted as catastrophic
/// outliers. This is a fixed, documented convention, not a fitted value.
const OUTLIER_Z: f64 = 5.0;

/// Fixed vocabulary of comparison metrics computed for one candidate/reference
/// sample (all-sky or one region).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsSummary {
    /// Mean of `candidate - reference`.
    pub signed_bias: f64,
    /// Absolute value of `signed_bias`.
    pub absolute_bias: f64,
    /// `signed_bias` divided by the mean reference value.
    pub relative_bias: f64,
    /// Mean absolute residual.
    pub mae: f64,
    /// Median absolute residual.
    pub median_absolute_error: f64,
    /// Root-mean-square residual.
    pub rmse: f64,
    pub relative_error_p50: f64,
    pub relative_error_p68: f64,
    pub relative_error_p95: f64,
    /// Fraction of samples whose residual is within one declared sigma.
    pub coverage_68: f64,
    /// Fraction of samples whose residual is within the 95% Gaussian z-score
    /// times the declared sigma.
    pub coverage_95: f64,
    /// Fraction of samples whose residual exceeds `OUTLIER_Z` declared sigma.
    pub outlier_fraction: f64,
    pub sample_count: u64,
}

/// Compare a candidate sample against a reference sample with declared
/// candidate statistical uncertainty. All three slices must have equal,
/// non-zero length and contain only finite values; reference values must be
/// strictly positive because several metrics are normalized by them.
pub fn compute(
    candidate: &[f64],
    reference: &[f64],
    statistical_uncertainty: &[f64],
) -> Result<MetricsSummary> {
    if candidate.is_empty() {
        bail!("cannot compute validation metrics over an empty sample");
    }
    if candidate.len() != reference.len() || candidate.len() != statistical_uncertainty.len() {
        bail!(
            "candidate, reference, and uncertainty samples must have equal length: {} vs {} vs {}",
            candidate.len(),
            reference.len(),
            statistical_uncertainty.len()
        );
    }
    for (label, values) in [
        ("candidate", candidate),
        ("reference", reference),
        ("statistical uncertainty", statistical_uncertainty),
    ] {
        if values.iter().any(|value| !value.is_finite()) {
            bail!("{label} sample contains a non-finite value");
        }
    }
    if reference.iter().any(|value| *value <= 0.0) {
        bail!("reference sample must be strictly positive for relative validation metrics");
    }
    if statistical_uncertainty.iter().any(|value| *value < 0.0) {
        bail!("statistical uncertainty sample must be non-negative");
    }

    let n = candidate.len();
    let residuals = candidate
        .iter()
        .zip(reference)
        .map(|(value, reference)| value - reference)
        .collect::<Vec<_>>();

    let signed_bias = mean(&residuals);
    let absolute_bias = signed_bias.abs();
    let mean_reference = mean(reference);
    let relative_bias = signed_bias / mean_reference;

    let absolute_residuals = residuals
        .iter()
        .map(|value| value.abs())
        .collect::<Vec<_>>();
    let mae = mean(&absolute_residuals);
    let median_absolute_error = median(&absolute_residuals);
    let rmse = mean(
        &residuals
            .iter()
            .map(|value| value.powi(2))
            .collect::<Vec<_>>(),
    )
    .sqrt();

    let relative_errors = residuals
        .iter()
        .zip(reference)
        .map(|(residual, reference)| (residual / reference).abs())
        .collect::<Vec<_>>();
    let relative_error_p50 = percentile(&relative_errors, 0.50);
    let relative_error_p68 = percentile(&relative_errors, 0.68);
    let relative_error_p95 = percentile(&relative_errors, 0.95);

    let mut within_68 = 0_u64;
    let mut within_95 = 0_u64;
    let mut outliers = 0_u64;
    for (residual, sigma) in residuals.iter().zip(statistical_uncertainty) {
        if *sigma > 0.0 {
            let z = residual.abs() / sigma;
            if z <= COVERAGE_68_Z {
                within_68 += 1;
            }
            if z <= COVERAGE_95_Z {
                within_95 += 1;
            }
            if z > OUTLIER_Z {
                outliers += 1;
            }
        } else if *residual == 0.0 {
            // A declared-exact (zero-uncertainty) pixel with a zero residual
            // trivially falls inside every coverage interval.
            within_68 += 1;
            within_95 += 1;
        } else {
            outliers += 1;
        }
    }
    let sample_count = u64::try_from(n).map_err(|_| anyhow::anyhow!("sample count exceeds u64"))?;

    Ok(MetricsSummary {
        signed_bias,
        absolute_bias,
        relative_bias,
        mae,
        median_absolute_error,
        rmse,
        relative_error_p50,
        relative_error_p68,
        relative_error_p95,
        coverage_68: within_68 as f64 / n as f64,
        coverage_95: within_95 as f64 / n as f64,
        outlier_fraction: outliers as f64 / n as f64,
        sample_count,
    })
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Nearest-rank percentile over `[0, 1]` with deterministic tie-breaking.
/// This is an intentionally simple convention documented here rather than a
/// scientific choice; it is stable and reproducible across runs.
fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((fraction * sorted.len() as f64).ceil() as usize)
        .max(1)
        .min(sorted.len());
    sorted[rank - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_samples_are_perfectly_unbiased_and_fully_covered() {
        let candidate = [10.0, 20.0, 30.0, 40.0];
        let reference = [10.0, 20.0, 30.0, 40.0];
        let sigma = [1.0, 1.0, 1.0, 1.0];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert_eq!(summary.signed_bias, 0.0);
        assert_eq!(summary.absolute_bias, 0.0);
        assert_eq!(summary.relative_bias, 0.0);
        assert_eq!(summary.mae, 0.0);
        assert_eq!(summary.median_absolute_error, 0.0);
        assert_eq!(summary.rmse, 0.0);
        assert_eq!(summary.relative_error_p50, 0.0);
        assert_eq!(summary.coverage_68, 1.0);
        assert_eq!(summary.coverage_95, 1.0);
        assert_eq!(summary.outlier_fraction, 0.0);
        assert_eq!(summary.sample_count, 4);
    }

    #[test]
    fn constant_offset_produces_exact_signed_and_relative_bias() {
        let reference = [100.0, 200.0, 300.0, 400.0];
        let candidate = reference.map(|value| value + 10.0);
        let sigma = [10.0, 10.0, 10.0, 10.0];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert_eq!(summary.signed_bias, 10.0);
        assert_eq!(summary.absolute_bias, 10.0);
        assert!((summary.relative_bias - 10.0 / 250.0).abs() < 1.0e-12);
        assert_eq!(summary.mae, 10.0);
        assert_eq!(summary.median_absolute_error, 10.0);
        assert_eq!(summary.rmse, 10.0);
    }

    #[test]
    fn coverage_uses_declared_sigma_thresholds() {
        // Both residuals sit exactly at one declared sigma, so both fall
        // inside the (inclusive) 68% and 95% coverage intervals.
        let candidate = [11.0, 9.0];
        let reference = [10.0, 10.0];
        let sigma = [1.0, 1.0];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert_eq!(summary.coverage_68, 1.0);
        assert_eq!(summary.coverage_95, 1.0);
        assert_eq!(summary.outlier_fraction, 0.0);

        // Residual of 6 sigma is a declared catastrophic outlier.
        let candidate = [16.0];
        let reference = [10.0];
        let sigma = [1.0];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert_eq!(summary.coverage_68, 0.0);
        assert_eq!(summary.coverage_95, 0.0);
        assert_eq!(summary.outlier_fraction, 1.0);
    }

    #[test]
    fn median_absolute_error_is_robust_to_a_single_large_residual() {
        let reference = [10.0, 10.0, 10.0, 10.0, 10.0];
        let candidate = [10.0, 10.0, 10.0, 10.0, 1000.0];
        let sigma = [1.0, 1.0, 1.0, 1.0, 1.0];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert_eq!(summary.median_absolute_error, 0.0);
        assert!(summary.mae > 100.0);
        assert!(summary.rmse > summary.mae);
    }

    #[test]
    fn rejects_empty_mismatched_or_non_finite_input() {
        assert!(compute(&[], &[], &[]).is_err());
        assert!(compute(&[1.0], &[1.0, 2.0], &[1.0, 1.0]).is_err());
        assert!(compute(&[f64::NAN], &[1.0], &[1.0]).is_err());
        assert!(compute(&[1.0], &[0.0], &[1.0]).is_err());
        assert!(compute(&[1.0], &[1.0], &[-1.0]).is_err());
    }

    #[test]
    fn percentiles_are_monotonic_and_bounded_by_the_sample() {
        let reference = [10.0; 10];
        let mut candidate = reference;
        for (index, value) in candidate.iter_mut().enumerate() {
            *value += index as f64;
        }
        let sigma = [1.0; 10];
        let summary = compute(&candidate, &reference, &sigma).unwrap();
        assert!(summary.relative_error_p50 <= summary.relative_error_p68);
        assert!(summary.relative_error_p68 <= summary.relative_error_p95);
        assert!(summary.relative_error_p95 <= 0.9 + 1.0e-9);
    }
}
