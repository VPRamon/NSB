//! Domain unit re-exports for NSB.
//!
//! Most quantities now come from `qtty::radiometry` (gated on the
//! `radiometry` feature in our `Cargo.toml`). Kept locally:
//!
//! - [`SurfaceBrightness`] — logarithmic mag/arcsec² surface brightness,
//!   which has no widely accepted physical-quantity dimension and so stays
//!   in NSB until a dedicated upstream type is designed.

mod surface_brightness;

pub use surface_brightness::SurfaceBrightness;

/// `S10`: 10th-magnitude stars per square degree
/// (re-exported from [`qtty::radiometry::S10s`]).
pub type S10 = qtty::radiometry::S10s;

/// Band-integrated photon radiance: `ph · cm⁻² · ns⁻¹ · sr⁻¹`
/// (re-exported from
/// [`qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian`]).
pub type BandPhotonRadiance = qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian;

/// Spectral photon radiance: `ph · s⁻¹ · cm⁻² · sr⁻¹ · Å⁻¹`
/// (re-exported from
/// [`qtty::radiometry::PhotonsPerSquareCentimeterSecondSteradianAngstrom`]).
pub type SpectralPhotonRadiance =
    qtty::radiometry::PhotonsPerSquareCentimeterSecondSteradianAngstrom;

/// Convert spectral *energy* radiance `[erg / (s · cm² · sr · Å)]` to spectral
/// *photon* radiance `[ph / (s · cm² · sr · Å)]` at wavelength
/// `lambda_angstrom`.
///
/// This is the constant `5.03e7` used throughout `NSB_Utils.py`, derived from
/// `1 / (h · c)` expressed in CGS with the wavelength in Å. Now backed by the
/// typed [`qtty::radiometry::erg_to_photon`] helper, which uses the exact
/// CODATA value `≈ 5.034 116 5 × 10⁷ ph / (erg · Å)`.
#[inline]
pub fn erg_to_photon(energy_radiance_cgs: f64, lambda_angstrom: f64) -> f64 {
    let e_rad =
        qtty::radiometry::ErgsPerSecondSquareCentimeterSteradianAngstrom::new(energy_radiance_cgs);
    // λ [m] = λ [Å] · 1e-10
    let lambda = qtty::length::Meters::new(lambda_angstrom * 1.0e-10);
    qtty::radiometry::erg_to_photon(e_rad, lambda).value()
}

