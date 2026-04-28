//! Scattered moonlight component.
//!
//! Status: not yet ported. The Python reference (`CalculateMoon`) chains the
//! `MoonObj` reflectance spectrum through a multi-shell single-scattering
//! integrator (`scat_moon`) using the `mie_m15s1.dat` and `sscatcor_m15s1.dat`
//! grids and the `LUT_moon` lookup tables.
//!
//! For the first iteration we expose the function but return zero flux so
//! the orchestrator can already compose a complete `NsbResult`. The Python
//! `get_NSB.py` active path also leaves moonlight commented out.
//!
//! TODO: full port using `data/lut_moon/Phase_*_LUT.csv`, the Mie phase
//! function, and the single-scatter correction grid. See
//! `docs/NSB_STAGED_IMPLEMENTATION_PLAN.md` (stages 9–11).

use crate::error::Result;
use qtty::angular::Degrees;
use qtty::radiometry;
use siderust::MoonPhaseGeometry;

#[derive(Debug, Clone, Copy)]
pub struct MoonInputs {
    /// Moon-source angular separation.
    pub separation: Degrees,
    /// Moon zenith distance.
    pub moon_zenith: Degrees,
    /// Geocentric lunar phase geometry from siderust.
    pub phase: MoonPhaseGeometry,
    /// Source zenith distance.
    pub source_zenith: Degrees,
}

#[derive(Debug, Clone)]
pub struct MoonOutputs {
    pub integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian,
    pub b_flux_s10: radiometry::S10s,
    pub v_flux_s10: radiometry::S10s,
}

pub fn compute(_inp: &MoonInputs) -> Result<MoonOutputs> {
    // TODO: implement the Jones et al. (2013) scattered-moonlight model.
    Ok(MoonOutputs {
        integrated: radiometry::PhotonsPerSquareCentimeterNanosecondSteradian::new(0.0),
        b_flux_s10: radiometry::S10s::new(0.0),
        v_flux_s10: radiometry::S10s::new(0.0),
    })
}
