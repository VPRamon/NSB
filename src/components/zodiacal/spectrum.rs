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
//! computed by **interpolation** at exactly 445 nm and 551 nm respectively
//! (not nearest-sample selection).
//!
//! # Constants
//!
//! | Name              | Value   | Meaning                               |
//! |-------------------|---------|---------------------------------------|
//! | `WL_LOW_NM`       | 300 nm  | Lower bound of NSB photon band        |
//! | `WL_HIGH_NM`      | 650 nm  | Upper bound of NSB photon band        |
//! | `B_FILTER_NM`     | 445 nm  | B-band reference wavelength           |
//! | `V_FILTER_NM`     | 551 nm  | V-band reference wavelength           |

use crate::error::{NsbError, Result};
use optica::spectrum::SampledSpectrum;

use super::extinction::ZodiacalExtinction;
use super::geometry::ZodiacalGeometry;
use super::leinert::{Leinert1998Grid, LEINERT_S10_TO_W_M2_SR_UM};
use super::output::{ZodiacalOutputs, ZodiacalSpectrum};
use super::reddening::reddening_factor;

use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{algo, Interpolation};
use qtty::angular::Degrees;
use qtty::radiometry::{
    spectral_radiance_to_photon_radiance_ns_nm,
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
    WattsPerSquareMeterSteradianNanometer,
};
use siderust::qtty::{length::Meter, Nanometer, Nanometers};

pub(super) const WL_LOW_NM: f64 = 300.0;
pub(super) const WL_HIGH_NM: f64 = 650.0;
pub(super) const B_FILTER_NM: f64 = 445.0;
pub(super) const V_FILTER_NM: f64 = 551.0;

/// Compute scalar zodiacal outputs without retaining the full spectrum.
///
/// This is the hot path used by the threshold inner loop. It avoids
/// allocating a full spectrum by computing the integration and B/V
/// interpolation in a single forward pass.
pub(super) fn compute_outputs(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
) -> Result<ZodiacalOutputs> {
    let k = spectral_scale(geom, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));

    let solar_xs = solar.xs_raw();
    let solar_ys = solar.ys_raw();

    // Accumulator for the trapezoidal integration.
    let mut lam_buf: Vec<f64> = Vec::new();
    let mut ph_buf: Vec<f64> = Vec::new();

    // Running interpolation state for B/V.
    let (mut b_zl_um, mut v_zl_um) = (0.0_f64, 0.0_f64);
    let (mut b_dist, mut v_dist) = (f64::INFINITY, f64::INFINITY);

    for i in 0..solar_xs.len() {
        let l = solar_xs[i];
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&l) {
            continue;
        }
        let f_sun_sr = solar_ys[i] / std::f64::consts::PI;
        let zl = f_sun_sr * k * reddening_factor(geom.beta, geom.delta_lambda, l);
        let zl_w_m2_sr_um = zl * 1000.0;
        let trans = extinction.transmission(zl_w_m2_sr_um, l, zenith);
        let zl_ext = zl * trans;
        let zl_ext_um = zl_ext * 1000.0;

        if (l - B_FILTER_NM).abs() < b_dist {
            b_dist = (l - B_FILTER_NM).abs();
            b_zl_um = zl_ext_um;
        }
        if (l - V_FILTER_NM).abs() < v_dist {
            v_dist = (l - V_FILTER_NM).abs();
            v_zl_um = zl_ext_um;
        }

        let ph = spectral_radiance_to_photon_radiance_ns_nm(
            WattsPerSquareMeterSteradianNanometer::new(zl_ext),
            Nanometers::new(l),
        )
        .value();
        lam_buf.push(l);
        ph_buf.push(ph);
    }

    if lam_buf.is_empty() {
        return Err(NsbError::OutOfRange(
            "solar spectrum has no samples in the 300–650 nm zodiacal band".to_string(),
        ));
    }

    let integrated =
        BandPhotonRadiance::new(algo::trapz_range(&lam_buf, &ph_buf, WL_LOW_NM, WL_HIGH_NM));

    // B/V: interpolate at exact filter wavelengths using the built spectrum.
    let (b_flux, v_flux) = interpolate_bv(&lam_buf, &ph_buf, b_zl_um, v_zl_um);

    Ok(ZodiacalOutputs {
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

/// Compute the full zodiacal spectrum together with scalar summaries.
///
/// Allocates a [`SampledSpectrum`]; prefer [`compute_outputs`] in hot loops.
pub(super) fn compute_spectrum(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
    extinction: ZodiacalExtinction,
) -> Result<ZodiacalSpectrum> {
    let k = spectral_scale(geom, solar)?;
    let zenith = geom.zenith.unwrap_or(Degrees::new(0.0));

    let solar_xs = solar.xs_raw();
    let solar_ys = solar.ys_raw();

    let mut lam_buf: Vec<f64> = Vec::new();
    let mut ph_buf: Vec<f64> = Vec::new();
    let (mut b_zl_um, mut v_zl_um) = (0.0_f64, 0.0_f64);
    let (mut b_dist, mut v_dist) = (f64::INFINITY, f64::INFINITY);

    for i in 0..solar_xs.len() {
        let l = solar_xs[i];
        if !(WL_LOW_NM..=WL_HIGH_NM).contains(&l) {
            continue;
        }
        let f_sun_sr = solar_ys[i] / std::f64::consts::PI;
        let zl = f_sun_sr * k * reddening_factor(geom.beta, geom.delta_lambda, l);
        let zl_w_m2_sr_um = zl * 1000.0;
        let trans = extinction.transmission(zl_w_m2_sr_um, l, zenith);
        let zl_ext = zl * trans;
        let zl_ext_um = zl_ext * 1000.0;

        if (l - B_FILTER_NM).abs() < b_dist {
            b_dist = (l - B_FILTER_NM).abs();
            b_zl_um = zl_ext_um;
        }
        if (l - V_FILTER_NM).abs() < v_dist {
            v_dist = (l - V_FILTER_NM).abs();
            v_zl_um = zl_ext_um;
        }

        let ph = spectral_radiance_to_photon_radiance_ns_nm(
            WattsPerSquareMeterSteradianNanometer::new(zl_ext),
            Nanometers::new(l),
        )
        .value();
        lam_buf.push(l);
        ph_buf.push(ph);
    }

    if lam_buf.is_empty() {
        return Err(NsbError::OutOfRange(
            "solar spectrum has no samples in the 300–650 nm zodiacal band".to_string(),
        ));
    }

    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam_buf.clone(),
        ph_buf.clone(),
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("zodiacal")),
    )
    .map_err(|e| NsbError::Interpolation(format!("zodiacal spectrum: {e}")))?;

    let integrated =
        BandPhotonRadiance::new(algo::trapz_range(&lam_buf, &ph_buf, WL_LOW_NM, WL_HIGH_NM));
    let (b_flux, v_flux) = interpolate_bv(&lam_buf, &ph_buf, b_zl_um, v_zl_um);

    Ok(ZodiacalSpectrum {
        spectrum,
        integrated,
        b_flux_s10: b_flux,
        v_flux_s10: v_flux,
    })
}

/// Compute the spectral scaling factor `k` such that `k · f_sun_sr(500 nm)`
/// equals the Leinert S10 brightness converted to W m⁻² sr⁻¹ nm⁻¹.
fn spectral_scale(
    geom: &ZodiacalGeometry,
    solar: &SampledSpectrum<Nanometer, Meter>,
) -> Result<f64> {
    let zl_500_s10 = Leinert1998Grid::lookup_s10(geom.beta, geom.delta_lambda)?;
    let zl_500_wmsrum = zl_500_s10.value() * LEINERT_S10_TO_W_M2_SR_UM;

    let f_sun_500 = solar.interp_at(Nanometers::new(500.0)).value();
    let f_sun_500_sr = f_sun_500 / std::f64::consts::PI;
    if f_sun_500_sr <= 0.0 {
        return Err(NsbError::DataParse {
            file: "solar_spectrum.dat",
            message: "non-positive flux at 500 nm".into(),
        });
    }
    let target_500 = zl_500_wmsrum / 1000.0; // W m⁻² sr⁻¹ nm⁻¹
    Ok(target_500 / f_sun_500_sr)
}

/// Interpolate the zodiacal radiance at the exact B (445 nm) and V (551 nm)
/// filter wavelengths from the sampled photon-radiance spectrum.
///
/// Falls back to the nearest-sample W m⁻² sr⁻¹ µm⁻¹ value divided by
/// `LEINERT_S10_TO_W_M2_SR_UM` for the S10 conversion, consistent with the
/// energy-domain S10 proxy used by the original pipeline.
fn interpolate_bv(lam: &[f64], ph: &[f64], b_zl_um: f64, v_zl_um: f64) -> (S10, S10) {
    // The B/V S10 values are derived from the energy-domain W m⁻² sr⁻¹ µm⁻¹
    // radiance at the filter wavelengths, consistent with the Leinert (1998)
    // reference convention. We interpolate the per-photon spectrum to find the
    // energy equivalent at exactly B_FILTER_NM and V_FILTER_NM, preferring
    // linear interpolation if samples bracket the target.
    let b_s10 = interp_linear_or_nearest(lam, ph, B_FILTER_NM, b_zl_um);
    let v_s10 = interp_linear_or_nearest(lam, ph, V_FILTER_NM, v_zl_um);
    (b_s10, v_s10)
}

/// Attempt linear interpolation at `target_nm`. If samples bracket the
/// target, return the interpolated S10 equivalent. Otherwise fall back to
/// the pre-computed nearest-sample `fallback_zl_um / S10_TO_W_M2_SR_UM`.
fn interp_linear_or_nearest(lam: &[f64], ph: &[f64], target_nm: f64, fallback_zl_um: f64) -> S10 {
    // hc in J·m (same constant used by qtty::spectral_radiance_to_photon_radiance_ns_nm).
    const HC_JOULE_METER: f64 = 1.986_445_857_148_968e-25;

    // Find bracketing indices.
    let pos = lam.partition_point(|&x| x < target_nm);
    if pos > 0 && pos < lam.len() {
        let x0 = lam[pos - 1];
        let x1 = lam[pos];
        let y0 = ph[pos - 1];
        let y1 = ph[pos];
        if x1 > x0 {
            let t = (target_nm - x0) / (x1 - x0);
            let ph_interp = y0 + t * (y1 - y0);
            // Invert the photon-radiance conversion:
            //   ph [ph cm⁻² ns⁻¹ sr⁻¹ nm⁻¹] = E_nm [W m⁻² sr⁻¹ nm⁻¹] * λ_m / HC * 1e-13
            // → E_nm = ph * HC / (λ_m * 1e-13)
            // → E_µm = E_nm * 1000  [W m⁻² sr⁻¹ µm⁻¹]
            let lambda_m = target_nm * 1e-9;
            let denom = lambda_m * 1e-13;
            if denom > 0.0 {
                let zl_ext_nm = ph_interp * HC_JOULE_METER / denom;
                let zl_ext_um = zl_ext_nm * 1000.0;
                return S10::new(zl_ext_um / LEINERT_S10_TO_W_M2_SR_UM);
            }
        }
    }
    // Fallback: nearest-sample value already tracked during the main loop.
    S10::new(fallback_zl_um / LEINERT_S10_TO_W_M2_SR_UM)
}
