//! Frozen, in-process Gaia DR3 XP continuous reconstruction.

use anyhow::{bail, Context, Result};

pub mod calibrate;
pub mod canonical;

pub use calibrate::GaiaXpContinuousCalibrator;
pub use canonical::{
    packed_correlation_len, stream_bulk_ecsv_gz, CanonicalXpContinuousRecord,
    XpContinuousSourceFormat, CANONICAL_XP_CONTINUOUS_SCHEMA,
};

/// GaiaXPy release used to export the checked-in design matrices and oracle.
pub const PINNED_GAIA_XPY_VERSION: &str = "2.1.4";
/// Lower edge represented by the frozen continuous design fixture.
pub const BAND_MIN_NM: f64 = 336.0;
/// Upper edge of the production photon integration band.
pub const BAND_MAX_NM: f64 = 650.0;
pub(crate) const XP_SAMPLED_GRID_STEP_NM: f64 = 2.0;

const REQUESTED_BAND_MIN_NM: f64 = 300.0;
const PLANCK_CONSTANT_J_S: f64 = 6.626_070_15e-34;
const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;

/// Calibrated spectral samples for one Gaia source.
#[derive(Debug, Clone, PartialEq)]
pub struct XpProduct {
    pub source_id: String,
    pub wavelengths_nm: Vec<f64>,
    pub flux_w_m2_nm: Vec<f64>,
    pub flux_error_w_m2_nm: Option<Vec<f64>>,
}

/// Integrate calibrated photon flux over the available part of 300–650 nm.
///
/// The pinned GaiaXPy fixture begins at 336 nm, so the 300–336 nm part of the
/// requested band has no extrapolated contribution. The merge report records
/// that limitation and must not describe this value as a corrected 300–650 nm
/// production integral.
pub fn integrate_photon_flux(product: &XpProduct) -> Result<f64> {
    let (first, last) = integration_bounds(product)?;
    let hc = PLANCK_CONSTANT_J_S * SPEED_OF_LIGHT_M_S;
    let mut total = 0.0;
    for index in first..last {
        let left = photon_density(
            product.wavelengths_nm[index],
            product.flux_w_m2_nm[index],
            hc,
        );
        let right = photon_density(
            product.wavelengths_nm[index + 1],
            product.flux_w_m2_nm[index + 1],
            hc,
        );
        total += 0.5
            * (left + right)
            * (product.wavelengths_nm[index + 1] - product.wavelengths_nm[index]);
    }
    if !total.is_finite() {
        bail!("Gaia XP integrated photon flux is not finite");
    }
    Ok(total)
}

/// Propagate independent per-sample errors through the photon-flux trapezoid.
pub fn integrate_photon_flux_uncertainty(product: &XpProduct) -> Result<f64> {
    let errors = product
        .flux_error_w_m2_nm
        .as_ref()
        .context("calibrated XP product has no per-sample uncertainty")?;
    let (first, last) = integration_bounds(product)?;
    if errors.len() != product.wavelengths_nm.len() {
        bail!("Gaia XP wavelength/flux_error length mismatch");
    }
    let hc = PLANCK_CONSTANT_J_S * SPEED_OF_LIGHT_M_S;
    let mut variance = 0.0;
    for (index, error) in errors.iter().enumerate().take(last + 1).skip(first) {
        let left_width = if index > first {
            product.wavelengths_nm[index] - product.wavelengths_nm[index - 1]
        } else {
            0.0
        };
        let right_width = if index < last {
            product.wavelengths_nm[index + 1] - product.wavelengths_nm[index]
        } else {
            0.0
        };
        let weight_nm = 0.5 * (left_width + right_width);
        let error_density = photon_density(product.wavelengths_nm[index], *error, hc);
        variance += (error_density * weight_nm).powi(2);
    }
    let uncertainty = variance.sqrt();
    if !uncertainty.is_finite() {
        bail!("Gaia XP integrated uncertainty is not finite");
    }
    Ok(uncertainty)
}

fn integration_bounds(product: &XpProduct) -> Result<(usize, usize)> {
    if product.source_id.trim().is_empty()
        || product.wavelengths_nm.len() != product.flux_w_m2_nm.len()
        || product.wavelengths_nm.len() < 2
    {
        bail!("invalid Gaia XP product dimensions");
    }
    if product
        .wavelengths_nm
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
        || product.flux_w_m2_nm.iter().any(|value| !value.is_finite())
        || product
            .wavelengths_nm
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        bail!("Gaia XP product contains invalid spectral samples");
    }
    if let Some(errors) = &product.flux_error_w_m2_nm {
        if errors.len() != product.wavelengths_nm.len()
            || errors
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("Gaia XP product contains invalid uncertainty samples");
        }
    }
    let first = product
        .wavelengths_nm
        .iter()
        .position(|wavelength| *wavelength >= REQUESTED_BAND_MIN_NM)
        .context("Gaia XP spectrum does not cover 300–650 nm")?;
    let last = product
        .wavelengths_nm
        .iter()
        .rposition(|wavelength| *wavelength <= BAND_MAX_NM)
        .context("Gaia XP spectrum does not cover 300–650 nm")?;
    if first >= last || product.wavelengths_nm[last] != BAND_MAX_NM {
        bail!("Gaia XP spectrum must reach an exact 650 nm sample");
    }
    Ok((first, last))
}

fn photon_density(wavelength_nm: f64, flux_w_m2_nm: f64, hc: f64) -> f64 {
    flux_w_m2_nm * (wavelength_nm * 1.0e-9) / hc
}
