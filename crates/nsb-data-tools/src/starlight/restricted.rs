// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Explicit experimental XP-only interval products through the dataset lifecycle.
//!
//! Separate band-neutral shard fields prevent legacy 336–650 diagnostic columns
//! from acquiring a second meaning. Acquisition, joins, admission geometry,
//! exact accumulation, SLURM and checkpoint receipts reuse the production engine.
use super::config::StarlightConfig;
use super::healpix::{gaia_nested_npix, galactic_nested_pixel_from_icrs_position};
use super::map::accumulator::StableSum;
use super::sources::acquisition;
use super::worker::gaia_source::load_gaia_sources;
use super::worker::processing::scientific_exclusion_reason;
use super::xp::{stream_bulk_ecsv_gz, GaiaXpContinuousCalibrator};
use crate::dataset::{Artifact, ValidationGate};
use crate::platform::{artifact_store::atomic_write, checksum_io};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const MAP_NAME: &str = "starlight_360_570_candidate.json";
pub const REPORT_NAME: &str = "starlight_360_570_validation.json";
const BINS: usize = 7;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pixel {
    flux_ph_m2_s: [StableSum; BINS],
    variance_ph_m2_s_squared: [StableSum; BINS],
    integrated_variance_ph_m2_s_squared: StableSum,
    sources: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Shard {
    schema: String,
    band_nm: [u16; 2],
    partition: String,
    nside: u32,
    input_sha256: BTreeMap<String, String>,
    design_sha256: String,
    catalogue_rows: u64,
    xp_rows: u64,
    unmatched_xp_rows: u64,
    excluded: BTreeMap<String, u64>,
    pixels: BTreeMap<u32, Pixel>,
    training_samples: Vec<serde_json::Value>,
}

fn artifact(path: &Path, name: &str) -> Result<Artifact> {
    Ok(Artifact {
        name: name.to_owned(),
        path: path.to_owned(),
        sha256: checksum_io::sha256_file(path)?,
        bytes: path.metadata()?.len(),
    })
}

/// Old-band calibration products may not enter the interval candidate.
pub(crate) fn validate_config(config: &StarlightConfig) -> Result<()> {
    if config.ultraviolet_correction.is_some()
        || config.photometric_inference.is_some()
        || config.selection_function.is_some()
        || config.bright_star_supplement.is_some()
    {
        bail!("360–570 XP-only candidate cannot consume legacy-band corrections or supplements");
    }
    Ok(())
}

pub(crate) fn build(
    root: &Path,
    config: &StarlightConfig,
    partitions: &[String],
) -> Result<Vec<Artifact>> {
    validate_config(config)?;
    let worker = root
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|p| p == "workers");
    let shared = if worker {
        root.parent()
            .and_then(Path::parent)
            .context("worker root")?
    } else {
        root
    };
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let design_sha256 = checksum_io::sha256_file(&fixture)?;
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;
    let mut bands = vec![[360.0, 570.0]];
    bands.extend((0..BINS).map(|i| [360.0 + i as f64 * 30.0, 390.0 + i as f64 * 30.0]));
    let integrator = calibrator.bp_integrator(&bands)?;
    let mut artifacts = Vec::new();
    for partition in partitions {
        let mut inputs = BTreeMap::new();
        for product in ["gaia-source", "xp-continuous"] {
            let p = acquisition::verified_object_for_partition(
                shared,
                &config.gaia_products,
                product,
                partition,
            )?;
            inputs.insert(product.to_owned(), p);
        }
        let gaia = load_gaia_sources(&inputs["gaia-source"], &[])?;
        let mut shard = Shard {
            schema: "nsb-starlight-interval-shard-v1".into(),
            band_nm: [360, 570],
            partition: partition.clone(),
            nside: config.map.canonical_nside,
            input_sha256: inputs
                .iter()
                .map(|(k, p)| Ok((k.clone(), checksum_io::sha256_file(p)?)))
                .collect::<Result<_>>()?,
            design_sha256: design_sha256.clone(),
            catalogue_rows: gaia.len() as u64,
            xp_rows: 0,
            unmatched_xp_rows: 0,
            excluded: BTreeMap::new(),
            pixels: BTreeMap::new(),
            training_samples: Vec::new(),
        };
        let mut seen = BTreeSet::new();
        let mut stream = stream_bulk_ecsv_gz(&inputs["xp-continuous"])?;
        while let Some(record) = stream.next_record()? {
            shard.xp_rows += 1;
            let id = record.source_id.parse::<u64>()?;
            if !seen.insert(id) {
                bail!("duplicate XP source {id}");
            }
            let Some(source) = gaia.get(&id) else {
                shard.unmatched_xp_rows += 1;
                continue;
            };
            let reason = scientific_exclusion_reason(source);
            if let Some(reason) = reason {
                *shard.excluded.entry(reason.into()).or_default() += 1;
                continue;
            }
            let (flux, covariance) = match integrator.integrate(&record) {
                Ok(value) => value,
                Err(_) => {
                    *shard
                        .excluded
                        .entry("calibration_failed".into())
                        .or_default() += 1;
                    continue;
                }
            };
            if flux.iter().any(|f| *f <= 0.0) {
                *shard
                    .excluded
                    .entry("nonpositive_interval_flux".into())
                    .or_default() += 1;
                continue;
            }
            let sum: f64 = flux[1..].iter().sum();
            if (sum - flux[0]).abs() > flux[0].abs() * 1e-10 {
                bail!("sub-band conservation failed for {id}");
            }
            let pixel = galactic_nested_pixel_from_icrs_position(
                source.icrs.ra_deg,
                source.icrs.dec_deg,
                shard.nside,
            )?;
            // Frozen source-id sampling before any downstream regression errors.
            // Retain measured targets for band-specific non-XP calibration research.
            if id % 4093 == 0 {
                shard.training_samples.push(serde_json::json!({
                    "source_id":id.to_string(), "pixel":pixel,
                    "g":source.phot_g_mean_mag,"bp":source.phot_bp_mean_mag,
                    "rp":source.phot_rp_mean_mag,"bp_rp":source.bp_rp,
                    "flux_ph_m2_s":flux,"variance":covariance.iter().enumerate().map(|(i,r)|r[i]).collect::<Vec<_>>()
                }));
            }
            let p = shard.pixels.entry(pixel).or_default();
            for i in 0..BINS {
                p.flux_ph_m2_s[i].add(flux[i + 1])?;
                p.variance_ph_m2_s_squared[i].add(covariance[i + 1][i + 1])?;
            }
            p.integrated_variance_ph_m2_s_squared
                .add(covariance[0][0])?;
            p.sources += 1;
        }
        for (id, source) in &gaia {
            if !seen.contains(id) {
                let reason = scientific_exclusion_reason(source)
                    .unwrap_or("no_band_calibrated_non_xp_inference");
                *shard.excluded.entry(reason.into()).or_default() += 1;
            }
        }
        validate_shard(&shard)?;
        let path = if worker {
            root.join("shard.json")
        } else {
            root.join("workers").join(partition).join("shard.json")
        };
        atomic_write(&path, &serde_json::to_vec(&shard)?)?;
        artifacts.push(artifact(&path, &format!("shards/{partition}.json"))?);
    }
    Ok(artifacts)
}

fn validate_shard(shard: &Shard) -> Result<()> {
    if shard.schema != "nsb-starlight-interval-shard-v1" || shard.band_nm != [360, 570] {
        bail!("interval shard band/schema mismatch");
    }
    let admitted: u64 = shard.pixels.values().map(|p| p.sources).sum();
    if admitted + shard.excluded.values().sum::<u64>() != shard.catalogue_rows {
        bail!("interval population accounting failed");
    }
    let npix = gaia_nested_npix(shard.nside)?;
    for (&index, p) in &shard.pixels {
        if u64::from(index) >= npix || p.sources == 0 {
            bail!("invalid interval pixel");
        }
        for sum in p
            .flux_ph_m2_s
            .iter()
            .chain(&p.variance_ph_m2_s_squared)
            .chain(std::iter::once(&p.integrated_variance_ph_m2_s_squared))
        {
            if !sum.value().is_finite() || sum.value() < 0.0 {
                bail!("invalid interval accumulator");
            }
        }
    }
    Ok(())
}

pub(crate) fn finalize(
    root: &Path,
    config: &StarlightConfig,
    partitions: &[String],
) -> Result<Vec<Artifact>> {
    validate_config(config)?;
    let mut merged: BTreeMap<u32, Pixel> = BTreeMap::new();
    let mut identities = BTreeMap::new();
    let mut excluded: BTreeMap<String, u64> = BTreeMap::new();
    let mut rows = 0_u64;
    let mut xp_rows = 0_u64;
    let mut unmatched = 0_u64;
    let mut design = None;
    let mut source_flux = StableSum::default();
    for partition in partitions {
        let path = root
            .join("outputs/shards")
            .join(format!("{partition}.json"));
        let shard: Shard = serde_json::from_slice(&fs::read(&path)?)?;
        validate_shard(&shard)?;
        if shard.partition != *partition || shard.nside != config.map.canonical_nside {
            bail!("interval partition identity mismatch");
        }
        if design.as_ref().is_some_and(|d| d != &shard.design_sha256) {
            bail!("mixed XP calibration designs");
        }
        design = Some(shard.design_sha256.clone());
        identities.insert(partition, serde_json::json!({"shard_sha256":checksum_io::sha256_file(&path)?, "inputs":shard.input_sha256}));
        rows += shard.catalogue_rows;
        xp_rows += shard.xp_rows;
        unmatched += shard.unmatched_xp_rows;
        for (reason, n) in shard.excluded {
            *excluded.entry(reason).or_default() += n;
        }
        for (index, p) in shard.pixels {
            let m = merged.entry(index).or_default();
            for i in 0..BINS {
                source_flux.merge(&p.flux_ph_m2_s[i])?;
                m.flux_ph_m2_s[i].merge(&p.flux_ph_m2_s[i])?;
                m.variance_ph_m2_s_squared[i].merge(&p.variance_ph_m2_s_squared[i])?;
            }
            m.integrated_variance_ph_m2_s_squared
                .merge(&p.integrated_variance_ph_m2_s_squared)?;
            m.sources += p.sources;
        }
    }
    let npix = gaia_nested_npix(config.map.canonical_nside)?;
    let factor = npix as f64 / (4.0 * std::f64::consts::PI) * 1e-13;
    let mut pixels = Vec::new();
    let mut map_flux = StableSum::default();
    let mut admitted = 0_u64;
    for index in 0..npix {
        let p = merged.remove(&(index as u32)).unwrap_or_default();
        admitted += p.sources;
        for f in &p.flux_ph_m2_s {
            map_flux.merge(f)?;
        }
        pixels.push(serde_json::json!({
            "pixel":index, "sources":p.sources,
            "radiance":p.flux_ph_m2_s.iter().map(|s|s.value()*factor).collect::<Vec<_>>(),
            "statistical_sigma":p.variance_ph_m2_s_squared.iter().map(|s|s.value().sqrt()*factor).collect::<Vec<_>>(),
            "integrated_statistical_sigma":p.integrated_variance_ph_m2_s_squared.value().sqrt()*factor,
        }));
    }
    if map_flux.value().to_bits() != source_flux.value().to_bits()
        || admitted + excluded.values().sum::<u64>() != rows
    {
        bail!("interval conservation/population validation failed");
    }
    let map = serde_json::json!({
        "schema":"nsb-starlight-interval-map-v1", "product_id":"starlight-gaia-dr3-xp-360-570-v1",
        "band_nm":[360,570], "bin_edges_nm":[360,390,420,450,480,510,540,570],
        "nside":config.map.canonical_nside,"ordering":"nested","coordinate_frame":"galactic",
        "domain":"top-of-atmosphere","units":"photon/(cm2 ns sr)","wavelength_convention":"vacuum",
        "maturity":"experimental", "population_policy":"gaia-dr3-quality-xp-only-positive-subbands-v1",
        "non_xp_inference":null,"selection_correction":null,"bright_star_supplement":null,
        "systematic_uncertainty":null,"statistical_covariance":"full BP coefficient covariance; source-independent; bin cross covariance not packed",
        "design_sha256":design,"input_partitions":identities,"catalogue_rows":rows,"xp_rows":xp_rows,
        "unmatched_xp_rows":unmatched,"admitted_sources":admitted,"excluded_sources":excluded,"pixels":pixels,
    });
    let map_path = root.join("outputs").join(MAP_NAME);
    atomic_write(&map_path, &serde_json::to_vec(&map)?)?;
    let report = serde_json::json!({"schema":"nsb-starlight-interval-validation-v1",
        "map_sha256":checksum_io::sha256_file(&map_path)?, "expected_partitions":partitions.len(),
        "processed_partitions":identities.len(),"population_accounting":true,"exact_flux_conservation":true,
        "total_flux_ph_m2_s":source_flux.value(),"admitted_sources":admitted,
        "all_sky_pixels":npix,"limitations":["XP-only population, no non-XP or missing bright-star completion", "No completeness correction", "Calibration systematic uncertainty unquantified", "TOA pixel average does not represent DESI source-masked fiber radiance"]});
    let report_path = root.join("outputs").join(REPORT_NAME);
    atomic_write(&report_path, &serde_json::to_vec_pretty(&report)?)?;
    Ok(vec![
        artifact(&map_path, MAP_NAME)?,
        artifact(&report_path, REPORT_NAME)?,
    ])
}

pub(crate) fn gates(root: &Path) -> Result<Vec<ValidationGate>> {
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("outputs").join(REPORT_NAME))?)?;
    let actual = checksum_io::sha256_file(&root.join("outputs").join(MAP_NAME))?;
    Ok(vec![ValidationGate {
        name: "interval-map-identity-and-conservation".into(),
        passed: report["map_sha256"].as_str() == Some(&actual)
            && report["exact_flux_conservation"] == true
            && report["population_accounting"] == true
            && report["processed_partitions"] == report["expected_partitions"],
        detail: "Experimental XP-only map; scientific omissions remain explicit".into(),
    }])
}
