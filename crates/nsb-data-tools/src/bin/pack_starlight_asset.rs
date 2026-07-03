use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::PathBuf;

const PACK_MAGIC: &[u8] = b"NSBSTARLIGHT1\n";

/// Pack a validated starlight map into a checksummed release asset.
#[derive(Debug, Parser)]
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
}

#[derive(Debug, Deserialize)]
struct ValidationSummary {
    production_ready: bool,
}

#[derive(Debug, Serialize)]
struct PackedManifest {
    schema_version: u32,
    asset_format: &'static str,
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
    if validation_summary.production_ready {
        // This tool can pack production assets, but production readiness must be
        // earned by the report rather than inferred by the packer.
    } else {
        eprintln!("warning: packing a non-production-ready starlight asset candidate");
    }

    let raw_map = std::str::from_utf8(&map).context("map must be UTF-8 CSV")?;
    let header = parse_header_metadata(raw_map);
    if header.get("photometry_model").is_some_and(|model| {
        let lower = model.to_ascii_lowercase();
        lower.contains("proxy") || lower.contains("experimental")
    }) {
        bail!("pack_starlight_asset rejects proxy or experimental photometry models");
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
        })?;

        let packed = std::fs::read(output)?;
        assert!(packed.starts_with(PACK_MAGIC));
        let manifest_raw = std::fs::read_to_string(manifest)?;
        assert!(manifest_raw.contains("nsb-starlight-map-csv-framed-v1"));
        assert!(manifest_raw.contains("packed_asset_sha256"));
        Ok(())
    }
}
