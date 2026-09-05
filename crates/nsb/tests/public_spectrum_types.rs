use nsb::{AirglowContinuum, SolarSpectralIrradianceUnit, ZodiacalLight, ZodiacalSpectrum};
use optica::spectrum::SampledSpectrum;
use qtty::length::Nanometer;
use qtty::radiometry::PhotonPerSquareCentimeterNanosecondSteradianNanometer;
use qtty::unit::Ratio;

#[test]
fn public_spectrum_types_encode_physical_units() {
    fn assert_airglow_types(continuum: &AirglowContinuum) {
        let _: &SampledSpectrum<Nanometer, Ratio> = &continuum.spectrum;
        let _: &SampledSpectrum<Nanometer, Ratio> = &continuum.uncertainty;
    }

    fn assert_zodiacal_type(output: &ZodiacalSpectrum) {
        let _: &SampledSpectrum<Nanometer, PhotonPerSquareCentimeterNanosecondSteradianNanometer> =
            &output.spectrum;
    }

    fn assert_solar_setter(
        model: ZodiacalLight,
        spectrum: SampledSpectrum<Nanometer, SolarSpectralIrradianceUnit>,
    ) -> ZodiacalLight {
        model.with_solar_spectrum(spectrum)
    }

    let _: fn(&AirglowContinuum) = assert_airglow_types;
    let _: fn(&ZodiacalSpectrum) = assert_zodiacal_type;
    let _: fn(
        ZodiacalLight,
        SampledSpectrum<Nanometer, SolarSpectralIrradianceUnit>,
    ) -> ZodiacalLight = assert_solar_setter;
}
