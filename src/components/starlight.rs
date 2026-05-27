//! Integrated starlight component.
//!
//! Port of `CalculateSL`. The Python implementation reads the SkyCalc
//! starlight radiance, integrates over `[wl_low, wl_high]`, and reports
//! two hardcoded S10 magnitudes for B and V (constants in `NSB_Utils.py`).
//!
//! Scientific role:
//! even where no individual star dominates the field, unresolved stars produce
//! a diffuse optical glow. That integrated starlight is part of the baseline
//! night-sky background.
//!
//! Contribution to the science:
//! this file turns the bundled SkyCalc-derived starlight spectrum into the
//! crate's working radiance units and contributes a direction-independent
//! baseline term to the total NSB in the current implementation.

use crate::error::Result;
use crate::spectra::starlight;
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{algo, Interpolation, SampledSpectrum};
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};
use siderust::qtty::{length::Meter, Nanometer};

const WL_LOW_NM: f64 = 300.0;
const WL_HIGH_NM: f64 = 650.0;

/// Hardcoded constants from `NSB_Utils.py:65-66`.
const SL_S10_B: f64 = 17.225_803_202_042_27;
const SL_S10_V: f64 = 9.011_178_802_900_696;

#[derive(Debug, Clone)]
pub struct SlOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
    pub spectrum: SampledSpectrum<Nanometer, Meter>,
}

/// Compute the starlight contribution.
///
/// The starlight spectrum is in SkyCalc units: `ph s⁻¹ m⁻² μm⁻¹ arcsec⁻²`.
/// We convert to `ph ns⁻¹ cm⁻² nm⁻¹ sr⁻¹` exactly as Python does:
/// * `ph s⁻¹ m⁻² μm⁻¹ arcsec⁻² → ph ns⁻¹ cm⁻² nm⁻¹ sr⁻¹` is a fixed factor.
pub fn compute() -> Result<SlOutputs> {
    let raw = starlight::load()?;

    // Conversion factor:
    //   per s   → per ns      : ×1e-9
    //   per m²  → per cm²     : ×1e-4
    //   per μm  → per nm      : ×1e-3
    //   per arcsec² → per sr  : ×1 / (1 arcsec in rad)² = / (4.8481368e-6)²
    //                                                  = / 2.350443e-11
    const ARCSEC2_PER_SR: f64 = 4.254_517_029_022_576e10;
    const FACTOR: f64 = 1e-9 * 1e-4 * 1e-3 * ARCSEC2_PER_SR;

    let lam = raw.xs_raw();
    let flx: Vec<f64> = raw.ys_raw().iter().map(|&y| y * FACTOR).collect();
    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam.to_vec(),
        flx,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::computed("starlight")),
    )
    .expect("starlight spectrum invariants");
    let integrated = BandPhotonRadiance::new(algo::trapz_range(
        spectrum.xs_raw(),
        spectrum.ys_raw(),
        WL_LOW_NM,
        WL_HIGH_NM,
    ));
    Ok(SlOutputs {
        integrated,
        b_flux_s10: S10::new(SL_S10_B),
        v_flux_s10: S10::new(SL_S10_V),
        spectrum,
    })
}
