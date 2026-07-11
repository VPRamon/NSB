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
use crate::spectrum::{
    interpolate_spectral_photon_radiance_nm, spectral_photon_radiance_values,
    trapz_spectral_photon_radiance_nm, wavelength_values_nm,
};
use crate::units::{s10_for_spectral_photon_radiance, S10_TO_W_M2_SR_NM};
use optica::spectrum::SampledSpectrum;

use super::extinction::ZodiacalExtinction;
use super::geometry::ZodiacalGeometry;
use super::leinert::{Leinert1998Grid, LEINERT_S10_TO_W_M2_SR_UM};
use super::output::{ZodiacalOutputs, ZodiacalSpectrum};
use super::reddening::reddening_factor;

use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::Interpolation;
use qtty::angular::Degrees;
use qtty::radiometry::{
    spectral_radiance_to_photon_radiance_ns_nm,
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance,
    S10s as S10, WattsPerSquareMeterSteradianNanometer,
};
use siderust::qtty::{length::Meter, Nanometer, Nanometers};

pub(super) const WL_LOW: Nanometers = Nanometers::new(300.0);
pub(super) const WL_HIGH: Nanometers = Nanometers::new(650.0);
pub(super) const B_FILTER: Nanometers = Nanometers::new(445.0);
pub(super) const V_FILTER: Nanometers = Nanometers::new(551.0);
const S10_SCALE_WAVELENGTH: Nanometers = Nanometers::new(500.0);

/// Compute scalar zodiacal outputs using the default Leinert brightness source.
pub(super) fn compute_outputs(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
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
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
    s10_500: S10,
) -> Result<ZodiacalOutputs> {
    let k = spectral_scale_from_s10(s10_500, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));
    let (wavelengths, photon_radiance, b_zl_um, v_zl_um) =
        zodiacal_samples(geom, solar, extinction, k, zenith)?;

    let integrated =
        trapz_spectral_photon_radiance_nm(&wavelengths, &photon_radiance, WL_LOW, WL_HIGH);
    let (b_flux, v_flux) = interpolate_bv(&wavelengths, &photon_radiance, b_zl_um, v_zl_um);

    Ok(ZodiacalOutputs {
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

/// Compute the full zodiacal spectrum using the default Leinert brightness source.
pub(super) fn compute_spectrum(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
) -> Result<ZodiacalSpectrum> {
    let s10_500 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)?;
    compute_spectrum_with_s10(geom, solar, extinction, s10_500)
}

/// Compute the full zodiacal spectrum from an explicit 500 nm S10 brightness.
pub(super) fn compute_spectrum_with_s10(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
    s10_500: S10,
) -> Result<ZodiacalSpectrum> {
    let k = spectral_scale_from_s10(s10_500, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));
    let (wavelengths, photon_radiance, b_zl_um, v_zl_um) =
        zodiacal_samples(geom, solar, extinction, k, zenith)?;

    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        wavelength_values_nm(&wavelengths),
        spectral_photon_radiance_values(&photon_radiance),
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("zodiacal")),
    )
    .map_err(|e| NsbError::Interpolation(format!("zodiacal spectrum: {e}")))?;

    let integrated =
        trapz_spectral_photon_radiance_nm(&wavelengths, &photon_radiance, WL_LOW, WL_HIGH);
    let (b_flux, v_flux) = interpolate_bv(&wavelengths, &photon_radiance, b_zl_um, v_zl_um);

    Ok(ZodiacalSpectrum {
        spectrum,
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

fn zodiacal_samples(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
    scale: f64,
    zenith: Degrees,
) -> Result<(Vec<Nanometers>, Vec<SpectralBandPhotonRadiance>, f64, f64)> {
    let solar_xs = solar.xs_raw();
    let solar_ys = solar.ys_raw();

    let mut wavelengths: Vec<Nanometers> = Vec::new();
    let mut photon_radiance: Vec<SpectralBandPhotonRadiance> = Vec::new();
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

        let f_sun_sr = solar_flux / std::f64::consts::PI;
        let zl = WattsPerSquareMeterSteradianNanometer::new(
            f_sun_sr * scale * reddening_factor(geom.beta, geom.delta_lambda, lambda_nm),
        );
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
        wavelengths.push(wavelength);
        photon_radiance.push(density);
    }

    if wavelengths.is_empty() {
        return Err(NsbError::OutOfRange(
            "solar spectrum has no samples in the 300–650 nm zodiacal band".to_string(),
        ));
    }

    Ok((wavelengths, photon_radiance, b_zl_um, v_zl_um))
}

fn spectral_scale_from_s10(s10_500: S10, solar: &SampledSpectrum<Nanometer, Meter>) -> Result<f64> {
    if !s10_500.is_finite() || s10_500.value() < 0.0 {
        return Err(NsbError::OutOfRange(format!(
            "zodiacal 500 nm S10 brightness must be finite and non-negative, got {}",
            s10_500.value()
        )));
    }
    let target_500 = (S10_TO_W_M2_SR_NM * s10_500.value()).value();
    let f_sun_500 = solar.interp_at(S10_SCALE_WAVELENGTH).value();
    let f_sun_500_sr = f_sun_500 / std::f64::consts::PI;
    if !f_sun_500_sr.is_finite() || f_sun_500_sr <= 0.0 {
        return Err(NsbError::DataParse {
            file: "solar_spectrum.dat",
            message: "non-positive flux at 500 nm".into(),
        });
    }
    Ok(target_500 / f_sun_500_sr)
}

fn interpolate_bv(
    wavelengths: &[Nanometers],
    photon_radiance: &[SpectralBandPhotonRadiance],
    b_zl_um: f64,
    v_zl_um: f64,
) -> (S10, S10) {
    let b_s10 = interp_linear_or_nearest(wavelengths, photon_radiance, B_FILTER, b_zl_um);
    let v_s10 = interp_linear_or_nearest(wavelengths, photon_radiance, V_FILTER, v_zl_um);
    (b_s10, v_s10)
}

fn interp_linear_or_nearest(
    wavelengths: &[Nanometers],
    photon_radiance: &[SpectralBandPhotonRadiance],
    target: Nanometers,
    fallback_zl_um: f64,
) -> S10 {
    if let Some(density) =
        interpolate_spectral_photon_radiance_nm(wavelengths, photon_radiance, target)
    {
        return s10_for_spectral_photon_radiance(density, target);
    }

    S10::new(fallback_zl_um / LEINERT_S10_TO_W_M2_SR_UM.value())
}
