// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Fit/export a registered continuum calibration from independent measurements.
use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb::site::airglow_profile::{
    AirglowCalibrationObservation, AirglowCalibrationProfile, AirglowCalibrationRequest,
};
use siderust::checksum::{sha256, to_hex};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    /// TOML request, including physical assumptions and registered measurements.
    #[arg(long)]
    input: PathBuf,
    /// New portable calibration artifact (must not already exist).
    #[arg(long)]
    output: PathBuf,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let raw = std::fs::read_to_string(&args.input)?;
    let request: AirglowCalibrationRequest = toml::from_str(&raw)?;
    // Validate before accessing paths: reference paths must be relative, normalized.
    request.definition.validate()?;
    let mut references = std::collections::BTreeMap::new();
    for (id, relative_path, expected_sha256) in request.definition.reference_checksums() {
        let path = args
            .input
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(relative_path);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("read calibration reference {}", path.display()))?;
        if to_hex(&sha256(&bytes)) != expected_sha256 {
            bail!("calibration reference checksum mismatch: {}", id);
        }
        if request
            .measurement_sources
            .iter()
            .any(|s| s.reference_id == id)
        {
            references.insert(id, bytes);
        }
    }
    verify_measurements(&request, &references)?;
    let profile = AirglowCalibrationProfile::fit(request)?;
    let raw = profile.to_toml_string()?;
    // Verify round-trip before publication and refuse accidental overwrite.
    AirglowCalibrationProfile::from_toml_str(&raw)?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.output)?;
    file.write_all(raw.as_bytes())?;
    println!(
        "scale={} variance={} sha256={} maturity=registered-fit-not-independently-validated",
        profile.scale().value(),
        profile.variance(),
        profile.checksum_sha256()
    );
    Ok(())
}

/// A small normalized interchange format, not a raw-spectrum reduction pipeline.
/// Original spectra/reduction products remain additional checksum-pinned references.
fn verify_measurements(
    request: &AirglowCalibrationRequest,
    references: &std::collections::BTreeMap<&str, Vec<u8>>,
) -> Result<()> {
    if request.measurement_sources.is_empty() {
        bail!("measurement_sources must explicitly identify checksum-pinned measurement tables");
    }
    let mut records = std::collections::BTreeMap::new();
    for source in &request.measurement_sources {
        let bytes = references
            .get(source.reference_id.as_str())
            .context("measurement source reference is missing")?;
        let observations: Vec<AirglowCalibrationObservation> = match source.format.as_str() {
            "observations-json-v1" => serde_json::from_slice(bytes)?,
            "observations-toml-v1" => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Table {
                    observations: Vec<AirglowCalibrationObservation>,
                }
                toml::from_str::<Table>(std::str::from_utf8(bytes)?)?.observations
            }
            _ => bail!("unsupported measurement source format version"),
        };
        for observation in observations {
            if records
                .insert(observation.id.clone(), observation)
                .is_some()
            {
                bail!("ambiguous duplicate measurement record ID");
            }
        }
    }
    for observation in &request.observations {
        if records.get(&observation.id) != Some(observation) {
            bail!(
                "training observation does not match verified measurement record: {}",
                observation.id
            );
        }
    }
    Ok(())
}
