//! PALACE v1.0 continuum runtime data and validated loader.
//!
//! The bundled NSB product preserves the three PALACE continuum templates and
//! the 12-month by 12-local-time climatology for each template. It intentionally
//! excludes PALACE emission lines and atmospheric propagation; the latter stays
//! an independent NSB runtime stage.

use crate::data::bundled::{bundled_asset, BundledAssetMetadata};
use crate::error::{NsbError, Result};
use crate::units::length::Kilometers;

const RAW: &str = include_str!("../../../data/airglow_palace_v1.dat");
const FILE: &str = "airglow_palace_v1.dat";
const SCHEMA: &str = "nsb-airglow-palace-continuum-v1";
pub(crate) const SAMPLE_COUNT: usize = 351;
pub(crate) const COMPONENT_COUNT: usize = 3;
const CLIMATOLOGY_ROWS: usize = 12 * 12;

pub(crate) const AIRGLOW_CONTINUUM_RELATIVE_PATH: &str = "airglow_palace_v1.dat";
pub(crate) const AIRGLOW_CONTINUUM_ASSET_PATH: &str = "NSB/data/airglow_palace_v1.dat";

pub(crate) fn airglow_continuum_asset() -> &'static BundledAssetMetadata {
    bundled_asset(AIRGLOW_CONTINUUM_RELATIVE_PATH)
        .expect("airglow_palace_v1.dat must be registered by the build script")
}

#[derive(Debug, Clone)]
pub(crate) struct PalaceContinuumComponent {
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) variability_class: String,
    pub(crate) emission_height_km: Kilometers,
    spectrum_rayleigh_per_nm: [f64; SAMPLE_COUNT],
}

#[derive(Debug, Clone, Copy)]
struct PalaceVariability {
    relative_mean: f64,
    solar_slope_per_100_sfu: f64,
    residual_sigma: f64,
}

#[derive(Debug, Clone, Copy)]
struct PalaceClimatologyCell {
    #[allow(dead_code)]
    nighttime_weight: f64,
    components: [PalaceVariability; COMPONENT_COUNT],
}

#[derive(Debug, Clone)]
pub(crate) struct AirglowContinuum {
    wavelengths_nm: [f64; SAMPLE_COUNT],
    components: [PalaceContinuumComponent; COMPONENT_COUNT],
    climatology: [PalaceClimatologyCell; CLIMATOLOGY_ROWS],
    reference_solar_flux_sfu: f64,
    #[allow(dead_code)]
    solar_flux_evidence_range_sfu: (f64, f64),
}

impl AirglowContinuum {
    pub(crate) fn wavelengths_nm(&self) -> &[f64; SAMPLE_COUNT] {
        &self.wavelengths_nm
    }

    /// PALACE continuum mean and residual one-sigma variability in R/nm.
    pub(crate) fn sample(
        &self,
        sample: usize,
        month: u32,
        local_time_bin: usize,
        solar_flux_sfu: f64,
    ) -> Result<(f64, f64)> {
        if sample >= SAMPLE_COUNT
            || !(1..=12).contains(&month)
            || !(1..=12).contains(&local_time_bin)
        {
            return Err(data_error(
                "PALACE sample/month/time index is out of range".into(),
            ));
        }
        let cell = self.climatology[(month as usize - 1) * 12 + local_time_bin - 1];
        let mut mean = 0.0;
        let mut sigma = 0.0;
        for (component, variability) in self.components.iter().zip(cell.components) {
            let scale = variability.relative_mean
                * (1.0
                    + 0.01
                        * variability.solar_slope_per_100_sfu
                        * (solar_flux_sfu - self.reference_solar_flux_sfu));
            if !scale.is_finite() || scale < 0.0 {
                return Err(NsbError::OutOfRange(format!(
                    "PALACE continuum extrapolation produced a negative/non-finite {} scale at F10.7={solar_flux_sfu} sfu",
                    component.name
                )));
            }
            let template = component.spectrum_rayleigh_per_nm[sample];
            mean += template * scale;
            // This matches PALACE v1.0's linear sum of component deviations.
            sigma += template * variability.residual_sigma;
        }
        Ok((mean, sigma))
    }

    pub(crate) fn representative_emission_height_km(&self) -> Kilometers {
        // The current public geometry API is scalar. The middle continuum-layer
        // height is explicit here; all three source heights remain in the schema.
        self.components[1].emission_height_km
    }

    #[cfg(test)]
    pub(crate) fn solar_flux_evidence_range_sfu(&self) -> (f64, f64) {
        self.solar_flux_evidence_range_sfu
    }

    #[cfg(test)]
    pub(crate) fn nighttime_weight(&self, month: u32, time_bin: usize) -> f64 {
        self.climatology[(month as usize - 1) * 12 + time_bin - 1].nighttime_weight
    }
}

impl std::str::FromStr for AirglowContinuum {
    type Err = NsbError;

    fn from_str(source: &str) -> Result<Self> {
        parse(source)
    }
}

pub(crate) fn load_builtin_standard() -> Result<AirglowContinuum> {
    parse(RAW)
}

fn parse(source: &str) -> Result<AirglowContinuum> {
    let mut lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'));

    expect_fields(&mut lines, &["schema", SCHEMA])?;
    expect_fields(&mut lines, &["wavelength_unit", "nm"])?;
    expect_fields(&mut lines, &["radiance_unit", "R_per_nm"])?;
    let reference_solar_flux_sfu = parse_scalar(&mut lines, "reference_solar_flux_sfu")?;
    let evidence = next_fields(&mut lines, "solar_flux_evidence_range_sfu")?;
    if evidence.len() != 3 {
        return Err(data_error(
            "solar evidence range requires two values".into(),
        ));
    }
    let solar_flux_evidence_range_sfu = (parse_f64(evidence[1])?, parse_f64(evidence[2])?);
    expect_fields(&mut lines, &["wavelength_range_nm", "300", "650"])?;
    expect_fields(&mut lines, &["wavelength_step_nm", "1"])?;
    expect_fields(&mut lines, &["component_count", "3"])?;
    expect_fields(&mut lines, &["climatology_rows", "144"])?;

    let mut definitions = Vec::with_capacity(COMPONENT_COUNT);
    for _ in 0..COMPONENT_COUNT {
        let fields = next_fields(&mut lines, "component")?;
        if fields.len() != 4 {
            return Err(data_error(
                "component requires name, class, and height".into(),
            ));
        }
        let height = parse_f64(fields[3])?;
        if !height.is_finite() || height <= 0.0 {
            return Err(data_error(
                "component height must be finite and positive".into(),
            ));
        }
        definitions.push((fields[1].to_string(), fields[2].to_string(), height));
    }
    expect_fields(&mut lines, &["spectra_begin"])?;
    let mut wavelengths = Vec::with_capacity(SAMPLE_COUNT);
    let mut spectra: [Vec<f64>; COMPONENT_COUNT] =
        std::array::from_fn(|_| Vec::with_capacity(SAMPLE_COUNT));
    for index in 0..SAMPLE_COUNT {
        let fields: Vec<_> = lines
            .next()
            .ok_or_else(|| data_error("premature EOF in spectra".into()))?
            .split_whitespace()
            .collect();
        if fields.len() != 4 {
            return Err(data_error(format!(
                "spectral row {index} must have four values"
            )));
        }
        let wavelength = parse_f64(fields[0])?;
        if wavelength != (300 + index) as f64 {
            return Err(data_error(format!(
                "non-canonical wavelength at row {index}"
            )));
        }
        wavelengths.push(wavelength);
        for component in 0..COMPONENT_COUNT {
            let value = parse_f64(fields[component + 1])?;
            if !value.is_finite() || value < 0.0 {
                return Err(data_error(format!(
                    "invalid continuum value at row {index}"
                )));
            }
            spectra[component].push(value);
        }
    }
    expect_fields(&mut lines, &["spectra_end"])?;
    expect_fields(&mut lines, &["climatology_begin"])?;
    let mut climatology = Vec::with_capacity(CLIMATOLOGY_ROWS);
    for index in 0..CLIMATOLOGY_ROWS {
        let fields: Vec<_> = lines
            .next()
            .ok_or_else(|| data_error("premature EOF in climatology".into()))?
            .split_whitespace()
            .collect();
        if fields.len() != 12 {
            return Err(data_error(format!(
                "climatology row {index} must have 12 values"
            )));
        }
        let month = fields[0]
            .parse::<usize>()
            .map_err(|_| data_error("invalid month".into()))?;
        let time_bin = fields[1]
            .parse::<usize>()
            .map_err(|_| data_error("invalid time bin".into()))?;
        if month != index / 12 + 1 || time_bin != index % 12 + 1 {
            return Err(data_error(
                "climatology is not in canonical month/time order".into(),
            ));
        }
        let nighttime_weight = parse_f64(fields[2])?;
        if !nighttime_weight.is_finite() || nighttime_weight < 0.0 {
            return Err(data_error("invalid nighttime weight".into()));
        }
        let mut parsed = Vec::with_capacity(COMPONENT_COUNT);
        for component in 0..COMPONENT_COUNT {
            let offset = 3 + component * 3;
            parsed.push(PalaceVariability {
                relative_mean: parse_f64(fields[offset])?,
                solar_slope_per_100_sfu: parse_f64(fields[offset + 1])?,
                residual_sigma: parse_f64(fields[offset + 2])?,
            });
        }
        if parsed.iter().any(|value| {
            !value.relative_mean.is_finite()
                || value.relative_mean < 0.0
                || !value.solar_slope_per_100_sfu.is_finite()
                || !value.residual_sigma.is_finite()
                || value.residual_sigma < 0.0
        }) {
            return Err(data_error("invalid climatology values".into()));
        }
        climatology.push(PalaceClimatologyCell {
            nighttime_weight,
            components: parsed.try_into().expect("validated component count"),
        });
    }
    expect_fields(&mut lines, &["climatology_end"])?;
    if lines.next().is_some() {
        return Err(data_error("unexpected trailing data".into()));
    }

    let components: Vec<_> = definitions
        .into_iter()
        .enumerate()
        .map(
            |(index, (name, variability_class, height))| PalaceContinuumComponent {
                name,
                variability_class,
                emission_height_km: Kilometers::new(height),
                spectrum_rayleigh_per_nm: spectra[index]
                    .clone()
                    .try_into()
                    .expect("validated length"),
            },
        )
        .collect();
    Ok(AirglowContinuum {
        wavelengths_nm: wavelengths.try_into().expect("validated length"),
        components: components
            .try_into()
            .map_err(|_| data_error("component count".into()))?,
        climatology: climatology
            .try_into()
            .map_err(|_| data_error("climatology count".into()))?,
        reference_solar_flux_sfu,
        solar_flux_evidence_range_sfu,
    })
}

fn next_fields<'a>(lines: &mut impl Iterator<Item = &'a str>, label: &str) -> Result<Vec<&'a str>> {
    let fields: Vec<_> = lines
        .next()
        .ok_or_else(|| data_error(format!("missing {label}")))?
        .split_whitespace()
        .collect();
    if fields.first().copied() != Some(label) {
        return Err(data_error(format!("expected {label}")));
    }
    Ok(fields)
}

fn expect_fields<'a>(lines: &mut impl Iterator<Item = &'a str>, expected: &[&str]) -> Result<()> {
    let fields: Vec<_> = lines
        .next()
        .ok_or_else(|| data_error(format!("missing {}", expected[0])))?
        .split_whitespace()
        .collect();
    if fields != expected {
        return Err(data_error(format!("expected {}", expected.join(" "))));
    }
    Ok(())
}

fn parse_scalar<'a>(lines: &mut impl Iterator<Item = &'a str>, label: &str) -> Result<f64> {
    let fields = next_fields(lines, label)?;
    if fields.len() != 2 {
        return Err(data_error(format!("{label} requires one value")));
    }
    parse_f64(fields[1])
}

fn parse_f64(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|_| data_error(format!("invalid numeric value {value:?}")))
}

fn data_error(message: String) -> NsbError {
    NsbError::DataParse {
        file: FILE,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::checksum::{sha256, to_hex};

    #[test]
    fn bundled_product_checksum_and_schema_match_manifest() {
        let asset = airglow_continuum_asset();
        assert_eq!(asset.schema, SCHEMA);
        assert_eq!(to_hex(&sha256(RAW.as_bytes())), asset.sha256);
        assert_eq!(asset.license, "CC-BY-4.0");
    }

    #[test]
    fn bundled_product_preserves_palace_dimensions() {
        let model = load_builtin_standard().unwrap();
        assert_eq!(model.wavelengths_nm[0], 300.0);
        assert_eq!(model.wavelengths_nm[SAMPLE_COUNT - 1], 650.0);
        assert_eq!(model.components[0].name, "HO2");
        assert_eq!(model.components[1].variability_class, "FeO");
        assert_eq!(model.components[2].emission_height_km.value(), 94.0);
        assert_eq!(model.solar_flux_evidence_range_sfu(), (67.0, 166.0));
        assert_eq!(model.nighttime_weight(1, 1), 0.0);
    }

    #[test]
    fn representative_dimensions_change_the_continuum() {
        let model = load_builtin_standard().unwrap();
        let january_early = model.sample(250, 1, 2, 100.0).unwrap().0;
        let january_late = model.sample(250, 1, 10, 100.0).unwrap().0;
        let july_early = model.sample(250, 7, 2, 100.0).unwrap().0;
        let solar_high = model.sample(250, 1, 2, 160.0).unwrap().0;
        assert_ne!(january_early.to_bits(), january_late.to_bits());
        assert_ne!(january_early.to_bits(), july_early.to_bits());
        assert_ne!(january_early.to_bits(), solar_high.to_bits());
    }

    #[test]
    fn malformed_and_negative_data_fail_closed() {
        assert!(RAW
            .replace("wavelength_unit nm", "wavelength_unit um")
            .parse::<AirglowContinuum>()
            .is_err());
        assert!(RAW
            .replace("300 2.1607424e-1", "300 -2.1607424e-1")
            .parse::<AirglowContinuum>()
            .is_err());
    }
}
