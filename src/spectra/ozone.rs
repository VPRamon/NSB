//! Ozone transmittance.

use crate::error::{NsbError, Result};
use super::spectrum::Spectrum;

const RAW: &str = include_str!("../../data/o3trans.dat");

/// `(wavelength [nm], transmittance [-])`. The raw file uses micrometres for
/// the wavelength column; we convert to nm here.
pub fn load() -> Result<Spectrum> {
    let mut lam = Vec::new();
    let mut t = Vec::new();
    for (n, line) in RAW.lines().enumerate() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') { continue; }
        let mut parts = s.split_whitespace();
        let l_um: f64 = parts.next().and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse { file: "o3trans.dat", message: format!("line {n}: lambda") })?;
        let v: f64 = parts.next().and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse { file: "o3trans.dat", message: format!("line {n}: trans") })?;
        lam.push(l_um * 1000.0);
        t.push(v);
    }
    Ok(Spectrum::new(lam, t).with_tag("ozone_trans"))
}
