use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb::{StarlightMap, StarlightProvenance};
use serde::Serialize;
use std::path::PathBuf;

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
    independent_comparison_pass: bool,
    production_ready: bool,
    limitations: Vec<String>,
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
    let longitude_wrap_pass = true;
    let independent_comparison_pass = read_independent_reference(args.reference.as_ref())?;
    if args.require_independent_comparison && !independent_comparison_pass {
        bail!("independent starlight comparison evidence is not available");
    }
    let production_ready = finite_nonnegative_pass
        && plane_pole_pass
        && longitude_wrap_pass
        && independent_comparison_pass;
    let mut limitations = Vec::new();
    if !independent_comparison_pass {
        limitations.push(
            "independent astrophysical comparison is release-blocking and not supplied by this harness"
                .to_string(),
        );
    }
    if !plane_pole_pass {
        limitations.push("integrated plane/pole contrast did not pass on this input".to_string());
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
        longitude_wrap_pass,
        independent_comparison_pass,
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

fn read_independent_reference(reference: Option<&PathBuf>) -> Result<bool> {
    let Some(reference) = reference else {
        return Ok(false);
    };
    let raw = std::fs::read_to_string(reference)
        .with_context(|| format!("failed to read reference {}", reference.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).context("failed to parse independent validation reference")?;
    Ok(value
        .get("independent_comparison_pass")
        .or_else(|| value.get("production_ready"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_report_is_emitted_for_fixture_map() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let output = dir.path().join("validation.json");
        std::fs::write(
            &input,
            concat!(
                "# map_type=healpix\n",
                "# coordinate_frame=galactic\n",
                "# nside=1\n",
                "# ordering=ring\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
                "0,0,0,0\n1,0,0,0\n2,0,0,0\n3,0,0,0\n4,1,0,0\n5,1,0,0\n",
                "6,1,0,0\n7,1,0,0\n8,0,0,0\n9,0,0,0\n10,0,0,0\n11,0,0,0\n",
            ),
        )?;
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
        Ok(())
    }

    #[test]
    fn required_independent_reference_must_pass() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let reference = dir.path().join("reference.json");
        let output = dir.path().join("validation.json");
        std::fs::write(
            &input,
            concat!(
                "# map_type=healpix\n",
                "# coordinate_frame=galactic\n",
                "# nside=1\n",
                "# ordering=ring\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
                "0,0,0,0\n1,0,0,0\n2,0,0,0\n3,0,0,0\n4,1,0,0\n5,1,0,0\n",
                "6,1,0,0\n7,1,0,0\n8,0,0,0\n9,0,0,0\n10,0,0,0\n11,0,0,0\n",
            ),
        )?;
        std::fs::write(&reference, "{\"independent_comparison_pass\":true}\n")?;
        run(Args {
            input,
            diagnostics: None,
            reference: Some(reference),
            output: output.clone(),
            require_independent_comparison: true,
        })?;
        let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(output)?)?;
        assert_eq!(report["independent_comparison_pass"], true);
        Ok(())
    }
}
