//! In-process Gaia DR3 XP continuous calibration (GaiaXPy 2.1.4 parity subset).
//!
//! Uses precomputed design matrices exported from pinned GaiaXPy config (v375wi / v142r)
//! and the NSB 336–650 nm @ 2 nm sampling grid.

use crate::gaia_xp::{integrate_photon_flux, XpProduct};
use crate::gaia_xp_continuous_canonical::CanonicalXpContinuousRecord;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const BP_WL_HIGH: f64 = 643.0;
const RP_WL_LOW: f64 = 635.0;

/// Calibrated 336–650 nm photon flux for one XP continuous source.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuousCalibratedFlux {
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
}

#[derive(Debug, Clone)]
pub struct GaiaXpContinuousCalibrator {
    sampling_nm: Vec<f64>,
    merge_bp: Vec<f64>,
    merge_rp: Vec<f64>,
    design_bp: Vec<Vec<f64>>,
    design_rp: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
struct DesignFixture {
    schema_version: u32,
    sampling_nm: Vec<f64>,
    merge_bp: Vec<f64>,
    merge_rp: Vec<f64>,
    design_bp: Vec<Vec<f64>>,
    design_rp: Vec<Vec<f64>>,
}

impl GaiaXpContinuousCalibrator {
    pub fn from_design_fixture(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read GaiaXPy design fixture {}", path.display()))?;
        let fixture: DesignFixture = serde_json::from_str(&text)?;
        if fixture.schema_version != 1 {
            bail!(
                "unsupported design fixture schema {}",
                fixture.schema_version
            );
        }
        Self::from_parts(
            fixture.sampling_nm,
            fixture.merge_bp,
            fixture.merge_rp,
            fixture.design_bp,
            fixture.design_rp,
        )
    }

    pub fn default_fixture_path() -> &'static Path {
        Path::new(
            "crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json",
        )
    }

    pub fn resolve_design_fixture_path(
        explicit: Option<&Path>,
        gaiaxpy_environment: Option<&Path>,
    ) -> PathBuf {
        if let Some(path) = explicit {
            return path.to_path_buf();
        }
        if let Ok(path) = std::env::var("STARLIGHT_GAIAXPY_DESIGN_FIXTURE") {
            return PathBuf::from(path);
        }
        if let Some(env_path) = gaiaxpy_environment {
            let sibling = env_path
                .parent()
                .map(|parent| parent.join("gaiaxpy_continuous_design_v375wi_v142r.json"));
            if let Some(path) = sibling {
                if path.is_file() {
                    return path;
                }
            }
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json")
    }

    fn from_parts(
        sampling_nm: Vec<f64>,
        merge_bp: Vec<f64>,
        merge_rp: Vec<f64>,
        design_bp: Vec<Vec<f64>>,
        design_rp: Vec<Vec<f64>>,
    ) -> Result<Self> {
        let n = sampling_nm.len();
        if merge_bp.len() != n || merge_rp.len() != n {
            bail!("merge weights length mismatch with sampling grid");
        }
        for (label, design) in [("bp", &design_bp), ("rp", &design_rp)] {
            if design.is_empty() {
                bail!("{label} design matrix is empty");
            }
            if design[0].len() != n {
                bail!("{label} design matrix column count mismatch");
            }
        }
        Ok(Self {
            sampling_nm,
            merge_bp,
            merge_rp,
            design_bp,
            design_rp,
        })
    }

    pub fn calibrate_record(
        &self,
        record: &CanonicalXpContinuousRecord,
    ) -> Result<ContinuousCalibratedFlux> {
        let product = self.calibrate_record_product(record)?;
        let integral = integrate_photon_flux(&product)?;
        let uncertainty = integrate_gaiaxpy_manifest_uncertainty(
            &product.wavelengths_nm,
            product.flux_error_w_m2_nm.as_ref().expect("flux errors"),
        )?;
        Ok(ContinuousCalibratedFlux {
            flux_336_650_ph_m2_s: integral.total_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s: uncertainty,
        })
    }

    /// Calibrated 336–650 nm spectrum samples (W m⁻² nm⁻¹) for normalized CSV export.
    pub fn calibrate_record_product(
        &self,
        record: &CanonicalXpContinuousRecord,
    ) -> Result<XpProduct> {
        record.validate()?;
        let bp = self.calibrate_band(
            record.bp_n_parameters,
            &record.bp_coefficients,
            &record.bp_coefficient_errors,
            &record.bp_coefficient_correlations,
            record.bp_standard_deviation,
            &self.design_bp,
            &self.merge_bp,
        )?;
        let rp = self.calibrate_band(
            record.rp_n_parameters,
            &record.rp_coefficients,
            &record.rp_coefficient_errors,
            &record.rp_coefficient_correlations,
            record.rp_standard_deviation,
            &self.design_rp,
            &self.merge_rp,
        )?;
        let (flux_w, err_w) =
            merge_bp_rp(&bp, &rp, &self.sampling_nm, &self.merge_bp, &self.merge_rp)?;
        Ok(XpProduct {
            source_id: record.source_id.clone(),
            wavelengths_nm: self.sampling_nm.clone(),
            flux_w_m2_nm: flux_w,
            flux_error_w_m2_nm: Some(err_w),
        })
    }

    fn calibrate_band(
        &self,
        n_parameters: usize,
        coefficients: &[f64],
        errors: &[f64],
        correlations: &[f64],
        standard_deviation: f64,
        design: &[Vec<f64>],
        _merge: &[f64],
    ) -> Result<BandSpectrum> {
        if n_parameters == 0 || coefficients.is_empty() {
            return Ok(BandSpectrum::missing());
        }
        if coefficients.len() != n_parameters
            || errors.len() != n_parameters
            || correlations.len() != n_parameters * (n_parameters - 1) / 2
        {
            bail!("band coefficient/error/correlation dimension mismatch");
        }
        let covariance =
            correlation_to_covariance_dr3int5(correlations, errors, standard_deviation)?;
        let n_bases = design.len();
        if n_bases != n_parameters {
            bail!("coefficient count {n_parameters} != design row count {n_bases}");
        }
        let flux = mat_vec_mul_rows(coefficients, design);
        let error = sample_error(&covariance, design, standard_deviation)?;
        Ok(BandSpectrum {
            flux,
            error,
            present: true,
        })
    }
}

#[derive(Debug, Clone)]
struct BandSpectrum {
    flux: Vec<f64>,
    error: Vec<f64>,
    present: bool,
}

impl BandSpectrum {
    fn missing() -> Self {
        Self {
            flux: Vec::new(),
            error: Vec::new(),
            present: false,
        }
    }
}

fn merge_bp_rp(
    bp: &BandSpectrum,
    rp: &BandSpectrum,
    sampling_nm: &[f64],
    merge_bp: &[f64],
    merge_rp: &[f64],
) -> Result<(Vec<f64>, Vec<f64>)> {
    let n = merge_bp.len();
    if n != merge_rp.len() || n != sampling_nm.len() {
        bail!("merge/sampling length mismatch");
    }
    if bp.present && rp.present {
        let mut flux = vec![0.0; n];
        let mut err2 = vec![0.0; n];
        for i in 0..n {
            flux[i] = bp.flux[i] * merge_bp[i] + rp.flux[i] * merge_rp[i];
            err2[i] = (bp.error[i] * merge_bp[i]).powi(2) + (rp.error[i] * merge_rp[i]).powi(2);
        }
        let error: Vec<f64> = err2.into_iter().map(f64::sqrt).collect();
        return Ok((flux, error));
    }
    if bp.present ^ rp.present {
        let (band, existing) = if bp.present { (bp, "bp") } else { (rp, "rp") };
        let mut flux = band.flux.clone();
        let mut error = band.error.clone();
        for (i, wl) in sampling_nm.iter().enumerate() {
            let masked = if existing == "rp" {
                *wl <= RP_WL_LOW
            } else {
                *wl >= BP_WL_HIGH
            };
            if masked {
                flux[i] = f64::NAN;
                error[i] = f64::NAN;
            }
        }
        return Ok((flux, error));
    }
    bail!("no usable BP/RP band for calibration")
}

fn mat_vec_mul_rows(coefficients: &[f64], design: &[Vec<f64>]) -> Vec<f64> {
    let n_samples = design[0].len();
    let mut out = vec![0.0; n_samples];
    for (coef, row) in coefficients.iter().zip(design.iter()) {
        for (j, sample) in row.iter().enumerate() {
            out[j] += coef * sample;
        }
    }
    out
}

fn sample_error(
    covariance: &[Vec<f64>],
    design: &[Vec<f64>],
    standard_deviation: f64,
) -> Result<Vec<f64>> {
    let n_samples = design[0].len();
    let n_bases = design.len();
    let mut out = vec![0.0; n_samples];
    for j in 0..n_samples {
        let mut sum = 0.0;
        for k in 0..n_bases {
            let mut acc = 0.0;
            for i in 0..n_bases {
                acc += design[i][j] * covariance[i][k];
            }
            sum += acc * design[k][j];
        }
        out[j] = sum.sqrt() * standard_deviation;
    }
    Ok(out)
}

pub fn integrate_gaiaxpy_manifest_uncertainty(
    wavelengths_nm: &[f64],
    flux_error_w_m2_nm: &[f64],
) -> Result<f64> {
    if wavelengths_nm.len() != flux_error_w_m2_nm.len() || wavelengths_nm.len() < 2 {
        bail!("wavelength/flux_error length mismatch for uncertainty integration");
    }
    let c_m_s = 299_792_458.0;
    let hc_j_m = 6.626_070_15e-34 * c_m_s;
    let photon_unc: Vec<f64> = wavelengths_nm
        .iter()
        .zip(flux_error_w_m2_nm.iter())
        .map(|(wl_nm, err)| err * (wl_nm * 1e-9) / hc_j_m)
        .collect();
    let mut integral = 0.0;
    for index in 0..wavelengths_nm.len() - 1 {
        let width_nm = wavelengths_nm[index + 1] - wavelengths_nm[index];
        integral += 0.5 * (photon_unc[index] + photon_unc[index + 1]) * width_nm;
    }
    Ok(integral)
}

#[allow(clippy::needless_range_loop)]
fn packed_correlation_to_matrix(packed: &[f64], n: usize) -> Vec<Vec<f64>> {
    // Mirror GaiaXPy `array_to_symmetric_matrix` (packed lower triangle, k=-1).
    let mut matrix = vec![vec![0.0; n]; n];
    for i in 0..n {
        matrix[i][i] = 1.0;
    }
    let mut idx = 0;
    for row in 1..n {
        for col in 0..row {
            matrix[row][col] = packed[idx];
            idx += 1;
        }
    }
    let mut transpose = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            transpose[i][j] = matrix[j][i];
        }
    }
    for row in 1..n {
        for col in 0..row {
            transpose[row][col] = matrix[row][col];
        }
    }
    transpose
}

fn correlation_to_covariance_dr3int5(
    packed_correlations: &[f64],
    formal_errors: &[f64],
    standard_deviation: f64,
) -> Result<Vec<Vec<f64>>> {
    let n = formal_errors.len();
    if !standard_deviation.is_finite() || standard_deviation <= 0.0 {
        bail!("invalid standard deviation for covariance reconstruction");
    }
    let correlation = packed_correlation_to_matrix(packed_correlations, n);
    let mut diag_inv = vec![vec![0.0; n]; n];
    for i in 0..n {
        diag_inv[i][i] = formal_errors[i] / standard_deviation;
    }
    Ok(mat_mul(&diag_inv, &mat_mul(&correlation, &diag_inv)))
}

fn mat_mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let m = b[0].len();
    let mut out = vec![vec![0.0; m]; n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i][k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..m {
                out[i][j] += aik * b[k][j];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json")
    }

    #[test]
    fn loads_design_fixture() -> Result<()> {
        let cal = GaiaXpContinuousCalibrator::from_design_fixture(&fixture_path())?;
        assert_eq!(cal.sampling_nm.len(), 158);
        assert_eq!(cal.design_bp.len(), 55);
        Ok(())
    }
}
