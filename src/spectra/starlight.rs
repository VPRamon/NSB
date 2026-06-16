//! Integrated starlight spectrum from SkyCalc (Noll et al. 2012).
//!
//! Scientific role:
//! this file holds the reference spectrum for unresolved integrated starlight,
//! one of the diffuse astronomical components of the night sky.
//!
//! Contribution to the science:
//! the loader preserves the bundled SkyCalc-derived radiance table and makes
//! it available to `components::starlight`, which converts and integrates it
//! into the contribution added to the total NSB.

use crate::error::{NsbError, Result};
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{loaders::ascii::two_column, Interpolation, SampledSpectrum};
use siderust::qtty::{length::Meter, Nanometer};

const RAW: &str = include_str!("../../data/radiance_starlight.txt");

// Pinned SHA-256 of the SkyCalc-derived integrated starlight spectrum.
siderust::assert_data_checksum!(
    "NSB/data/radiance_starlight.txt",
    RAW.as_bytes(),
    "69b0fc4edc08a38a62ef9cdfd27e2ecefef61f3d36205cba2e941c61193b638d"
);

/// Loads `(wavelength [nm], radiance [ph s⁻¹ m⁻² μm⁻¹ arcsec⁻²])`.
///
/// Format mirrors SkyCalc's two-column ASCII output.
pub fn load() -> Result<SampledSpectrum<Nanometer, Meter>> {
    two_column::<Nanometer, Meter>(
        RAW,
        1.0,
        1.0,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::bundled_file("NSB/data/radiance_starlight.txt")),
    )
    .map_err(|e| NsbError::DataParse {
        file: "radiance_starlight.txt",
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_sha256_matches_runtime_hash() {
        use siderust::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "69b0fc4edc08a38a62ef9cdfd27e2ecefef61f3d36205cba2e941c61193b638d",
        );
    }
}
