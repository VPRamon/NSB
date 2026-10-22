// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Opt-in PALACE oxygen subset. Integrated delta-line fluxes in vacuum wavelength.
use super::extinction::{
    noll_airglow_scattering_geometry, spectral_airglow_scattering_transmission_with_geometry,
};
use super::geometry::VanRhijnConfig;
use super::temporal::palace_climatology_coordinates;
use crate::site::AtmosphericConditions;
use crate::{NsbError, Result};
use serde::Deserialize;
use siderust::coordinates::{centers::Geodetic, frames::ECEF};
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use std::sync::OnceLock;
use tempoch::{Time, UTC};

const RAW: &str = include_str!("../../../data/airglow_palace_oxygen_v1.json");
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Line {
    wavelength_nm: f64,
    intensity_r: f64,
    height_km: f64,
    family: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Oxygen {
    schema_version: u32,
    wavelength_convention: String,
    radiance_unit: String,
    lines: [Line; 4],
    climatology: Vec<[[f64; 3]; 2]>,
}
fn data() -> &'static Oxygen {
    static DATA: OnceLock<Oxygen> = OnceLock::new();
    DATA.get_or_init(|| {
        // Bytes are checksum verified at build time; no runtime file access.
        let d: Oxygen = serde_json::from_str(RAW).expect("build-verified oxygen asset");
        assert_eq!(d.schema_version, 1);
        assert_eq!(d.wavelength_convention, "vacuum_nm");
        assert_eq!(d.radiance_unit, "R");
        assert_eq!(d.climatology.len(), 144);
        assert!(d.lines.iter().all(|l| l.family < 2
            && l.wavelength_nm.is_finite()
            && l.intensity_r > 0.
            && l.height_km > 0.));
        assert!(d
            .climatology
            .iter()
            .flatten()
            .flatten()
            .all(|x| x.is_finite()));
        d
    })
}
/// Conservative sum of family residual sigma; no independence assumed.
pub(crate) fn integrate(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    altitude: Degrees,
    atmosphere: AtmosphericConditions,
    solar_sfu: f64,
    lower: f64,
    upper: f64,
) -> Result<(f64, f64)> {
    let zenith = Degrees::new(90. - altitude.value());
    if !(0. ..=60.).contains(&zenith.value()) {
        return Err(NsbError::OutOfRange(
            "experimental PALACE oxygen requires zenith distance <= 60 degrees".into(),
        ));
    }
    let (month, bin) = palace_climatology_coordinates(time, location)
        .ok_or_else(|| NsbError::Unsupported("time outside PALACE domain".into()))?;
    let d = data();
    let cell = d.climatology[(month as usize - 1) * 12 + bin - 1];
    let scattering = noll_airglow_scattering_geometry(zenith);
    let mut mean = 0.;
    let mut sigma = 0.;
    for l in &d.lines {
        let [relative, slope, residual] = cell[l.family];
        let factor = relative * (1. + 0.01 * slope * (solar_sfu - 100.));
        if !factor.is_finite() || factor < 0. {
            return Err(NsbError::OutOfRange(
                "negative/nonfinite PALACE oxygen solar extrapolation".into(),
            ));
        }
        // Half-open bins conserve flux on adjacent intervals. All lines lie
        // strictly inside the supported 300..650 nm outer interval.
        if l.wavelength_nm < lower || l.wavelength_nm >= upper {
            continue;
        }
        let geometry = VanRhijnConfig::from_continuum_height(Kilometers::new(l.height_km))
            .geometry_factor(zenith)?
            .value();
        let transmission = spectral_airglow_scattering_transmission_with_geometry(
            Nanometers::new(l.wavelength_nm),
            atmosphere,
            &scattering,
        )
        .value();
        let scale = l.intensity_r * geometry * transmission * 1.0e-3 / (4. * std::f64::consts::PI);
        mean += scale * factor;
        sigma += scale * relative * residual;
    }
    Ok((mean, sigma))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_units_families_and_flux_are_pinned() {
        let d = data();
        assert_eq!(d.lines[0].height_km, 97.);
        assert_eq!(d.lines[1].height_km, 250.);
        let total: f64 = d.lines.iter().map(|l| l.intensity_r).sum();
        assert!((total - 326.40829679742455).abs() < 1.0e-8);
        // 1 R = 10^6/(4 pi) photons cm^-2 s^-1 sr^-1.
        assert!((total * 1.0e-3 / (4. * std::f64::consts::PI) - 0.025974).abs() < 1.0e-6);
    }
}
