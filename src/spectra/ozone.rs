//! Ozone transmittance — delegates to `siderust::atmosphere::ozone`.

use crate::error::Result;
use super::spectrum::Spectrum;

/// `(wavelength [nm], transmittance [-])`.
///
/// Delegates to the upstream dataset in `siderust::atmosphere::ozone`,
/// which holds the canonical copy of the pre-computed ozone transmittance
/// table (originally from the NSB/darknsb `o3trans.dat` data file).
pub fn load() -> Result<Spectrum> {
    let table = siderust::atmosphere::ozone::transmission_table();
    Ok(Spectrum::new(table.xs_raw(), table.ys_raw()).with_tag("ozone_trans"))
}
