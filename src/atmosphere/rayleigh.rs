//! Rayleigh scattering: optical depth and phase function.
//!
//! Port of `GetRayleighOptDepth` from `NSB_Utils.py:1599`.

/// Rayleigh optical depth at sea-level pressure `p [hPa]`, observatory height
/// `h [km]`, and wavelength `lambda [nm]`. Uses the Bodhaine et al. (1999)
/// approximation as in the Python code.
pub fn optical_depth(lambda_nm: f64, pressure_hpa: f64, h_km: f64) -> f64 {
    let lam_um = lambda_nm * 1e-3;
    let p_atm = pressure_hpa / 1013.25;
    // Bodhaine et al. 1999 simplified: τ_R(λ) = p · 0.0021520 · (1.0455996 - 341.29061·λ⁻² - 0.90230850·λ²)
    //                                                    / (1 + 0.0027059889·λ⁻² - 85.968563·λ²)
    let l2 = lam_um * lam_um;
    let inv_l2 = 1.0 / l2;
    let tau_sea = 0.0021520
        * (1.0455996 - 341.29061 * inv_l2 - 0.90230850 * l2)
        / (1.0 + 0.0027059889 * inv_l2 - 85.968563 * l2);
    // Scale-height correction for observatory altitude (~8 km scale height).
    let height_factor = (-h_km / 8.0).exp();
    p_atm * tau_sea * height_factor
}

/// Rayleigh phase function `P(θ) = 3/(16π) · (1 + cos²θ)`.
#[inline]
pub fn phase(cos_theta: f64) -> f64 {
    3.0 / (16.0 * std::f64::consts::PI) * (1.0 + cos_theta * cos_theta)
}
