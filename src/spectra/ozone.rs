//! Ozone transmittance — delegates to `siderust::atmosphere::ozone`.
//!
//! Scientific role:
//! ozone in Earth's atmosphere modifies the transmission of light through the
//! optical path, especially in wavelength regions where ozone absorption is
//! important.
//!
//! Contribution to the science:
//! this file gives the NSB crate access to a canonical ozone-transmission
//! table maintained upstream in `siderust`, avoiding duplicated atmospheric
//! reference data while keeping the NSB modelling stack scientifically
//! traceable.

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
