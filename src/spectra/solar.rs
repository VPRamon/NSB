//! Solar irradiance spectrum loader.
//!
//! Loads `data/solar_spectrum.dat` (CSV: `wavelength_nm, irradiance_W_m2_nm`)
//! shipped with the Python package, embedded via `include_str!`.

use crate::error::{NsbError, Result};
use super::spectrum::Spectrum;
use siderust::spectra::loaders::ascii::two_column;
use siderust::spectra::{Interpolation, OutOfRange};
use siderust::qtty::{length::Meter, Nanometer};

const RAW: &str = include_str!("../../data/solar_spectrum.dat");

/// Returns the solar spectrum as `(wavelength [nm], irradiance [W m⁻² nm⁻¹])`.
pub fn load() -> Result<Spectrum> {
    let s = two_column::<Nanometer, Meter>(
        RAW, 1.0, 1.0, Interpolation::Linear, OutOfRange::ClampToEndpoints, None,
    )
    .map_err(|e| NsbError::DataParse { file: "solar_spectrum.dat", message: e.to_string() })?;
    Ok(Spectrum::new(s.xs_raw(), s.ys_raw()).with_tag("solar"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_nonempty() {
        let s = load().expect("load solar spectrum");
        assert!(!s.is_empty());
        assert!(s.lambda_nm[0] > 0.0);
    }
}
