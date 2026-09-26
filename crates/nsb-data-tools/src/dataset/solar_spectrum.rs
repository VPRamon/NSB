//! TSIS-1 HSRS v2 acquisition contract, deterministic runtime transform, and validation.

use super::{Artifact, RunConfig, SourceConfig, ValidationGate};
use anyhow::{bail, Context, Result};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const RUNTIME_NAME: &str = "solar_spectrum.dat";
pub(super) const SOURCE_NAME: &str = "tsis1_hsrs_p025nm_300_650.csv";
const HIGH_RESOLUTION_NAME: &str = "tsis1_hsrs_native_300_650.csv";
const PRODUCT_ID: &str = "tsis1_hsrs_p025nm";
const RELEASE: &str = "TSIS-1 HSRS Version 2";
const UNITS: &str = "W m^-2 nm^-1";
const REFERENCE_DISTANCE: &str = "1 AU";
const BAND_MIN_NM: f64 = 300.0;
const BAND_MAX_NM: f64 = 650.0;
const RUNTIME_STEP_NM: f64 = 1.0;
const RUNTIME_SAMPLE_COUNT: usize = 351;
const REQUIRED_ANCHORS_NM: [f64; 3] = [445.0, 500.0, 551.0];

#[derive(Clone, Copy, Debug)]
struct Sample {
    wavelength_nm: f64,
    irradiance_w_m2_nm: f64,
}

pub(super) fn output_name(source_name: &str) -> Result<&str> {
    if source_name == SOURCE_NAME {
        Ok(RUNTIME_NAME)
    } else {
        bail!("source {source_name:?} is validation-only and has no runtime output")
    }
}

pub(super) fn validate_config(config: &RunConfig) -> Result<()> {
    if config.sources.len() != 2 {
        bail!("solar-spectrum requires the pinned p025nm and native-resolution HSRS sources");
    }
    let source = required_source(config, SOURCE_NAME)?;
    require_provenance(source, PRODUCT_ID)?;
    let high_resolution = required_source(config, HIGH_RESOLUTION_NAME)?;
    require_provenance(high_resolution, "tsis1_hsrs")?;
    Ok(())
}

fn require_provenance(source: &SourceConfig, product_id: &str) -> Result<()> {
    let required = [
        ("product_id", source.product_id.as_deref(), product_id),
        ("release", source.release.as_deref(), RELEASE),
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
    for (field, value) in [
        ("metadata_url", source.metadata_url.as_deref()),
        ("retrieved_at", source.retrieved_at.as_deref()),
        ("license", source.license.as_deref()),
    ] {
        if value.is_none_or(|value| value.trim().is_empty()) {
            bail!("source {:?} requires non-empty {field}", source.name);
        }
    }
    Ok(())
}

pub(super) fn transform(source_name: &str, input: &Path, output: &Path) -> Result<()> {
    if source_name != SOURCE_NAME {
        bail!("cannot build runtime solar spectrum from {source_name:?}");
    }
    let samples = parse_candidate_source(input)?;
    let bytes = render_runtime(&samples)?;
    crate::dataset::engine::atomic_write(output, bytes.as_bytes())
}

pub(super) fn validate_artifact(name: &str, path: &Path) -> Result<()> {
    if name != RUNTIME_NAME {
        bail!("unexpected solar-spectrum artifact {name:?}");
    }
    let text = fs::read_to_string(path)?;
    for required in [
        "# wavelength_nm,irradiance_W_m2_nm\n",
        "# source_product=tsis1_hsrs_p025nm\n",
        "# source_release=TSIS-1 HSRS Version 2\n",
        "# units=W m^-2 nm^-1\n",
        "# reference_distance=1 AU\n",
        "# runtime_grid_method=flux-conserving 1 nm cell means with exact 445/500/551 nm anchors\n",
        "# runtime_sampling_interval_nm=1\n",
    ] {
        if !text.contains(required) {
            bail!("solar spectrum is missing required header {required:?}");
        }
    }
    let samples = parse_runtime(&text)?;
    validate_samples(&samples, true)?;
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
    let selected_source =
        parse_candidate_source(&config.workspace.root.join("sources").join(SOURCE_NAME))?;
    let regenerated = render_runtime(&selected_source)?;
    let deterministic = regenerated.as_bytes() == runtime_text.as_bytes();

    let native = parse_native_source(
        &config
            .workspace
            .root
            .join("sources")
            .join(HIGH_RESOLUTION_NAME),
    )?;
    validate_samples(&native, true)?;
    let runtime_integral = trapezoid_integral(&runtime);
    let selected_integral = trapezoid_integral(&selected_source);
    let native_integral = trapezoid_integral(&native);
    let selected_native_integral_difference =
        relative_difference(selected_integral, native_integral);
    let runtime_integral_difference = relative_difference(runtime_integral, selected_integral);
    let bv_runtime = ratio_at(&runtime, 445.0, 551.0)?;
    let bv_selected = ratio_at(&selected_source, 445.0, 551.0)?;
    let bv_native = ratio_at(&native, 445.0, 551.0)?;
    let selected_native_bv_difference = relative_difference(bv_selected, bv_native);
    let runtime_bv_difference = relative_difference(bv_runtime, bv_selected);
    let anchors_preserved = REQUIRED_ANCHORS_NM.iter().all(|wavelength| {
        let Ok(runtime_value) = exact_value_at(&runtime, *wavelength) else {
            return false;
        };
        let Ok(selected_value) = exact_value_at(&selected_source, *wavelength) else {
            return false;
        };
        relative_difference(runtime_value, selected_value) <= 1.0e-14
    });

    Ok(vec![
        ValidationGate {
            name: "deterministic-regeneration".into(),
            passed: deterministic,
            detail: format!("regenerated bytes equal {}", artifact.sha256),
        },
        ValidationGate {
            name: "source-provenance".into(),
            passed: true,
            detail: format!(
                "product={PRODUCT_ID}; release={RELEASE}; units={UNITS}; reference_distance={REFERENCE_DISTANCE}"
            ),
        },
        ValidationGate {
            name: "runtime-grid-complexity".into(),
            passed: runtime.len() <= RUNTIME_SAMPLE_COUNT,
            detail: format!(
                "runtime_samples={}; maximum={RUNTIME_SAMPLE_COUNT}; upstream_samples={}",
                runtime.len(),
                selected_source.len()
            ),
        },
        ValidationGate {
            name: "runtime-required-anchors".into(),
            passed: anchors_preserved,
            detail: "445, 500, and 551 nm are present exactly with p025nm irradiances".into(),
        },
        ValidationGate {
            name: "runtime-p025nm-integral-comparison".into(),
            passed: runtime_integral_difference <= 1.0e-12,
            detail: format!(
                "runtime={runtime_integral:.12} W m^-2; p025nm={selected_integral:.12} W m^-2; relative_difference={runtime_integral_difference:.12e}"
            ),
        },
        ValidationGate {
            name: "runtime-p025nm-b-v-shape-comparison".into(),
            passed: runtime_bv_difference <= 1.0e-12,
            detail: format!(
                "runtime_445_over_551={bv_runtime:.12}; p025nm_445_over_551={bv_selected:.12}; relative_difference={runtime_bv_difference:.12e}"
            ),
        },
        ValidationGate {
            name: "p025nm-native-integral-comparison".into(),
            passed: selected_native_integral_difference <= 5.0e-4,
            detail: format!(
                "p025nm={selected_integral:.12} W m^-2; native={native_integral:.12} W m^-2; relative_difference={selected_native_integral_difference:.12e}"
            ),
        },
        ValidationGate {
            name: "p025nm-native-b-v-shape-comparison".into(),
            passed: selected_native_bv_difference <= 1.0e-2,
            detail: format!(
                "p025nm_445_over_551={bv_selected:.12}; native_445_over_551={bv_native:.12}; relative_difference={selected_native_bv_difference:.12e}"
            ),
        },
    ])
}

fn render_runtime(samples: &[Sample]) -> Result<String> {
    let mut bytes = String::from(
        "# wavelength_nm,irradiance_W_m2_nm\n\
# source_product=tsis1_hsrs_p025nm\n\
# source_release=TSIS-1 HSRS Version 2\n\
# source_doi=https://doi.org/10.25980/ta3f-7h90\n\
# units=W m^-2 nm^-1\n\
# upstream_spectral_resolution_nm=0.025\n\
# upstream_sampling_interval_nm=0.005\n\
# reference_distance=1 AU\n\
# runtime_grid_method=flux-conserving 1 nm cell means with exact 445/500/551 nm anchors\n\
# runtime_sampling_interval_nm=1\n",
    );
    let selected: Vec<_> = samples
        .iter()
        .copied()
        .filter(|sample| (BAND_MIN_NM..=BAND_MAX_NM).contains(&sample.wavelength_nm))
        .collect();
    validate_samples(&selected, true)?;
    let runtime = reduce_to_runtime(&selected)?;
    for sample in runtime {
        writeln!(
            bytes,
            "{:.3},{:.17e}",
            sample.wavelength_nm, sample.irradiance_w_m2_nm
        )?;
    }
    Ok(bytes)
}

/// Reduce the official high-resolution source to the grid the runtime models need.
///
/// Each integer-nanometre node initially stores the mean irradiance in its
/// one-nanometre Voronoi cell (half-width cells at the band edges). With the
/// trapezoidal integration used by NSB, those means preserve the source's
/// 300–650 nm integral. The three model diagnostics then replace their cell
/// means with the exact p025nm values; equal compensating corrections at the
/// adjacent nodes preserve the integral without adding hot-path samples.
fn reduce_to_runtime(samples: &[Sample]) -> Result<Vec<Sample>> {
    validate_samples(samples, true)?;
    let mut runtime = Vec::with_capacity(RUNTIME_SAMPLE_COUNT);
    for index in 0..RUNTIME_SAMPLE_COUNT {
        let wavelength_nm = BAND_MIN_NM + index as f64 * RUNTIME_STEP_NM;
        let left = (wavelength_nm - RUNTIME_STEP_NM / 2.0).max(BAND_MIN_NM);
        let right = (wavelength_nm + RUNTIME_STEP_NM / 2.0).min(BAND_MAX_NM);
        runtime.push(Sample {
            wavelength_nm,
            irradiance_w_m2_nm: integrate_range(samples, left, right)? / (right - left),
        });
    }

    for wavelength_nm in REQUIRED_ANCHORS_NM {
        let index = runtime
            .binary_search_by(|sample| sample.wavelength_nm.total_cmp(&wavelength_nm))
            .map_err(|_| anyhow::anyhow!("runtime anchor {wavelength_nm} nm is missing"))?;
        let exact = exact_value_at(samples, wavelength_nm)?;
        let correction = exact - runtime[index].irradiance_w_m2_nm;
        runtime[index].irradiance_w_m2_nm = exact;
        runtime[index - 1].irradiance_w_m2_nm -= correction / 2.0;
        runtime[index + 1].irradiance_w_m2_nm -= correction / 2.0;
    }

    validate_samples(&runtime, true)?;
    validate_runtime_representation(&runtime)?;
    Ok(runtime)
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

fn parse_candidate_source(path: &Path) -> Result<Vec<Sample>> {
    let mut reader = csv::ReaderBuilder::new().from_path(path)?;
    let expected = ["wavelength (nm)", "irradiance (W/m^2/nm)"];
    if reader.headers()?.iter().ne(expected) {
        bail!("unexpected TSIS-1 HSRS p025nm source schema");
    }
    let mut samples = Vec::new();
    for row in reader.records() {
        let row = row?;
        if row.len() != 2 {
            bail!("TSIS-1 HSRS p025nm row must contain two fields");
        }
        samples.push(Sample {
            wavelength_nm: row[0].parse()?,
            irradiance_w_m2_nm: row[1].parse()?,
        });
    }
    validate_samples(&samples, false)?;
    Ok(samples)
}

fn parse_native_source(path: &Path) -> Result<Vec<Sample>> {
    let mut reader = csv::ReaderBuilder::new().from_path(path)?;
    let expected = ["wavelength (nm)", "irradiance (W/m^2/nm)"];
    if reader.headers()?.iter().ne(expected) {
        bail!("unexpected native-resolution TSIS-1 HSRS source schema");
    }
    let mut samples = Vec::new();
    for row in reader.records() {
        let row = row?;
        if row.len() != 2 {
            bail!("native-resolution TSIS-1 HSRS row must contain two fields");
        }
        samples.push(Sample {
            wavelength_nm: row[0].parse()?,
            irradiance_w_m2_nm: row[1].parse()?,
        });
    }
    Ok(samples)
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

fn validate_samples(samples: &[Sample], require_runtime_band: bool) -> Result<()> {
    if samples.len() < 2 {
        bail!("solar spectrum requires at least two samples");
    }
    for (index, sample) in samples.iter().enumerate() {
        if !sample.wavelength_nm.is_finite() || sample.wavelength_nm <= 0.0 {
            bail!("solar wavelength at row {index} must be finite and positive");
        }
        if !sample.irradiance_w_m2_nm.is_finite() || sample.irradiance_w_m2_nm < 0.0 {
            bail!("solar irradiance at row {index} must be finite and non-negative");
        }
        if index > 0 && sample.wavelength_nm <= samples[index - 1].wavelength_nm {
            bail!("solar wavelengths must be strictly increasing without duplicates");
        }
    }
    if require_runtime_band
        && (samples[0].wavelength_nm > BAND_MIN_NM
            || samples.last().unwrap().wavelength_nm < BAND_MAX_NM)
    {
        bail!("solar spectrum must cover 300–650 nm");
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

fn integrate_range(samples: &[Sample], start_nm: f64, end_nm: f64) -> Result<f64> {
    if start_nm >= end_nm {
        bail!("solar integration range must have positive width");
    }
    let mut previous = Sample {
        wavelength_nm: start_nm,
        irradiance_w_m2_nm: interpolate(samples, start_nm)?,
    };
    let mut integral = 0.0;
    for sample in samples
        .iter()
        .copied()
        .filter(|sample| sample.wavelength_nm > start_nm && sample.wavelength_nm < end_nm)
    {
        integral += (sample.wavelength_nm - previous.wavelength_nm)
            * (sample.irradiance_w_m2_nm + previous.irradiance_w_m2_nm)
            / 2.0;
        previous = sample;
    }
    let end = Sample {
        wavelength_nm: end_nm,
        irradiance_w_m2_nm: interpolate(samples, end_nm)?,
    };
    integral += (end.wavelength_nm - previous.wavelength_nm)
        * (end.irradiance_w_m2_nm + previous.irradiance_w_m2_nm)
        / 2.0;
    Ok(integral)
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
    Ok(interpolate(samples, numerator_nm)? / interpolate(samples, denominator_nm)?)
}

fn interpolate(samples: &[Sample], wavelength_nm: f64) -> Result<f64> {
    let upper = samples.partition_point(|sample| sample.wavelength_nm < wavelength_nm);
    if upper < samples.len() && samples[upper].wavelength_nm == wavelength_nm {
        return Ok(samples[upper].irradiance_w_m2_nm);
    }
    if upper == 0 || upper == samples.len() {
        bail!("wavelength {wavelength_nm} nm is outside the spectrum");
    }
    let low = samples[upper - 1];
    let high = samples[upper];
    let fraction = (wavelength_nm - low.wavelength_nm) / (high.wavelength_nm - low.wavelength_nm);
    Ok(low.irradiance_w_m2_nm + fraction * (high.irradiance_w_m2_nm - low.irradiance_w_m2_nm))
}

fn relative_difference(left: f64, right: f64) -> f64 {
    (left - right).abs() / right.abs().max(f64::MIN_POSITIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn runtime(rows: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(
            file,
            "# wavelength_nm,irradiance_W_m2_nm\n# source_product=tsis1_hsrs_p025nm\n# source_release=TSIS-1 HSRS Version 2\n# units=W m^-2 nm^-1\n# reference_distance=1 AU\n# runtime_grid_method=flux-conserving 1 nm cell means with exact 445/500/551 nm anchors\n# runtime_sampling_interval_nm=1\n{rows}"
        )
        .unwrap();
        file
    }

    #[test]
    fn runtime_validation_accepts_physical_coverage() {
        let rows = (0..RUNTIME_SAMPLE_COUNT)
            .map(|index| format!("{:.3},1.0\n", BAND_MIN_NM + index as f64 * RUNTIME_STEP_NM))
            .collect::<String>();
        let file = runtime(&rows);
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
            assert!(
                validate_artifact(RUNTIME_NAME, file.path()).is_err(),
                "accepted {rows:?}"
            );
        }
    }

    #[test]
    fn candidate_parser_rejects_unexpected_schema() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        writeln!(file, "wavelength,flux\n300,1").unwrap();
        assert!(parse_candidate_source(file.path()).is_err());
    }

    #[test]
    fn candidate_parser_rejects_malformed_physical_values() {
        for rows in [
            "NaN,1.0\n650.0,1.0\n",
            "0.0,1.0\n650.0,1.0\n",
            "300.0,1.0\n299.0,1.0\n",
            "300.0,1.0\n300.0,1.1\n",
            "300.0,-1.0\n650.0,1.0\n",
            "300.0,NaN\n650.0,1.0\n",
        ] {
            let mut file = tempfile::NamedTempFile::new().unwrap();
            write!(
                file,
                "wavelength (nm),irradiance (W/m^2/nm)\n{rows}"
            )
            .unwrap();
            assert!(
                parse_candidate_source(file.path()).is_err(),
                "accepted {rows:?}"
            );
        }
    }

    #[test]
    fn reduction_rejects_incomplete_band_coverage() {
        let samples = [
            Sample {
                wavelength_nm: 301.0,
                irradiance_w_m2_nm: 1.0,
            },
            Sample {
                wavelength_nm: 650.0,
                irradiance_w_m2_nm: 1.0,
            },
        ];
        assert!(reduce_to_runtime(&samples).is_err());
    }

    #[test]
    fn runtime_sample_count_guard_rejects_a_short_grid() {
        let rows = (0..RUNTIME_SAMPLE_COUNT - 1)
            .map(|index| {
                format!(
                    "{:.3},1.0\n",
                    BAND_MIN_NM + index as f64 * RUNTIME_STEP_NM
                )
            })
            .collect::<String>();
        let file = runtime(&rows);
        assert!(validate_artifact(RUNTIME_NAME, file.path()).is_err());
    }

    #[test]
    fn reduction_preserves_integral_and_required_anchors() {
        let samples = (3000..=6500)
            .map(|index| {
                let wavelength_nm = f64::from(index) / 10.0;
                Sample {
                    wavelength_nm,
                    irradiance_w_m2_nm: 1.0
                        + wavelength_nm / 1000.0
                        + (wavelength_nm * 0.7).sin().abs(),
                }
            })
            .collect::<Vec<_>>();
        let runtime = reduce_to_runtime(&samples).unwrap();
        assert_eq!(runtime.len(), RUNTIME_SAMPLE_COUNT);
        assert!(
            relative_difference(trapezoid_integral(&runtime), trapezoid_integral(&samples))
                < 1.0e-13
        );
        for wavelength_nm in REQUIRED_ANCHORS_NM {
            assert_eq!(
                exact_value_at(&runtime, wavelength_nm).unwrap(),
                exact_value_at(&samples, wavelength_nm).unwrap()
            );
        }
    }

    #[test]
    fn reduction_is_byte_deterministic() {
        let samples = (3000..=6500)
            .map(|index| Sample {
                wavelength_nm: f64::from(index) / 10.0,
                irradiance_w_m2_nm: 1.0 + f64::from(index % 17) / 100.0,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            render_runtime(&samples).unwrap(),
            render_runtime(&samples).unwrap()
        );
    }
}
