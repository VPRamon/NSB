use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Parser};
use nsb::ValidatedStarlightMap;
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Pack a validated starlight map into a checksummed release asset.
#[derive(Debug, Parser)]
#[command(group(
    ArgGroup::new("mode")
        .required(true)
        .args(["candidate", "production"])
))]
struct Args {
    /// Generated starlight map CSV.
    #[arg(long)]
    input: PathBuf,
    /// Build diagnostics JSON.
    #[arg(long)]
    diagnostics: PathBuf,
    /// Validation report JSON.
    #[arg(long)]
    validation: PathBuf,
    /// Release CSV path. The output is a raw UTF-8 HEALPix CSV map.
    #[arg(long)]
    output: PathBuf,
    /// Output runtime TOML manifest path.
    #[arg(long)]
    manifest: PathBuf,
    /// Pack a non-production release candidate artifact.
    #[arg(long)]
    candidate: bool,
    /// Pack a production artifact. Requires complete production validation evidence.
    #[arg(long)]
    production: bool,
}

#[derive(Debug, Deserialize)]
struct ValidationSummary {
    production_ready: bool,
    #[serde(default)]
    independent_comparison_pass: bool,
    #[serde(default)]
    flux_conservation_pass: Option<bool>,
    #[serde(default)]
    radiance_field: Option<String>,
    #[serde(default)]
    finite_nonnegative_pass: bool,
    #[serde(default)]
    plane_pole_pass: bool,
    #[serde(default)]
    longitude_wrap_pass: bool,
}

#[derive(Debug, Deserialize)]
struct DiagnosticsSummary {
    #[serde(default)]
    output_sha256: String,
    #[serde(default)]
    photometry_model: String,
    input_integrated_flux_sum_ph_cm2_ns: Option<f64>,
    #[serde(default)]
    integrated_flux_conservation_pass: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RuntimeManifest {
    schema_version: u32,
    calibration_status: String,
    dataset_name: String,
    version: String,
    generation_date: String,
    source_catalogue: String,
    source_catalogue_release: String,
    source_catalogue_license: String,
    source_catalogue_checksum: String,
    source_selection: String,
    magnitude_limit: String,
    map_resolution: String,
    photometry_model: String,
    band_definition: String,
    smoothing: String,
    generated_by: String,
    generation_command: String,
    map_sha256: String,
    validation_report: String,
    independent_comparison: String,
    flux_conservation_validated: bool,
    input_integrated_flux_sum: Option<f64>,
    integrated_flux_conservation_tolerance: Option<f64>,
    header: BTreeMap<String, String>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    let map = std::fs::read(&args.input)
        .with_context(|| format!("failed to read map {}", args.input.display()))?;
    let diagnostics = std::fs::read(&args.diagnostics)
        .with_context(|| format!("failed to read diagnostics {}", args.diagnostics.display()))?;
    let validation = std::fs::read(&args.validation)
        .with_context(|| format!("failed to read validation {}", args.validation.display()))?;
    let validation_summary: ValidationSummary =
        serde_json::from_slice(&validation).context("failed to parse validation report")?;
    let diagnostics_summary: DiagnosticsSummary =
        serde_json::from_slice(&diagnostics).context("failed to parse diagnostics report")?;
    if args.candidate && !validation_summary.production_ready {
        eprintln!("warning: packing a non-production-ready starlight asset candidate");
    }

    let raw_map = std::str::from_utf8(&map).context("map must be UTF-8 CSV")?;
    let mut header = parse_header_metadata(raw_map);
    if args.production {
        enforce_production_gates(&validation_summary, &diagnostics_summary, &header)?;
    } else if header
        .get("photometry_model")
        .is_some_and(|model| is_proxy_or_experimental(model))
    {
        eprintln!("warning: packing candidate with proxy or experimental photometry model");
    }

    let mode = if args.production {
        "production"
    } else {
        "candidate"
    };
    let validation_sha256 = format!("sha256:{}", to_hex(&sha256(&validation)));
    complete_runtime_header(
        &mut header,
        mode,
        &args,
        &validation_sha256,
        &validation_summary,
    )?;
    let release_csv = rewrite_csv_with_header(raw_map, &header)?;
    std::fs::write(&args.output, release_csv.as_bytes())
        .with_context(|| format!("failed to write release CSV {}", args.output.display()))?;

    let map_sha256 = format!("sha256:{}", to_hex(&sha256(release_csv.as_bytes())));
    let manifest = RuntimeManifest {
        schema_version: 1,
        calibration_status: required_header(&header, "calibration_status")?.to_string(),
        dataset_name: required_header(&header, "dataset_name")?.to_string(),
        version: required_header(&header, "version")?.to_string(),
        generation_date: required_header(&header, "generation_date_utc")?.to_string(),
        source_catalogue: required_header(&header, "source_catalogue")?.to_string(),
        source_catalogue_release: required_header(&header, "source_catalogue_release")?.to_string(),
        source_catalogue_license: required_header(&header, "source_catalogue_license")?.to_string(),
        source_catalogue_checksum: required_header(&header, "source_catalogue_checksum")?
            .to_string(),
        source_selection: required_header(&header, "source_selection")?.to_string(),
        magnitude_limit: required_header(&header, "magnitude_limit")?.to_string(),
        map_resolution: required_header(&header, "map_resolution")?.to_string(),
        photometry_model: required_header(&header, "photometry_model")?.to_string(),
        band_definition: required_header(&header, "band_definition")?.to_string(),
        smoothing: required_header(&header, "smoothing")?.to_string(),
        generated_by: required_header(&header, "generated_by")?.to_string(),
        generation_command: required_header(&header, "generation_command")?.to_string(),
        map_sha256,
        validation_report: required_header(&header, "validation_report")?.to_string(),
        independent_comparison: required_header(&header, "independent_comparison")?.to_string(),
        flux_conservation_validated: validation_summary.flux_conservation_pass.unwrap_or(false)
            && diagnostics_summary
                .integrated_flux_conservation_pass
                .unwrap_or(true),
        input_integrated_flux_sum: diagnostics_summary.input_integrated_flux_sum_ph_cm2_ns,
        integrated_flux_conservation_tolerance: diagnostics_summary
            .input_integrated_flux_sum_ph_cm2_ns
            .map(|_| 1.0e-9),
        header: header.clone(),
    };
    let raw = toml::to_string_pretty(&manifest)?;
    std::fs::write(&args.manifest, raw)
        .with_context(|| format!("failed to write manifest {}", args.manifest.display()))?;
    if args.production {
        ValidatedStarlightMap::from_files(&args.output, &args.manifest)
            .context("packer output failed runtime validated-starlight self-load")?;
    }
    Ok(())
}

fn enforce_production_gates(
    validation: &ValidationSummary,
    diagnostics: &DiagnosticsSummary,
    header: &BTreeMap<String, String>,
) -> Result<()> {
    if !validation.production_ready {
        bail!("--production requires validation production_ready=true");
    }
    if !validation.independent_comparison_pass {
        bail!("--production requires independent_comparison_pass=true");
    }
    if validation.radiance_field.as_deref() != Some("integrated_ph_cm2_ns_sr") {
        bail!("--production requires integrated radiance validation");
    }
    if validation.flux_conservation_pass != Some(true) {
        bail!("--production requires integrated flux_conservation_pass=true");
    }
    if diagnostics.integrated_flux_conservation_pass == Some(false)
        || diagnostics.input_integrated_flux_sum_ph_cm2_ns.is_none()
    {
        bail!("--production requires diagnostics integrated flux-conservation evidence");
    }
    if !validation.finite_nonnegative_pass
        || !validation.plane_pole_pass
        || !validation.longitude_wrap_pass
    {
        bail!("--production requires finite/nonnegative, plane/pole, and longitude-wrap validation passes");
    }
    if diagnostics.output_sha256.trim().is_empty() {
        bail!("--production requires diagnostics output_sha256");
    }
    let photometry_model = required_header(header, "photometry_model")?;
    if is_proxy_or_experimental(photometry_model) {
        bail!("--production rejects proxy or experimental photometry models");
    }
    if !diagnostics.photometry_model.trim().is_empty()
        && diagnostics.photometry_model != *photometry_model
    {
        bail!("--production requires diagnostics photometry_model to match map header");
    }
    let calibration_status = required_header(header, "calibration_status")?;
    if calibration_status.eq_ignore_ascii_case("experimental") {
        bail!("--production rejects experimental calibration metadata");
    }
    for key in [
        "source_catalogue",
        "source_catalogue_release",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "band_definition",
        "validation_report",
    ] {
        let value = required_header(header, key)?;
        if is_placeholder(value) {
            bail!("--production requires non-placeholder header {key}");
        }
    }
    let band = required_header(header, "band_definition")?.to_ascii_lowercase();
    if !(band.contains("330-650") || band.contains("330–650")) {
        bail!("--production requires the explicitly validated 330-650 nm band");
    }
    Ok(())
}

fn complete_runtime_header(
    header: &mut BTreeMap<String, String>,
    mode: &str,
    args: &Args,
    validation_sha256: &str,
    validation: &ValidationSummary,
) -> Result<()> {
    let nside = required_header(header, "nside")?.to_string();
    let ordering = required_header(header, "ordering")?.to_string();
    set_default(header, "map_type", "healpix");
    set_default(header, "coordinate_frame", "galactic");
    set_default(header, "dataset_name", "NSB Gaia DR3 starlight map");
    set_default(header, "version", "v1");
    set_default(
        header,
        "map_resolution",
        &format!("HEALPix nside={nside} ordering={ordering}"),
    );
    set_default(
        header,
        "source_selection",
        "Gaia DR3 XP-selected source population",
    );
    set_default(
        header,
        "magnitude_limit",
        "Gaia DR3 release input selection",
    );
    set_default(header, "smoothing", "none");
    set_default(header, "source_catalogue_release", "DR3");
    header.insert(
        "validation_report".to_string(),
        format!("{} sha256 {}", args.validation.display(), validation_sha256),
    );
    header.insert(
        "independent_comparison".to_string(),
        format!(
            "structured independent comparison passed: {}",
            validation.independent_comparison_pass
        ),
    );
    header.insert("calibration_status".to_string(), mode.to_string());
    for required in [
        "generation_date_utc",
        "source_catalogue",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "photometry_model",
        "band_definition",
        "generated_by",
        "generation_command",
    ] {
        required_header(header, required)?;
    }
    Ok(())
}

fn set_default(header: &mut BTreeMap<String, String>, key: &str, value: &str) {
    header
        .entry(key.to_string())
        .or_insert_with(|| value.to_string());
}

fn rewrite_csv_with_header(raw_map: &str, header: &BTreeMap<String, String>) -> Result<String> {
    let data_start = raw_map
        .lines()
        .position(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .ok_or_else(|| anyhow::anyhow!("map CSV has no data header"))?;
    let lines = raw_map.lines().collect::<Vec<_>>();
    let mut out = String::new();
    for key in [
        "map_type",
        "coordinate_frame",
        "nside",
        "ordering",
        "map_resolution",
        "dataset_name",
        "version",
        "calibration_status",
        "generation_date_utc",
        "source_catalogue",
        "source_catalogue_release",
        "source_catalogue_license",
        "source_catalogue_checksum",
        "source_selection",
        "magnitude_limit",
        "band_definition",
        "photometry_model",
        "smoothing",
        "generated_by",
        "generation_command",
        "validation_report",
        "independent_comparison",
    ] {
        let value = required_header(header, key)?;
        out.push_str("# ");
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    for line in &lines[data_start..] {
        out.push_str(line);
        out.push('\n');
    }
    Ok(out)
}

fn required_header<'a>(header: &'a BTreeMap<String, String>, key: &str) -> Result<&'a String> {
    header
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("--production requires map header {key}"))
}

fn is_proxy_or_experimental(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("proxy") || lower.contains("experimental")
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("placeholder")
        || lower.contains("not recorded")
        || lower.contains("review required")
        || lower.contains("review-required")
        || lower.contains("manual-seed")
}

fn parse_header_metadata(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .filter_map(|line| line.strip_prefix('#'))
        .filter_map(|line| line.trim().split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::cartesian::Direction;
    use siderust::coordinates::frames::Galactic;
    use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};

    #[test]
    fn packs_fixture_map_and_manifest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.release.csv");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(
            &input,
            production_map("gaia_dr3_xp_photon_radiance_330_650nm_v1", "candidate"),
        )?;
        std::fs::write(&diagnostics, "{}\n")?;
        std::fs::write(&validation, "{\"production_ready\":false}\n")?;

        run(Args {
            input,
            diagnostics,
            validation,
            output: output.clone(),
            manifest: manifest.clone(),
            candidate: true,
            production: false,
        })?;

        let csv = std::fs::read_to_string(output)?;
        assert!(csv.starts_with("# map_type=healpix"));
        assert!(csv.contains("# calibration_status=candidate"));
        assert!(csv.contains("healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10"));
        let manifest_raw = std::fs::read_to_string(manifest)?;
        assert!(manifest_raw.contains("calibration_status = \"candidate\""));
        assert!(manifest_raw.contains("map_sha256 = \"sha256:"));
        Ok(())
    }

    #[test]
    fn production_output_is_runtime_loadable() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        run(args)?;
        Ok(())
    }

    #[test]
    fn production_rejects_not_ready_validation() -> Result<()> {
        let (args, _dir) = fixture_args("{\"production_ready\":false}\n", true)?;
        let err = run(args).expect_err("not production ready");
        assert!(err
            .to_string()
            .contains("--production requires validation production_ready=true"));
        Ok(())
    }

    #[test]
    fn production_rejects_proxy_model() -> Result<()> {
        let (args, _dir) = fixture_args(production_validation(), true)?;
        std::fs::write(
            &args.input,
            production_map("v_s10_scaled_integrated_proxy_v1", "production"),
        )?;
        let err = run(args).expect_err("proxy photometry rejected");
        assert!(err
            .to_string()
            .contains("--production rejects proxy or experimental photometry models"));
        Ok(())
    }

    #[test]
    fn production_rejects_missing_independent_comparison() -> Result<()> {
        let validation = r#"{
  "production_ready": true,
  "independent_comparison_pass": false,
  "finite_nonnegative_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true
}
"#;
        let (args, _dir) = fixture_args(validation, true)?;
        let err = run(args).expect_err("missing comparison rejected");
        assert!(err
            .to_string()
            .contains("--production requires independent_comparison_pass=true"));
        Ok(())
    }

    fn fixture_args(validation_raw: &str, production: bool) -> Result<(Args, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.release.csv");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(
            &input,
            production_map("gaia_dr3_xp_photon_radiance_330_650nm_v1", "production"),
        )?;
        std::fs::write(
            &diagnostics,
            format!(
                r#"{{
  "output_sha256":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "photometry_model":"gaia_dr3_xp_photon_radiance_330_650nm_v1",
  "input_integrated_flux_sum_ph_cm2_ns": {:.17},
  "integrated_flux_conservation_pass": true
}}"#,
                production_source_flux()
            ),
        )?;
        std::fs::write(&validation, validation_raw)?;
        Ok((
            Args {
                input,
                diagnostics,
                validation,
                output,
                manifest,
                candidate: !production,
                production,
            },
            dir,
        ))
    }

    fn production_validation() -> &'static str {
        r#"{
  "production_ready": true,
  "independent_comparison_pass": true,
  "radiance_field": "integrated_ph_cm2_ns_sr",
  "flux_conservation_pass": true,
  "finite_nonnegative_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true
}
"#
    }

    fn production_map(photometry_model: &str, calibration_status: &str) -> String {
        let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
        let mut raw = format!(
            concat!(
                "# map_type=healpix\n",
                "# coordinate_frame=galactic\n",
                "# nside=8\n",
                "# ordering=ring\n",
                "# dataset_name=synthetic Gaia XP fixture\n",
                "# version=fixture-v1\n",
                "# generation_date_utc=2026-06-24T00:00:00Z\n",
                "# photometry_model={}\n",
                "# calibration_status={}\n",
                "# source_catalogue=Gaia\n",
                "# source_catalogue_release=DR3\n",
                "# source_catalogue_license=reviewed redistribution policy\n",
                "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
                "# source_selection=synthetic Gaia XP fixture source selection\n",
                "# magnitude_limit=G <= 20\n",
                "# map_resolution=HEALPix nside=8 ordering=ring\n",
                "# band_definition=Gaia DR3 XP passband-integrated 330-650 nm photon radiance\n",
                "# smoothing=none\n",
                "# generated_by=pack_starlight_asset unit test\n",
                "# generation_command=pack_starlight_asset fixture\n",
                "# validation_report=independent validation report\n",
                "# independent_comparison=structured fixture comparison\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
            ),
            photometry_model, calibration_status
        );
        for index in 0..grid.npix() {
            let direction: Direction<Galactic> =
                grid.pixel_center(HealpixIndex::new(index)).unwrap();
            let latitude = direction.as_array()[2].asin().to_degrees().abs();
            let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
            raw.push_str(&format!("{index},{value},0,0\n"));
        }
        raw
    }

    fn production_source_flux() -> f64 {
        let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
        let mut source_flux = 0.0;
        for index in 0..grid.npix() {
            let direction: Direction<Galactic> =
                grid.pixel_center(HealpixIndex::new(index)).unwrap();
            let latitude = direction.as_array()[2].asin().to_degrees().abs();
            let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
            source_flux += value * grid.pixel_area_sr();
        }
        source_flux
    }
}
