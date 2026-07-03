use anyhow::{bail, Context, Result};
use clap::{ArgGroup, Parser};
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::PathBuf;

const PACK_MAGIC: &[u8] = b"NSBSTARLIGHT1\n";

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
    /// Packed derived asset path.
    #[arg(long)]
    output: PathBuf,
    /// Output TOML manifest path.
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
}

#[derive(Debug, Serialize)]
struct PackedManifest {
    schema_version: u32,
    asset_format: &'static str,
    artifact_mode: &'static str,
    map_sha256: String,
    packed_asset_sha256: String,
    diagnostics_sha256: String,
    validation_sha256: String,
    production_ready: bool,
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
    let header = parse_header_metadata(raw_map);
    if args.production {
        enforce_production_gates(&validation_summary, &diagnostics_summary, &header)?;
    } else if header
        .get("photometry_model")
        .is_some_and(|model| is_proxy_or_experimental(model))
    {
        eprintln!("warning: packing candidate with proxy or experimental photometry model");
    }

    let mut packed = Vec::with_capacity(PACK_MAGIC.len() + 8 + map.len());
    packed.extend_from_slice(PACK_MAGIC);
    packed.extend_from_slice(&(map.len() as u64).to_le_bytes());
    packed.extend_from_slice(&map);
    std::fs::write(&args.output, &packed)
        .with_context(|| format!("failed to write packed asset {}", args.output.display()))?;

    let manifest = PackedManifest {
        schema_version: 1,
        asset_format: "nsb-starlight-map-csv-framed-v1",
        artifact_mode: if args.production {
            "production"
        } else {
            "candidate"
        },
        map_sha256: format!("sha256:{}", to_hex(&sha256(&map))),
        packed_asset_sha256: format!("sha256:{}", to_hex(&sha256(&packed))),
        diagnostics_sha256: format!("sha256:{}", to_hex(&sha256(&diagnostics))),
        validation_sha256: format!("sha256:{}", to_hex(&sha256(&validation))),
        production_ready: validation_summary.production_ready,
        header,
    };
    let raw = toml::to_string_pretty(&manifest)?;
    std::fs::write(&args.manifest, raw)
        .with_context(|| format!("failed to write manifest {}", args.manifest.display()))?;
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
    if calibration_status.eq_ignore_ascii_case("experimental")
        || calibration_status
            .to_ascii_lowercase()
            .contains("candidate")
    {
        bail!("--production rejects experimental or candidate calibration metadata");
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

    #[test]
    fn packs_fixture_map_and_manifest() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("map.csv");
        let diagnostics = dir.path().join("diagnostics.json");
        let validation = dir.path().join("validation.json");
        let output = dir.path().join("map.bin.zst");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(
            &input,
            concat!(
                "# photometry_model=gaia_dr3_xp_photon_radiance_330_650nm_v1\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
                "0,1,0,0\n",
            ),
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

        let packed = std::fs::read(output)?;
        assert!(packed.starts_with(PACK_MAGIC));
        let manifest_raw = std::fs::read_to_string(manifest)?;
        assert!(manifest_raw.contains("nsb-starlight-map-csv-framed-v1"));
        assert!(manifest_raw.contains("artifact_mode = \"candidate\""));
        assert!(manifest_raw.contains("packed_asset_sha256"));
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
        let output = dir.path().join("map.bin.zst");
        let manifest = dir.path().join("map.manifest.toml");
        std::fs::write(
            &input,
            production_map("gaia_dr3_xp_photon_radiance_330_650nm_v1", "production"),
        )?;
        std::fs::write(
            &diagnostics,
            r#"{"output_sha256":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","photometry_model":"gaia_dr3_xp_photon_radiance_330_650nm_v1"}"#,
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
  "finite_nonnegative_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true
}
"#
    }

    fn production_map(photometry_model: &str, calibration_status: &str) -> String {
        format!(
            concat!(
                "# photometry_model={}\n",
                "# calibration_status={}\n",
                "# source_catalogue=Gaia\n",
                "# source_catalogue_release=DR3\n",
                "# source_catalogue_license=reviewed redistribution policy\n",
                "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
                "# band_definition=Gaia DR3 XP passband-integrated 330-650 nm photon radiance\n",
                "# validation_report=independent validation report\n",
                "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
                "0,1,0,0\n",
            ),
            photometry_model, calibration_status
        )
    }
}
