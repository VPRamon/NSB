//! Tabulated scattering grids used by the advanced moonlight model.
//!
//! The bundled `mie_m15s1.dat` table stores the Paranal aerosol/Mie phase
//! function as wavelength × scattering angle.  The bundled
//! `sscatcor_m15s1.dat` table stores multiple-scattering correction factors
//! over the same kind of axes.  This module keeps those datasets NSB-local but
//! uses the generic `siderust::tables` interpolation kernels.

use crate::error::{NsbError, Result};
use siderust::qtty::{Degrees, Nanometers};
use siderust::tables::{algo, AxisDirection, OutOfRange};

const MIE_RAW: &str = include_str!("../data/mie_m15s1.dat");
const SSCAT_RAW: &str = include_str!("../data/sscatcor_m15s1.dat");

siderust::assert_data_checksum!(
    "NSB/data/mie_m15s1.dat",
    MIE_RAW.as_bytes(),
    "dba01f9b49ddf9a547bccc7eaca013bec1e4b1d8e081ec5ec4dd284ea7ec425e"
);
siderust::assert_data_checksum!(
    "NSB/data/sscatcor_m15s1.dat",
    SSCAT_RAW.as_bytes(),
    "2bf48a71e007bc557bd088d53ede15e97163d9154f19b1e411b104c38c4a18b8"
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterGridKind {
    MiePhase,
    MultipleScatteringCorrection,
}

/// Pre-computed scattering grid indexed by scattering angle and wavelength.
#[derive(Clone, Debug)]
pub struct ScatterGrid {
    kind: ScatterGridKind,
    angle_deg: Vec<f64>,
    wavelength_nm: Vec<f64>,
    /// Row-major storage: data[angle_idx * wavelength_count + wavelength_idx].
    data: Vec<f64>,
}

impl ScatterGrid {
    /// Load the production Mie phase grid.
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

    pub fn kind(&self) -> ScatterGridKind {
        self.kind
    }

    /// Returns the scattering angles covered by this grid.
    pub fn angles(&self) -> &[f64] {
        &self.angle_deg
    }

    /// Backwards-compatible alias for callers that used the old placeholder
    /// grid's zenith-angle terminology.
    pub fn zenith_angles(&self) -> &[f64] {
        self.angles()
    }

    /// Returns the wavelengths covered by this grid.
    pub fn wavelengths(&self) -> &[f64] {
        &self.wavelength_nm
    }

    /// Returns the grid dimensions as (angle_count, wavelength_count).
    pub fn dimensions(&self) -> (usize, usize) {
        (self.angle_deg.len(), self.wavelength_nm.len())
    }

    /// Bilinear lookup at `angle` and `wavelength`; out-of-range queries clamp
    /// to the nearest table boundary for parity with the original Python path.
    pub fn lookup(&self, angle: Degrees, wavelength: Nanometers) -> f64 {
        let na = self.angle_deg.len();
        let nw = self.wavelength_nm.len();
        let rows: Vec<&[f64]> = (0..na).map(|i| &self.data[i * nw..(i + 1) * nw]).collect();
        algo::bilinear(
            &self.wavelength_nm,
            &self.angle_deg,
            &rows,
            wavelength.value(),
            angle.value(),
            OutOfRange::ClampToEndpoints,
            OutOfRange::ClampToEndpoints,
            AxisDirection::Ascending,
            AxisDirection::Ascending,
        )
        .expect("ScatterGrid::lookup: bilinear interpolation failed")
    }
}

impl Default for ScatterGrid {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_grid(raw: &str, file: &'static str, kind: ScatterGridKind) -> Result<ScatterGrid> {
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
                &format!("grid row {row_idx} length mismatch"),
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
        kind,
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
    use siderust::provenance::checksum::{sha256, to_hex};

    #[test]
    fn pinned_checksums_match_runtime_hashes() {
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
    fn mie_phase_grid_loads_known_value() {
        let grid = ScatterGrid::mie_phase().unwrap();
        assert_eq!(grid.kind(), ScatterGridKind::MiePhase);
        assert_eq!(grid.dimensions(), (181, 40));
        let v = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert!((v - 57.433_337).abs() < 1.0e-6);
    }

    #[test]
    fn correction_grid_loads_known_value() {
        let grid = ScatterGrid::multiple_scattering_correction().unwrap();
        assert_eq!(grid.kind(), ScatterGridKind::MultipleScatteringCorrection);
        assert_eq!(grid.dimensions(), (16, 40));
        let v = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert!((v - 1.936).abs() < 1.0e-12);
    }

    #[test]
    fn lookup_clamps_to_boundaries() {
        let grid = ScatterGrid::mie_phase().unwrap();
        let low = grid.lookup(Degrees::new(-10.0), Nanometers::new(100.0));
        let edge = grid.lookup(Degrees::new(0.0), Nanometers::new(300.0));
        assert_eq!(low, edge);
    }
}
