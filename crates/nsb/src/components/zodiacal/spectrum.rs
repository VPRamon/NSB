//! Zodiacal-light spectral computation.
//!
//! Builds the wavelength-resolved and integrated zodiacal photon radiance from:
//!
//! 1. An S10 brightness at 500 nm (from the Leinert grid or a custom source).
//! 2. A solar spectrum (scaled to match the 500 nm S10 value).
//! 3. A wavelength-dependent reddening factor (Leinert 1997).
//! 4. An optional atmospheric extinction strategy.
//!
//! The integration covers the 300–650 nm band. B/V surface brightnesses are
//! computed by interpolation at exactly 445 nm and 551 nm respectively.

use crate::error::{NsbError, Result};
use crate::reference::solar::SolarSpectrum;
use crate::units::{
    s10_for_spectral_photon_radiance, SolarSpectralIrradiance, S10_TO_W_M2_SR_NM,
};
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};

use super::extinction::ZodiacalExtinction;
use super::geometry::ZodiacalGeometry;
use super::leinert::{Leinert1998Grid, LEINERT_S10_TO_W_M2_SR_UM};
use super::output::{ZodiacalOutputs, ZodiacalSpectrum};
use super::reddening::reddening_factor;

use qtty::angular::Degrees;
use qtty::length::{Nanometer, Nanometers};
use qtty::radiometry::{
    spectral_radiance_to_photon_radiance_ns_nm,
    PhotonPerSquareCentimeterNanosecondSteradian as BandPhotonRadianceUnit,
    PhotonPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadianceUnit,
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    WattsPerSquareMeterSteradianNanometer,
};

pub(super) const WL_LOW: Nanometers = Nanometers::new(300.0);
pub(super) const WL_HIGH: Nanometers = Nanometers::new(650.0);
pub(super) const B_FILTER: Nanometers = Nanometers::new(445.0);
pub(super) const V_FILTER: Nanometers = Nanometers::new(551.0);
const S10_SCALE_WAVELENGTH: Nanometers = Nanometers::new(500.0);

type ZodiacalPhotonSpectrum = SampledSpectrum<Nanometer, SpectralBandPhotonRadianceUnit>;

/// Compute scalar zodiacal outputs using the default Leinert brightness source.
pub(super) fn compute_outputs(
    geom: &ZodiacalGeometry,
    solar: &SolarSpectrum,
    extinction: ZodiacalExtinction,
) -> Result<ZodiacalOutputs> {
    let s10_500 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)?;
    compute_outputs_with_s10(geom, solar, extinction, s10_500)
}

/// Compute scalar zodiacal outputs from an explicit 500 nm S10 brightness.
///
/// This is used by custom brightness grids so they do not need to be routed
/// through the built-in Leinert lookup.
pub(super) fn compute_outputs_with_s10(
    geom: &ZodiacalGeometry,
    solar: &SolarSpectrum,
    extinction: ZodiacalExtinction,
    s10_500: S10,
) -> Result<ZodiacalOutputs> {
    let k = spectral_scale_from_s10(s10_500, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));
    let (spectrum, b_zl_um, v_zl_um) = zodiacal_samples(geom, solar, extinction, k, zenith)?;

    let integrated = integrate_photon_spectrum(&spectrum);
    let (b_flux, v_flux) = interpolate_bv(&spectrum, b_zl_um, v_zl_um);

    Ok(ZodiacalOutputs {
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

/// Compute the full zodiacal spectrum using the default Leinert brightness source.
pub(super) fn compute_spectrum(
    geom: &ZodiacalGeometry,
    solar: &SolarSpectrum,
    extinction: ZodiacalExtinction,
) -> Result<ZodiacalSpectrum> {
    let s10_500 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)?;
    compute_spectrum_with_s10(geom, solar, extinction, s10_500)
}

/// Compute the full zodiacal spectrum from an explicit 500 nm S10 brightness.
pub(super) fn compute_spectrum_with_s10(
    geom: &ZodiacalGeometry,
    solar: &SolarSpectrum,
    extinction: ZodiacalExtinction,
    s10_500: S10,
) -> Result<ZodiacalSpectrum> {
    let k = spectral_scale_from_s10(s10_500, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));
    let (spectrum, b_zl_um, v_zl_um) = zodiacal_samples(geom, solar, extinction, k, zenith)?;

    let integrated = integrate_photon_spectrum(&spectrum);
    let (b_flux, v_flux) = interpolate_bv(&spectrum, b_zl_um, v_zl_um);

    Ok(ZodiacalSpectrum {
        spectrum,
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

fn zodiacal_samples(
    geom: &ZodiacalGeometry,
    solar: &SolarSpectrum,
    extinction: ZodiacalExtinction,
    scale: f64,
    zenith: Degrees,
) -> Result<(ZodiacalPhotonSpectrum, f64, f64)> {
    let solar_xs = solar.xs_raw();
    let solar_ys = solar.ys_raw();

    let mut wavelengths_nm = Vec::new();
    let mut photon_radiance = Vec::new();
    let (mut b_zl_um, mut v_zl_um) = (0.0_f64, 0.0_f64);
    let (mut b_dist, mut v_dist) = (
        Nanometers::new(f64::INFINITY),
        Nanometers::new(f64::INFINITY),
    );

    for (&lambda_nm, &solar_flux) in solar_xs.iter().zip(solar_ys.iter()) {
        let wavelength = Nanometers::new(lambda_nm);
        if !wavelength.is_finite() || wavelength < WL_LOW || wavelength > WL_HIGH {
            continue;
        }

        let solar_irradiance = SolarSpectralIrradiance::new(solar_flux);
        let f_sun_sr = solar_irradiance_to_mean_radiance(solar_irradiance);
        let zl = f_sun_sr * scale * reddening_factor(geom.beta, geom.delta_lambda, lambda_nm);
        let trans = extinction.transmission_for_spectral_radiance(zl, wavelength, zenith);
        let zl_ext = zl * trans.value();
        let zl_ext_um = zl_ext
            .to::<crate::units::WattPerSquareMeterSteradianMicrometer>()
            .value();

        let b_delta = (wavelength - B_FILTER).abs();
        if b_delta < b_dist {
            b_dist = b_delta;
            b_zl_um = zl_ext_um;
        }

        let v_delta = (wavelength - V_FILTER).abs();
        if v_delta < v_dist {
            v_dist = v_delta;
            v_zl_um = zl_ext_um;
        }

        let density = spectral_radiance_to_photon_radiance_ns_nm(zl_ext, wavelength);
        wavelengths_nm.push(lambda_nm);
        photon_radiance.push(density.value());
    }

    if wavelengths_nm.is_empty() {
        return Err(NsbError::OutOfRange(
            "solar spectrum has no samples in the 300–650 nm zodiacal band".to_string(),
        ));
    }

    let spectrum = ZodiacalPhotonSpectrum::from_raw(
        wavelengths_nm,
        photon_radiance,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("zodiacal")),
    )
    .map_err(|e| NsbError::Interpolation(format!("zodiacal spectrum: {e}")))?;

    Ok((spectrum, b_zl_um, v_zl_um))
}

fn integrate_photon_spectrum(spectrum: &ZodiacalPhotonSpectrum) -> BandPhotonRadiance {
    spectrum
        .integrate_range(WL_LOW, WL_HIGH)
        .to::<BandPhotonRadianceUnit>()
}

/// Convert the solar spectral irradiance convention to the mean radiance used
/// by the historical zodiacal model (`F_sun / π`).
///
/// This is a domain transform, not a unit conversion: the division by π sr
/// encodes the model convention. Both sides remain explicitly typed, while the
/// scalar extraction is confined to this physical boundary instead of being
/// used to drive interpolation or integration kernels.
fn solar_irradiance_to_mean_radiance(
    irradiance: SolarSpectralIrradiance,
) -> WattsPerSquareMeterSteradianNanometer {
    WattsPerSquareMeterSteradianNanometer::new(irradiance.value() / std::f64::consts::PI)
}

fn spectral_scale_from_s10(s10_500: S10, solar: &SolarSpectrum) -> Result<f64> {
    if !s10_500.is_finite() || s10_500.value() < 0.0 {
        return Err(NsbError::OutOfRange(format!(
            "zodiacal 500 nm S10 brightness must be finite and non-negative, got {}",
            s10_500.value()
        )));
    }
    let target_500 = S10_TO_W_M2_SR_NM * s10_500.value();
    let f_sun_500 = solar.interp_at(S10_SCALE_WAVELENGTH);
    let f_sun_500_sr = solar_irradiance_to_mean_radiance(f_sun_500);
    if !f_sun_500_sr.is_finite() || f_sun_500_sr.value() <= 0.0 {
        return Err(NsbError::DataParse {
            file: "solar_spectrum.dat",
            message: "non-positive flux at 500 nm".into(),
        });
    }
    Ok(target_500 / f_sun_500_sr)
}

fn interpolate_bv(
    spectrum: &ZodiacalPhotonSpectrum,
    b_zl_um: f64,
    v_zl_um: f64,
) -> (S10, S10) {
    let b_s10 = interp_linear_or_nearest(spectrum, B_FILTER, b_zl_um);
    let v_s10 = interp_linear_or_nearest(spectrum, V_FILTER, v_zl_um);
    (b_s10, v_s10)
}

fn interp_linear_or_nearest(
    spectrum: &ZodiacalPhotonSpectrum,
    target: Nanometers,
    fallback_zl_um: f64,
) -> S10 {
    if spectrum.contains(target) {
        if let Ok(density) = spectrum.try_interp_at(target) {
            return s10_for_spectral_photon_radiance(density, target);
        }
    }

    S10::new(fallback_zl_um / LEINERT_S10_TO_W_M2_SR_UM.value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtty::radiometry::PhotonPerSquareCentimeterNanosecondSteradianNanometer;

    #[test]
    fn typed_zodiacal_spectrum_integrates_to_band_photon_radiance() {
        let spectrum = ZodiacalPhotonSpectrum::from_raw(
            vec![300.0, 650.0],
            vec![1.0, 1.0],
            Interpolation::Linear,
            OutOfRange::ClampToEndpoints,
            None,
        )
        .expect("typed spectrum");

        let integrated = integrate_photon_spectrum(&spectrum);
        assert!((integrated.value() - 350.0).abs() < 1.0e-12);
        let midpoint = spectrum.interp_at(Nanometers::new(475.0));
        let _: qtty::Quantity<PhotonPerSquareCentimeterNanosecondSteradianNanometer> = midpoint;
    }
}
