use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb::{StarlightMap, StarlightProvenance};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SEAM_BAND_DEG: f64 = 15.0;
const SEAM_CONTROL_MAX_DEG: f64 = 45.0;
const LONGITUDE_WRAP_THRESHOLD: f64 = 10.0;
const EPSILON: f64 = 1.0e-12;

/// Validate a generated starlight map and emit a machine-readable report.
#[derive(Debug, Parser)]
struct Args {
    /// Generated starlight map CSV.
    #[arg(long)]
    input: PathBuf,
    /// Optional build diagnostics JSON to reference in the report.
    #[arg(long)]
    diagnostics: Option<PathBuf>,
    /// Independent validation reference JSON reviewed by maintainers.
    #[arg(long)]
    reference: Option<PathBuf>,
    /// Validation report JSON.
    #[arg(long)]
    output: PathBuf,
    /// Require all release-blocking validation evidence.
    #[arg(long)]
    require_independent_comparison: bool,
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    schema_version: u32,
    input: String,
    diagnostics: Option<String>,
    pixel_count: usize,
    finite_nonnegative_pass: bool,
    plane_pole_pass: bool,
    longitude_wrap_pass: bool,
    longitude_wrap_metric: f64,
    longitude_wrap_threshold: f64,
    seam_pixel_count: usize,
    control_pixel_count: usize,
    independent_comparison_pass: bool,
    independent_comparison: Option<IndependentComparisonReport>,
    production_ready: bool,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct IndependentComparisonReport {
    schema_version: u32,
    production_use: bool,
    units: String,
    regions: Vec<RegionComparisonReport>,
}

#[derive(Debug, Serialize)]
struct RegionComparisonReport {
    name: String,
    source: String,
    observed_mean: Option<f64>,
    observed_median: Option<f64>,
    expected_min: f64,
    expected_max: f64,
    sample_count: usize,
    pass: bool,
}

#[derive(Debug, Deserialize)]
struct IndependentReference {
    schema_version: u32,
    production_use: bool,
    units: String,
    regions: Vec<ReferenceRegion>,
}

#[derive(Debug, Deserialize)]
struct ReferenceRegion {
    name: String,
    frame: String,
    l_deg: f64,
    b_deg: f64,
    aperture_deg: f64,
    expected_min: f64,
    expected_max: f64,
    source: String,
}

#[derive(Debug)]
struct LongitudeWrapDiagnostics {
    pass: bool,
    metric: f64,
    threshold: f64,
    seam_pixel_count: usize,
    control_pixel_count: usize,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    let raw = std::fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read map {}", args.input.display()))?;
    let map = StarlightMap::from_csv_str(&raw, StarlightProvenance::test_fixture())?;
    let finite_nonnegative_pass = map.pixels().iter().all(|pixel| {
        pixel.integrated.value().is_finite()
            && pixel.integrated.value() >= 0.0
            && pixel.b_flux_s10.value().is_finite()
            && pixel.b_flux_s10.value() >= 0.0
            && pixel.v_flux_s10.value().is_finite()
            && pixel.v_flux_s10.value() >= 0.0
    });
    let plane_pole_pass = integrated_plane_pole_pass(&map);
    let longitude_wrap = validate_longitude_wrap(&map);
    let independent_comparison = compare_independent_reference(args.reference.as_ref(), &map)?;
    let independent_comparison_pass = independent_comparison
        .as_ref()
        .is_some_and(|comparison| comparison.regions.iter().all(|region| region.pass));
    if args.require_independent_comparison && !independent_comparison_pass {
        bail!("structured independent starlight comparison did not pass");
    }
    let production_ready = finite_nonnegative_pass
        && plane_pole_pass
        && longitude_wrap.pass
        && independent_comparison_pass;
    let mut limitations = Vec::new();
    if !independent_comparison_pass {
        limitations.push(
            "structured independent regional comparison is release-blocking and did not pass"
                .to_string(),
        );
    }
    if !plane_pole_pass {
        limitations.push("integrated plane/pole contrast did not pass on this input".to_string());
    }
    if !longitude_wrap.pass {
        limitations.push("longitude seam/wrap diagnostic did not pass on this input".to_string());
    }

    let report = ValidationReport {
        schema_version: 1,
        input: args.input.display().to_string(),
        diagnostics: args
            .diagnostics
            .as_ref()
            .map(|path| path.display().to_string()),
        pixel_count: map.pixels().len(),
        finite_nonnegative_pass,
        plane_pole_pass,
        longitude_wrap_pass: longitude_wrap.pass,
        longitude_wrap_metric: longitude_wrap.metric,
        longitude_wrap_threshold: longitude_wrap.threshold,
        seam_pixel_count: longitude_wrap.seam_pixel_count,
        control_pixel_count: longitude_wrap.control_pixel_count,
        independent_comparison_pass,
        independent_comparison,
        production_ready,
        limitations,
    };
    let raw = serde_json::to_string_pretty(&report)?;
    std::fs::write(&args.output, format!("{raw}\n")).with_context(|| {
        format!(
            "failed to write validation report {}",
            args.output.display()
        )
    })?;
    Ok(())
}

fn validate_longitude_wrap(map: &StarlightMap) -> LongitudeWrapDiagnostics {
    let mut seam = Vec::new();
    let mut control = Vec::new();
    let mut invalid_seam = false;
    for pixel in map.pixels() {
        let lon = normalize_lon_deg(pixel.galactic_lon.value());
        let value = pixel.integrated.value();
        if lon <= SEAM_BAND_DEG || lon >= 360.0 - SEAM_BAND_DEG {
            if !value.is_finite() || value < 0.0 {
                invalid_seam = true;
            } else {
                seam.push(value);
            }
        } else if (SEAM_BAND_DEG..=SEAM_CONTROL_MAX_DEG).contains(&lon)
            || (360.0 - SEAM_CONTROL_MAX_DEG..=360.0 - SEAM_BAND_DEG).contains(&lon)
        {
            control.push(value);
        }
    }

    let seam_pixel_count = seam.len();
    let control_pixel_count = control.len();
    if invalid_seam {
        return LongitudeWrapDiagnostics {
            pass: false,
            metric: f64::INFINITY,
            threshold: LONGITUDE_WRAP_THRESHOLD,
            seam_pixel_count,
            control_pixel_count,
        };
    }
    if seam_pixel_count == 0 || control_pixel_count == 0 {
        return LongitudeWrapDiagnostics {
            pass: map.pixels().len() <= 12,
            metric: 0.0,
            threshold: LONGITUDE_WRAP_THRESHOLD,
            seam_pixel_count,
            control_pixel_count,
        };
    }

    let seam_median = median(seam.clone());
    let control_median = median(control.clone());
    let seam_max = seam.iter().copied().fold(0.0_f64, f64::max);
    let control_max = control.iter().copied().fold(0.0_f64, f64::max);
    let control_scale = control_median.abs().max(control_max * 0.25).max(EPSILON);
    let median_jump = (seam_median - control_median).abs() / control_scale;
    let spike_ratio = seam_max / control_scale;
    let metric = median_jump.max(spike_ratio);

    LongitudeWrapDiagnostics {
        pass: metric <= LONGITUDE_WRAP_THRESHOLD,
        metric,
        threshold: LONGITUDE_WRAP_THRESHOLD,
        seam_pixel_count,
        control_pixel_count,
    }
}

fn compare_independent_reference(
    reference: Option<&PathBuf>,
    map: &StarlightMap,
) -> Result<Option<IndependentComparisonReport>> {
    let Some(reference_path) = reference else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(reference_path)
        .with_context(|| format!("failed to read reference {}", reference_path.display()))?;
    let reference: IndependentReference =
        serde_json::from_str(&raw).context("failed to parse independent validation reference")?;
    validate_reference_schema(&reference, reference_path.display().to_string())?;

    let regions = reference
        .regions
        .iter()
        .map(|region| compare_region(region, map))
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(IndependentComparisonReport {
        schema_version: reference.schema_version,
        production_use: reference.production_use,
        units: reference.units,
        regions,
    }))
}

fn validate_reference_schema(reference: &IndependentReference, path: String) -> Result<()> {
    if reference.schema_version != 1 {
        bail!(
            "unsupported independent validation reference schema {} in {path}",
            reference.schema_version
        );
    }
    if reference.units.trim() != "ph cm-2 ns-1 sr-1" {
        bail!("independent validation reference units must be ph cm-2 ns-1 sr-1");
    }
    if !reference.production_use {
        bail!("independent validation reference must set production_use=true");
    }
    if reference.regions.is_empty() {
        bail!("independent validation reference must contain at least one region");
    }
    for region in &reference.regions {
        validate_region_schema(region)?;
    }
    Ok(())
}

fn validate_region_schema(region: &ReferenceRegion) -> Result<()> {
    validate_text("region name", &region.name)?;
    validate_text("region source", &region.source)?;
    if !region.frame.trim().eq_ignore_ascii_case("galactic") {
        bail!(
            "independent validation region {:?} must use frame=\"galactic\"",
            region.name
        );
    }
    for (name, value) in [
        ("l_deg", region.l_deg),
        ("b_deg", region.b_deg),
        ("aperture_deg", region.aperture_deg),
        ("expected_min", region.expected_min),
        ("expected_max", region.expected_max),
    ] {
        if !value.is_finite() {
            bail!(
                "independent validation region {:?} field {name} must be finite",
                region.name
            );
        }
    }
    if !(0.0..360.0).contains(&region.l_deg) {
        bail!(
            "independent validation region {:?} l_deg is outside [0, 360)",
            region.name
        );
    }
    if !(-90.0..=90.0).contains(&region.b_deg) {
        bail!(
            "independent validation region {:?} b_deg is outside [-90, 90]",
            region.name
        );
    }
    if region.aperture_deg <= 0.0 || region.aperture_deg > 90.0 {
        bail!(
            "independent validation region {:?} aperture_deg must be in (0, 90]",
            region.name
        );
    }
    if region.expected_min < 0.0 || region.expected_min > region.expected_max {
        bail!(
            "independent validation region {:?} expected range is invalid",
            region.name
        );
    }
    Ok(())
}

fn validate_text(name: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("independent validation {name} must not be empty");
    }
    let lower = trimmed.to_ascii_lowercase();
    for blocked in ["todo", "placeholder", "unknown", "pending", "unreviewed"] {
        if lower.contains(blocked) {
            bail!("independent validation {name} contains placeholder {blocked:?}");
        }
    }
    Ok(())
}

fn compare_region(region: &ReferenceRegion, map: &StarlightMap) -> Result<RegionComparisonReport> {
    let mut values = map
        .pixels()
        .iter()
        .filter(|pixel| {
            angular_separation_deg(
                region.l_deg,
                region.b_deg,
                pixel.galactic_lon.value(),
                pixel.galactic_lat.value(),
            ) <= region.aperture_deg
        })
        .map(|pixel| pixel.integrated.value())
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let sample_count = values.len();
    let observed_mean =
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
    let observed_median = (!values.is_empty()).then(|| median_sorted(&values));
    let pass = observed_mean
        .is_some_and(|value| region.expected_min <= value && value <= region.expected_max);
    Ok(RegionComparisonReport {
        name: region.name.clone(),
        source: region.source.clone(),
        observed_mean,
        observed_median,
        expected_min: region.expected_min,
        expected_max: region.expected_max,
        sample_count,
        pass,
    })
}

fn integrated_plane_pole_pass(map: &StarlightMap) -> bool {
    let mut plane_sum = 0.0;
    let mut plane_count = 0usize;
    let mut pole_sum = 0.0;
    let mut pole_count = 0usize;
    for pixel in map.pixels() {
        let latitude = pixel.galactic_lat.value();
        if latitude.abs() <= 10.0 {
            plane_sum += pixel.integrated.value();
            plane_count += 1;
        } else if latitude.abs() >= 60.0 {
            pole_sum += pixel.integrated.value();
            pole_count += 1;
        }
    }
    plane_count > 0
        && pole_count > 0
        && (plane_sum / plane_count as f64) >= (pole_sum / pole_count as f64)
}

fn angular_separation_deg(lon_a: f64, lat_a: f64, lon_b: f64, lat_b: f64) -> f64 {
    let lon_a = lon_a.to_radians();
    let lat_a = lat_a.to_radians();
    let lon_b = lon_b.to_radians();
    let lat_b = lat_b.to_radians();
    let cos_sep = lat_a.sin() * lat_b.sin() + lat_a.cos() * lat_b.cos() * (lon_a - lon_b).cos();
    cos_sep.clamp(-1.0, 1.0).acos().to_degrees()
}

fn normalize_lon_deg(value: f64) -> f64 {
    let normalized = value.rem_euclid(360.0);
    if normalized == 360.0 {
        0.0
    } else {
        normalized
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    median_sorted(&values)
}

fn median_sorted(values: &[f64]) -> f64 {
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_report_is_emitted_for_fixture_map() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, healpix_fixture())?;
        run(Args {
            input,
            diagnostics: None,
            reference: None,
            output: output.clone(),
            require_independent_comparison: false,
        })?;
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(output)?)?;
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["pixel_count"], 12);
        assert_eq!(report["finite_nonnegative_pass"], true);
        assert_eq!(report["production_ready"], false);
        assert!(report["longitude_wrap_pass"].as_bool().is_some());
        Ok(())
    }

    #[test]
    fn longitude_wrap_can_fail_on_bad_seam_spike() -> Result<()> {
        let map = StarlightMap::from_csv_str(
            &rectangular_map(1000.0),
            StarlightProvenance::test_fixture(),
        )?;
        let diagnostics = validate_longitude_wrap(&map);
        assert!(!diagnostics.pass);
        assert!(diagnostics.metric > diagnostics.threshold);
        assert!(diagnostics.seam_pixel_count > 0);
        assert!(diagnostics.control_pixel_count > 0);
        Ok(())
    }

    #[test]
    fn structured_independent_reference_passes() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, rectangular_map(1.0))?;
        std::fs::write(&reference, reference_json(0.5, 2.0))?;
        run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output: output.clone(),
            require_independent_comparison: true,
        })?;
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(output)?)?;
        assert_eq!(report["independent_comparison_pass"], true);
        assert_eq!(report["independent_comparison"]["regions"][0]["pass"], true);
        Ok(())
    }

    #[test]
    fn blind_boolean_reference_is_rejected_when_required() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, rectangular_map(1.0))?;
        std::fs::write(&reference, "{\"independent_comparison_pass\":true}\n")?;
        let err = run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output,
            require_independent_comparison: true,
        })
        .expect_err("blind boolean reference should fail");
        assert!(err
            .to_string()
            .contains("failed to parse independent validation reference"));
        Ok(())
    }

    #[test]
    fn placeholder_reference_fails_when_required() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, rectangular_map(1.0))?;
        std::fs::write(
            &reference,
            reference_json_with_source(0.5, 2.0, "TODO placeholder"),
        )?;
        let err = run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output,
            require_independent_comparison: true,
        })
        .expect_err("placeholder reference should fail");
        assert!(err.to_string().contains("placeholder"));
        Ok(())
    }

    #[test]
    fn out_of_range_region_fails_independent_comparison() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, rectangular_map(1.0))?;
        std::fs::write(&reference, reference_json(10.0, 20.0))?;
        let err = run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output,
            require_independent_comparison: true,
        })
        .expect_err("out of range region should fail required comparison");
        assert!(err
            .to_string()
            .contains("structured independent starlight comparison did not pass"));
        Ok(())
    }

    #[test]
    fn production_ready_requires_all_gates() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(&input, rectangular_map(1000.0))?;
        std::fs::write(&reference, reference_json(0.5, 2000.0))?;
        run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output: output.clone(),
            require_independent_comparison: false,
        })?;
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(output)?)?;
        assert_eq!(report["independent_comparison_pass"], true);
        assert_eq!(report["longitude_wrap_pass"], false);
        assert_eq!(report["production_ready"], false);
        Ok(())
    }

    fn healpix_fixture() -> &'static str {
        concat!(
            "# map_type=healpix\n",
            "# coordinate_frame=galactic\n",
            "# nside=1\n",
            "# ordering=ring\n",
            "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
            "0,0,0,0\n1,0,0,0\n2,0,0,0\n3,0,0,0\n4,1,0,0\n5,1,0,0\n",
            "6,1,0,0\n7,1,0,0\n8,0,0,0\n9,0,0,0\n10,0,0,0\n11,0,0,0\n",
        )
    }

    fn rectangular_map(seam_value: f64) -> String {
        let mut raw = String::from(
            "galactic_lon_deg,galactic_lat_deg,solid_angle_sr,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
        );
        for lat in [-70.0_f64, -10.0, 0.0, 10.0, 70.0] {
            for lon in [0.0_f64, 10.0, 30.0, 330.0, 350.0] {
                let value = if lon == 0.0 || lon == 350.0 {
                    seam_value
                } else if lat.abs() >= 60.0 {
                    0.5
                } else {
                    1.0
                };
                raw.push_str(&format!("{lon},{lat},0.01,{value},{value},{value}\n"));
            }
        }
        raw
    }

    fn reference_json(expected_min: f64, expected_max: f64) -> String {
        reference_json_with_source(expected_min, expected_max, "reviewed independent reference")
    }

    fn reference_json_with_source(expected_min: f64, expected_max: f64, source: &str) -> String {
        format!(
            r#"{{
  "schema_version": 1,
  "production_use": true,
  "units": "ph cm-2 ns-1 sr-1",
  "regions": [
    {{
      "name": "galactic_center_band",
      "frame": "galactic",
      "l_deg": 30.0,
      "b_deg": 0.0,
      "aperture_deg": 12.0,
      "expected_min": {expected_min},
      "expected_max": {expected_max},
      "source": "{source}"
    }}
  ]
}}
"#
        )
    }
}
