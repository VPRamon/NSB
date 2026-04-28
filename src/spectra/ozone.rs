//! Ozone transmittance — delegates to `siderust::atmosphere::ozone`.

use crate::error::Result;
use siderust::atmosphere::Transmittance;
use siderust::qtty::Nanometer;
use siderust::spectra::SampledSpectrum;

/// `(wavelength [nm], transmittance [-])`.
///
/// Forwards the upstream dataset in `siderust::atmosphere::ozone`, which
/// holds the canonical copy of the pre-computed ozone transmittance table
/// (originally from the NSB/darknsb `o3trans.dat` data file).
pub fn load() -> Result<SampledSpectrum<Nanometer, Transmittance>> {
    Ok(siderust::atmosphere::ozone::transmission_table().clone())
}
