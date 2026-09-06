//! Airglow continuum calibration data and validated loader.
//!
//! The reference file `data/airglow_cont.dat` is a multi-block format with
//! per-season and per-night-phase scaling factors. Parsing produces a transient
//! definition which is validated once before an [`AirglowContinuum`] can exist.
//! Runtime evaluation therefore consumes fixed-shape correction tables and
//! validated spectra rather than re-checking persistent structure.
//!
//! Provenance:
//! Scientific metadata for the bundled continuum is owned by
//! `crates/nsb/data/manifest.toml` and surfaced through build-generated
//! [`crate::assets::BundledAssetMetadata`]. Integrity of the embedded bytes is
//! guaranteed by the build script (existence + SHA-256) before compilation.

use super::domain::{AirglowNightPhase, AirglowSeason};
use crate::assets::{bundled_asset, BundledAssetMetadata};
use crate::error::{NsbError, Result};
use crate::units::ScaleFactors;
use optica::data::Provenance;
use optica::grid::OutOfRange;
use optica::spectrum::{Interpolation, SampledSpectrum};
use qtty::length::{Kilometers, Micrometers, Nanometer};
use qtty::unit::Ratio;

const RAW: &str = include_str!("../../../data/airglow_cont.dat");
const AIRGLOW_CONTINUUM_FILE: &str = "airglow_cont.dat";
const NAMED_SEASON_COUNT: usize = 6;
const NAMED_NIGHT_PHASE_COUNT: usize = 3;
const CORRECTION_ROWS: usize = NAMED_NIGHT_PHASE_COUNT + 1;
const CORRECTION_COLS: usize = NAMED_SEASON_COUNT + 1;

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

#[derive(Debug, Clone)]
struct CorrectionTable {
    values: [[f64; CORRECTION_COLS]; CORRECTION_ROWS],
}

impl CorrectionTable {
    fn try_from_rows(rows: Vec<Vec<f64>>, label: &'static str, non_negative: bool) -> Result<Self> {
        if rows.len() != CORRECTION_ROWS {
            return Err(data_error(format!(
                "{label} has {} rows, expected {CORRECTION_ROWS}",
                rows.len()
            )));
        }

        let mut values = [[0.0; CORRECTION_COLS]; CORRECTION_ROWS];
        for (row_idx, row) in rows.into_iter().enumerate() {
            if row.len() != CORRECTION_COLS {
                return Err(data_error(format!(
                    "{label} row {row_idx} has {} columns, expected {CORRECTION_COLS}",
                    row.len()
                )));
            }
            for (col_idx, value) in row.into_iter().enumerate() {
                if !value.is_finite() {
                    return Err(data_error(format!(
                        "{label} entry ({row_idx}, {col_idx}) must be finite"
                    )));
                }
                if non_negative && value < 0.0 {
                    return Err(data_error(format!(
                        "{label} entry ({row_idx}, {col_idx}) must be non-negative"
                    )));
                }
                values[row_idx][col_idx] = value;
            }
        }
        Ok(Self { values })
    }

    fn get(&self, phase: AirglowNightPhase, season: AirglowSeason) -> f64 {
        let row = match phase {
            AirglowNightPhase::FullNight => 0,
            AirglowNightPhase::FirstThird => 1,
            AirglowNightPhase::MiddleThird => 2,
            AirglowNightPhase::LastThird => 3,
        };
        let col = match season {
            AirglowSeason::FullYear => 0,
            AirglowSeason::DecJan => 1,
            AirglowSeason::FebMar => 2,
            AirglowSeason::AprMay => 3,
            AirglowSeason::JunJul => 4,
            AirglowSeason::AugSep => 5,
            AirglowSeason::OctNov => 6,
        };
        self.values[row][col]
    }
}

/// Validated Airglow continuum calibration.
///
/// All persistent fields are private. Construction validates the empirical
/// schema, numeric coefficients, fixed `4 × 7` correction tables, and spectral
/// grids before runtime evaluation can access them.
#[derive(Debug, Clone)]
pub struct AirglowContinuum {
    global_scale: ScaleFactors,
    emission_height_km: Kilometers,
    solar_activity_const: f64,
    solar_activity_slope: f64,
    mean_corrections: CorrectionTable,
    sigma_corrections: CorrectionTable,
    spectrum: SampledSpectrum<Nanometer, Ratio>,
    uncertainty: SampledSpectrum<Nanometer, Ratio>,
}

impl AirglowContinuum {
    /// Return the validated relative mean spectrum.
    pub fn spectrum(&self) -> &SampledSpectrum<Nanometer, Ratio> {
        &self.spectrum
    }

    /// Return the validated relative uncertainty spectrum.
    pub fn uncertainty(&self) -> &SampledSpectrum<Nanometer, Ratio> {
        &self.uncertainty
    }

    pub(crate) fn emission_height_km(&self) -> Kilometers {
        self.emission_height_km
    }

    pub(crate) fn global_scale(&self) -> ScaleFactors {
        self.global_scale
    }

    pub(crate) fn solar_activity_correction(&self, solar_radio_flux: f64) -> f64 {
        self.solar_activity_const + self.solar_activity_slope * solar_radio_flux
    }

    pub(crate) fn mean_correction(
        &self,
        phase: AirglowNightPhase,
        season: AirglowSeason,
    ) -> f64 {
        self.mean_corrections.get(phase, season)
    }

    pub(crate) fn sigma_correction(
        &self,
        phase: AirglowNightPhase,
        season: AirglowSeason,
    ) -> f64 {
        self.sigma_corrections.get(phase, season)
    }
}

/// Parse a caller-provided SkyCalc-format Airglow continuum calibration.
///
/// Parsing is fallible and returns only a fully validated [`AirglowContinuum`].
/// The same validation path is used by the bundled calibration loader.
impl std::str::FromStr for AirglowContinuum {
    type Err = NsbError;

    fn from_str(source: &str) -> Result<Self> {
        parse_definition(source)?.validate(None)
    }
}

/// Load the built-in empirical Airglow continuum calibration.
///
/// The embedded file passes through the same parser and validation boundary as
/// caller-provided calibration text.
pub(crate) fn load_builtin_standard() -> Result<AirglowContinuum> {
    parse_definition(RAW)?.validate(Some(AIRGLOW_CONTINUUM_ASSET_PATH))
}

#[derive(Debug, Clone)]
struct AirglowContinuumDefinition {
    n_season: usize,
    n_time: usize,
    n_dat: usize,
    emission_height_km: f64,
    global_scale: f64,
    solar_activity_const: f64,
    solar_activity_slope: f64,
    mean_corrections: Vec<Vec<f64>>,
    sigma_corrections: Vec<Vec<f64>>,
    wavelengths_nm: Vec<f64>,
    relative_mean: Vec<f64>,
    relative_sigma: Vec<f64>,
}

impl AirglowContinuumDefinition {
    fn validate(self, bundled_provenance: Option<&'static str>) -> Result<AirglowContinuum> {
        if self.n_season != NAMED_SEASON_COUNT || self.n_time != NAMED_NIGHT_PHASE_COUNT {
            return Err(data_error(format!(
                "unsupported correction dimensions: nseason={} ntime={}; expected {NAMED_SEASON_COUNT} and {NAMED_NIGHT_PHASE_COUNT}",
                self.n_season, self.n_time
            )));
        }
        if !self.emission_height_km.is_finite() || self.emission_height_km <= 0.0 {
            return Err(data_error(
                "emission height must be finite and greater than zero".into(),
            ));
        }
        if !self.global_scale.is_finite() || self.global_scale < 0.0 {
            return Err(data_error(
                "global scale must be finite and non-negative".into(),
            ));
        }
        if !self.solar_activity_const.is_finite() || !self.solar_activity_slope.is_finite() {
            return Err(data_error(
                "solar-activity intercept and slope must be finite".into(),
            ));
        }

        validate_matching_matrix_shapes(&self.mean_corrections, &self.sigma_corrections)?;
        let mean_corrections =
            CorrectionTable::try_from_rows(self.mean_corrections, "mean corrections", false)?;
        let sigma_corrections =
            CorrectionTable::try_from_rows(self.sigma_corrections, "sigma corrections", true)?;

        if self.n_dat < 2 {
            return Err(data_error(
                "airglow spectrum must contain at least two wavelength samples".into(),
            ));
        }
        for (label, len) in [
            ("wavelength", self.wavelengths_nm.len()),
            ("relative mean", self.relative_mean.len()),
            ("relative uncertainty", self.relative_sigma.len()),
        ] {
            if len != self.n_dat {
                return Err(data_error(format!(
                    "{label} sample count is {len}, expected ndat={}",
                    self.n_dat
                )));
            }
        }
        for (idx, wavelength_nm) in self.wavelengths_nm.iter().copied().enumerate() {
            if !wavelength_nm.is_finite() || wavelength_nm <= 0.0 {
                return Err(data_error(format!(
                    "wavelength sample {idx} must be finite and greater than zero"
                )));
            }
            if idx > 0 && wavelength_nm <= self.wavelengths_nm[idx - 1] {
                return Err(data_error(format!(
                    "wavelength grid must be strictly increasing at sample {idx}"
                )));
            }
        }
        for (idx, value) in self.relative_mean.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(data_error(format!(
                    "relative mean sample {idx} must be finite"
                )));
            }
        }
        for (idx, value) in self.relative_sigma.iter().copied().enumerate() {
            if !value.is_finite() || value < 0.0 {
                return Err(data_error(format!(
                    "relative uncertainty sample {idx} must be finite and non-negative"
                )));
            }
        }

        let spectrum = SampledSpectrum::<Nanometer, Ratio>::from_raw(
            self.wavelengths_nm.clone(),
            self.relative_mean,
            Interpolation::Linear,
            OutOfRange::ClampToEndpoints,
            bundled_provenance.map(Provenance::bundled_file),
        )
        .map_err(|error| data_error(format!("invalid relative mean spectrum: {error}")))?;
        let uncertainty = SampledSpectrum::<Nanometer, Ratio>::from_raw(
            self.wavelengths_nm,
            self.relative_sigma,
            Interpolation::Linear,
            OutOfRange::ClampToEndpoints,
            bundled_provenance.map(Provenance::bundled_file),
        )
        .map_err(|error| data_error(format!("invalid uncertainty spectrum: {error}")))?;

        Ok(AirglowContinuum {
            global_scale: ScaleFactors::new(self.global_scale),
            emission_height_km: Kilometers::new(self.emission_height_km),
            solar_activity_const: self.solar_activity_const,
            solar_activity_slope: self.solar_activity_slope,
            mean_corrections,
            sigma_corrections,
            spectrum,
            uncertainty,
        })
    }
}

fn parse_definition(source: &str) -> Result<AirglowContinuumDefinition> {
    let mut iter = source.lines().filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            Some(trimmed)
        }
    });

    let dimensions = iter
        .next()
        .ok_or_else(|| data_error("missing nseason/ntime".into()))?;
    let (n_season, n_time) = parse_two_usizes(dimensions, "nseason/ntime")?;
    if n_season != NAMED_SEASON_COUNT || n_time != NAMED_NIGHT_PHASE_COUNT {
        return Err(data_error(format!(
            "unsupported correction dimensions: nseason={n_season} ntime={n_time}; expected {NAMED_SEASON_COUNT} and {NAMED_NIGHT_PHASE_COUNT}"
        )));
    }

    let n_dat = parse_one_usize(
        iter.next()
            .ok_or_else(|| data_error("missing ndat".into()))?,
        "ndat",
    )?;
    let emission_height_km = parse_one_float(
        iter.next()
            .ok_or_else(|| data_error("missing emission height".into()))?,
        "height",
    )?;
    let global_scale = parse_one_float(
        iter.next()
            .ok_or_else(|| data_error("missing global scale".into()))?,
        "scale",
    )?;
    let solar = parse_floats(
        iter.next()
            .ok_or_else(|| data_error("missing solar activity correction".into()))?,
        "solar activity correction",
    )?;
    if solar.len() != 2 {
        return Err(data_error(format!(
            "expected 2 solar-activity values, got {}",
            solar.len()
        )));
    }

    let mean_corrections = parse_matrix(
        &mut iter,
        CORRECTION_ROWS,
        CORRECTION_COLS,
        "mean corrections",
    )?;
    let sigma_corrections = parse_matrix(
        &mut iter,
        CORRECTION_ROWS,
        CORRECTION_COLS,
        "sigma corrections",
    )?;

    let mut wavelengths_nm = Vec::with_capacity(n_dat);
    let mut relative_mean = Vec::with_capacity(n_dat);
    let mut relative_sigma = Vec::with_capacity(n_dat);
    for sample_idx in 0..n_dat {
        let row = iter
            .next()
            .ok_or_else(|| data_error("premature EOF in data block".into()))?;
        let values = parse_floats(row, "spectral data")?;
        if values.len() != 3 {
            return Err(data_error(format!(
                "spectral data row {sample_idx} has {} columns, expected 3",
                values.len()
            )));
        }
        wavelengths_nm.push(Micrometers::new(values[0]).to::<Nanometer>().value());
        relative_mean.push(values[1]);
        relative_sigma.push(values[2]);
    }
    if iter.next().is_some() {
        return Err(data_error(
            "unexpected trailing data after declared spectral samples".into(),
        ));
    }

    Ok(AirglowContinuumDefinition {
        n_season,
        n_time,
        n_dat,
        emission_height_km,
        global_scale,
        solar_activity_const: solar[0],
        solar_activity_slope: solar[1],
        mean_corrections,
        sigma_corrections,
        wavelengths_nm,
        relative_mean,
        relative_sigma,
    })
}

fn validate_matching_matrix_shapes(mean: &[Vec<f64>], sigma: &[Vec<f64>]) -> Result<()> {
    if mean.len() != sigma.len() {
        return Err(data_error(format!(
            "mean/sigma correction row counts differ: {} versus {}",
            mean.len(),
            sigma.len()
        )));
    }
    for (row_idx, (mean_row, sigma_row)) in mean.iter().zip(sigma).enumerate() {
        if mean_row.len() != sigma_row.len() {
            return Err(data_error(format!(
                "mean/sigma correction column counts differ in row {row_idx}: {} versus {}",
                mean_row.len(),
                sigma_row.len()
            )));
        }
    }
    Ok(())
}

fn parse_two_usizes(row: &str, label: &'static str) -> Result<(usize, usize)> {
    let values: Vec<_> = row.split_whitespace().collect();
    if values.len() != 2 {
        return Err(data_error(format!(
            "{label} row has {} values, expected 2",
            values.len()
        )));
    }
    let first = values[0]
        .parse::<usize>()
        .map_err(|_| data_error(format!("invalid {label} value: {:?}", values[0])))?;
    let second = values[1]
        .parse::<usize>()
        .map_err(|_| data_error(format!("invalid {label} value: {:?}", values[1])))?;
    Ok((first, second))
}

fn parse_one_usize(row: &str, label: &'static str) -> Result<usize> {
    let values: Vec<_> = row.split_whitespace().collect();
    if values.len() != 1 {
        return Err(data_error(format!(
            "{label} row has {} values, expected 1",
            values.len()
        )));
    }
    values[0]
        .parse::<usize>()
        .map_err(|_| data_error(format!("invalid {label} value: {:?}", values[0])))
}

fn parse_one_float(row: &str, label: &'static str) -> Result<f64> {
    let values = parse_floats(row, label)?;
    if values.len() != 1 {
        return Err(data_error(format!(
            "{label} row has {} values, expected 1",
            values.len()
        )));
    }
    Ok(values[0])
}

fn parse_floats(row: &str, label: &'static str) -> Result<Vec<f64>> {
    row.split_whitespace()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| data_error(format!("bad numeric value in {label}: {value:?}")))
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
        let row = iter
            .next()
            .ok_or_else(|| data_error(format!("premature EOF in {label}")))?;
        let values = parse_floats(row, label)?;
        if values.len() != cols {
            return Err(data_error(format!(
                "{label} row {row_idx} has {} columns, expected {cols}",
                values.len()
            )));
        }
        out.push(values);
    }
    Ok(out)
}

fn data_error(message: String) -> NsbError {
    NsbError::DataParse {
        file: AIRGLOW_CONTINUUM_FILE,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::Kilometer;

    fn raw_definition() -> AirglowContinuumDefinition {
        parse_definition(RAW).expect("raw bundled airglow definition")
    }

    fn assert_invalid(definition: AirglowContinuumDefinition, needle: &str) {
        let error = definition
            .validate(None)
            .expect_err("malformed calibration must be rejected");
        assert!(
            error.to_string().contains(needle),
            "expected {needle:?} in {error}"
        );
    }

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
    fn airglow_builtin_calibration_parses_full_validated_structure() {
        let continuum = load_builtin_standard().expect("airglow continuum calibration");
        assert_eq!(continuum.spectrum().len(), 46);
        assert_eq!(continuum.uncertainty().len(), 46);
        assert!(
            (continuum.emission_height_km().to::<Kilometer>().value() - 90.0).abs() < 1.0e-12
        );
        assert!((continuum.global_scale().value() - 79.829).abs() < 1.0e-12);
        assert_eq!(
            continuum.mean_correction(AirglowNightPhase::FullNight, AirglowSeason::FullYear),
            0.998
        );
        assert_eq!(
            continuum.mean_correction(AirglowNightPhase::LastThird, AirglowSeason::OctNov),
            1.035
        );
        assert_eq!(
            continuum.sigma_correction(AirglowNightPhase::FirstThird, AirglowSeason::DecJan),
            0.202
        );
    }

    #[test]
    fn correction_table_maps_every_typed_domain_combination() {
        let rows = (0..CORRECTION_ROWS)
            .map(|row| {
                (0..CORRECTION_COLS)
                    .map(|col| (row * 100 + col) as f64)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let table = CorrectionTable::try_from_rows(rows, "test corrections", false).unwrap();
        let phases = [
            (AirglowNightPhase::FullNight, 0),
            (AirglowNightPhase::FirstThird, 1),
            (AirglowNightPhase::MiddleThird, 2),
            (AirglowNightPhase::LastThird, 3),
        ];
        let seasons = [
            (AirglowSeason::FullYear, 0),
            (AirglowSeason::DecJan, 1),
            (AirglowSeason::FebMar, 2),
            (AirglowSeason::AprMay, 3),
            (AirglowSeason::JunJul, 4),
            (AirglowSeason::AugSep, 5),
            (AirglowSeason::OctNov, 6),
        ];
        for (phase, row) in phases {
            for (season, col) in seasons {
                assert_eq!(table.get(phase, season), (row * 100 + col) as f64);
            }
        }
    }

    #[test]
    fn malformed_correction_shapes_are_rejected() {
        let mut too_few_rows = raw_definition();
        too_few_rows.mean_corrections.pop();
        assert_invalid(too_few_rows, "row counts differ");

        let mut wrong_columns = raw_definition();
        wrong_columns.mean_corrections[0].push(42.0);
        assert_invalid(wrong_columns, "column counts differ");

        let mut too_many_rows = raw_definition();
        too_many_rows
            .mean_corrections
            .push(too_many_rows.mean_corrections[0].clone());
        too_many_rows
            .sigma_corrections
            .push(too_many_rows.sigma_corrections[0].clone());
        assert_invalid(too_many_rows, "has 5 rows, expected 4");

        let mut mismatched_sigma = raw_definition();
        mismatched_sigma.sigma_corrections.pop();
        assert_invalid(mismatched_sigma, "row counts differ");
    }

    #[test]
    fn non_finite_correction_and_coefficients_are_rejected() {
        let mut correction = raw_definition();
        correction.mean_corrections[0][0] = f64::NAN;
        assert_invalid(correction, "must be finite");

        let mut solar = raw_definition();
        solar.solar_activity_slope = f64::INFINITY;
        assert_invalid(solar, "solar-activity intercept and slope must be finite");
    }

    #[test]
    fn invalid_spectral_structure_is_rejected() {
        let mut bad_grid = raw_definition();
        bad_grid.wavelengths_nm[1] = bad_grid.wavelengths_nm[0];
        assert_invalid(bad_grid, "wavelength grid must be strictly increasing");

        let mut mismatched_uncertainty = raw_definition();
        mismatched_uncertainty.relative_sigma.pop();
        assert_invalid(mismatched_uncertainty, "relative uncertainty sample count");

        let mut negative_uncertainty = raw_definition();
        negative_uncertainty.relative_sigma[0] = -0.1;
        assert_invalid(negative_uncertainty, "must be finite and non-negative");
    }

    #[test]
    fn invalid_emission_height_and_scale_are_rejected() {
        let mut height = raw_definition();
        height.emission_height_km = 0.0;
        assert_invalid(height, "emission height must be finite and greater than zero");

        let mut scale = raw_definition();
        scale.global_scale = -1.0;
        assert_invalid(scale, "global scale must be finite and non-negative");
    }

    #[test]
    fn parser_rejects_premature_eof_and_non_finite_numeric_values() {
        let truncated = RAW
            .trim_end()
            .rsplit_once('\n')
            .expect("fixture has multiple rows")
            .0;
        let error = truncated
            .parse::<AirglowContinuum>()
            .expect_err("truncated calibration must fail");
        assert!(error.to_string().contains("premature EOF in data block"));

        let non_finite = RAW.replacen("0.998", "NaN", 1);
        let error = non_finite
            .parse::<AirglowContinuum>()
            .expect_err("non-finite correction must fail");
        assert!(error.to_string().contains("must be finite"));
    }
}
