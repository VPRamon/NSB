//! Tabulated scattering grids used by the Jones (2013) spectral moonlight model.
//!
//! The bundled tables are owned by the moonlight component. `mie_m15s1.dat`
//! provides the wavelength/angle Mie phase grid, and `sscatcor_m15s1.dat`
//! provides the matching multiple-scattering correction grid.

use crate::error::{NsbError, Result};
use siderust::qtty::{Degrees, Nanometers};

const MIE_RAW: &str = include_str!("../../../data/mie_m15s1.dat");
const SSCAT_RAW: &str = include_str!("../../../data/sscatcor_m15s1.dat");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterGridKind {
    MiePhase,
    MultipleScatteringCorrection,
}

#[derive(Clone, Debug)]
pub struct ScatterGrid {
    #[cfg(test)]
    kind: ScatterGridKind,
    angle_deg: Vec<f64>,
    wavelength_nm: Vec<f64>,
    data: Vec<f64>,
}

impl ScatterGrid {
    pub fn new() -> Self {
        Self::mie_phase().expect("bundled Mie phase grid must parse")
    }

    pub fn mie_phase() -> Result<Self> {
        parse_grid(MIE_RAW, "mie_m15s1.dat", ScatterGridKind::MiePhase)
    }

    pub fn multiple_scattering_correction() -> Result<Self> {
        parse_grid(
            SSCAT_RAW,
            "sscatcor_m15s1.dat",
            ScatterGridKind::MultipleScatteringCorrection,
        )
    }

    #[cfg(test)]
    pub fn kind(&self) -> ScatterGridKind {
        self.kind
    }

    #[cfg(test)]
    pub fn dimensions(&self) -> (usize, usize) {
        (self.angle_deg.len(), self.wavelength_nm.len())
    }

    pub fn lookup(&self, angle: Degrees, wavelength: Nanometers) -> f64 {
        let nw = self.wavelength_nm.len();
        let (a0, a1, ta) = bracket_clamped(&self.angle_deg, angle.value());
        let (w0, w1, tw) = bracket_clamped(&self.wavelength_nm, wavelength.value());

        let v00 = self.data[a0 * nw + w0];
        let v01 = self.data[a0 * nw + w1];
        let v10 = self.data[a1 * nw + w0];
        let v11 = self.data[a1 * nw + w1];

        let row0 = v00 + tw * (v01 - v00);
        let row1 = v10 + tw * (v11 - v10);
        row0 + ta * (row1 - row0)
    }
}

impl Default for ScatterGrid {
    fn default() -> Self {
        Self::new()
    }
}

fn bracket_clamped(axis: &[f64], value: f64) -> (usize, usize, f64) {
    debug_assert!(!axis.is_empty());
    if value <= axis[0] {
        return (0, 0, 0.0);
    }
    let last = axis.len() - 1;
    if value >= axis[last] {
        return (last, last, 0.0);
    }
    let upper = axis.partition_point(|&x| x <= value);
    let lower = upper - 1;
    let denom = axis[upper] - axis[lower];
    let t = if denom > 0.0 {
        (value - axis[lower]) / denom
    } else {
        0.0
    };
    (lower, upper, t.clamp(0.0, 1.0))
}

fn parse_grid(raw: &str, file: &'static str, _kind: ScatterGridKind) -> Result<ScatterGrid> {
    let mut lines = raw.lines().filter_map(|line| {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            None
        } else {
            Some(t)
        }
    });

    let dims = lines
        .next()
        .ok_or_else(|| parse_err(file, "missing dimensions"))?;
    let dims = parse_usizes(dims, file, "dimensions")?;
    if dims.len() != 2 {
        return Err(parse_err(file, "dimensions must contain two values"));
    }
    let (n_wavelength, n_angle) = (dims[0], dims[1]);

    let wavelength_um = parse_f64s(
        lines
            .next()
            .ok_or_else(|| parse_err(file, "missing wavelength axis"))?,
        file,
        "wavelength axis",
    )?;
    if wavelength_um.len() != n_wavelength {
        return Err(parse_err(file, "wavelength axis length mismatch"));
    }
    let wavelength_nm: Vec<f64> = wavelength_um.into_iter().map(|x| x * 1000.0).collect();

    let angle_deg = parse_f64s(
        lines
            .next()
            .ok_or_else(|| parse_err(file, "missing angle axis"))?,
        file,
        "angle axis",
    )?;
    if angle_deg.len() != n_angle {
        return Err(parse_err(file, "angle axis length mismatch"));
    }

    let mut wavelength_major = Vec::with_capacity(n_wavelength);
    for row_idx in 0..n_wavelength {
        let row = lines
            .next()
            .ok_or_else(|| parse_err(file, "premature EOF in grid data"))?;
        let values = parse_f64s(row, file, "grid row")?;
        if values.len() != n_angle {
            return Err(parse_err(
                file,
                format!("grid row {row_idx} length mismatch"),
            ));
        }
        wavelength_major.push(values);
    }

    let mut data = Vec::with_capacity(n_angle * n_wavelength);
    for angle_idx in 0..n_angle {
        for row in wavelength_major.iter().take(n_wavelength) {
            data.push(row[angle_idx]);
        }
    }

    Ok(ScatterGrid {
        #[cfg(test)]
        kind: _kind,
        angle_deg,
        wavelength_nm,
        data,
    })
}

fn parse_f64s(row: &str, file: &'static str, label: &'static str) -> Result<Vec<f64>> {
    row.split_whitespace()
        .map(|x| {
            x.parse::<f64>()
                .map_err(|_| parse_err(file, format!("bad {label} value: {x:?}")))
        })
        .collect()
}

fn parse_usizes(row: &str, file: &'static str, label: &'static str) -> Result<Vec<usize>> {
    row.split_whitespace()
        .map(|x| {
            x.parse::<usize>()
                .map_err(|_| parse_err(file, format!("bad {label} value: {x:?}")))
        })
        .collect()
}

fn parse_err(file: &'static str, message: impl Into<String>) -> NsbError {
    NsbError::DataParse {
        file,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::checksum::{sha256, to_hex};

    #[test]
    fn moonlight_scattering_checksums_match() {
        assert_eq!(
            to_hex(&sha256(MIE_RAW.as_bytes())),
            "dba01f9b49ddf9a547bccc7eaca013bec1e4b1d8e081ec5ec4dd284ea7ec425e"
        );
        assert_eq!(
            to_hex(&sha256(SSCAT_RAW.as_bytes())),
            "2bf48a71e007bc557bd088d53ede15e97163d9154f19b1e411b104c38c4a18b8"
        );
    }

    #[test]
    fn moonlight_mie_phase_grid_loads_known_value() {
        let grid = ScatterGrid::mie_phase().unwrap();
        assert_eq!(grid.kind(), ScatterGridKind::MiePhase);
        assert_eq!(grid.dimensions(), (181, 40));
        let v = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert!((v - 57.433_337).abs() < 1.0e-6);
    }

    #[test]
    fn moonlight_scattering_correction_grid_loads_known_value() {
        let grid = ScatterGrid::multiple_scattering_correction().unwrap();
        assert_eq!(grid.kind(), ScatterGridKind::MultipleScatteringCorrection);
        assert_eq!(grid.dimensions(), (16, 40));
        let v = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert!((v - 1.936).abs() < 1.0e-12);
    }

    #[test]
    fn moonlight_scattering_lookup_clamps_to_boundaries() {
        let grid = ScatterGrid::mie_phase().unwrap();
        let low = grid.lookup(Degrees::new(-10.0), Nanometers::new(100.0));
        let edge = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert_eq!(low, edge);
    }
}
