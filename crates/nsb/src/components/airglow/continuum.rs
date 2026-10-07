// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::calibration::{AirglowContinuum, SAMPLE_COUNT};
use super::domain::AirglowNightPhase;
use super::extinction::{
    noll_airglow_scattering_geometry, spectral_airglow_scattering_transmission_with_geometry,
};
use super::geometry::AirglowGeometryModel;
use super::output::AirglowOutputs;
use super::selection::{AirglowPhysicalOutcome, AirglowPhysicalZeroReason};
use super::temporal::{night_phase, palace_climatology_coordinates};
use super::units::{is_valid_solar_flux, SolarFluxUnits};
use crate::error::{NsbError, Result};
use crate::site::AtmosphericConditions;
use crate::units::angular::Degrees;
use crate::units::length::Nanometers;
use crate::units::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance,
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance,
};
use crate::units::s10_for_spectral_photon_radiance;
use crate::units::ScaleFactors;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Time, UTC};

const B_FILTER: Nanometers = Nanometers::new(445.0);
const V_FILTER: Nanometers = Nanometers::new(551.0);
/// 1 R = 10^10 / (4 pi) photons m^-2 s^-1 sr^-1.
const RAYLEIGH_PER_NM_TO_BAND_SPECTRAL: f64 = 1.0e-3 / (4.0 * std::f64::consts::PI);

#[derive(Clone)]
pub(crate) struct AirglowEvaluationContext {
    pub(crate) location: Geodetic<ECEF>,
    pub(crate) atmosphere: AtmosphericConditions,
    pub(crate) geometry: AirglowGeometryModel,
    pub(crate) solar_radio_flux: SolarFluxUnits,
    pub(crate) user_scale: ScaleFactors,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpectralContinuumIntegrals {
    pub(crate) integrated: BandPhotonRadiance,
    pub(crate) integrated_uncertainty: BandPhotonRadiance,
    pub(crate) b_density: SpectralBandPhotonRadiance,
    pub(crate) v_density: SpectralBandPhotonRadiance,
}

pub(crate) fn evaluate_continuum(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: AirglowEvaluationContext,
) -> Result<AirglowOutputs> {
    validate_airglow_inputs(altitude, &ctx)?;
    let Some(phase) = night_phase(time, ctx.location) else {
        return Ok(AirglowOutputs::zero(
            AirglowPhysicalZeroReason::OutsideAstronomicalNight,
        ));
    };
    evaluate_continuum_with_night_phase_validated(continuum, time, altitude, ctx, phase)
}

pub(crate) fn validate_airglow_inputs(
    altitude: Degrees,
    ctx: &AirglowEvaluationContext,
) -> Result<()> {
    let alt = altitude.value();
    if !alt.is_finite() || alt <= -90.0 {
        return Err(NsbError::OutOfRange(
            "airglow target altitude must be finite and greater than -90 degrees".into(),
        ));
    }
    if !is_valid_solar_flux(ctx.solar_radio_flux) {
        return Err(NsbError::OutOfRange(
            "airglow solar radio flux (F10.7) must be finite and positive".into(),
        ));
    }
    if !ctx.user_scale.is_finite() || ctx.user_scale < ScaleFactors::new(0.0) {
        return Err(NsbError::OutOfRange(
            "airglow scale must be finite and non-negative".into(),
        ));
    }
    Ok(())
}

fn evaluate_continuum_with_night_phase_validated(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: AirglowEvaluationContext,
    _phase: AirglowNightPhase,
) -> Result<AirglowOutputs> {
    let spectral = integrate_attenuated_continuum(continuum, time, altitude, &ctx)?;
    let integrated_value = spectral.integrated.value();
    let relative_uncertainty = (integrated_value > 0.0)
        .then(|| spectral.integrated_uncertainty.value().abs() / integrated_value.abs());
    Ok(AirglowOutputs {
        integrated: spectral.integrated,
        b_flux_s10: s10_for_spectral_photon_radiance(spectral.b_density, B_FILTER),
        v_flux_s10: s10_for_spectral_photon_radiance(spectral.v_density, V_FILTER),
        relative_uncertainty: relative_uncertainty.filter(|value| value.is_finite()),
        physical_outcome: AirglowPhysicalOutcome::Evaluated,
        physical_zero_reason: None,
    })
}

pub(crate) fn integrate_attenuated_continuum(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: &AirglowEvaluationContext,
) -> Result<SpectralContinuumIntegrals> {
    let (month, time_bin) = palace_climatology_coordinates(time, ctx.location)
        .ok_or_else(|| NsbError::Unsupported("time cannot be represented for PALACE".into()))?;
    let zenith = Degrees::new((90.0 - altitude.value()).clamp(0.0, 90.0));
    let geometry_factor = ctx.geometry.geometry_factor(ctx.location, zenith)?.value();
    let total_scale = geometry_factor * ctx.user_scale.value();
    let scattering = noll_airglow_scattering_geometry(zenith);
    let wavelengths = continuum.wavelengths_nm();

    let mut means = [0.0; SAMPLE_COUNT];
    let mut sigmas = [0.0; SAMPLE_COUNT];
    for index in 0..SAMPLE_COUNT {
        let (mean, sigma) =
            continuum.sample(index, month, time_bin, ctx.solar_radio_flux.value())?;
        let transmission = spectral_airglow_scattering_transmission_with_geometry(
            Nanometers::new(wavelengths[index]),
            ctx.atmosphere,
            &scattering,
        )
        .value();
        means[index] = mean * transmission;
        sigmas[index] = sigma * transmission;
    }

    let integrate = |values: &[f64; SAMPLE_COUNT]| {
        values
            .windows(2)
            .zip(wavelengths.windows(2))
            .map(|(value, wavelength)| {
                0.5 * (value[0] + value[1]) * (wavelength[1] - wavelength[0])
            })
            .sum::<f64>()
            * RAYLEIGH_PER_NM_TO_BAND_SPECTRAL
            * total_scale
    };
    let density = |wavelength_nm: usize| {
        let index = wavelength_nm - 300;
        SpectralBandPhotonRadiance::new(
            means[index] * RAYLEIGH_PER_NM_TO_BAND_SPECTRAL * total_scale,
        )
    };
    Ok(SpectralContinuumIntegrals {
        integrated: BandPhotonRadiance::new(integrate(&means)),
        integrated_uncertainty: BandPhotonRadiance::new(integrate(&sigmas)),
        b_density: density(445),
        v_density: density(551),
    })
}

pub(crate) fn evaluate_integrated_continuum_with_night_phase(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: AirglowEvaluationContext,
    phase: AirglowNightPhase,
) -> Result<BandPhotonRadiance> {
    validate_airglow_inputs(altitude, &ctx)?;
    Ok(
        evaluate_continuum_with_night_phase_validated(continuum, time, altitude, ctx, phase)?
            .integrated,
    )
}

pub(crate) fn validate_integrated_continuum_inputs(
    altitude: Degrees,
    ctx: &AirglowEvaluationContext,
) -> Result<()> {
    validate_airglow_inputs(altitude, ctx)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn evaluate_continuum_with_night_phase(
        continuum: &AirglowContinuum,
        time: Time<UTC>,
        altitude: Degrees,
        ctx: AirglowEvaluationContext,
        phase: AirglowNightPhase,
    ) -> Result<AirglowOutputs> {
        validate_airglow_inputs(altitude, &ctx)?;
        super::evaluate_continuum_with_night_phase_validated(continuum, time, altitude, ctx, phase)
    }
}
