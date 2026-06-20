//! Solar irradiance reference spectrum loader.
//!
//! Loads `data/solar_spectrum.dat` (CSV: `wavelength_nm, irradiance_W_m2_nm`)
//! shipped with the crate, embedded via `include_str!`.
//!
//! Scientific role:
//! zodiacal light is modelled as scattered sunlight, so the spectral shape of
//! the Sun is the starting point for the zodiacal component.
//!
//! Contribution to the science:
//! this file loads the bundled solar reference spectrum that is rescaled and
//! reddened in `components::zodiacal`. Without it, the crate could only model
//! zodiacal light as a scalar brightness rather than as a physically motivated
//! spectrum integrated over the NSB band.
//!
//! Provenance:
//! solar reference spectrum lives in `reference::solar`.

use crate::error::{NsbError, Result};
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{loaders::ascii::two_column, Interpolation, SampledSpectrum};
use siderust::qtty::{length::Meter, Nanometer};

const RAW: &str = include_str!("../../data/solar_spectrum.dat");

/// Returns the solar reference spectrum as
/// `(wavelength [nm], irradiance [W m⁻² nm⁻¹])`.
pub(crate) fn load() -> Result<SampledSpectrum<Nanometer, Meter>> {
    two_column::<Nanometer, Meter>(
        RAW,
        1.0,
        1.0,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::bundled_file("NSB/data/solar_spectrum.dat")),
    )
    .map_err(|e| NsbError::DataParse {
        file: "solar_spectrum.dat",
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_reference_loader_loads_nonempty() {
        let s = load().expect("load solar reference spectrum");
        assert!(!s.is_empty());
        assert!(s.xs_raw()[0] > 0.0);
    }

    #[test]
    fn solar_reference_checksum_matches() {
        use siderust::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "dbf6a6205c9311782f4a084c1a3ded8d9331c3616dd7e71fbaa1db9fdcc7a7df",
        );
    }
}
