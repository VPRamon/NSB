//! Surface brightness in `mag / arcsec²`.
//!
//! Re-exports [`qtty::photometry::SurfaceBrightness`] and provides an
//! NSB-specific constructor using the model zero-point `27.78`.

pub use qtty::photometry::SurfaceBrightness;

use qtty::photometry::band_flux_to_surface_brightness;

/// NSB photometric zero-point (get_NSB.py, B-band-equivalent S10 units).
pub(crate) const NSB_ZERO_POINT: f64 = 27.78;

/// NSB-specific extension methods for [`SurfaceBrightness`].
pub trait SurfaceBrightnessExt {
    /// Convert a band-integrated photon radiance to surface brightness using
    /// the `get_NSB.py` zero-point `27.78`:
    /// `mag = 27.78 - 2.5 · log10(flux)`.
    fn from_band_flux(flux: f64) -> Self;
}

impl SurfaceBrightnessExt for SurfaceBrightness {
    fn from_band_flux(flux: f64) -> Self {
        band_flux_to_surface_brightness(flux, NSB_ZERO_POINT)
    }
}
