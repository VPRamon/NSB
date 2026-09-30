use siderust::qtty;
use siderust::qtty::area::SquareMeter;
use siderust::qtty::length::Nanometer;
use siderust::qtty::power::Watt;
use siderust::qtty::{unit, Per, Quantity};

/// A generic multiplicative scale factor.
pub(crate) type ScaleFactors = siderust::qtty::dimensionless::Ratios;

/// Structural qtty unit for spectral solar irradiance, W m⁻² nm⁻¹.
///
/// This is deliberately expressed as qtty unit algebra rather than as an
/// unrelated placeholder unit, so dimensional correctness remains enforced by
/// the compiler without requiring a bespoke NSB unit marker.
pub(crate) type SolarSpectralIrradianceUnit = Per<Per<Watt, SquareMeter>, Nanometer>;

/// Spectral solar irradiance in W m⁻² nm⁻¹.
pub(crate) type SolarSpectralIrradiance = Quantity<SolarSpectralIrradianceUnit>;

/// SkyCalc spectral photon radiance unit:
/// photons s⁻¹ m⁻² arcsec⁻² µm⁻¹.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, siderust::qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "ph·s⁻¹·m⁻²·arcsec⁻²·µm⁻¹",
    dimension = siderust::qtty::radiometry::SpectralPhotonRadiance,
    ratio = 4.254_517_029_022_576e16
)]
pub(crate) struct SkyCalcPhotonPerSquareMeterSecondSquareArcsecondMicrometer;

/// Spectral photon radiance in SkyCalc's native tabulated convention.
pub(crate) type SkyCalcSpectralPhotonRadiance =
    Quantity<SkyCalcPhotonPerSquareMeterSecondSquareArcsecondMicrometer>;

/// Spectral radiance unit: W m⁻² sr⁻¹ µm⁻¹.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, siderust::qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "W·m⁻²·sr⁻¹·µm⁻¹",
    dimension = siderust::qtty::radiometry::SpectralRadiance,
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
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, siderust::qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "SFU",
    dimension = siderust::qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub struct SolarFluxUnit;

/// Solar radio flux in solar flux units.
pub type SolarFluxUnits = Quantity<SolarFluxUnit>;

/// Atmospheric extinction coefficient in magnitudes per airmass.
///
/// This is a dimensionless atmospheric convention. The name preserves the
/// domain meaning while allowing qtty-style construction and comparison.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, siderust::qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "mag·airmass⁻¹",
    dimension = siderust::qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub(crate) struct MagnitudePerAirmass;

/// Atmospheric extinction coefficients in mag per airmass.
pub(crate) type MagnitudesPerAirmass = Quantity<MagnitudePerAirmass>;

/// Luminance convention used by Krisciunas & Schaefer: nanolamberts.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, siderust::qtty::Unit)]
#[unit(
    crate = qtty,
    symbol = "nL",
    dimension = siderust::qtty::dimensionless::Dimensionless,
    ratio = 1.0
)]
pub(crate) struct Nanolambert;

/// Brightness in nanolamberts.
pub(crate) type Nanolamberts = Quantity<Nanolambert>;

/// Integrated photon flux over a pixel solid angle.
pub(crate) type PixelIntegratedPhotonFlux =
    Quantity<unit::Prod<unit::PhotonPerSquareCentimeterNanosecondSteradian, unit::Steradian>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::radiometry::PhotonPerSquareCentimeterNanosecondSteradianNanometer;

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
    fn solar_spectral_irradiance_uses_qtty_dimension_algebra() {
        let irradiance = SolarSpectralIrradiance::new(2.5);
        assert_eq!(irradiance.value(), 2.5);
    }
}
