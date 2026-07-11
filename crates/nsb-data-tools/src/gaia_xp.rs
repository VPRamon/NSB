//! Gaia DR3 XP sampled-spectrum parsing and photon-flux integration.
//!
//! Gaia DataLink `XP_SAMPLED` CSV products use wavelength in nanometres and
//! flux / flux error in W m⁻² nm⁻¹.  Negative finite flux samples are valid
//! noisy measurements and are retained with their sign.  Only the final
//! passband integral is required to be positive for a source to contribute to
//! the starlight map.

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, StringRecord};
use qtty::{unit, Quantity};
use serde::{Deserialize, Serialize};
use siderust::qtty::{Meter, Nanometers};

pub const BAND_MIN_NM: f64 = 336.0;
pub const BAND_MAX_NM: f64 = 650.0;
pub const PHOTOMETRY_MODEL: &str = "gaia_dr3_xp_photon_radiance_336_650nm_v1";
pub const PHOTON_FLUX_COLUMN: &str = "photon_flux_336_650_ph_m2_s";
pub const NORMALIZED_WAVELENGTH_COLUMN: &str = "xp_wavelength_nm";
pub const NORMALIZED_FLUX_COLUMN: &str = "xp_flux_w_m2_nm";
pub const NORMALIZED_FLUX_ERROR_COLUMN: &str = "xp_flux_error_w_m2_nm";

type JouleSeconds = Quantity<unit::Prod<unit::Joule, unit::Second>>;

/// Planck constant, exact in the 2019 SI definition.
const PLANCK_CONSTANT: JouleSeconds = JouleSeconds::new(6.626_070_15e-34);

#[derive(Debug, Clone, PartialEq)]
pub struct XpProduct {
    pub source_id: String,
    pub wavelengths_nm: Vec<f64>,
    pub flux_w_m2_nm: Vec<f64>,
    pub flux_error_w_m2_nm: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PhotonFluxIntegral {
    pub total_ph_m2_s: f64,
    pub positive_ph_m2_s: f64,
    pub negative_ph_m2_s: f64,
    pub negative_contribution_ratio: f64,
    pub uncertainty_ph_m2_s: Option<f64>,
    pub negative_samples: usize,
    pub band_samples: usize,
}

impl PhotonFluxIntegral {
    pub fn negative_sample_fraction(self) -> f64 {
        if self.band_samples == 0 {
            0.0
        } else {
            self.negative_samples as f64 / self.band_samples as f64
        }
    }
}

pub fn contains_service_error(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    [
        "service error",
        "context: dataretrieval",
        "unable to create connection to database",
        "could not retrieve data from table",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

pub fn parse_gaia_datalink_csv(bytes: &[u8], expected_source_id: &str) -> Result<XpProduct> {
    if bytes.is_empty() {
        bail!("empty Gaia DataLink response");
    }
    if contains_service_error(bytes) {
        bail!("Gaia DataLink response contains SERVICE ERROR");
    }
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(bytes);
    let headers = reader
        .headers()
        .context("failed to read Gaia XP CSV header")?
        .clone();
    let source = required_header(&headers, "source_id")?;
    let wavelength = required_header(&headers, "wavelength")?;
    let flux = required_header(&headers, "flux")?;
    let flux_error = required_header(&headers, "flux_error")?;

    let mut wavelengths_nm = Vec::new();
    let mut flux_w_m2_nm = Vec::new();
    let mut flux_error_w_m2_nm = Vec::new();
    for (row_index, row) in reader.records().enumerate() {
        let row = row.with_context(|| format!("malformed Gaia XP CSV row {}", row_index + 2))?;
        let source_id = field(&row, source, "source_id")?;
        if source_id != expected_source_id {
            bail!(
                "Gaia XP source_id mismatch: expected {expected_source_id}, found {source_id}"
            );
        }
        wavelengths_nm.push(parse_f64(&row, wavelength, "wavelength")?);
        flux_w_m2_nm.push(parse_f64(&row, flux, "flux")?);
        flux_error_w_m2_nm.push(parse_f64(&row, flux_error, "flux_error")?);
    }
    let product = XpProduct {
        source_id: expected_source_id.to_string(),
        wavelengths_nm,
        flux_w_m2_nm,
        flux_error_w_m2_nm: Some(flux_error_w_m2_nm),
    };
    validate_product(&product)?;
    Ok(product)
}

pub fn parse_normalized_record(headers: &StringRecord, row: &StringRecord) -> Result<XpProduct> {
    let source = required_header(headers, "source_id")?;
    let wavelength = required_header(headers, NORMALIZED_WAVELENGTH_COLUMN)?;
    let flux = required_header(headers, NORMALIZED_FLUX_COLUMN)?;
    let flux_error = required_header(headers, NORMALIZED_FLUX_ERROR_COLUMN)?;
    let product = XpProduct {
        source_id: field(row, source, "source_id")?.to_string(),
        wavelengths_nm: parse_series(field(row, wavelength, NORMALIZED_WAVELENGTH_COLUMN)?)?,
        flux_w_m2_nm: parse_series(field(row, flux, NORMALIZED_FLUX_COLUMN)?)?,
        flux_error_w_m2_nm: Some(parse_series(field(
            row,
            flux_error,
            NORMALIZED_FLUX_ERROR_COLUMN,
        )?)?),
    };
    validate_product(&product)?;
    Ok(product)
}

pub fn validate_product(product: &XpProduct) -> Result<()> {
    if product.source_id.trim().is_empty() {
        bail!("empty Gaia XP source_id");
    }
    if product.wavelengths_nm.is_empty() {
        bail!("empty Gaia XP spectrum");
    }
    if product.wavelengths_nm.len() != product.flux_w_m2_nm.len() {
        bail!("Gaia XP wavelength/flux length mismatch");
    }
    if let Some(errors) = &product.flux_error_w_m2_nm {
        if errors.len() != product.wavelengths_nm.len() {
            bail!("Gaia XP wavelength/flux_error length mismatch");
        }
        if errors.iter().any(|value| !value.is_finite() || *value < 0.0) {
            bail!("Gaia XP flux_error values must be finite and non-negative");
        }
    }
    if product
        .wavelengths_nm
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        bail!("Gaia XP wavelengths must be finite and positive");
    }
    if product.flux_w_m2_nm.iter().any(|value| !value.is_finite()) {
        bail!("Gaia XP flux values must be finite");
    }
    if product
        .wavelengths_nm
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        bail!("Gaia XP wavelengths must be strictly increasing");
    }
    exact_sample_index(&product.wavelengths_nm, BAND_MIN_NM)
        .context("Gaia XP spectrum has no exact 336 nm sample")?;
    exact_sample_index(&product.wavelengths_nm, BAND_MAX_NM)
        .context("Gaia XP spectrum has no exact 650 nm sample")?;
    Ok(())
}

/// Integrate Fλ λ/(h c) with the trapezoidal rule on the exact 336–650 nm grid.
pub fn integrate_photon_flux(product: &XpProduct) -> Result<PhotonFluxIntegral> {
    validate_product(product)?;
    let first = exact_sample_index(&product.wavelengths_nm, BAND_MIN_NM)?;
    let last = exact_sample_index(&product.wavelengths_nm, BAND_MAX_NM)?;
    if first >= last {
        bail!("Gaia XP integration band contains fewer than two samples");
    }

    let wavelengths = &product.wavelengths_nm[first..=last];
    let fluxes = &product.flux_w_m2_nm[first..=last];
    let errors = product
        .flux_error_w_m2_nm
        .as_ref()
        .map(|values| &values[first..=last]);
    let c_m_s = qtty::velocity::C.value();
    let hc_j_m = PLANCK_CONSTANT.value() * c_m_s;
    let photon_density = wavelengths
        .iter()
        .zip(fluxes)
        .map(|(wavelength_nm, flux)| {
            let wavelength_m = Nanometers::new(*wavelength_nm).to::<Meter>();
            flux * wavelength_m.value() / hc_j_m
        })
        .collect::<Vec<_>>();

    let mut total = 0.0;
    let mut positive = 0.0;
    let mut negative = 0.0;
    for (wave_pair, density_pair) in wavelengths.windows(2).zip(photon_density.windows(2)) {
        let width_nm = wave_pair[1] - wave_pair[0];
        let signed = 0.5 * (density_pair[0] + density_pair[1]) * width_nm;
        total += signed;
        accumulate_signed_linear_segment(
            density_pair[0],
            density_pair[1],
            width_nm,
            &mut positive,
            &mut negative,
        );
    }
    if !total.is_finite() {
        bail!("Gaia XP integrated photon flux is not finite");
    }

    let uncertainty_ph_m2_s = errors.map(|errors| {
        let mut variance = 0.0;
        for sample_index in 0..wavelengths.len() {
            let left_width = sample_index
                .checked_sub(1)
                .map(|index| wavelengths[sample_index] - wavelengths[index])
                .unwrap_or(0.0);
            let right_width = wavelengths
                .get(sample_index + 1)
                .map(|next| *next - wavelengths[sample_index])
                .unwrap_or(0.0);
            let trapezoid_weight_nm = 0.5 * (left_width + right_width);
            let wavelength_m = Nanometers::new(wavelengths[sample_index]).to::<Meter>();
            let photon_error_density = errors[sample_index] * wavelength_m.value() / hc_j_m;
            variance += (photon_error_density * trapezoid_weight_nm).powi(2);
        }
        variance.sqrt()
    });
    let negative_magnitude = -negative;
    let negative_contribution_ratio = if positive > 0.0 {
        negative_magnitude / positive
    } else if negative_magnitude > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    Ok(PhotonFluxIntegral {
        total_ph_m2_s: total,
        positive_ph_m2_s: positive,
        negative_ph_m2_s: negative,
        negative_contribution_ratio,
        uncertainty_ph_m2_s,
        negative_samples: fluxes.iter().filter(|value| **value < 0.0).count(),
        band_samples: fluxes.len(),
    })
}

fn accumulate_signed_linear_segment(
    y0: f64,
    y1: f64,
    width: f64,
    positive: &mut f64,
    negative: &mut f64,
) {
    if y0 >= 0.0 && y1 >= 0.0 {
        *positive += 0.5 * (y0 + y1) * width;
    } else if y0 <= 0.0 && y1 <= 0.0 {
        *negative += 0.5 * (y0 + y1) * width;
    } else {
        let crossing = -y0 / (y1 - y0);
        let first_area = 0.5 * y0 * width * crossing;
        let second_area = 0.5 * y1 * width * (1.0 - crossing);
        for area in [first_area, second_area] {
            if area >= 0.0 {
                *positive += area;
            } else {
                *negative += area;
            }
        }
    }
}

pub fn format_series(values: &[f64], scientific: bool) -> String {
    values
        .iter()
        .map(|value| {
            if scientific {
                format!("{value:.8e}")
            } else {
                format!("{value:.8}")
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

pub fn parse_series(raw: &str) -> Result<Vec<f64>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    raw.split(';')
        .map(|value| {
            value
                .trim()
                .parse::<f64>()
                .with_context(|| format!("invalid numeric series value {value:?}"))
        })
        .collect()
}

fn exact_sample_index(wavelengths: &[f64], target_nm: f64) -> Result<usize> {
    wavelengths
        .binary_search_by(|value| value.total_cmp(&target_nm))
        .map_err(|_| anyhow::anyhow!("missing exact {target_nm} nm sample"))
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|header| header.trim() == name)
        .ok_or_else(|| anyhow::anyhow!("missing required Gaia XP column {name:?}"))
}

fn field<'a>(row: &'a StringRecord, index: usize, name: &str) -> Result<&'a str> {
    row.get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing Gaia XP field {name:?}"))
}

fn parse_f64(row: &StringRecord, index: usize, name: &str) -> Result<f64> {
    field(row, index, name)?
        .parse::<f64>()
        .with_context(|| format!("invalid Gaia XP numeric field {name:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(fluxes: Vec<f64>) -> XpProduct {
        XpProduct {
            source_id: "42".to_string(),
            wavelengths_nm: vec![336.0, 400.0, 650.0, 652.0],
            flux_error_w_m2_nm: Some(vec![1.0e-14; 4]),
            flux_w_m2_nm: fluxes,
        }
    }

    #[test]
    fn parses_real_gaia_schema_and_preserves_error() -> Result<()> {
        let raw = concat!(
            "source_id,solution_id,ra,dec,wavelength,flux,flux_error\n",
            "42,1,0,0,336,1e-12,1e-14\n",
            "42,1,0,0,650,2e-12,2e-14\n",
        );
        let parsed = parse_gaia_datalink_csv(raw.as_bytes(), "42")?;
        assert_eq!(parsed.wavelengths_nm, vec![336.0, 650.0]);
        assert_eq!(parsed.flux_error_w_m2_nm, Some(vec![1.0e-14, 2.0e-14]));
        Ok(())
    }

    #[test]
    fn analytic_constant_energy_flux_matches_closed_form() -> Result<()> {
        let flux = 2.5e-12;
        let spectrum = product(vec![flux, flux, flux, flux]);
        let integrated = integrate_photon_flux(&spectrum)?;
        let hc = PLANCK_CONSTANT.value() * qtty::velocity::C.value();
        let expected = flux * 0.5 * (650.0_f64.powi(2) - 336.0_f64.powi(2)) * 1.0e-9 / hc;
        let relative = (integrated.total_ph_m2_s - expected).abs() / expected;
        assert!(relative < 1.0e-14, "relative error {relative}");
        Ok(())
    }

    #[test]
    fn negative_sample_is_integrated_with_sign_when_total_is_positive() -> Result<()> {
        let integrated = integrate_photon_flux(&product(vec![-1.0e-14, 1.0e-12, 1.0e-12, 1.0e-12]))?;
        assert!(integrated.total_ph_m2_s > 0.0);
        assert!(integrated.positive_ph_m2_s > 0.0);
        assert!(integrated.negative_ph_m2_s < 0.0);
        assert_eq!(integrated.negative_samples, 1);
        assert!(integrated.negative_contribution_ratio > 0.0);
        Ok(())
    }

    #[test]
    fn non_positive_integral_remains_explicit() -> Result<()> {
        let integrated = integrate_photon_flux(&product(vec![-1.0e-12; 4]))?;
        assert!(integrated.total_ph_m2_s < 0.0);
        assert_eq!(integrated.positive_ph_m2_s, 0.0);
        Ok(())
    }

    #[test]
    fn rejects_non_finite_flux_and_non_monotonic_wavelengths() {
        let mut invalid = product(vec![1.0, f64::NAN, 1.0, 1.0]);
        assert!(validate_product(&invalid).is_err());
        invalid.flux_w_m2_nm = vec![1.0; 4];
        invalid.wavelengths_nm = vec![336.0, 650.0, 400.0, 652.0];
        assert!(validate_product(&invalid).is_err());
    }

    #[test]
    fn rejects_infinite_error_mismatched_arrays_and_missing_coverage() {
        let mut invalid = product(vec![1.0; 4]);
        invalid.flux_error_w_m2_nm = Some(vec![1.0, f64::INFINITY, 1.0, 1.0]);
        assert!(validate_product(&invalid).is_err());
        invalid.flux_error_w_m2_nm = Some(vec![1.0]);
        assert!(validate_product(&invalid).is_err());
        invalid.flux_error_w_m2_nm = None;
        invalid.wavelengths_nm = vec![338.0, 400.0, 650.0, 652.0];
        assert!(validate_product(&invalid).is_err());
    }

    #[test]
    fn malformed_empty_and_service_error_responses_are_rejected() {
        assert!(parse_gaia_datalink_csv(b"", "42").is_err());
        assert!(parse_gaia_datalink_csv(b"SERVICE ERROR: unavailable", "42").is_err());
        assert!(parse_gaia_datalink_csv(b"not,csv\n1\n", "42").is_err());
    }
}
