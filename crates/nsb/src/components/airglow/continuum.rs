use super::calibration::AirglowContinuum;
use super::extinction::{
    noll_airglow_scattering_geometry, spectral_airglow_scattering_transmission_with_geometry,
};
use super::geometry::AirglowGeometryModel;
use super::output::AirglowOutputs;
use super::temporal::{season_bin, time_of_night_bin};
use super::units::{is_valid_solar_flux, SolarFluxUnits};
use crate::components::moonlight::AtmosphericConditions;
use crate::error::Result;
use crate::units::ScaleFactors;
use crate::units::{s10_for_spectral_photon_radiance, SkyCalcSpectralPhotonRadiance};
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};
use qtty::angular::Degrees;
use qtty::dimensionless::Ratios;
use qtty::length::{Nanometer, Nanometers};
use qtty::radiometry::{
    PhotonPerSquareCentimeterNanosecondSteradian as BandPhotonRadianceUnit,
    PhotonPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadianceUnit,
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance,
};
use qtty::unit::Ratio;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use tempoch::{Time, UTC};

const WL_LOW: Nanometers = Nanometers::new(300.0);
const WL_HIGH: Nanometers = Nanometers::new(650.0);
const B_FILTER: Nanometers = Nanometers::new(445.0);
const V_FILTER: Nanometers = Nanometers::new(551.0);

pub(crate) struct SpectralContinuumIntegrals {
    pub(crate) integrated_relative: Nanometers,
    pub(crate) integrated_uncertainty_abs: Nanometers,
    pub(crate) b_relative: Ratios,
    pub(crate) v_relative: Ratios,
}

pub(crate) fn integrate_attenuated_continuum(
    continuum: &AirglowContinuum,
    zenith: Degrees,
    atmosphere: AtmosphericConditions,
) -> SpectralContinuumIntegrals {
    let xs = continuum.spectrum.xs_raw();
    let ys = continuum.spectrum.ys_raw();
    let sigs = continuum.uncertainty.ys_raw();
    let geometry = noll_airglow_scattering_geometry(zenith);

    let mut attenuated_ys = Vec::with_capacity(ys.len());
    let mut attenuated_sigs = Vec::with_capacity(sigs.len());
    for (idx, &wl_nm) in xs.iter().enumerate() {
        let wavelength = Nanometers::new(wl_nm);
        let transmission = spectral_airglow_scattering_transmission_with_geometry(
            wavelength, atmosphere, &geometry,
        )
        .value();
        attenuated_ys.push(ys[idx] * transmission);
        attenuated_sigs.push((sigs[idx] * transmission).abs());
    }

    let attenuated = SampledSpectrum::<Nanometer, Ratio>::from_raw(
        xs.to_vec(),
        attenuated_ys,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .expect("validated airglow spectrum remains valid after attenuation");
    let attenuated_uncertainty = SampledSpectrum::<Nanometer, Ratio>::from_raw(
        xs.to_vec(),
        attenuated_sigs,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .expect("validated airglow uncertainty spectrum remains valid after attenuation");

    let integrated_relative = attenuated
        .integrate_range(WL_LOW, WL_HIGH)
        .to::<Nanometer>();
    let integrated_uncertainty_abs = attenuated_uncertainty
        .integrate_range(WL_LOW, WL_HIGH)
        .to::<Nanometer>();
    let b_relative = attenuated.interp_at(B_FILTER);
    let v_relative = attenuated.interp_at(V_FILTER);

    SpectralContinuumIntegrals {
        integrated_relative,
        integrated_uncertainty_abs,
        b_relative,
        v_relative,
    }
}

pub(crate) struct AirglowEvaluationContext {
    pub(crate) location: Geodetic<ECEF>,
    pub(crate) atmosphere: AtmosphericConditions,
    pub(crate) geometry: AirglowGeometryModel,
    pub(crate) solar_radio_flux: SolarFluxUnits,
    pub(crate) user_scale: ScaleFactors,
}

pub(crate) fn evaluate_continuum(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: AirglowEvaluationContext,
) -> Result<AirglowOutputs> {
    let Some(time_bin) = time_of_night_bin(time, ctx.location) else {
        return Ok(AirglowOutputs::zero());
    };
    evaluate_continuum_with_time_bin(continuum, time, altitude, ctx, time_bin)
}

pub(crate) fn evaluate_continuum_with_time_bin(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    altitude: Degrees,
    ctx: AirglowEvaluationContext,
    time_bin: usize,
) -> Result<AirglowOutputs> {
    let alt = altitude.value();
    if !alt.is_finite()
        || alt <= -90.0
        || !is_valid_solar_flux(ctx.solar_radio_flux)
        || !ctx.user_scale.is_finite()
        || ctx.user_scale < ScaleFactors::new(0.0)
    {
        return Ok(AirglowOutputs::zero());
    }

    let zenith_deg = (90.0 - alt).clamp(0.0, 90.0);
    let zenith = Degrees::new(zenith_deg);
    let geometry_factor = ctx.geometry.geometry_factor(ctx.location, zenith)?.value();
    let solar_corr = continuum.solar_activity_const
        + continuum.solar_activity_slope * ctx.solar_radio_flux.value();
    let season_bin = season_bin(time, ctx.location);
    let seasonal_corr = continuum
        .mean_corrections
        .get(time_bin)
        .and_then(|row| row.get(season_bin))
        .copied()
        .unwrap_or(1.0);
    let user_scale = ctx.user_scale.value();
    // Emitting-volume LOS geometry is scalar. Noll effective Rayleigh/Mie
    // atmospheric scattering remains an independent spectral stage (#114).
    let scalar_scale =
        continuum.global_scale.value() * solar_corr * seasonal_corr * geometry_factor * user_scale;

    let spectral = integrate_attenuated_continuum(continuum, zenith, ctx.atmosphere);

    let radiance_scale: SpectralBandPhotonRadiance =
        SkyCalcSpectralPhotonRadiance::new(scalar_scale).to::<SpectralBandPhotonRadianceUnit>();
    let integrated = (radiance_scale * spectral.integrated_relative).to::<BandPhotonRadianceUnit>();

    let relative_uncertainty = continuum
        .sigma_corrections
        .get(time_bin)
        .and_then(|row| row.get(season_bin))
        .copied()
        .and_then(|seasonal_sigma| {
            let integrated_value = integrated.value().abs();
            let seasonal_corr_value = seasonal_corr.abs();
            if integrated_value <= 0.0 || seasonal_corr_value <= 0.0 {
                return None;
            }

            let common_scale = continuum.global_scale.abs().value()
                * solar_corr.abs()
                * seasonal_corr_value
                * geometry_factor.abs()
                * user_scale;
            let uncertainty_scale: SpectralBandPhotonRadiance =
                SkyCalcSpectralPhotonRadiance::new(common_scale)
                    .to::<SpectralBandPhotonRadianceUnit>();
            let shape_sigma_integrated = (uncertainty_scale * spectral.integrated_uncertainty_abs)
                .to::<BandPhotonRadianceUnit>()
                .value();
            let level_relative_uncertainty = seasonal_sigma.abs() / seasonal_corr_value;
            let shape_relative_uncertainty = shape_sigma_integrated / integrated_value;
            let relative_uncertainty = level_relative_uncertainty.hypot(shape_relative_uncertainty);

            relative_uncertainty
                .is_finite()
                .then_some(relative_uncertainty)
        });

    // qtty's Ratio marker is intentionally not registered as a built-in unit
    // arithmetic operand. Extracting the dimensionless scalar here preserves
    // the physical unit of `radiance_scale` while avoiding any unit erasure in
    // interpolation or integration.
    let b_density = radiance_scale * spectral.b_relative.value();
    let v_density = radiance_scale * spectral.v_relative.value();

    Ok(AirglowOutputs {
        integrated,
        b_flux_s10: s10_for_spectral_photon_radiance(b_density, B_FILTER),
        v_flux_s10: s10_for_spectral_photon_radiance(v_density, V_FILTER),
        relative_uncertainty,
    })
}
