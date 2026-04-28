//! Solar irradiance spectrum loader.
//!
//! Loads `data/solar_spectrum.dat` (CSV: `wavelength_nm, irradiance_W_m2_nm`)
//! shipped with the Python package, embedded via `include_str!`.

use crate::error::{NsbError, Result};
use siderust::qtty::{length::Meter, Nanometer};
use siderust::spectra::loaders::ascii::two_column;
use siderust::spectra::{Interpolation, OutOfRange, Provenance, SampledSpectrum};

const RAW: &str = include_str!("../../data/solar_spectrum.dat");

// Pinned SHA-256 of the bundled solar spectrum table. See
// `siderust::provenance::checksum` for the update workflow.
siderust::assert_data_checksum!(
    "NSB/data/solar_spectrum.dat",
    RAW.as_bytes(),
    "dbf6a6205c9311782f4a084c1a3ded8d9331c3616dd7e71fbaa1db9fdcc7a7df"
);

/// Returns the solar spectrum as `(wavelength [nm], irradiance [W m⁻² nm⁻¹])`.
pub fn load() -> Result<SampledSpectrum<Nanometer, Meter, f64>> {
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
    fn loads_nonempty() {
        let s = load().expect("load solar spectrum");
        assert!(!s.is_empty());
        assert!(s.xs_raw()[0] > 0.0);
    }

    #[test]
    fn pinned_sha256_matches_runtime_hash() {
        use siderust::provenance::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "dbf6a6205c9311782f4a084c1a3ded8d9331c3616dd7e71fbaa1db9fdcc7a7df",
        );
    }
}
