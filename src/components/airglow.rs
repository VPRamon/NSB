//! Airglow component.
//!
//! Port of `CalculateAG` from `NSB_Utils.py:1572-1596`. The Python active
//! path is a cubic polynomial in source altitude (degrees) returning
//! `ph sr⁻¹ ns⁻¹ cm⁻²` directly:
//!
//! ```text
//! airglow_param = [-1.38267419e-07, 4.71757583e-05, -5.16178594e-03, 2.96338243e-01]
//! airglow = a*alt³ + b*alt² + c*alt + d
//! ```
//!
//! The B/V S10 fluxes are hardcoded constants matching the Python file.
//!
//! The crate also exposes a SkyCalc-continuum path that uses
//! `spectra::airglow_cont::load`, solar-activity scaling, season/time
//! corrections, and a Van Rhijn geometry correction.
//!
//! Scientific role:
//! airglow is light emitted by Earth's upper atmosphere, even on moonless
//! nights. It is a terrestrial contributor rather than an astrophysical one,
//! but it is part of what astronomers actually observe from the ground.
//!
//! Contribution to the science:
//! this file provides the current first-order airglow model used by the crate.
//! It is intentionally simple: an empirical altitude-dependent polynomial that
//! approximates how the airglow contribution changes with line of sight.

use crate::error::Result;
use crate::leinert::S10_TO_W_M2_SR_UM;
use crate::spectra::airglow_cont::AirglowContinuum;
use optica::grid::OutOfRange;
use optica::spectrum::algo;
use qtty::angular::Degrees;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use siderust::atmosphere::van_rhijn_factor;
use siderust::qtty::{Kilometers, Nanometers, Radian};
use tempoch::{Time, UTC};

const AG_PARAM: [f64; 4] = [
    -1.382_674_19e-7,
    4.717_575_83e-5,
    -5.161_785_94e-3,
    2.963_382_43e-1,
];

const AG_S10_B: f64 = 163.189_810_469_037_2;
const AG_S10_V: f64 = 228.735_856_150_608_16;

/// Solar radio flux for which the `airglow_cont.dat` linear correction is 1.
///
/// The file gives `solar_corr = const + slope * F10.7`; solving with the
/// bundled constants gives this neutral default.
pub const DEFAULT_SOLAR_RADIO_FLUX_SFU: f64 = (1.0 - 2.068e-1) / 6.139e-3;

const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;
const B_FILTER_NM: f64 = 445.0;
const V_FILTER_NM: f64 = 551.0;
const ARCSEC2_PER_SR: f64 = 4.254_517_029_022_576e10;
const SKYCALC_PH_PER_S_M2_UM_ARCSEC2_TO_PH_PER_NS_CM2_NM_SR: f64 =
    1.0e-9 * 1.0e-4 * 1.0e-3 * ARCSEC2_PER_SR;
const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;

#[derive(Debug, Clone)]
pub struct AgInputs {
    /// Source altitude.
    pub altitude: Degrees,
}

#[derive(Debug, Clone)]
pub struct AgOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
}

pub fn compute(inp: &AgInputs) -> Result<AgOutputs> {
    let alt_deg = inp.altitude.value();
    let v = AG_PARAM[0] * alt_deg.powi(3)
        + AG_PARAM[1] * alt_deg.powi(2)
        + AG_PARAM[2] * alt_deg
        + AG_PARAM[3];
    Ok(AgOutputs {
        integrated: BandPhotonRadiance::new(v),
        b_flux_s10: S10::new(AG_S10_B),
        v_flux_s10: S10::new(AG_S10_V),
    })
}

/// Compute the wavelength-resolved SkyCalc-continuum airglow model.
pub fn compute_skycalc_continuum(
    inp: &AgInputs,
    continuum: &AirglowContinuum,
    time: Time<UTC>,
    solar_radio_flux_sfu: f64,
) -> Result<AgOutputs> {
    let alt = inp.altitude.value();
    if !alt.is_finite() || alt <= -90.0 {
        return Ok(AgOutputs {
            integrated: BandPhotonRadiance::zero(),
            b_flux_s10: S10::zero(),
            v_flux_s10: S10::zero(),
        });
    }

    let zenith = (90.0 - alt).clamp(0.0, 90.0);
    let van_rhijn = van_rhijn_factor(
        Degrees::new(zenith).to::<Radian>(),
        Kilometers::new(continuum.emission_height_km),
    )
    .value();
    let solar_corr =
        continuum.solar_activity_const + continuum.solar_activity_slope * solar_radio_flux_sfu;
    let season = season_bin(time);
    let time_bin = time_of_night_bin(time);
    let seasonal_corr = continuum
        .mean_corrections
        .get(time_bin)
        .and_then(|row| row.get(season))
        .copied()
        .unwrap_or(1.0);
    let scale = continuum.global_scale * solar_corr * seasonal_corr * van_rhijn;

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

    Ok(AgOutputs {
        integrated,
        b_flux_s10: spectral_photon_density_to_s10(b_density, Nanometers::new(B_FILTER_NM)),
        v_flux_s10: spectral_photon_density_to_s10(v_density, Nanometers::new(V_FILTER_NM)),
    })
}

fn spectral_photon_density_to_s10(density: f64, wavelength: Nanometers) -> S10 {
    let lambda_m = wavelength.value() * 1.0e-9;
    let photon_energy = HC_JOULE_METER / lambda_m;
    let w_m2_sr_nm = density * 1.0e13 * photon_energy;
    let w_m2_sr_um = w_m2_sr_nm * 1.0e3;
    S10::new(w_m2_sr_um / S10_TO_W_M2_SR_UM)
}

fn season_bin(time: Time<UTC>) -> usize {
    use chrono::Datelike;
    let Some(dt) = time.to_chrono() else {
        return 0;
    };
    match dt.month() {
        12 | 1 => 1,
        2 | 3 => 2,
        4 | 5 => 3,
        6 | 7 => 4,
        8 | 9 => 5,
        10 | 11 => 6,
        _ => 0,
    }
}

fn time_of_night_bin(time: Time<UTC>) -> usize {
    use chrono::Timelike;
    let Some(dt) = time.to_chrono() else {
        return 0;
    };
    match dt.hour() {
        18..=23 => 1,
        0..=5 => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectra::airglow_cont;
    use chrono::{DateTime, Utc};

    fn t() -> Time<UTC> {
        Time::<UTC>::from_chrono(
            DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
    }

    #[test]
    fn skycalc_continuum_is_positive_and_geometry_sensitive() {
        let c = airglow_cont::load().unwrap();
        let zenith = compute_skycalc_continuum(
            &AgInputs {
                altitude: Degrees::new(90.0),
            },
            &c,
            t(),
            DEFAULT_SOLAR_RADIO_FLUX_SFU,
        )
        .unwrap();
        let low = compute_skycalc_continuum(
            &AgInputs {
                altitude: Degrees::new(30.0),
            },
            &c,
            t(),
            DEFAULT_SOLAR_RADIO_FLUX_SFU,
        )
        .unwrap();
        assert!(zenith.integrated > BandPhotonRadiance::zero());
        assert!(low.integrated > zenith.integrated);
        assert!(low.b_flux_s10 > S10::zero());
        assert!(low.v_flux_s10 > S10::zero());
    }

    #[test]
    fn skycalc_continuum_below_horizon_returns_zero() {
        let c = airglow_cont::load().unwrap();
        // Altitude below −90° (non-physical) should return zero contribution.
        let out = compute_skycalc_continuum(
            &AgInputs {
                altitude: Degrees::new(-91.0),
            },
            &c,
            t(),
            DEFAULT_SOLAR_RADIO_FLUX_SFU,
        )
        .unwrap();
        assert_eq!(
            out.integrated,
            BandPhotonRadiance::zero(),
            "below-horizon altitude must yield zero airglow"
        );
    }

    #[test]
    fn skycalc_continuum_nan_altitude_returns_zero() {
        let c = airglow_cont::load().unwrap();
        let out = compute_skycalc_continuum(
            &AgInputs {
                altitude: Degrees::new(f64::NAN),
            },
            &c,
            t(),
            DEFAULT_SOLAR_RADIO_FLUX_SFU,
        )
        .unwrap();
        assert_eq!(
            out.integrated,
            BandPhotonRadiance::zero(),
            "NaN altitude must yield zero airglow"
        );
    }

    #[test]
    fn skycalc_continuum_solar_scaling_changes_result() {
        let c = airglow_cont::load().unwrap();
        let inp = AgInputs {
            altitude: Degrees::new(60.0),
        };
        let low_flux = compute_skycalc_continuum(&inp, &c, t(), 50.0).unwrap();
        let high_flux = compute_skycalc_continuum(&inp, &c, t(), 250.0).unwrap();
        assert!(
            low_flux.integrated != high_flux.integrated,
            "solar radio flux scaling must change airglow output"
        );
    }
}
