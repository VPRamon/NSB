// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Evaluate a small checksum-pinned DataLink source set with the existing XP integrator.
//! Source files and outputs remain private; this example contains no catalogue records.
use anyhow::{bail, Context, Result};
use nsb_data_tools::platform::checksum_io;
use nsb_data_tools::starlight::xp::canonical::parse_datalink_gaiaxpy_csv;
use nsb_data_tools::starlight::xp::GaiaXpContinuousCalibrator;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct InputFile {
    source_id: String,
    sha256: String,
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<InputFile>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 7 {
        bail!(
            "usage: xp_source_intervals DIRECTORY MANIFEST MANIFEST_SHA DESIGN DESIGN_SHA OUTPUT"
        );
    }
    for (path, expected) in [(&args[2], &args[3]), (&args[4], &args[5])] {
        if checksum_io::sha256_file(Path::new(path))? != *expected {
            bail!("pinned input checksum mismatch");
        }
    }
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    if manifest.files.is_empty() {
        bail!("empty source manifest");
    }
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(Path::new(&args[4]))?;
    let intervals = [
        [360.0, 570.0],
        [360.0, 390.0],
        [390.0, 420.0],
        [420.0, 450.0],
        [450.0, 480.0],
        [480.0, 510.0],
        [510.0, 540.0],
        [540.0, 570.0],
    ];
    let integrator = calibrator.bp_integrator(&intervals)?;
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entry in manifest.files {
        let _: u64 = entry
            .source_id
            .parse()
            .context("source ID must be numeric")?;
        if !seen.insert(entry.source_id.clone()) {
            bail!("duplicate source ID");
        }
        let path = Path::new(&args[1]).join(format!("{}.csv", entry.source_id));
        if checksum_io::sha256_file(&path)? != entry.sha256 {
            bail!("source checksum mismatch");
        }
        let record = parse_datalink_gaiaxpy_csv(&std::fs::read(path)?, &entry.source_id)?;
        let (flux, covariance) = integrator.integrate(&record)?;
        rows.push(serde_json::json!({"source_id":entry.source_id,
            "input_sha256":entry.sha256,"flux_ph_m2_s":flux,
            "statistical_covariance":covariance,
            "positive_all_intervals":flux.iter().all(|f| *f>0.0)}));
    }
    let output = Path::new(&args[6]);
    if output.exists() {
        bail!("new immutable output required");
    }
    let report = serde_json::json!({"schema":"nsb-xp-source-intervals-v1",
        "intervals_nm":intervals,"manifest_sha256":args[3],"design_sha256":args[5],
        "sources":rows,"units":"photon/(m2 s)",
        "covariance_units":"photon^2/(m4 s2)",
        "population_selection_applied":false,
        "systematic_uncertainty":null,
        "covariance":"full within-source BP coefficient covariance; no between-source covariance"});
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
