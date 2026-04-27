//! Mie scattering — optical depth and phase function table.
//!
//! Loads `data/mie_m15s1.dat` (the Cerro Paranal aerosol model from the
//! ESO Sky Model). Format: tabulated phase function over scattering angle.
//!
//! TODO: complete the Mie phase-function loader. The current implementation
//! provides only a placeholder `optical_depth` based on the wavelength power
//! law used by Python `GetMieOptDepth` (`NSB_Utils.py:1631`).

use crate::error::Result;

/// Mie (aerosol) optical depth approximation: τ_M(λ) = τ₀ · (λ/550nm)^α.
/// Python uses τ₀ ≈ 0.05 and α ≈ -1.38 for Paranal.
pub fn optical_depth(lambda_nm: f64) -> Result<f64> {
    const TAU0: f64 = 0.05;
    const ALPHA: f64 = -1.38;
    Ok(TAU0 * (lambda_nm / 550.0).powf(ALPHA))
}
