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
//! TODO: a future stage will use `spectra::airglow_cont::load` and per-season
//! corrections to produce a wavelength-resolved airglow spectrum instead of
//! the polynomial point estimate.
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
use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};

const AG_PARAM: [f64; 4] = [
    -1.382_674_19e-7,
    4.717_575_83e-5,
    -5.161_785_94e-3,
    2.963_382_43e-1,
];

const AG_S10_B: f64 = 163.189_810_469_037_2;
const AG_S10_V: f64 = 228.735_856_150_608_16;

#[derive(Debug, Clone)]
pub struct AgInputs {
    /// Source altitude [deg].
    pub altitude_deg: f64,
}

#[derive(Debug, Clone)]
pub struct AgOutputs {
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10,
    pub v_flux_s10: S10,
}

pub fn compute(inp: &AgInputs) -> Result<AgOutputs> {
    let x = inp.altitude_deg;
    let v = AG_PARAM[0] * x.powi(3) + AG_PARAM[1] * x.powi(2) + AG_PARAM[2] * x + AG_PARAM[3];
    Ok(AgOutputs {
        integrated: BandPhotonRadiance::new(v),
        b_flux_s10: S10::new(AG_S10_B),
        v_flux_s10: S10::new(AG_S10_V),
    })
}
