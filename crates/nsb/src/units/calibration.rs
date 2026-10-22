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
pub(crate) const HC: JouleMeters = JouleMeters::new(6.626_070_15e-34 * 299_792_458.0);

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
    fn photon_energy_to_s10_matches_si_definition() {
        // Independent SI oracle: exact h and c, cm^-2 -> m^-2 (1e4),
        // ns^-1 -> s^-1 (1e9). Density is per nm on both sides, so no
        // spectral Jacobian remains. S10 is an NSB calibration convention.
        let mut maximum_relative_error = 0.0_f64;
        for wavelength_nm in [300.0, 445.0, 551.0, 650.0] {
            for density in [0.0, 1e-8, 0.001, 1.0] {
                let expected = density * 1e13 * 6.626_070_15e-34 * 299_792_458.0
                    / (wavelength_nm * 1e-9)
                    / 1.28e-11;
                let actual = s10_for_spectral_photon_radiance(
                    SpectralBandPhotonRadiance::new(density),
                    Nanometers::new(wavelength_nm),
                )
                .value();
                if expected != 0.0 {
                    maximum_relative_error =
                        maximum_relative_error.max((actual - expected).abs() / expected.abs());
                }
                assert!(
                    (actual - expected).abs() <= 3e-15 * expected.abs(),
                    "wavelength={wavelength_nm} nm, density={density}: {actual} vs {expected} S10"
                );
            }
        }
        eprintln!(
            "NSB_PHOTON_SI_RECEIPT={}",
            serde_json::json!({"family":"photon_energy_to_s10", "cases":16,
                "reference":"exact SI h*c and explicit cm^-2 ns^-1 scaling / 1.28e-11 S10 convention",
                "wavelength_nm":[300,445,551,650], "density_ph_cm2_ns_sr_nm":[0,1e-8,0.001,1],
                "units":"S10 diagnostic", "relative_tolerance":3e-15,
                "max_relative_error":maximum_relative_error, "result":"PASS"})
        );
    }

    #[test]
    fn s10_calibration_constants_preserve_historical_um_and_nm_values() {
        assert!((S10_TO_W_M2_SR_UM.value() - 1.28e-8).abs() < 1.0e-20);
        assert!((S10_TO_W_M2_SR_NM.value() - 1.28e-11).abs() < 1.0e-23);
    }
}
