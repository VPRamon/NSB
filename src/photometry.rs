//! NSB-specific photometric helpers.
//!
//! Lives in NSB rather than upstream `siderust` because the `27.78`
//! zero-point is model-specific (mirrors the Python `get_NSB.py`
//! reference implementation).

/// Photometric zero-point conversion mirroring `get_NSB.py`:
/// `mag = 27.78 - 2.5 · log10(flux)`.
#[inline]
pub fn flux_to_mag(flux: f64) -> f64 {
    27.78 - 2.5 * flux.log10()
}
