// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Source-level B/V references for offline calibration and parity checks.

use crate::starlight::bv::BvFlux;
use crate::starlight::healpix::galactic_nested_pixel_from_icrs_position;
use crate::starlight::photometric::{PhotometricCorrection, PhotometricFeatures};
use crate::starlight::worker::processing::measured_xp_flux_and_uncertainty;
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
pub(crate) fn export(
    xp: &Path,
    gaia: &Path,
    output: &Path,
    shape_photometric: Option<(&Path, &str)>,
) -> Result<()> {
    let photometric = shape_photometric
        .map(|(path, sha)| {
            let correction = PhotometricCorrection::load(path, sha)?;
            correction.require_production_status()?;
            Ok::<_, anyhow::Error>(correction)
        })
        .transpose()?;
    let design = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&design)?;
    let sampler = calibrator.bv_sampler()?;
    let mut reader = csv::Reader::from_path(xp)?;
    let columns: HashMap<String, usize> = reader
        .headers()?
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_owned(), i))
        .collect();

    write_labels(
        reader.records().map(|record| {
            let row = parse_bulk_ecsv_record(&record?, &columns, Some(xp))?;
            let estimate = sampler.sample(&row)?;
            Ok(ReferenceFlux {
                source_id: row.source_id.parse()?,
                b_w_m2_nm: estimate.flux_w_m2_nm[0],
                v_w_m2_nm: estimate.flux_w_m2_nm[1],
                stat_b_var: estimate.covariance[0][0],
                stat_v_var: estimate.covariance[1][1],
                stat_cross: estimate.covariance[0][1],
                photon: photometric.as_ref().and_then(|_| {
                    let product = calibrator.calibrate(&row).ok()?;
                    measured_xp_flux_and_uncertainty(&product).ok()
                }),
            })
        }),
        gaia,
        output,
        photometric.as_ref(),
    )
}

#[derive(Debug, Deserialize)]
struct ReferenceFlux {
    source_id: u64,
    b_w_m2_nm: f64,
    v_w_m2_nm: f64,
    stat_b_var: f64,
    stat_v_var: f64,
    stat_cross: f64,
    #[serde(skip)]
    photon: Option<(f64, f64)>,
}

/// Preserve Gaia astrometry and observed missingness when joining external labels.
pub(crate) fn export_independent(reference: &Path, gaia: &Path, output: &Path) -> Result<()> {
    let mut reader = csv::Reader::from_path(reference)?;
    write_labels(reader.deserialize().map(|row| Ok(row?)), gaia, output, None)
}

fn write_labels(
    rows: impl Iterator<Item = Result<ReferenceFlux>>,
    gaia: &Path,
    output: &Path,
    photometric: Option<&PhotometricCorrection>,
) -> Result<()> {
    let mut photometry = BTreeMap::new();
    for row in csv::Reader::from_path(gaia)?.deserialize::<GaiaPhotometry>() {
        let row = row?;
        if photometry.insert(row.source_id, row).is_some() {
            bail!("duplicate source identity in Gaia photometry input");
        }
    }
    let mut seen = BTreeSet::new();
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = csv::Writer::from_writer(temp.as_file_mut());
    let mut header = vec![
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
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if photometric.is_some() {
        header.extend(["xp_photon_flux".into(), "xp_photon_sigma".into()]);
        for name in ["full", "g_and_bp", "g_and_rp", "g_only"] {
            header.extend([
                format!("{name}_photon"),
                format!("{name}_stat"),
                format!("{name}_sys"),
            ]);
        }
    }
    writer.write_record(header)?;
    for row in rows {
        let ReferenceFlux {
            source_id: id,
            b_w_m2_nm,
            v_w_m2_nm,
            stat_b_var,
            stat_v_var,
            stat_cross,
            photon,
        } = row?;
        if id == 0 || !seen.insert(id) {
            bail!("duplicate or invalid spectral source identity {id}");
        }
        let phot = photometry
            .get(&id)
            .with_context(|| format!("missing Gaia source {id}"))?;
        let pixel = galactic_nested_pixel_from_icrs_position(phot.ra, phot.dec, 16)?;
        BvFlux {
            flux_w_m2_nm: [b_w_m2_nm, v_w_m2_nm],
            statistical_covariance: [stat_b_var, stat_v_var, stat_cross],
            independent_systematic_sigma: [0.0; 2],
            shared_systematic_sigma: [0.0; 2],
        }
        .validate()
        .with_context(|| format!("invalid spectral reference for Gaia source {id}"))?;
        let string = |x: Option<f64>| x.map(|x| x.to_string()).unwrap_or_default();
        let mut fields = vec![
            id.to_string(),
            pixel.to_string(),
            string(phot.phot_g_mean_mag),
            string(phot.phot_bp_mean_mag),
            string(phot.phot_rp_mean_mag),
            b_w_m2_nm.to_string(),
            v_w_m2_nm.to_string(),
            stat_b_var.to_string(),
            stat_v_var.to_string(),
            stat_cross.to_string(),
        ];
        if let Some(correction) = photometric {
            fields.extend([string(photon.map(|x| x.0)), string(photon.map(|x| x.1))]);
            for (bp, rp) in [(true, true), (true, false), (false, true), (false, false)] {
                let bp = phot.phot_bp_mean_mag.filter(|_| bp);
                let rp = phot.phot_rp_mean_mag.filter(|_| rp);
                let flux = correction
                    .route_and_evaluate(PhotometricFeatures {
                        phot_g_mean_mag: phot.phot_g_mean_mag,
                        phot_bp_mean_mag: bp,
                        phot_rp_mean_mag: rp,
                        bp_rp: bp.zip(rp).map(|(b, r)| b - r),
                        quality_flag: true,
                    })?
                    .flux;
                fields.extend([
                    string(flux.as_ref().map(|x| x.flux_336_650_ph_m2_s)),
                    string(
                        flux.as_ref()
                            .map(|x| x.statistical_uncertainty_336_650_ph_m2_s),
                    ),
                    string(
                        flux.as_ref()
                            .map(|x| x.systematic_uncertainty_336_650_ph_m2_s),
                    ),
                ]);
            }
        } else {
            // Preserve the existing reference CSV's serde float representation.
            writer.serialize((
                id,
                pixel,
                phot.phot_g_mean_mag,
                phot.phot_bp_mean_mag,
                phot.phot_rp_mean_mag,
                b_w_m2_nm,
                v_w_m2_nm,
                stat_b_var,
                stat_v_var,
                stat_cross,
            ))?;
            continue;
        }
        writer.write_record(fields)?;
    }
    if seen.is_empty() || seen.len() != photometry.len() {
        bail!("Gaia photometry and spectral CSVs must describe the same nonempty source set");
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
