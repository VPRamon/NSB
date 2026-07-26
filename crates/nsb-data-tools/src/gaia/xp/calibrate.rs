//! In-process Gaia DR3 XP continuous calibration with frozen GaiaXPy 2.1.4 parity.
//!
//! The implementation consumes design matrices exported from the pinned GaiaXPy
//! v375wi/v142r configuration. Runtime and maintainer workflows do not invoke
//! Python, GaiaXPy, Cargo, or sibling executables.

use crate::gaia::xp::canonical::CanonicalXpContinuousRecord;
use crate::gaia::xp::continuous::PINNED_GAIA_XPY_VERSION;
use crate::gaia::xp::sampled::{
    integrate_photon_flux, XpProduct, BAND_MAX_NM, BAND_MIN_NM, XP_SAMPLED_GRID_STEP_NM,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const BP_WL_HIGH: f64 = 643.0;
const RP_WL_LOW: f64 = 635.0;
const BP_MODEL: &str = "v375wi";
const RP_MODEL: &str = "v142r";
const EXPECTED_SAMPLE_COUNT: usize = 158;

/// Calibrated 336–650 nm photon flux for one XP continuous source.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuousCalibratedFlux {
    pub flux_336_650_ph_m2_s: f64,
    pub statistical_uncertainty_336_650_ph_m2_s: f64,
}

/// Frozen, in-process XP continuous calibrator.
#[derive(Debug, Clone)]
pub struct GaiaXpContinuousCalibrator {
    gaiaxpy_version: String,
    bp_model: String,
    rp_model: String,
    sampling_nm: Vec<f64>,
    merge_bp: Vec<f64>,
    merge_rp: Vec<f64>,
    design_bp: Vec<Vec<f64>>,
    design_rp: Vec<Vec<f64>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignFixture {
    schema_version: u32,
    gaiaxpy_version: String,
    bp_model: String,
    rp_model: String,
    band_nm: [f64; 2],
    grid_step_nm: f64,
    sampling_nm: Vec<f64>,
    merge_bp: Vec<f64>,
    merge_rp: Vec<f64>,
    design_bp: Vec<Vec<f64>>,
    design_rp: Vec<Vec<f64>>,
}

impl GaiaXpContinuousCalibrator {
    /// Load and validate a frozen GaiaXPy design-matrix fixture.
    pub fn from_design_fixture(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read GaiaXPy design fixture {}", path.display()))?;
        let fixture: DesignFixture = serde_json::from_str(&text)
            .with_context(|| format!("parse GaiaXPy design fixture {}", path.display()))?;
        if fixture.schema_version != 1 {
            bail!(
                "unsupported design fixture schema {}",
                fixture.schema_version
            );
        }
        if fixture.gaiaxpy_version != PINNED_GAIA_XPY_VERSION {
            bail!(
                "design fixture GaiaXPy version {} does not match pinned {}",
                fixture.gaiaxpy_version,
                PINNED_GAIA_XPY_VERSION
            );
        }
        if fixture.bp_model != BP_MODEL || fixture.rp_model != RP_MODEL {
            bail!(
                "design fixture model mismatch: bp={}, rp={}",
                fixture.bp_model,
                fixture.rp_model
            );
        }
        if fixture.band_nm != [BAND_MIN_NM, BAND_MAX_NM] {
            bail!("design fixture band must be exactly 336–650 nm");
        }
        if fixture.grid_step_nm != XP_SAMPLED_GRID_STEP_NM {
            bail!("design fixture grid step must be exactly 2 nm");
        }
        Self::from_parts(
            fixture.gaiaxpy_version,
            fixture.bp_model,
            fixture.rp_model,
            fixture.sampling_nm,
            fixture.merge_bp,
            fixture.merge_rp,
            fixture.design_bp,
            fixture.design_rp,
        )
    }

    /// Repository-relative default design fixture.
    pub fn default_fixture_path() -> &'static Path {
        Path::new(
            "crates/nsb-data-tools/tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json",
        )
    }

    /// Resolve an explicit or repository-bundled design fixture.
    pub fn resolve_design_fixture_path(explicit: Option<&Path>) -> PathBuf {
        if let Some(path) = explicit {
            return path.to_path_buf();
        }
        if let Ok(path) = std::env::var("STARLIGHT_GAIAXPY_DESIGN_FIXTURE") {
            return PathBuf::from(path);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json")
    }

    /// GaiaXPy version from which the frozen design matrix was exported.
    pub fn gaiaxpy_version(&self) -> &str {
        &self.gaiaxpy_version
    }

    /// Frozen BP calibration model identifier.
    pub fn bp_model(&self) -> &str {
        &self.bp_model
    }

    /// Frozen RP calibration model identifier.
    pub fn rp_model(&self) -> &str {
        &self.rp_model
    }

    // The constructor mirrors the eight independently validated fixture components.
    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        gaiaxpy_version: String,
        bp_model: String,
        rp_model: String,
        sampling_nm: Vec<f64>,
        merge_bp: Vec<f64>,
        merge_rp: Vec<f64>,
        design_bp: Vec<Vec<f64>>,
        design_rp: Vec<Vec<f64>>,
    ) -> Result<Self> {
        let sample_count = sampling_nm.len();
        if sample_count != EXPECTED_SAMPLE_COUNT {
            bail!(
                "design fixture must contain {EXPECTED_SAMPLE_COUNT} samples, found {sample_count}"
            );
        }
        for (index, wavelength) in sampling_nm.iter().enumerate() {
            let expected = BAND_MIN_NM + XP_SAMPLED_GRID_STEP_NM * index as f64;
            if !wavelength.is_finite() || (*wavelength - expected).abs() > 1.0e-12 {
                bail!("design fixture sampling grid mismatch at index {index}");
            }
        }
        if merge_bp.len() != sample_count || merge_rp.len() != sample_count {
            bail!("merge weights length mismatch with sampling grid");
        }
        for index in 0..sample_count {
            let bp = merge_bp[index];
            let rp = merge_rp[index];
            if !bp.is_finite() || !rp.is_finite() || bp < 0.0 || rp < 0.0 {
                bail!("invalid BP/RP merge weight at index {index}");
            }
            if (bp + rp - 1.0).abs() > 1.0e-12 {
                bail!("BP/RP merge weights do not sum to one at index {index}");
            }
        }
        for (label, design) in [("bp", &design_bp), ("rp", &design_rp)] {
            if design.len() != 55 {
                bail!("{label} design matrix must contain 55 basis rows");
            }
            for (row_index, row) in design.iter().enumerate() {
                if row.len() != sample_count {
                    bail!("{label} design row {row_index} column count mismatch");
                }
                if row.iter().any(|value| !value.is_finite()) {
                    bail!("{label} design row {row_index} contains non-finite values");
                }
            }
        }
        Ok(Self {
            gaiaxpy_version,
            bp_model,
            rp_model,
            sampling_nm,
            merge_bp,
            merge_rp,
            design_bp,
            design_rp,
        })
    }

    /// Reconstruct and integrate one canonical coefficient record.
    pub fn calibrate_record(
        &self,
        record: &CanonicalXpContinuousRecord,
    ) -> Result<ContinuousCalibratedFlux> {
        let product = self.calibrate_record_product(record)?;
        let integral = integrate_photon_flux(&product)?;
        let uncertainty = integral
            .uncertainty_ph_m2_s
            .context("calibrated XP continuous product has no uncertainty")?;
        Ok(ContinuousCalibratedFlux {
            flux_336_650_ph_m2_s: integral.total_ph_m2_s,
            statistical_uncertainty_336_650_ph_m2_s: uncertainty,
        })
    }

    /// Reconstruct calibrated 336–650 nm spectral samples.
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
        )?;
        let rp = self.calibrate_band(
            record.rp_n_parameters,
            &record.rp_coefficients,
            &record.rp_coefficient_errors,
            &record.rp_coefficient_correlations,
            record.rp_standard_deviation,
            &self.design_rp,
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
        for index in 0..n {
            flux[index] = bp.flux[index] * merge_bp[index] + rp.flux[index] * merge_rp[index];
            err2[index] = (bp.error[index] * merge_bp[index]).powi(2)
                + (rp.error[index] * merge_rp[index]).powi(2);
        }
        let error = err2.into_iter().map(f64::sqrt).collect();
        return Ok((flux, error));
    }
    if bp.present ^ rp.present {
        let (band, existing) = if bp.present { (bp, "bp") } else { (rp, "rp") };
        let mut flux = band.flux.clone();
        let mut error = band.error.clone();
        for (index, wavelength) in sampling_nm.iter().enumerate() {
            let masked = if existing == "rp" {
                *wavelength <= RP_WL_LOW
            } else {
                *wavelength >= BP_WL_HIGH
            };
            if masked {
                flux[index] = f64::NAN;
                error[index] = f64::NAN;
            }
        }
        return Ok((flux, error));
    }
    bail!("no usable BP/RP band for calibration")
}

fn mat_vec_mul_rows(coefficients: &[f64], design: &[Vec<f64>]) -> Vec<f64> {
    let sample_count = design[0].len();
    let mut out = vec![0.0; sample_count];
    for (coefficient, row) in coefficients.iter().zip(design) {
        for (index, sample) in row.iter().enumerate() {
            out[index] += coefficient * sample;
        }
    }
    out
}

fn sample_error(
    covariance: &[Vec<f64>],
    design: &[Vec<f64>],
    standard_deviation: f64,
) -> Result<Vec<f64>> {
    let sample_count = design[0].len();
    let basis_count = design.len();
    let mut out = vec![0.0; sample_count];
    for sample_index in 0..sample_count {
        let mut sum = 0.0;
        for column in 0..basis_count {
            let mut accumulator = 0.0;
            for row in 0..basis_count {
                accumulator += design[row][sample_index] * covariance[row][column];
            }
            sum += accumulator * design[column][sample_index];
        }
        if !sum.is_finite() || sum < -1.0e-24 {
            bail!("invalid propagated XP continuous variance");
        }
        out[sample_index] = sum.max(0.0).sqrt() * standard_deviation;
    }
    Ok(out)
}

#[allow(clippy::needless_range_loop)]
fn packed_correlation_to_matrix(packed: &[f64], n: usize) -> Vec<Vec<f64>> {
    // Mirrors GaiaXPy `array_to_symmetric_matrix` (packed lower triangle, k=-1).
    let mut matrix = vec![vec![0.0; n]; n];
    for index in 0..n {
        matrix[index][index] = 1.0;
    }
    let mut packed_index = 0;
    for row in 1..n {
        for column in 0..row {
            matrix[row][column] = packed[packed_index];
            packed_index += 1;
        }
    }
    for row in 0..n {
        for column in 0..row {
            matrix[column][row] = matrix[row][column];
        }
    }
    matrix
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
    if formal_errors
        .iter()
        .any(|error| !error.is_finite() || *error < 0.0)
    {
        bail!("formal coefficient errors must be finite and non-negative");
    }
    let correlation = packed_correlation_to_matrix(packed_correlations, n);
    let mut diagonal = vec![vec![0.0; n]; n];
    for index in 0..n {
        diagonal[index][index] = formal_errors[index] / standard_deviation;
    }
    Ok(mat_mul(&diagonal, &mat_mul(&correlation, &diagonal)))
}

fn mat_mul(left: &[Vec<f64>], right: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let rows = left.len();
    let columns = right[0].len();
    let shared = right.len();
    let mut out = vec![vec![0.0; columns]; rows];
    for row in 0..rows {
        for pivot in 0..shared {
            let value = left[row][pivot];
            if value == 0.0 {
                continue;
            }
            for column in 0..columns {
                out[row][column] += value * right[pivot][column];
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
    fn loads_and_validates_design_fixture() -> Result<()> {
        let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture_path())?;
        assert_eq!(calibrator.sampling_nm.len(), EXPECTED_SAMPLE_COUNT);
        assert_eq!(calibrator.design_bp.len(), 55);
        assert_eq!(calibrator.gaiaxpy_version(), PINNED_GAIA_XPY_VERSION);
        assert_eq!(calibrator.bp_model(), BP_MODEL);
        assert_eq!(calibrator.rp_model(), RP_MODEL);
        Ok(())
    }
}
