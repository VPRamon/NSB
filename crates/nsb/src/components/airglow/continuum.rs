use super::calibration::AirglowContinuum;
use super::output::AirglowOutputs;
use super::temporal::{season_bin, time_of_night_bin};
use super::units::SolarFluxUnits;
use qtty::angular::Degrees;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use siderust::atmosphere::van_rhijn_factor;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Kilometers, Nanometers, Radian};
use tempoch::{Time, UTC};

const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;
const ARCSEC2_PER_SR: f64 = 4.254_517_029_022_576e10;
const SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR: f64 =
    1.0e-9 * 1.0e-4 * 1.0e-3 * ARCSEC2_PER_SR;
const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;
const S10_TO_W_M2_SR_UM: f64 = 1.28e-8;

pub(crate) fn evaluate_continuum(
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    altitude: Degrees,
    solar_radio_flux: SolarFluxUnits,
    user_scale: f64,
) -> AirglowOutputs {
    let alt = altitude.value();
    if !alt.is_finite()
        || alt <= -90.0
        || !solar_radio_flux.is_valid()
        || !user_scale.is_finite()
        || user_scale < 0.0
    {
        return AirglowOutputs::zero();
    }

    let zenith = (90.0 - alt).clamp(0.0, 90.0);
    let van_rhijn = van_rhijn_factor(
        Degrees::new(zenith).to::<Radian>(),
        Kilometers::new(continuum.emission_height_km),
    )
    .value();
    let solar_corr =
        continuum.solar_activity_const + continuum.solar_activity_slope * solar_radio_flux.value();
    let Some(time_bin) = time_of_night_bin(time, location) else {
        return AirglowOutputs::zero();
    };
    let season_bin = season_bin(time, location);
    let seasonal_corr = continuum
        .mean_corrections
        .get(time_bin)
        .and_then(|row| row.get(season_bin))
        .copied()
        .unwrap_or(1.0);
    let scale = continuum.global_scale * solar_corr * seasonal_corr * van_rhijn * user_scale;

    let radiance_scale = scale * SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR;
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

            let common_scale = continuum.global_scale.abs()
                * solar_corr.abs()
                * seasonal_corr_value
                * van_rhijn.abs()
                * user_scale
                * SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR;
            let shape_sigma_integrated =
                continuum.integrated_uncertainty_abs_300_650 * common_scale;
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
        b_flux_s10: spectral_photon_density_to_s10(b_density, Nanometers::new(B_FILTER_NM)),
        v_flux_s10: spectral_photon_density_to_s10(v_density, Nanometers::new(V_FILTER_NM)),
        relative_uncertainty,
    }
}

fn spectral_photon_density_to_s10(density: f64, wavelength: Nanometers) -> S10s {
    let lambda_m = wavelength.value() * 1.0e-9;
    let photon_energy = HC_JOULE_METER / lambda_m;
    let w_m2_sr_nm = density * 1.0e13 * photon_energy;
    let w_m2_sr_um = w_m2_sr_nm * 1.0e3;
    S10s::new(w_m2_sr_um / S10_TO_W_M2_SR_UM)
}
