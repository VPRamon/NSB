// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Source-level B/V references for offline calibration and parity checks.

use crate::starlight::healpix::galactic_nested_pixel_from_icrs_position;
use crate::starlight::xp::{canonical::parse_bulk_ecsv_record, GaiaXpContinuousCalibrator};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Deserialize)]
struct GaiaPhotometry {
    source_id: u64,
    ra: f64,
    dec: f64,
    phot_g_mean_mag: Option<f64>,
    phot_bp_mean_mag: Option<f64>,
    phot_rp_mean_mag: Option<f64>,
}

/// Export signed coefficient-space references keyed by Gaia source identity.
/// The CSV extraction and design are pinned by the caller's experiment receipt.
pub(crate) fn export(xp: &Path, gaia: &Path, output: &Path) -> Result<()> {
    let design = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&design)?;
    let sampler = calibrator.bv_sampler()?;
    let mut photometry = BTreeMap::new();
    for row in csv::Reader::from_path(gaia)?.deserialize::<GaiaPhotometry>() {
        let row = row?;
        if photometry.insert(row.source_id, row).is_some() {
            bail!("duplicate source identity in Gaia photometry input");
        }
    }
    let mut reader = csv::Reader::from_path(xp)?;
    let columns: HashMap<String, usize> = reader
        .headers()?
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_owned(), i))
        .collect();

    let mut seen = BTreeSet::new();
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = csv::Writer::from_writer(temp.as_file_mut());
    writer.write_record([
        "source_id",
        "galactic_nest16",
        "g",
        "bp",
        "rp",
        "b_w_m2_nm",
        "v_w_m2_nm",
        "stat_b_var",
        "stat_v_var",
        "stat_cross",
    ])?;
    for row in reader.records() {
        let row = parse_bulk_ecsv_record(&row?, &columns, Some(xp))?;
        let id: u64 = row.source_id.parse()?;
        if !seen.insert(id) {
            bail!("duplicate XP source identity {id}");
        }
        let phot = photometry
            .get(&id)
            .with_context(|| format!("missing Gaia source {id}"))?;
        let pixel = galactic_nested_pixel_from_icrs_position(phot.ra, phot.dec, 16)?;
        let estimate = sampler.sample(&row)?;
        writer.serialize((
            id,
            pixel,
            phot.phot_g_mean_mag,
            phot.phot_bp_mean_mag,
            phot.phot_rp_mean_mag,
            estimate.flux_w_m2_nm[0],
            estimate.flux_w_m2_nm[1],
            estimate.covariance[0][0],
            estimate.covariance[1][1],
            estimate.covariance[0][1],
        ))?;
    }
    if seen.is_empty() || seen.len() != photometry.len() {
        bail!("Gaia photometry and continuous XP CSVs must describe the same nonempty source set");
    }
    writer.flush()?;
    drop(writer);
    temp.persist(output)?;
    Ok(())
}

/// Compare all integrated fields bit-for-bit under the same index geometry.
pub(crate) fn compare_integrated(
    baseline: &Path,
    baseline_sha256: &str,
    candidate: &Path,
    candidate_sha256: &str,
) -> Result<usize> {
    use crate::platform::checksum_io::sha256_bytes;
    use nsb::components::starlight::{StarlightMap, StarlightProvenance};

    let load = |path: &Path, expected_sha256: &str, magic: &[u8; 8]| -> Result<StarlightMap> {
        let bytes = std::fs::read(path)?;
        let digest = sha256_bytes(&bytes);
        if digest != expected_sha256 || !bytes.starts_with(magic) {
            bail!(
                "unexpected compact format or checksum for {}",
                path.display()
            );
        }
        let provenance = StarlightProvenance::new(
            "Starlight differential",
            "see header",
            "see header",
            "see header",
            "see header",
            "see header",
            "see header",
            "see header",
            Some(digest),
        );
        Ok(StarlightMap::from_compact_bytes(&bytes, provenance)?)
    };
    let baseline = load(baseline, baseline_sha256, b"NSBSTL01")?;
    let candidate = load(candidate, candidate_sha256, b"NSBSTL02")?;
    if baseline.grid() != candidate.grid() {
        bail!("compact Starlight grids differ");
    }
    for (index, (before, after)) in baseline.pixels().iter().zip(candidate.pixels()).enumerate() {
        for (label, b, a) in [
            ("integrated radiance", before.integrated, after.integrated),
            (
                "statistical uncertainty",
                before.statistical_uncertainty.unwrap(),
                after.statistical_uncertainty.unwrap(),
            ),
            (
                "systematic uncertainty",
                before.systematic_uncertainty.unwrap(),
                after.systematic_uncertainty.unwrap(),
            ),
            (
                "total uncertainty",
                before.total_uncertainty.unwrap(),
                after.total_uncertainty.unwrap(),
            ),
        ] {
            if b.value().to_bits() != a.value().to_bits() {
                bail!(
                    "pixel {index} {label} changed from {:.17e} to {:.17e}",
                    b.value(),
                    a.value()
                );
            }
        }
    }
    Ok(baseline.pixels().len())
}
