use super::calibration::AirglowContinuum;
use super::output::AirglowOutputs;
use super::temporal::{season_bin, time_of_night_bin};
use super::units::{is_valid_solar_flux, SolarFluxUnits};
use crate::units::ScaleFactors;
use crate::units::{s10_for_spectral_photon_radiance, SkyCalcSpectralPhotonRadiance};
use qtty::angular::Degrees;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance,
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance,
};
use qtty::unit;
use siderust::atmosphere::van_rhijn_factor;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Nanometers, Radian};
use tempoch::{Time, UTC};

const B_FILTER: Nanometers = Nanometers::new(445.0);
const V_FILTER: Nanometers = Nanometers::new(551.0);

pub(crate) fn evaluate_continuum(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    altitude: Degrees,
    solar_radio_flux: SolarFluxUnits,
    user_scale: ScaleFactors,
) -> AirglowOutputs {
    let Some(time_bin) = time_of_night_bin(time, location) else {
        return AirglowOutputs::zero();
    };
    evaluate_continuum_with_time_bin(
        continuum,
        time,
        location,
        altitude,
        solar_radio_flux,
        user_scale,
        time_bin,
    )
}

pub(crate) fn evaluate_continuum_with_time_bin(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    altitude: Degrees,
    solar_radio_flux: SolarFluxUnits,
    user_scale: ScaleFactors,
    time_bin: usize,
) -> AirglowOutputs {
    let alt = altitude.value();
    if !alt.is_finite()
        || alt <= -90.0
        || !is_valid_solar_flux(solar_radio_flux)
        || !user_scale.is_finite()
        || user_scale < ScaleFactors::new(0.0)
    {
        return AirglowOutputs::zero();
    }

    let zenith = (90.0 - alt).clamp(0.0, 90.0);
    let van_rhijn = van_rhijn_factor(
        Degrees::new(zenith).to::<Radian>(),
        continuum.emission_height_km,
    )
    .value();
    let solar_corr =
        continuum.solar_activity_const + continuum.solar_activity_slope * solar_radio_flux.value();
    let season_bin = season_bin(time, location);
    let seasonal_corr = continuum
        .mean_corrections
        .get(time_bin)
        .and_then(|row| row.get(season_bin))
        .copied()
        .unwrap_or(1.0);
    let user_scale = user_scale.value();
    let scale =
        continuum.global_scale.value() * solar_corr * seasonal_corr * van_rhijn * user_scale;

    let radiance_scale = SkyCalcSpectralPhotonRadiance::new(scale)
        .to::<unit::PhotonPerSquareCentimeterNanosecondSteradianNanometer>()
        .value();
    let integrated =
        BandPhotonRadiance::new(continuum.integrated_relative_300_650 * radiance_scale);

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
                * van_rhijn.abs()
                * user_scale;
            let shape_sigma_integrated = continuum.integrated_uncertainty_abs_300_650
                * SkyCalcSpectralPhotonRadiance::new(common_scale)
                    .to::<unit::PhotonPerSquareCentimeterNanosecondSteradianNanometer>()
                    .value();
            let level_relative_uncertainty = seasonal_sigma.abs() / seasonal_corr_value;
            let shape_relative_uncertainty = shape_sigma_integrated / integrated_value;
            let relative_uncertainty = level_relative_uncertainty.hypot(shape_relative_uncertainty);

            relative_uncertainty
                .is_finite()
                .then_some(relative_uncertainty)
        });

    let b_density = continuum.b_relative * radiance_scale;
    let v_density = continuum.v_relative * radiance_scale;

    AirglowOutputs {
        integrated,
        b_flux_s10: s10_for_spectral_photon_radiance(
            SpectralBandPhotonRadiance::new(b_density),
            B_FILTER,
        ),
        v_flux_s10: s10_for_spectral_photon_radiance(
            SpectralBandPhotonRadiance::new(v_density),
            V_FILTER,
        ),
        relative_uncertainty,
    }
}
