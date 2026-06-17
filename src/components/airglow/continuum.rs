use super::output::AirglowOutputs;
use super::temporal::{season_bin, time_of_night_bin};
use super::units::SolarFluxUnits;
use crate::leinert::S10_TO_W_M2_SR_UM;
use crate::spectra::airglow_cont::AirglowContinuum;
use optica::grid::OutOfRange;
use optica::spectrum::algo;
use qtty::angular::Degrees;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use siderust::atmosphere::van_rhijn_factor;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Kilometers, Nanometers, Radian};
use tempoch::{Time, UTC};

const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;
const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;
const ARCSEC2_PER_SR: f64 = 4.254_517_029_022_576e10;
const SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR: f64 =
    1.0e-9 * 1.0e-4 * 1.0e-3 * ARCSEC2_PER_SR;
const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;

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
    let seasonal_corr = continuum
        .mean_corrections
        .get(time_of_night_bin(time, location))
        .and_then(|row| row.get(season_bin(time, location)))
        .copied()
        .unwrap_or(1.0);
    let scale = continuum.global_scale * solar_corr * seasonal_corr * van_rhijn * user_scale;

    let lam = continuum.spectrum.xs_raw();
    let rel = continuum.spectrum.ys_raw();
    let flux: Vec<f64> = rel
        .iter()
        .map(|&r| r * scale * SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR)
        .collect();
    let integrated = BandPhotonRadiance::new(algo::trapz_range(lam, &flux, WL_LOW_NM, WL_HIGH_NM));

    let b_density = algo::interp_linear(lam, &flux, B_FILTER_NM, OutOfRange::ClampToEndpoints)
        .expect("airglow B interpolation");
    let v_density = algo::interp_linear(lam, &flux, V_FILTER_NM, OutOfRange::ClampToEndpoints)
        .expect("airglow V interpolation");

    AirglowOutputs {
        integrated,
        b_flux_s10: spectral_photon_density_to_s10(b_density, Nanometers::new(B_FILTER_NM)),
        v_flux_s10: spectral_photon_density_to_s10(v_density, Nanometers::new(V_FILTER_NM)),
    }
}

fn spectral_photon_density_to_s10(density: f64, wavelength: Nanometers) -> S10s {
    let lambda_m = wavelength.value() * 1.0e-9;
    let photon_energy = HC_JOULE_METER / lambda_m;
    let w_m2_sr_nm = density * 1.0e13 * photon_energy;
    let w_m2_sr_um = w_m2_sr_nm * 1.0e3;
    S10s::new(w_m2_sr_um / S10_TO_W_M2_SR_UM)
}
