//! Deterministic, NSB-authored analytic solar reference generation and validation.

use super::{Artifact, RunConfig, SourceConfig, ValidationGate};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const RUNTIME_NAME: &str = "solar_spectrum.dat";
pub(super) const SOURCE_NAME: &str = "solar-planck-v1.toml";
const PRODUCT_ID: &str = "nsb-planck-solar-reference";
const RELEASE: &str = "NSB analytic solar reference v1";
const LICENSE: &str = "AGPL-3.0-only";
const TERMS_URL: &str = "https://github.com/VPRamon/NSB/blob/main/LICENSE";
const UNITS: &str = "W m^-2 nm^-1";
const REFERENCE_DISTANCE: &str = "1 AU";
const BAND_MIN_NM: f64 = 300.0;
const BAND_MAX_NM: f64 = 650.0;
const RUNTIME_STEP_NM: f64 = 1.0;
const RUNTIME_SAMPLE_COUNT: usize = 351;
const REQUIRED_ANCHORS_NM: [f64; 3] = [445.0, 500.0, 551.0];
const EXP_SERIES_TERMS: u32 = 24;
const OUTPUT_DECIMALS: usize = 12;

#[derive(Clone, Copy, Debug)]
struct Sample {
    wavelength_nm: f64,
    irradiance_w_m2_nm: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalyticModel {
    schema_version: u32,
    model_id: String,
    model_version: String,
    effective_temperature_k: f64,
    solar_radius_m: f64,
    astronomical_unit_m: f64,
    planck_constant_j_s: f64,
    speed_of_light_m_s: f64,
    boltzmann_constant_j_k: f64,
    temperature_source: String,
    radius_source: String,
    si_constants_source: String,
    terms: String,
    terms_url: String,
}

pub(super) fn output_name(source_name: &str) -> Result<&str> {
    if source_name == SOURCE_NAME {
        Ok(RUNTIME_NAME)
    } else {
        bail!("unexpected solar-spectrum source {source_name:?}")
    }
}

pub(super) fn validate_config(config: &RunConfig) -> Result<()> {
    if config.sources.len() != 1 {
        bail!("solar-spectrum requires exactly one pinned NSB analytic-model specification");
    }
    let source = required_source(config, SOURCE_NAME)?;
    let required = [
        ("product_id", source.product_id.as_deref(), PRODUCT_ID),
        ("release", source.release.as_deref(), RELEASE),
        ("license", source.license.as_deref(), LICENSE),
        ("units", source.units.as_deref(), UNITS),
        (
            "reference_distance",
            source.reference_distance.as_deref(),
            REFERENCE_DISTANCE,
        ),
    ];
    for (field, actual, expected) in required {
        if actual != Some(expected) {
            bail!("source {:?} requires {field} = {expected:?}", source.name);
        }
    }
    if source.metadata_url.as_deref() != Some(TERMS_URL) {
        bail!(
            "source {:?} requires metadata_url = {TERMS_URL:?}",
            source.name
        );
    }
    if source
        .retrieved_at
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        bail!("source {:?} requires non-empty retrieved_at", source.name);
    }
    Ok(())
}

pub(super) fn transform(source_name: &str, input: &Path, output: &Path) -> Result<()> {
    if source_name != SOURCE_NAME {
        bail!("cannot build runtime solar spectrum from {source_name:?}");
    }
    let model = parse_model(input)?;
    let input_sha256 = crate::platform::checksum_io::sha256_file(input)?;
    let bytes = render_runtime(&model, &input_sha256)?;
    crate::dataset::engine::atomic_write(output, bytes.as_bytes())
}

pub(super) fn validate_artifact(name: &str, path: &Path) -> Result<()> {
    if name != RUNTIME_NAME {
        bail!("unexpected solar-spectrum artifact {name:?}");
    }
    let text = fs::read_to_string(path)?;
    let generator_header = format!("# generator=nsb-data-tools-{}\n", env!("CARGO_PKG_VERSION"));
    for required in [
        "# wavelength_nm,irradiance_W_m2_nm\n",
        "# source_product=nsb-planck-solar-reference\n",
        "# source_release=NSB analytic solar reference v1\n",
        "# source_terms=AGPL-3.0-only\n",
        "# source_terms_url=https://github.com/VPRamon/NSB/blob/main/LICENSE\n",
        generator_header.as_str(),
        "# numeric_method=range-reduced fixed-order Taylor expm1 (24 terms)\n",
        "# serialization=canonical scientific notation with 12 digits after decimal\n",
        "# units=W m^-2 nm^-1\n",
        "# reference_distance=1 AU\n",
        "# runtime_grid_method=analytic Planck spectral irradiance at integer-nanometre nodes\n",
        "# runtime_sampling_interval_nm=1\n",
    ] {
        if !text.contains(required) {
            bail!("solar spectrum is missing required header {required:?}");
        }
    }
    let checksum = header_value(&text, "input_sha256")?;
    if checksum.len() != 64
        || !checksum
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("solar spectrum input_sha256 is not a lowercase SHA-256 digest");
    }
    let samples = parse_runtime(&text)?;
    validate_samples(&samples)?;
    validate_runtime_representation(&samples)
}

pub(super) fn validation_gates(
    config: &RunConfig,
    artifacts: &[Artifact],
) -> Result<Vec<ValidationGate>> {
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.name == RUNTIME_NAME)
        .context("solar runtime artifact is missing")?;
    let runtime_text = fs::read_to_string(&artifact.path)?;
    let runtime = parse_runtime(&runtime_text)?;
    let source = required_source(config, SOURCE_NAME)?;
    let input = config.workspace.root.join("sources").join(SOURCE_NAME);
    let model = parse_model(&input)?;
    let input_sha256 = crate::platform::checksum_io::sha256_file(&input)?;
    let runtime_input_sha256 = header_value(&runtime_text, "input_sha256")?;
    let regenerated = render_runtime(&model, &input_sha256)?;
    let expected = generate_samples(&model)?;
    let anchors_exact = REQUIRED_ANCHORS_NM.iter().all(|wavelength| {
        exact_value_at(&runtime, *wavelength).ok() == exact_value_at(&expected, *wavelength).ok()
    });
    let integral = trapezoid_integral(&runtime);
    let expected_integral = trapezoid_integral(&expected);
    let bv = ratio_at(&runtime, 445.0, 551.0)?;

    Ok(vec![
        ValidationGate {
            name: "deterministic-regeneration".into(),
            passed: regenerated.as_bytes() == runtime_text.as_bytes(),
            detail: format!("regenerated bytes equal {}", artifact.sha256),
        },
        ValidationGate {
            name: "pinned-input-checksum".into(),
            passed: input_sha256 == source.sha256,
            detail: format!(
                "configured_sha256={}; workspace_sha256={input_sha256}",
                source.sha256
            ),
        },
        ValidationGate {
            name: "runtime-input-provenance".into(),
            passed: runtime_input_sha256 == source.sha256,
            detail: format!(
                "configured_sha256={}; runtime_input_sha256={runtime_input_sha256}",
                source.sha256
            ),
        },
        ValidationGate {
            name: "source-provenance-and-terms".into(),
            passed: input_sha256 == source.sha256 && runtime_input_sha256 == source.sha256,
            detail: format!(
                "product={PRODUCT_ID}; release={RELEASE}; input_sha256={input_sha256}; terms={LICENSE}; terms_url={TERMS_URL}"
            ),
        },
        ValidationGate {
            name: "runtime-grid".into(),
            passed: runtime.len() == RUNTIME_SAMPLE_COUNT,
            detail: format!("runtime_samples={}; required={RUNTIME_SAMPLE_COUNT}", runtime.len()),
        },
        ValidationGate {
            name: "runtime-required-anchors".into(),
            passed: anchors_exact,
            detail: "445, 500, and 551 nm are exact analytic-model samples".into(),
        },
        ValidationGate {
            name: "analytic-integral-regression".into(),
            passed: relative_difference(integral, expected_integral) <= 1.0e-14,
            detail: format!(
                "runtime={integral:.12} W m^-2; regenerated={expected_integral:.12} W m^-2"
            ),
        },
        ValidationGate {
            name: "analytic-b-v-regression".into(),
            passed: relative_difference(bv, 0.983_622_222_728_456_1) <= 1.0e-14,
            detail: format!("runtime_445_over_551={bv:.15}"),
        },
    ])
}

fn parse_model(path: &Path) -> Result<AnalyticModel> {
    let raw = fs::read_to_string(path)?;
    let model: AnalyticModel = toml::from_str(&raw)?;
    let exact_numbers = [
        (
            "effective_temperature_k",
            model.effective_temperature_k,
            5772.0,
        ),
        ("solar_radius_m", model.solar_radius_m, 695_700_000.0),
        (
            "astronomical_unit_m",
            model.astronomical_unit_m,
            149_597_870_700.0,
        ),
        (
            "planck_constant_j_s",
            model.planck_constant_j_s,
            6.626_070_15e-34,
        ),
        (
            "speed_of_light_m_s",
            model.speed_of_light_m_s,
            299_792_458.0,
        ),
        (
            "boltzmann_constant_j_k",
            model.boltzmann_constant_j_k,
            1.380_649e-23,
        ),
    ];
    if model.schema_version != 1
        || model.model_id != PRODUCT_ID
        || model.model_version != "1"
        || model.terms != LICENSE
        || model.terms_url != TERMS_URL
    {
        bail!("solar analytic-model identity, version, or terms do not match v1");
    }
    for (field, actual, expected) in exact_numbers {
        if actual != expected {
            bail!("solar analytic-model {field} must equal {expected:.17e}");
        }
    }
    for (field, value) in [
        ("temperature_source", model.temperature_source.as_str()),
        ("radius_source", model.radius_source.as_str()),
        ("si_constants_source", model.si_constants_source.as_str()),
    ] {
        if value.trim().is_empty() {
            bail!("solar analytic-model {field} must not be empty");
        }
    }
    Ok(model)
}

fn render_runtime(model: &AnalyticModel, input_sha256: &str) -> Result<String> {
    let mut bytes = format!(
        "# wavelength_nm,irradiance_W_m2_nm\n\
# source_product={PRODUCT_ID}\n\
# source_release={RELEASE}\n\
# source_terms={LICENSE}\n\
# source_terms_url={TERMS_URL}\n\
# input_sha256={input_sha256}\n\
# generator=nsb-data-tools-{}\n\
# model=Planck spectral radiance scaled by pi*(nominal solar radius/1 AU)^2\n\
# effective_temperature_k=5772\n\
# solar_radius_m=695700000\n\
# astronomical_unit_m=149597870700\n\
# planck_constant_j_s=6.62607015e-34\n\
# speed_of_light_m_s=299792458\n\
# boltzmann_constant_j_k=1.380649e-23\n\
# numeric_method=range-reduced fixed-order Taylor expm1 (24 terms)\n\
# serialization=canonical scientific notation with 12 digits after decimal\n\
# units={UNITS}\n\
# reference_distance={REFERENCE_DISTANCE}\n\
# runtime_grid_method=analytic Planck spectral irradiance at integer-nanometre nodes\n\
# runtime_sampling_interval_nm=1\n",
        env!("CARGO_PKG_VERSION")
    );
    for sample in generate_samples(model)? {
        writeln!(
            bytes,
            "{:.3},{:.prec$e}",
            sample.wavelength_nm,
            sample.irradiance_w_m2_nm,
            prec = OUTPUT_DECIMALS
        )?;
    }
    Ok(bytes)
}

fn generate_samples(model: &AnalyticModel) -> Result<Vec<Sample>> {
    let radius_ratio = model.solar_radius_m / model.astronomical_unit_m;
    let solid_angle_scale = std::f64::consts::PI * radius_ratio * radius_ratio;
    let speed_of_light_squared = model.speed_of_light_m_s * model.speed_of_light_m_s;
    let mut samples = Vec::with_capacity(RUNTIME_SAMPLE_COUNT);
    for index in 0..RUNTIME_SAMPLE_COUNT {
        let wavelength_nm = BAND_MIN_NM + index as f64 * RUNTIME_STEP_NM;
        let wavelength_m = wavelength_nm * 1.0e-9;
        let wavelength_squared = wavelength_m * wavelength_m;
        let wavelength_fourth = wavelength_squared * wavelength_squared;
        let wavelength_fifth = wavelength_fourth * wavelength_m;
        let exponent = model.planck_constant_j_s * model.speed_of_light_m_s
            / (wavelength_m * model.boltzmann_constant_j_k * model.effective_temperature_k);
        let spectral_radiance_per_m = 2.0 * model.planck_constant_j_s * speed_of_light_squared
            / (wavelength_fifth * reproducible_exp_m1(exponent));
        let irradiance = spectral_radiance_per_m * solid_angle_scale * 1.0e-9;
        samples.push(Sample {
            wavelength_nm,
            irradiance_w_m2_nm: canonicalize_irradiance(irradiance)?,
        });
    }
    validate_samples(&samples)?;
    validate_runtime_representation(&samples)?;
    Ok(samples)
}

/// Deterministic exp(x)-1 for the positive Planck exponents used by this dataset.
///
/// Range reduction keeps the fixed-order Taylor series near zero. Only IEEE-754
/// arithmetic with a fixed operation order is used; no platform libm
/// transcendental is involved.
fn reproducible_exp_m1(value: f64) -> f64 {
    let binary_shift = (value / std::f64::consts::LN_2 + 0.5) as u32;
    let reduced = value - f64::from(binary_shift) * std::f64::consts::LN_2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for order in 1..=EXP_SERIES_TERMS {
        term *= reduced / f64::from(order);
        sum += term;
    }
    let mut scale = 1.0;
    for _ in 0..binary_shift {
        scale *= 2.0;
    }
    sum * scale - 1.0
}

fn canonicalize_irradiance(value: f64) -> Result<f64> {
    format!("{value:.prec$e}", prec = OUTPUT_DECIMALS)
        .parse()
        .context("canonical solar irradiance formatting must parse as f64")
}

fn validate_runtime_representation(samples: &[Sample]) -> Result<()> {
    if samples.len() != RUNTIME_SAMPLE_COUNT {
        bail!(
            "solar runtime grid requires exactly {RUNTIME_SAMPLE_COUNT} samples, found {}",
            samples.len()
        );
    }
    for (index, sample) in samples.iter().enumerate() {
        let expected = BAND_MIN_NM + index as f64 * RUNTIME_STEP_NM;
        if sample.wavelength_nm != expected {
            bail!("solar runtime grid must use deterministic 1 nm nodes");
        }
    }
    for wavelength_nm in REQUIRED_ANCHORS_NM {
        exact_value_at(samples, wavelength_nm)
            .with_context(|| format!("solar runtime grid requires {wavelength_nm} nm anchor"))?;
    }
    Ok(())
}

fn header_value<'a>(text: &'a str, key: &str) -> Result<&'a str> {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("# {key}=")))
        .with_context(|| format!("solar spectrum is missing header {key:?}"))
}

fn parse_runtime(text: &str) -> Result<Vec<Sample>> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split(',').collect();
            if fields.len() != 2 {
                bail!("solar runtime row must contain exactly two fields");
            }
            Ok(Sample {
                wavelength_nm: fields[0].parse()?,
                irradiance_w_m2_nm: fields[1].parse()?,
            })
        })
        .collect()
}

fn validate_samples(samples: &[Sample]) -> Result<()> {
    if samples.len() < 2 {
        bail!("solar spectrum requires at least two samples");
    }
    for (index, sample) in samples.iter().enumerate() {
        if !sample.wavelength_nm.is_finite() || sample.wavelength_nm <= 0.0 {
            bail!("solar wavelength at row {index} must be finite and positive");
        }
        if !sample.irradiance_w_m2_nm.is_finite() || sample.irradiance_w_m2_nm <= 0.0 {
            bail!("solar irradiance at row {index} must be finite and positive");
        }
        if index > 0 && sample.wavelength_nm <= samples[index - 1].wavelength_nm {
            bail!("solar wavelengths must be strictly increasing without duplicates");
        }
    }
    if samples[0].wavelength_nm != BAND_MIN_NM
        || samples.last().unwrap().wavelength_nm != BAND_MAX_NM
    {
        bail!("solar spectrum must cover exactly 300–650 nm");
    }
    Ok(())
}

fn required_source<'a>(config: &'a RunConfig, name: &str) -> Result<&'a SourceConfig> {
    config
        .sources
        .iter()
        .find(|source| source.name == name)
        .with_context(|| format!("required solar source {name:?} is missing"))
}

fn trapezoid_integral(samples: &[Sample]) -> f64 {
    samples
        .windows(2)
        .map(|pair| {
            let width = pair[1].wavelength_nm - pair[0].wavelength_nm;
            width * (pair[0].irradiance_w_m2_nm + pair[1].irradiance_w_m2_nm) / 2.0
        })
        .sum()
}

fn exact_value_at(samples: &[Sample], wavelength_nm: f64) -> Result<f64> {
    let index = samples
        .binary_search_by(|sample| sample.wavelength_nm.total_cmp(&wavelength_nm))
        .map_err(|_| {
            anyhow::anyhow!("spectrum does not contain exact {wavelength_nm} nm sample")
        })?;
    Ok(samples[index].irradiance_w_m2_nm)
}

fn ratio_at(samples: &[Sample], numerator_nm: f64, denominator_nm: f64) -> Result<f64> {
    Ok(exact_value_at(samples, numerator_nm)? / exact_value_at(samples, denominator_nm)?)
}

fn relative_difference(left: f64, right: f64) -> f64 {
    (left - right).abs() / right.abs().max(f64::MIN_POSITIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn model() -> AnalyticModel {
        parse_model(Path::new("data/solar-planck-v1.toml")).unwrap()
    }

    fn runtime(rows: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(
            file,
            "# wavelength_nm,irradiance_W_m2_nm\n# source_product=nsb-planck-solar-reference\n# source_release=NSB analytic solar reference v1\n# source_terms=AGPL-3.0-only\n# source_terms_url=https://github.com/VPRamon/NSB/blob/main/LICENSE\n# input_sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n# generator=nsb-data-tools-{}\n# numeric_method=range-reduced fixed-order Taylor expm1 (24 terms)\n# serialization=canonical scientific notation with 12 digits after decimal\n# units=W m^-2 nm^-1\n# reference_distance=1 AU\n# runtime_grid_method=analytic Planck spectral irradiance at integer-nanometre nodes\n# runtime_sampling_interval_nm=1\n{rows}",
            env!("CARGO_PKG_VERSION")
        )
        .unwrap();
        file
    }

    #[test]
    fn runtime_validation_accepts_generated_product() {
        let rendered = render_runtime(&model(), &"a".repeat(64)).unwrap();
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(rendered.as_bytes()).unwrap();
        validate_artifact(RUNTIME_NAME, file.path()).unwrap();
    }

    #[test]
    fn runtime_validation_rejects_malformed_physics_and_schema() {
        for rows in [
            "301.0,1.0\n650.0,1.0\n",
            "300.0,1.0\n299.0,1.0\n650.0,1.0\n",
            "300.0,1.0\n300.0,2.0\n650.0,1.0\n",
            "300.0,NaN\n650.0,1.0\n",
            "300.0,inf\n650.0,1.0\n",
            "300.0,1.0\n650.0,-1.0\n",
            "300.0,1.0,2.0\n650.0,1.0\n",
        ] {
            let file = runtime(rows);
            assert!(validate_artifact(RUNTIME_NAME, file.path()).is_err());
        }
    }

    #[test]
    fn model_parser_rejects_changed_constants_or_terms() {
        let raw = fs::read_to_string("data/solar-planck-v1.toml").unwrap();
        for changed in [
            raw.replace("5772.0", "5773.0"),
            raw.replace("AGPL-3.0-only", "unresolved"),
            raw.replace("schema_version = 1", "schema_version = 2"),
        ] {
            let mut file = tempfile::NamedTempFile::new().unwrap();
            file.write_all(changed.as_bytes()).unwrap();
            assert!(parse_model(file.path()).is_err());
        }
    }

    #[test]
    fn analytic_product_has_expected_grid_anchors_and_scale() {
        let samples = generate_samples(&model()).unwrap();
        assert_eq!(samples.len(), RUNTIME_SAMPLE_COUNT);
        assert!(relative_difference(trapezoid_integral(&samples), 547.535_433_337_638) < 1e-12);
        assert!(
            relative_difference(exact_value_at(&samples, 500.0).unwrap(), 1.782_718_332_319)
                < 1e-14
        );
        assert!(
            relative_difference(
                ratio_at(&samples, 445.0, 551.0).unwrap(),
                0.983_622_222_728_456_1
            ) < 1e-14
        );
    }

    #[test]
    fn generation_is_byte_deterministic() {
        let model = model();
        assert_eq!(
            render_runtime(&model, &"a".repeat(64)).unwrap(),
            render_runtime(&model, &"a".repeat(64)).unwrap()
        );
    }

    #[test]
    fn canonical_numeric_method_matches_reference_nodes() {
        let samples = generate_samples(&model()).unwrap();
        assert_eq!(
            format!(
                "{:.prec$e}",
                samples[0].irradiance_w_m2_nm,
                prec = OUTPUT_DECIMALS
            ),
            "8.204315015917e-1"
        );
        assert_eq!(
            format!(
                "{:.prec$e}",
                exact_value_at(&samples, 500.0).unwrap(),
                prec = OUTPUT_DECIMALS
            ),
            "1.782718332319e0"
        );
        assert_eq!(
            format!(
                "{:.prec$e}",
                samples.last().unwrap().irradiance_w_m2_nm,
                prec = OUTPUT_DECIMALS
            ),
            "1.539976074351e0"
        );
    }
}
