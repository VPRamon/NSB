//! Integrated starlight spectrum from SkyCalc (Noll et al. 2012).

use crate::error::{NsbError, Result};
use super::spectrum::Spectrum;

const RAW: &str = include_str!("../../data/radiance_starlight.txt");

/// Loads `(wavelength [nm], radiance [ph s⁻¹ m⁻² μm⁻¹ arcsec⁻²])`.
///
/// Format mirrors SkyCalc's two-column ASCII output.
pub fn load() -> Result<Spectrum> {
    let mut lam = Vec::new();
    let mut flx = Vec::new();
    for (n, line) in RAW.lines().enumerate() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') { continue; }
        let mut parts = s.split_whitespace();
        let l: f64 = parts.next().and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse { file: "radiance_starlight.txt", message: format!("line {n}: lambda") })?;
        let f: f64 = parts.next().and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse { file: "radiance_starlight.txt", message: format!("line {n}: flux") })?;
        lam.push(l);
        flx.push(f);
    }
    Ok(Spectrum::new(lam, flx).with_tag("starlight"))
}
