//! Airglow continuum calibration data and loader.
//!
//! The reference file `data/airglow_cont.dat` is a multi-block format with
//! per-season and per-time-of-night scaling factors. The loader below extracts
//! the global scale, solar-activity correction, season/time correction
//! matrices, uncertainties, and the per-wavelength relative mean profile.
//!
//! Scientific role:
//! airglow is intrinsically spectral: different wavelengths and bands vary in
//! strength, season, and time of night. This file preserves the continuum-side
//! reference data needed for a more detailed airglow model.
//!
//! Contribution to the science:
//! the current crate uses a simpler polynomial airglow estimate for the
//! main evaluator, but this loader is the bridge toward a wavelength-resolved
//! model that can represent the spectral structure of atmospheric emission more
//! faithfully.
//!
//! Provenance:
//! airglow continuum calibration lives in `components::airglow`.

use crate::error::{NsbError, Result};
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};
use siderust::qtty::{length::Meter, Nanometer};

const RAW: &str = include_str!("../../../data/airglow_cont.dat");

// Pinned SHA-256 of the airglow continuum reference file.
siderust::assert_data_checksum!(
    "NSB/data/airglow_cont.dat",
    RAW.as_bytes(),
    "d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f"
);

/// Airglow continuum calibration data loaded from the bundled reference file.
#[derive(Debug, Clone)]
pub struct AirglowContinuum {
    /// Global scale factor (`scale` block in the file).
    pub global_scale: f64,
    /// Typical continuum emission height.
    pub emission_height_km: f64,
    /// Solar radio-flux correction intercept.
    pub solar_activity_const: f64,
    /// Solar radio-flux correction slope.
    pub solar_activity_slope: f64,
    /// Mean correction factors indexed `[time_bin][season_bin]`.
    ///
    /// Row 0 is full night; rows 1..=n_time are time-of-night bins.
    /// Column 0 is full year; columns 1..=n_season are seasonal bins.
    pub mean_corrections: Vec<Vec<f64>>,
    /// Uncertainty correction factors with the same shape as
    /// [`mean_corrections`](Self::mean_corrections).
    pub sigma_corrections: Vec<Vec<f64>>,
    /// Wavelength [nm] vs relative mean radiance.
    pub spectrum: SampledSpectrum<Nanometer, Meter>,
    /// Wavelength [nm] vs relative uncertainty.
    pub uncertainty: SampledSpectrum<Nanometer, Meter>,
    /// Number of seasons / time windows in the file.
    pub n_season: usize,
    pub n_time: usize,
}

/// Load the built-in empirical airglow continuum calibration.
///
/// Parses `data/airglow_cont.dat` embedded at compile time.
///
/// File format (excerpted from the file header):
/// 1. `nseason ntime` — counts of season/time bins.
/// 2. `ndat` — number of wavelength samples that follow.
/// 3. `height` — emission height [km].
/// 4. `scale` — global scale factor.
/// 5. `cons slope` — solar activity correction.
/// 6. `(ntime + 1)` mean-correction rows, each with `(nseason + 1)` values.
/// 7. `(ntime + 1)` sigma-correction rows, same shape.
/// 8. `ndat` rows of `wavelength_um  relative_mean  relative_sigma`.
pub(crate) fn load_builtin_standard() -> Result<AirglowContinuum> {
    let mut iter = RAW.lines().filter_map(|l| {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            None
        } else {
            Some(t)
        }
    });

    let header = iter.next().ok_or_else(|| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: "missing nseason/ntime".into(),
    })?;
    let mut hp = header.split_whitespace();
    let n_season: usize =
        hp.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "nseason".into(),
            })?;
    let n_time: usize =
        hp.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "ntime".into(),
            })?;

    let n_dat: usize =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "ndat".into(),
            })?;
    let emission_height_km: f64 =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "height".into(),
            })?;
    let global_scale: f64 =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "scale".into(),
            })?;

    let solar_row = iter.next().ok_or_else(|| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: "solar activity correction".into(),
    })?;
    let solar = parse_floats(solar_row, "solar activity correction")?;
    if solar.len() != 2 {
        return Err(NsbError::DataParse {
            file: "airglow_cont.dat",
            message: format!("expected 2 solar-activity values, got {}", solar.len()),
        });
    }
    let solar_activity_const = solar[0];
    let solar_activity_slope = solar[1];

    let correction_rows = n_time + 1;
    let correction_cols = n_season + 1;
    let mean_corrections = parse_matrix(
        &mut iter,
        correction_rows,
        correction_cols,
        "mean corrections",
    )?;
    let sigma_corrections = parse_matrix(
        &mut iter,
        correction_rows,
        correction_cols,
        "sigma corrections",
    )?;

    let mut lam = Vec::with_capacity(n_dat);
    let mut rel = Vec::with_capacity(n_dat);
    let mut sig = Vec::with_capacity(n_dat);
    for _ in 0..n_dat {
        let row = iter.next().ok_or_else(|| NsbError::DataParse {
            file: "airglow_cont.dat",
            message: "premature EOF in data block".into(),
        })?;
        let mut p = row.split_whitespace();
        let l_um: f64 =
            p.next()
                .and_then(|x| x.parse().ok())
                .ok_or_else(|| NsbError::DataParse {
                    file: "airglow_cont.dat",
                    message: "lambda".into(),
                })?;
        let r: f64 = p
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "rel_mean".into(),
            })?;
        let dr: f64 = p
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "rel_sigma".into(),
            })?;
        lam.push(l_um * 1000.0);
        rel.push(r);
        sig.push(dr);
    }
    let spectrum = SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam.clone(),
        rel,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::bundled_file("NSB/data/airglow_cont.dat")),
    )
    .map_err(|e| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: e.to_string(),
    })?;
    let uncertainty = SampledSpectrum::<Nanometer, Meter>::from_raw(
        lam,
        sig,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::bundled_file("NSB/data/airglow_cont.dat")),
    )
    .map_err(|e| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: e.to_string(),
    })?;
    Ok(AirglowContinuum {
        global_scale,
        emission_height_km,
        solar_activity_const,
        solar_activity_slope,
        mean_corrections,
        sigma_corrections,
        spectrum,
        uncertainty,
        n_season,
        n_time,
    })
}

fn parse_floats(row: &str, label: &'static str) -> Result<Vec<f64>> {
    row.split_whitespace()
        .map(|x| {
            x.parse::<f64>().map_err(|_| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: format!("bad numeric value in {label}: {x:?}"),
            })
        })
        .collect()
}

fn parse_matrix<'a>(
    iter: &mut impl Iterator<Item = &'a str>,
    rows: usize,
    cols: usize,
    label: &'static str,
) -> Result<Vec<Vec<f64>>> {
    let mut out = Vec::with_capacity(rows);
    for row_idx in 0..rows {
        let row = iter.next().ok_or_else(|| NsbError::DataParse {
            file: "airglow_cont.dat",
            message: format!("premature EOF in {label}"),
        })?;
        let values = parse_floats(row, label)?;
        if values.len() != cols {
            return Err(NsbError::DataParse {
                file: "airglow_cont.dat",
                message: format!(
                    "{label} row {row_idx} has {} columns, expected {cols}",
                    values.len()
                ),
            });
        }
        out.push(values);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airglow_builtin_calibration_checksum_matches() {
        use siderust::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f",
        );
    }

    #[test]
    fn airglow_builtin_calibration_parses_full_structure() {
        let c = load_builtin_standard().expect("airglow continuum calibration");
        assert_eq!(c.n_season, 6);
        assert_eq!(c.n_time, 3);
        assert_eq!(c.mean_corrections.len(), 4);
        assert_eq!(c.mean_corrections[0].len(), 7);
        assert_eq!(c.sigma_corrections.len(), 4);
        assert_eq!(c.spectrum.len(), 46);
        assert_eq!(c.uncertainty.len(), 46);
        assert!((c.emission_height_km - 90.0).abs() < 1.0e-12);
        assert!((c.global_scale - 79.829).abs() < 1.0e-12);
    }
}
