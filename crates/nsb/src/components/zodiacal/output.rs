//! Output types for the zodiacal-light model.
//!
//! Two output shapes are provided:
//!
//! - [`ZodiacalOutputs`]: the default output returned by [`ZodiacalLight::compute`],
//!   [`ZodiacalLight::compute_exoatmospheric`], and
//!   [`ZodiacalLight::compute_observed`]. It contains the integrated photon
//!   radiance and the B/V S10 proxies. It does **not** carry a full spectrum
//!   to avoid unnecessary allocations in the threshold inner loop.
//!
//! - [`ZodiacalSpectrum`]: the richer output returned by
//!   [`ZodiacalLight::compute_spectrum`]. It contains the full wavelength-
//!   resolved photon-radiance spectrum in addition to the scalar fields.

use optica::spectrum::SampledSpectrum;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use siderust::qtty::{length::Meter, Nanometer};

/// Default zodiacal-light output: scalar summaries only.
///
/// The integrated photon radiance is over the 300–650 nm band. B and V
/// S10 fluxes are interpolated at 445 nm and 551 nm respectively.
#[derive(Debug, Clone, Copy)]
pub struct ZodiacalOutputs {
    /// Integrated photon radiance in ph cm⁻² ns⁻¹ sr⁻¹ (300–650 nm band).
    pub integrated: BandPhotonRadiance,
    /// Zodiacal surface brightness at the B-band reference wavelength (445 nm)
    /// in S10 units (10th-magnitude stars per square degree).
    pub b_flux_s10: S10,
    /// Zodiacal surface brightness at the V-band reference wavelength (551 nm)
    /// in S10 units.
    pub v_flux_s10: S10,
}

/// Zodiacal-light output including the full wavelength-resolved spectrum.
///
/// Returned by [`crate::components::zodiacal::ZodiacalLight::compute_spectrum`]. Prefer [`ZodiacalOutputs`]
/// when only scalar summaries are needed, since this type allocates a full
/// sampled spectrum.
#[derive(Debug, Clone)]
pub struct ZodiacalSpectrum {
    /// Full photon-radiance spectrum in ph cm⁻² ns⁻¹ sr⁻¹ nm⁻¹
    /// sampled on the solar-spectrum wavelength grid, clipped to 300–650 nm.
    pub spectrum: SampledSpectrum<Nanometer, Meter>,
    /// Integrated photon radiance (300–650 nm band).
    pub integrated: BandPhotonRadiance,
    /// B-band (445 nm) S10 surface brightness.
    pub b_flux_s10: S10,
    /// V-band (551 nm) S10 surface brightness.
    pub v_flux_s10: S10,
}
