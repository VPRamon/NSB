//! Solar irradiance spectrum loader.
//!
//! Loads `data/solar_spectrum.dat` (CSV: `wavelength_nm, irradiance_W_m2_nm`)
//! shipped with the Python package, embedded via `include_str!`.

use crate::error::{NsbError, Result};
use super::spectrum::Spectrum;

const RAW: &str = include_str!("../../data/solar_spectrum.dat");

/// Returns the solar spectrum as `(wavelength [nm], irradiance [W m⁻² nm⁻¹])`.
pub fn load() -> Result<Spectrum> {
    let mut lam = Vec::new();
    let mut flx = Vec::new();
    for (n, line) in RAW.lines().enumerate() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') { continue; }
        let mut parts = s.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty());
        let l: f64 = parts.next()
            .ok_or_else(|| NsbError::DataParse { file: "solar_spectrum.dat", message: format!("line {n}: missing lambda") })?
            .parse().map_err(|_| NsbError::DataParse { file: "solar_spectrum.dat", message: format!("line {n}: bad lambda") })?;
        let f: f64 = parts.next()
            .ok_or_else(|| NsbError::DataParse { file: "solar_spectrum.dat", message: format!("line {n}: missing flux") })?
            .parse().map_err(|_| NsbError::DataParse { file: "solar_spectrum.dat", message: format!("line {n}: bad flux") })?;
        lam.push(l);
        flx.push(f);
    }
    Ok(Spectrum::new(lam, flx).with_tag("solar"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_nonempty() {
        let s = load().expect("load solar spectrum");
        assert!(!s.is_empty());
        assert!(s.lambda_nm[0] > 0.0);
    }
}
