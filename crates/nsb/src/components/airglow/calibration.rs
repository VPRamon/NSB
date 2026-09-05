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
//! calibration data used by the empirical airglow continuum model.
//!
//! Provenance:
//! Scientific metadata for the bundled continuum is owned by
//! `crates/nsb/data/manifest.toml` and surfaced through build-generated
//! [`crate::assets::BundledAssetMetadata`]. Integrity of the embedded bytes is
//! guaranteed by the build script (existence + SHA-256) before compilation.

use crate::assets::{bundled_asset, BundledAssetMetadata};
use crate::error::{NsbError, Result};
use crate::units::ScaleFactors;
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};
use qtty::dimensionless::Ratios;
use qtty::length::{Kilometers, Micrometers, Nanometer, Nanometers};
use qtty::unit::Ratio;

const RAW: &str = include_str!("../../../data/airglow_cont.dat");
const WL_LOW: Nanometers = Nanometers::new(300.0);
const WL_HIGH: Nanometers = Nanometers::new(650.0);
const B_FILTER: Nanometers = Nanometers::new(445.0);
const V_FILTER: Nanometers = Nanometers::new(551.0);

/// Path relative to `crates/nsb/data` as recorded in the scientific asset registry.
pub(crate) const AIRGLOW_CONTINUUM_RELATIVE_PATH: &str = "airglow_cont.dat";
/// Runtime/API asset path label for the bundled continuum.
pub(crate) const AIRGLOW_CONTINUUM_ASSET_PATH: &str = "NSB/data/airglow_cont.dat";

/// Canonical scientific provenance for the bundled airglow continuum.
///
/// Schema, source, license, generator, validation report, and calibration status
/// come from build-generated metadata derived from `crates/nsb/data/manifest.toml`.
pub(crate) fn airglow_continuum_asset() -> &'static BundledAssetMetadata {
    bundled_asset(AIRGLOW_CONTINUUM_RELATIVE_PATH)
        .expect("airglow_cont.dat must be registered by the build script")
}

/// Airglow continuum calibration data loaded from the bundled reference file.
#[derive(Debug, Clone)]
pub struct AirglowContinuum {
    /// Global scale factor (`scale` block in the file).
    pub global_scale: ScaleFactors,
    /// Typical continuum emission height.
    pub emission_height_km: Kilometers,
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
    /// Wavelength in nm versus relative mean radiance.
    pub spectrum: SampledSpectrum<Nanometer, Ratio>,
    /// Wavelength in nm versus relative uncertainty.
    pub uncertainty: SampledSpectrum<Nanometer, Ratio>,
    /// Number of seasons / time windows in the file.
    pub n_season: usize,
    /// Number of time-of-night bins in the file.
    pub n_time: usize,
    /// Unextincted integrated relative continuum over 300–650 nm (load-time reference).
    #[allow(dead_code)]
    pub(crate) integrated_relative_300_650: Nanometers,
    #[allow(dead_code)]
    pub(crate) integrated_uncertainty_abs_300_650: Nanometers,
    #[allow(dead_code)]
    pub(crate) b_relative: Ratios,
    #[allow(dead_code)]
    pub(crate) v_relative: Ratios,
}

/// Load the built-in empirical airglow continuum calibration.
///
/// Parses `data/airglow_cont.dat` embedded at compile time.
///
/// File format:
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
    let emission_height_km = iter
        .next()
        .and_then(|x| x.parse::<f64>().ok())
        .map(Kilometers::new)
        .ok_or_else(|| NsbError::DataParse {
            file: "airglow_cont.dat",
            message: "height".into(),
        })?;
    let global_scale: ScaleFactors = iter
        .next()
        .and_then(|x| x.parse().ok())
        .map(ScaleFactors::new)
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
        lam.push(Micrometers::new(l_um).to::<Nanometer>().value());
        rel.push(r);
        sig.push(dr);
    }
    let spectrum = SampledSpectrum::<Nanometer, Ratio>::from_raw(
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
    let uncertainty = SampledSpectrum::<Nanometer, Ratio>::from_raw(
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
    let integrated_relative_300_650 = spectrum.integrate_range(WL_LOW, WL_HIGH).to::<Nanometer>();
    let uncertainty_abs = SampledSpectrum::<Nanometer, Ratio>::from_raw(
        uncertainty.xs_raw().to_vec(),
        uncertainty
            .ys_raw()
            .iter()
            .map(|value| value.abs())
            .collect(),
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        None,
    )
    .map_err(|e| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: e.to_string(),
    })?;
    let integrated_uncertainty_abs_300_650 = uncertainty_abs
        .integrate_range(WL_LOW, WL_HIGH)
        .to::<Nanometer>();
    let b_relative = spectrum.interp_at(B_FILTER);
    let v_relative = spectrum.interp_at(V_FILTER);
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
        integrated_relative_300_650,
        integrated_uncertainty_abs_300_650,
        b_relative,
        v_relative,
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
    use siderust::qtty::Kilometer;

    #[test]
    fn airglow_builtin_calibration_checksum_matches() {
        use siderust::checksum::{sha256, to_hex};
        let asset = airglow_continuum_asset();
        assert_eq!(to_hex(&sha256(RAW.as_bytes())), asset.sha256);
    }

    #[test]
    fn airglow_continuum_build_metadata_provenance() {
        let asset = airglow_continuum_asset();
        assert_eq!(asset.path, AIRGLOW_CONTINUUM_RELATIVE_PATH);
        assert_eq!(asset.schema, "skycalc-airglow-continuum-v1");
        assert!(
            asset.source.contains("Cerro Paranal")
                && asset.source.contains("Noll")
                && asset.source.contains("FORS1"),
            "registry source must record Paranal/Noll/FORS1 lineage: {}",
            asset.source
        );
        assert!(
            asset.license.contains("not recorded"),
            "license must remain explicitly unresolved until verified"
        );
        assert_eq!(asset.calibration_status, "planning-proxy");
        assert_eq!(asset.generator, "historical import");
        assert!(!asset.validation_report.is_empty());
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
        assert!((c.emission_height_km.to::<Kilometer>().value() - 90.0).abs() < 1.0e-12);
        assert!((c.global_scale.value() - 79.829).abs() < 1.0e-12);
    }
}
