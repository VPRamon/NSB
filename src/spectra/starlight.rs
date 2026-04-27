//! Integrated starlight spectrum from SkyCalc (Noll et al. 2012).

use crate::error::{NsbError, Result};
use super::spectrum::Spectrum;
use siderust::spectra::loaders::ascii::two_column;
use siderust::spectra::{Interpolation, OutOfRange};
use siderust::qtty::{length::Meter, Nanometer};

const RAW: &str = include_str!("../../data/radiance_starlight.txt");

/// Loads `(wavelength [nm], radiance [ph s⁻¹ m⁻² μm⁻¹ arcsec⁻²])`.
///
/// Format mirrors SkyCalc's two-column ASCII output.
pub fn load() -> Result<Spectrum> {
    let s = two_column::<Nanometer, Meter>(
        RAW, 1.0, 1.0, Interpolation::Linear, OutOfRange::ClampToEndpoints, None,
    )
    .map_err(|e| NsbError::DataParse { file: "radiance_starlight.txt", message: e.to_string() })?;
    Ok(Spectrum::new(s.xs_raw(), s.ys_raw()).with_tag("starlight"))
}
