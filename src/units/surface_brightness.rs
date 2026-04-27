//! Surface brightness in `mag / arcsec²`.
//!
//! TODO: implement in siderust/qtty.

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SurfaceBrightness(pub f64);

impl SurfaceBrightness {
    pub const fn new(v: f64) -> Self { Self(v) }
    #[inline] pub fn value(self) -> f64 { self.0 }

    /// Convert a band-integrated photon radiance to surface brightness using
    /// the `get_NSB.py` zero-point `27.78`:
    /// `mag = 27.78 - 2.5 · log10(flux)`.
    pub fn from_band_flux(flux: f64) -> Self {
        Self(27.78 - 2.5 * flux.log10())
    }
}
