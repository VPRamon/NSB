//! NSB-specific photometric helpers.
//!
//! Lives in NSB rather than upstream `siderust` because the `27.78`
//! zero-point is model-specific (mirrors the Python `get_NSB.py`
//! reference implementation).

use qtty::photometry::flux_to_magnitude;

/// NSB photometric zero-point (get_NSB.py, B-band-equivalent S10 units).
const NSB_ZERO_POINT: f64 = 27.78;

/// Photometric zero-point conversion mirroring `get_NSB.py`:
/// `mag = 27.78 - 2.5 · log10(flux)`.
#[inline]
pub fn flux_to_mag(flux: f64) -> f64 {
    flux_to_magnitude(flux, NSB_ZERO_POINT).value()
}
