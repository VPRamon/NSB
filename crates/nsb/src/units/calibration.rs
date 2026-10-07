// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::conventions::WattsPerSquareMeterSteradianMicrometer;
use super::length::{Meter, Nanometers};
use super::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance, S10s,
    WattsPerSquareMeterSteradianMeter, WattsPerSquareMeterSteradianNanometer,
};
use super::{energy, unit, Quantity};

/// Quantity type for Planck's constant times the speed of light, in joule metre.
pub(crate) type JouleMeters = Quantity<unit::Prod<unit::Joule, unit::Meter>>;

/// Photon energy numerator, `h · c`, in joule metre.
pub(crate) const HC: JouleMeters = JouleMeters::new(1.986_445_857_148_968e-25);

/// Photometric calibration: 1 S10 → W m⁻² sr⁻¹ µm⁻¹.
///
/// This is a domain calibration convention, not a dimensional unit conversion.
pub(crate) const S10_TO_W_M2_SR_UM: WattsPerSquareMeterSteradianMicrometer =
    WattsPerSquareMeterSteradianMicrometer::new(1.28e-8);

/// Photometric calibration: 1 S10 → W m⁻² sr⁻¹ nm⁻¹.
///
/// This is derived from [`S10_TO_W_M2_SR_UM`] using qtty unit conversion, but
/// remains a photometric calibration convention rather than a generic `.to<>`
/// conversion from S10.
pub(crate) const S10_TO_W_M2_SR_NM: WattsPerSquareMeterSteradianNanometer =
    S10_TO_W_M2_SR_UM.to_const::<unit::WattPerSquareMeterSteradianNanometer>();

/// Compute the S10 diagnostic corresponding to a spectral photon radiance at a
/// reference wavelength.
///
/// This combines photon energy (`h·c/λ`) with the NSB S10 photometric
/// calibration; it is a physical/calibration operation, not a pure unit
/// conversion.
pub(crate) fn s10_for_spectral_photon_radiance(
    density: SpectralBandPhotonRadiance,
    wavelength: Nanometers,
) -> S10s {
    let wavelength_m = wavelength.to::<Meter>();
    let photon_energy = energy::Joules::new(HC.value() / wavelength_m.value());
    let spectral_radiance_per_m = WattsPerSquareMeterSteradianMeter::new(
        density
            .to::<unit::PhotonPerSquareMeterSecondSteradianMeter>()
            .value()
            * photon_energy.value(),
    );
    let spectral_radiance_per_nm =
        spectral_radiance_per_m.to::<unit::WattPerSquareMeterSteradianNanometer>();

    S10s::new(spectral_radiance_per_nm.value() / S10_TO_W_M2_SR_NM.value())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s10_calibration_constants_preserve_historical_um_and_nm_values() {
        assert!((S10_TO_W_M2_SR_UM.value() - 1.28e-8).abs() < 1.0e-20);
        assert!((S10_TO_W_M2_SR_NM.value() - 1.28e-11).abs() < 1.0e-23);
    }
}
