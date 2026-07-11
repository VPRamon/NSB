//! Typed spectral numeric adapters.
//!
//! Low-level interpolation and integration crates generally operate on raw
//! `f64` slices. This module is the narrow boundary where NSB deliberately
//! erases qtty units for those algorithms and immediately rebuilds typed
//! quantities on return.

use optica::spectrum::algo;
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance,
    PhotonsPerSquareCentimeterNanosecondSteradianNanometer as SpectralBandPhotonRadiance,
};
use siderust::qtty::Nanometers;

/// Integrate spectral photon radiance over a wavelength interval in nanometres.
pub(crate) fn trapz_spectral_photon_radiance_nm(
    wavelengths: &[Nanometers],
    densities: &[SpectralBandPhotonRadiance],
    low: Nanometers,
    high: Nanometers,
) -> BandPhotonRadiance {
    let xs = wavelength_values_nm(wavelengths);
    let ys = spectral_photon_radiance_values(densities);
    BandPhotonRadiance::new(algo::trapz_range(&xs, &ys, low.value(), high.value()))
}

/// Linearly interpolate spectral photon radiance at a typed wavelength.
///
/// Returns `None` when the target is outside the sampled interval or when the
/// input axes are malformed. Callers that need historical nearest-neighbour
/// fallback behaviour should apply it explicitly at their calibration boundary.
pub(crate) fn interpolate_spectral_photon_radiance_nm(
    wavelengths: &[Nanometers],
    densities: &[SpectralBandPhotonRadiance],
    target: Nanometers,
) -> Option<SpectralBandPhotonRadiance> {
    if wavelengths.len() != densities.len() || wavelengths.is_empty() {
        return None;
    }

    let target_nm = target.value();
    let pos = wavelengths.partition_point(|x| x.value() < target_nm);

    if pos < wavelengths.len() && (wavelengths[pos].value() - target_nm).abs() <= f64::EPSILON {
        return Some(densities[pos]);
    }

    if pos == 0 || pos >= wavelengths.len() {
        return None;
    }

    let x0 = wavelengths[pos - 1].value();
    let x1 = wavelengths[pos].value();
    if x1 <= x0 {
        return None;
    }

    let y0 = densities[pos - 1].value();
    let y1 = densities[pos].value();
    let t = (target_nm - x0) / (x1 - x0);
    Some(SpectralBandPhotonRadiance::new(y0 + t * (y1 - y0)))
}

/// Erase typed wavelengths to nanometre scalars for `optica::SampledSpectrum`.
pub(crate) fn wavelength_values_nm(wavelengths: &[Nanometers]) -> Vec<f64> {
    wavelengths
        .iter()
        .map(|wavelength| wavelength.value())
        .collect()
}

/// Erase typed spectral photon radiance values for low-level numeric kernels.
pub(crate) fn spectral_photon_radiance_values(
    densities: &[SpectralBandPhotonRadiance],
) -> Vec<f64> {
    densities.iter().map(|density| density.value()).collect()
}
