//! NSB-specific quantity units and physical/photometric calibration constants.
//!
//! This module contains units for external conventions that are not currently
//! part of `qtty` itself. Pure dimensional conversions should be expressed by
//! constructing one of these quantities and calling `.to::<unit::...>()`.

use qtty::area::SquareMeter;
use qtty::length::{Meter, Nanometer, Nanometers};
use qtty::power::Watt;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance, S10s,
    WattsPerSquareMeterSteradianMeter, WattsPerSquareMeterSteradianNanometer,
};
use qtty::{unit, Per, Quantity};

/// A generic multiplicative scale factor.
pub type ScaleFactors = qtty::dimensionless::Ratios;

/// Structural qtty unit for spectral solar irradiance, W m⁻² nm⁻¹.
///
/// This is deliberately expressed as qtty unit algebra rather than as an
/// unrelated placeholder unit, so dimensional correctness remains enforced by
/// the compiler without requiring a bespoke NSB unit marker.
pub type SolarSpectralIrradianceUnit = Per<Per<Watt, SquareMeter>, Nanometer>;

/// Spectral solar irradiance in W m⁻² nm⁻¹.
pub type SolarSpectralIrradiance = Quantity<SolarSpectralIrradianceUnit>;

/// Quantity type for Planck's constant times the speed of light, in joule metre.
pub(crate) type JouleMeters = Quantity<unit::Prod<unit::Joule, unit::Meter>>;

/// Photon energy numerator, `h · c`, in joule metre.
pub(crate) const HC: JouleMeters = JouleMeters::new(1.986_445_857_148_968e-25);

/// SkyCalc spectral photon radiance unit:
/// photons s⁻¹ m⁻² arcsec⁻² µm⁻¹.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "ph·s⁻¹·m⁻²·arcsec⁻²·µm⁻¹",
    dimension = qtty::radiometry::SpectralPhotonRadiance,
    ratio = 4.254_517_029_022_576e16
)]
pub(crate) struct SkyCalcPhotonPerSquareMeterSecondSquareArcsecondMicrometer;

/// Spectral photon radiance in SkyCalc's native tabulated convention.
pub(crate) type SkyCalcSpectralPhotonRadiance =
    Quantity<SkyCalcPhotonPerSquareMeterSecondSquareArcsecondMicrometer>;

/// Spectral radiance unit: W m⁻² sr⁻¹ µm⁻¹.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "W·m⁻²·sr⁻¹·µm⁻¹",
    dimension = qtty::radiometry::SpectralRadiance,
    ratio = 1.0e6
)]
pub(crate) struct WattPerSquareMeterSteradianMicrometer;

/// Spectral radiance in W m⁻² sr⁻¹ µm⁻¹.
pub(crate) type WattsPerSquareMeterSteradianMicrometer =
    Quantity<WattPerSquareMeterSteradianMicrometer>;

/// Solar radio flux unit:
/// 1 SFU = 10⁻²² W m⁻² Hz⁻¹.
///
/// `qtty` currently has no spectral-flux-density dimension, so this is kept as
/// a dimensionless convention unit until the upstream dimensional catalogue can
/// represent W m⁻² Hz⁻¹.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "SFU",
    dimension = qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub struct SolarFluxUnit;

/// Solar radio flux in solar flux units.
pub type SolarFluxUnits = Quantity<SolarFluxUnit>;

/// Atmospheric extinction coefficient in magnitudes per airmass.
///
/// This is a dimensionless atmospheric convention. The name preserves the
/// domain meaning while allowing qtty-style construction and comparison.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "mag·airmass⁻¹",
    dimension = qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub struct MagnitudePerAirmass;

/// Atmospheric extinction coefficients in mag per airmass.
pub type MagnitudesPerAirmass = Quantity<MagnitudePerAirmass>;

/// Luminance convention used by Krisciunas & Schaefer: nanolamberts.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "nL",
    dimension = qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub(crate) struct Nanolambert;

/// Brightness in nanolamberts.
pub(crate) type Nanolamberts = Quantity<Nanolambert>;

/// Integrated photon flux over a pixel solid angle.
pub(crate) type PixelIntegratedPhotonFlux =
    Quantity<unit::Prod<unit::PhotonPerSquareCentimeterNanosecondSteradian, unit::Steradian>>;

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
    let photon_energy = qtty::energy::Joules::new(HC.value() / wavelength_m.value());
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
    use qtty::radiometry::PhotonPerSquareCentimeterNanosecondSteradianNanometer;

    #[test]
    fn skycalc_spectral_photon_radiance_converts_to_band_units() {
        let converted = SkyCalcSpectralPhotonRadiance::new(1.0)
            .to::<PhotonPerSquareCentimeterNanosecondSteradianNanometer>();
        assert!((converted.value() - 4.254_517_029_022_576e-6).abs() < 1.0e-18);
    }

    #[test]
    fn skycalc_spectral_photon_radiance_round_trips() {
        let original = SkyCalcSpectralPhotonRadiance::new(123.456);
        let round_trip = original
            .to::<PhotonPerSquareCentimeterNanosecondSteradianNanometer>()
            .to::<SkyCalcPhotonPerSquareMeterSecondSquareArcsecondMicrometer>();
        let rel = (round_trip.value() - original.value()).abs() / original.value();
        assert!(rel < 1.0e-12);
    }

    #[test]
    fn s10_calibration_constants_preserve_historical_um_and_nm_values() {
        assert!((S10_TO_W_M2_SR_UM.value() - 1.28e-8).abs() < 1.0e-20);
        assert!((S10_TO_W_M2_SR_NM.value() - 1.28e-11).abs() < 1.0e-23);
    }

    #[test]
    fn solar_spectral_irradiance_uses_qtty_dimension_algebra() {
        let irradiance = SolarSpectralIrradiance::new(2.5);
        assert_eq!(irradiance.value(), 2.5);
    }
}
