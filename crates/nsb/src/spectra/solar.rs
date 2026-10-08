// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Solar spectral irradiance loader.
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
//! the bundled solar spectrum is owned by `spectra::solar` and shared by
//! components that model scattered sunlight.

use crate::error::{NsbError, Result};
use crate::units::length::Nanometer;
use crate::units::SolarSpectralIrradianceUnit;
use siderust::optica::data::Provenance;
use siderust::optica::grid::OutOfRange;
use siderust::optica::spectrum::{loaders::ascii::two_column, Interpolation, SampledSpectrum};

const RAW: &str = include_str!("../../data/solar_spectrum.dat");

pub(crate) type SolarSpectrum = SampledSpectrum<Nanometer, SolarSpectralIrradianceUnit>;

/// Returns the solar reference spectrum as
/// `(wavelength [nm], irradiance [W m⁻² nm⁻¹])`.
pub(crate) fn load() -> Result<SolarSpectrum> {
    two_column::<Nanometer, SolarSpectralIrradianceUnit>(
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
    fn solar_spectrum_loader_loads_nonempty() {
        let s = load().expect("load solar reference spectrum");
        assert!(!s.is_empty());
        assert!(s.xs_raw()[0] > 0.0);
    }

    #[test]
    fn solar_spectrum_checksum_matches() {
        use siderust::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "1cc24671052b7623752eb41dd99a84520393b2845442b8bbde217610fe5ed949",
        );
    }
}
