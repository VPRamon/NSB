//! Production processing for one immutable Gaia partition pair.

use super::config::{
    ArtifactPinConfig, GaiaProductConfig, StarlightProductBand, UvCorrectionConfig,
};
use super::healpix::gaia_source_id_equatorial_nested_pixel;
use super::healpix::IcrsSkyPosition;
use super::map::accumulator::{PartitionShard, UvCorrectionShardMetadata};
use super::photometric::{
    PhotometricCorrection, PhotometricFeatures, PopulationBranch, RouteDecision,
};
use super::selection::SelectionCorrection;
use super::sources::acquisition;
use super::uv::{EvaluationDecision, MeasuredBandInput, UvCorrection, UvEvaluationInput};
use super::xp::{
    integrate_photon_flux, integrate_photon_flux_uncertainty, GaiaXpContinuousCalibrator,
};
use crate::dataset::Artifact;
use crate::platform::artifact_store;
use anyhow::{bail, Context, Result};
use csv::ReaderBuilder;
use flate2::read::GzDecoder;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

const CSV_BUFFER_CAPACITY: usize = 1024 * 1024;

// Lifecycle inputs and optional calibrators are resolved independently; grouping
// them would obscure the borrowed process-wide artifact identities.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_partitions(
    configured_workspace: &Path,
    products: &[GaiaProductConfig],
    partitions: &[String],
    concurrency: usize,
    canonical_nside: u32,
    product_band: StarlightProductBand,
    ultraviolet_config: Option<&UvCorrectionConfig>,
    photometric_config: Option<&ArtifactPinConfig>,
    selection_config: Option<&ArtifactPinConfig>,
) -> Result<Vec<Artifact>> {
    if partitions.is_empty() {
        return Ok(Vec::new());
    }
    let (shared_workspace, worker_invocation) = workspace_roots(configured_workspace);
    let fixture = GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None);
    let calibrator = GaiaXpContinuousCalibrator::from_design_fixture(&fixture)?;
    let ultraviolet_correction = ultraviolet_config
        .map(|config| -> Result<UvCorrection> {
            let correction = UvCorrection::load(&config.artifact_path, &config.sha256)?;
            correction.require_production_status()?;
            Ok(correction)
        })
        .transpose()?;
    let photometric_correction = photometric_config
        .map(|config| -> Result<PhotometricCorrection> {
            let correction = PhotometricCorrection::load(&config.artifact_path, &config.sha256)?;
            correction.require_production_status()?;
            Ok(correction)
        })
        .transpose()?;
    let selection_correction = selection_config
        .map(|config| -> Result<SelectionCorrection> {
            let correction = SelectionCorrection::load(&config.artifact_path, &config.sha256)?;
            correction.require_production_status()?;
            Ok(correction)
        })
        .transpose()?;
    if product_band == StarlightProductBand::Combined300To650 && ultraviolet_correction.is_none() {
        bail!("300–650 nm Starlight product requires a validated UV correction artifact");
    }
    let chunk_size = partitions.len().div_ceil(concurrency.max(1)).max(1);
    let artifacts = std::thread::scope(|scope| -> Result<Vec<Artifact>> {
        let mut handles = Vec::new();
        for chunk in partitions.chunks(chunk_size) {
            let calibrator = &calibrator;
            let ultraviolet_correction = ultraviolet_correction.as_ref();
            let photometric_correction = photometric_correction.as_ref();
            let selection_correction = selection_correction.as_ref();
            let shared_workspace = shared_workspace.as_path();
            handles.push(scope.spawn(move || -> Result<Vec<Artifact>> {
                chunk
                    .iter()
                    .map(|partition| {
                        build_partition(
                            shared_workspace,
                            configured_workspace,
                            worker_invocation,
                            products,
                            partition,
                            calibrator,
                            canonical_nside,
                            product_band,
                            ultraviolet_correction,
                            photometric_correction,
                            selection_correction,
                        )
                    })
                    .collect()
            }));
        }
        let mut artifacts = Vec::with_capacity(partitions.len());
        for handle in handles {
            artifacts.extend(
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("Starlight partition worker panicked"))??,
            );
        }
        Ok(artifacts)
    })?;
    let mut artifacts = artifacts;
    artifacts.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(artifacts)
}

// The arguments are the immutable worker inputs independently resolved by the
// lifecycle; grouping them would obscure borrowed process-wide calibrators.
#[allow(clippy::too_many_arguments)]
fn build_partition(
    shared_workspace: &Path,
    configured_workspace: &Path,
    worker_invocation: bool,
    products: &[GaiaProductConfig],
    partition_id: &str,
    calibrator: &GaiaXpContinuousCalibrator,
    canonical_nside: u32,
    product_band: StarlightProductBand,
    ultraviolet_correction: Option<&UvCorrection>,
    photometric_correction: Option<&PhotometricCorrection>,
    selection_correction: Option<&SelectionCorrection>,
) -> Result<Artifact> {
    let gaia_path = acquisition::verified_object_for_partition(
        shared_workspace,
        products,
        "gaia-source",
        partition_id,
    )?;
    let xp_path = acquisition::verified_object_for_partition(
        shared_workspace,
        products,
        "xp-continuous",
        partition_id,
    )?;
    let predictor_names = ultraviolet_correction
        .map(|correction| {
            correction
                .artifact()
                .predictors
                .iter()
                .map(|predictor| predictor.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let gaia_sources = load_gaia_sources(&gaia_path, &predictor_names)?;
    let ultraviolet_metadata = ultraviolet_correction.map(|correction| UvCorrectionShardMetadata {
        model_id: correction.artifact().model_id.clone(),
        artifact_sha256: correction.artifact_sha256().to_string(),
        calibration_status: correction.artifact().calibration_status,
        response: correction.artifact().response.clone(),
        measured_conditional_residual_statistical_correlation_bits: correction
            .artifact()
            .uncertainty_model
            .measured_conditional_residual_statistical_correlation
            .to_bits(),
        systematic_correlation: correction
            .artifact()
            .uncertainty_model
            .systematic_correlation,
    });
    let mut shard = PartitionShard::new_with_policy(
        partition_id,
        canonical_nside,
        product_band,
        if product_band == StarlightProductBand::Combined300To650 {
            ultraviolet_metadata
        } else {
            None
        },
    )?;
    let mut processed = HashSet::new();
    let mut stream = super::xp::stream_bulk_ecsv_gz(&xp_path)?;
    while let Some(record) = stream.next_record()? {
        let source_id = record
            .source_id
            .parse::<u64>()
            .with_context(|| format!("invalid XP source_id {}", record.source_id))?;
        processed.insert(source_id);
        let Some(gaia_source) = gaia_sources.get(&source_id) else {
            // XP row without a matching GaiaSource row has no authoritative ICRS
            // position; skip spatial exclusion rather than invent coordinates.
            continue;
        };
        let product = match calibrator.calibrate(&record) {
            Ok(product) => product,
            Err(_) => {
                exclude_gaia_source(&mut shard, gaia_source, "calibration_failed")?;
                continue;
            }
        };
        let flux = match integrate_photon_flux(&product) {
            Ok(flux) if flux.is_finite() && flux > 0.0 => flux,
            _ => {
                exclude_gaia_source(&mut shard, gaia_source, "invalid_flux")?;
                continue;
            }
        };
        let statistical_uncertainty = match integrate_photon_flux_uncertainty(&product) {
            Ok(uncertainty) if uncertainty.is_finite() && uncertainty >= 0.0 => uncertainty,
            _ => {
                exclude_gaia_source(&mut shard, gaia_source, "invalid_uncertainty")?;
                continue;
            }
        };
        if let Err(reason) = admit_weighted_source(
            &mut shard,
            gaia_source,
            flux,
            statistical_uncertainty,
            0.0,
            product_band,
            ultraviolet_correction,
            selection_correction,
        ) {
            exclude_gaia_source(&mut shard, gaia_source, reason)?;
        }
    }

    let mut remaining: Vec<_> = gaia_sources
        .iter()
        .filter(|(source_id, _)| !processed.contains(source_id))
        .collect();
    remaining.sort_by_key(|(source_id, _)| *source_id);
    for (source_id, gaia_source) in remaining {
        let _source_id = *source_id;
        if gaia_source.duplicated_source {
            // Drop every row flagged as a Gaia duplicate. Uniqueness in the
            // source map already keeps a single entry per source_id.
            exclude_gaia_source(&mut shard, gaia_source, "duplicated_source")?;
            continue;
        }
        if gaia_source.in_qso_candidates || gaia_source.in_galaxy_candidates {
            exclude_gaia_source(&mut shard, gaia_source, "scientific_exclusion_nonstellar")?;
            continue;
        }
        let Some(photometric) = photometric_correction else {
            exclude_gaia_source(&mut shard, gaia_source, "no_xp_spectrum")?;
            continue;
        };
        let route = match photometric.route_and_evaluate(PhotometricFeatures {
            phot_g_mean_mag: gaia_source.phot_g_mean_mag,
            phot_bp_mean_mag: gaia_source.phot_bp_mean_mag,
            phot_rp_mean_mag: gaia_source.phot_rp_mean_mag,
            bp_rp: gaia_source.bp_rp,
            quality_flag: true,
        }) {
            Ok(route) => route,
            Err(_) => {
                exclude_gaia_source(&mut shard, gaia_source, "photometric_evaluation_failed")?;
                continue;
            }
        };
        let RouteDecision { branch, flux } = route;
        let Some(estimate) = flux else {
            exclude_gaia_source(&mut shard, gaia_source, population_branch_reason(branch))?;
            continue;
        };
        if let Err(reason) = admit_weighted_source(
            &mut shard,
            gaia_source,
            estimate.flux_336_650_ph_m2_s,
            estimate.statistical_uncertainty_336_650_ph_m2_s,
            estimate.systematic_uncertainty_336_650_ph_m2_s,
            product_band,
            ultraviolet_correction,
            selection_correction,
        ) {
            exclude_gaia_source(&mut shard, gaia_source, reason)?;
        }
    }

    shard.validate()?;
    let shard_path = if worker_invocation {
        configured_workspace.join("shard.json")
    } else {
        shared_workspace
            .join("workers")
            .join(partition_id)
            .join("shard.json")
    };
    let sha256 = shard.write(&shard_path)?;
    Ok(Artifact {
        name: format!("shards/{partition_id}.json"),
        bytes: shard_path.metadata()?.len(),
        path: shard_path,
        sha256,
    })
}

#[allow(clippy::too_many_arguments)]
fn admit_weighted_source(
    shard: &mut PartitionShard,
    gaia_source: &GaiaSourceEntry,
    flux: f64,
    statistical: f64,
    photometric_systematic: f64,
    product_band: StarlightProductBand,
    ultraviolet_correction: Option<&UvCorrection>,
    selection_correction: Option<&SelectionCorrection>,
) -> Result<(), &'static str> {
    let (weight, selection_systematic_fraction) =
        selection_weight(selection_correction, gaia_source)?;
    let weighted_flux = weight * flux;
    let weighted_statistical = weight * statistical;
    let systematic = photometric_systematic.hypot(selection_systematic_fraction * weighted_flux);
    if product_band == StarlightProductBand::Measured336To650 {
        return shard
            .admit(
                gaia_source.icrs,
                weighted_flux,
                weighted_statistical,
                systematic,
            )
            .map_err(|_| "admission_failed");
    }
    let correction = ultraviolet_correction.ok_or("uv_correction_missing")?;
    let Some(predictors) = &gaia_source.predictors else {
        return Err("invalid_uv_predictors");
    };
    let evaluation = match correction.evaluate(UvEvaluationInput {
        predictors,
        measured_band: Some(MeasuredBandInput {
            flux_336_650_ph_m2_s: weighted_flux,
            statistical_uncertainty_336_650_ph_m2_s: weighted_statistical,
        }),
    }) {
        Ok(evaluation) => evaluation,
        Err(_) => return Err("uv_evaluation_failed"),
    };
    if evaluation.decision == EvaluationDecision::Rejected {
        return Err("uv_out_of_domain");
    }
    let mut combined =
        match correction.combine_with_measured(weighted_flux, weighted_statistical, &evaluation) {
            Ok(combined) => combined,
            Err(_) => return Err("uv_evaluation_failed"),
        };
    combined.systematic_uncertainty_300_650_ph_m2_s = combined
        .systematic_uncertainty_300_650_ph_m2_s
        .hypot(systematic);
    shard
        .admit_corrected(gaia_source.icrs, &combined)
        .map_err(|_| "admission_failed")
}

fn exclude_gaia_source(
    shard: &mut PartitionShard,
    gaia_source: &GaiaSourceEntry,
    reason: &str,
) -> Result<()> {
    shard.exclude(gaia_source.icrs, reason)
}

fn selection_weight(
    selection: Option<&SelectionCorrection>,
    gaia_source: &GaiaSourceEntry,
) -> Result<(f64, f64), &'static str> {
    let Some(selection) = selection else {
        return Ok((1.0, 0.0));
    };
    let Some(g_mag) = gaia_source.phot_g_mean_mag else {
        return Err("selection_missing_g_magnitude");
    };
    let healpix = gaia_source_id_equatorial_nested_pixel(
        gaia_source.source_id,
        selection.artifact().healpix_nside,
    )
    .map_err(|_| "selection_healpix_failed")?;
    let evaluation = selection
        .evaluate(healpix, g_mag, gaia_source.bp_rp)
        .map_err(|_| "selection_evaluation_failed")?;
    Ok((
        evaluation.weight,
        evaluation.systematic_uncertainty_fraction,
    ))
}

fn population_branch_reason(branch: PopulationBranch) -> &'static str {
    match branch {
        PopulationBranch::XpContinuous => "xp_continuous",
        PopulationBranch::PhotometricGBpRp => "photometric_g_bp_rp",
        PopulationBranch::PhotometricPartial => "photometric_partial",
        PopulationBranch::PhotometricGOnly => "photometric_g_only",
        PopulationBranch::NoUsablePhotometry => "no_usable_photometry",
        PopulationBranch::ScientificExclusion => "scientific_exclusion",
    }
}

#[derive(Debug)]
struct GaiaSourceEntry {
    source_id: u64,
    icrs: IcrsSkyPosition,
    phot_g_mean_mag: Option<f64>,
    phot_bp_mean_mag: Option<f64>,
    phot_rp_mean_mag: Option<f64>,
    bp_rp: Option<f64>,
    duplicated_source: bool,
    in_qso_candidates: bool,
    in_galaxy_candidates: bool,
    predictors: Option<BTreeMap<String, f64>>,
}

fn load_gaia_sources(
    path: &Path,
    predictor_names: &[String],
) -> Result<HashMap<u64, GaiaSourceEntry>> {
    let decoder = GzDecoder::new(
        File::open(path).with_context(|| format!("open GaiaSource object {}", path.display()))?,
    );
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .buffer_capacity(CSV_BUFFER_CAPACITY)
        .from_reader(BufReader::new(decoder));
    let headers = reader.headers()?.clone();
    let source_index = headers
        .iter()
        .position(|header| header.trim() == "source_id")
        .context("GaiaSource partition has no source_id column")?;
    let ra_index = headers
        .iter()
        .position(|header| header.trim() == "ra")
        .context("GaiaSource partition has no ra column")?;
    let dec_index = headers
        .iter()
        .position(|header| header.trim() == "dec")
        .context("GaiaSource partition has no dec column")?;
    let phot_g_index = optional_column(&headers, "phot_g_mean_mag");
    let phot_bp_index = optional_column(&headers, "phot_bp_mean_mag");
    let phot_rp_index = optional_column(&headers, "phot_rp_mean_mag");
    let bp_rp_index = optional_column(&headers, "bp_rp");
    let duplicated_index = optional_column(&headers, "duplicated_source");
    let qso_index = optional_column(&headers, "in_qso_candidates");
    let galaxy_index = optional_column(&headers, "in_galaxy_candidates");
    let predictor_indexes = predictor_names
        .iter()
        .map(|name| {
            headers
                .iter()
                .position(|header| header.trim() == name)
                .with_context(|| format!("GaiaSource partition has no UV predictor column {name}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut source_ids = HashMap::new();
    for (row_index, row) in reader.records().enumerate() {
        let row = row.with_context(|| {
            format!(
                "read GaiaSource row {} from {}",
                row_index + 2,
                path.display()
            )
        })?;
        let source_id = row
            .get(source_index)
            .context("GaiaSource row has no source_id field")?
            .trim()
            .parse::<u64>()
            .context("GaiaSource source_id is not u64")?;
        let ra_deg = row
            .get(ra_index)
            .context("GaiaSource row has no ra field")?
            .trim()
            .parse::<f64>()
            .context("GaiaSource ra is not numeric")?;
        let dec_deg = row
            .get(dec_index)
            .context("GaiaSource row has no dec field")?
            .trim()
            .parse::<f64>()
            .context("GaiaSource dec is not numeric")?;
        let icrs = match IcrsSkyPosition::new(ra_deg, dec_deg) {
            Ok(position) => position,
            Err(_) => continue,
        };
        let predictors = predictor_names
            .iter()
            .zip(&predictor_indexes)
            .map(|(name, index)| {
                let value = row
                    .get(*index)
                    .context("GaiaSource row has no UV predictor field")?
                    .trim()
                    .parse::<f64>()
                    .context("GaiaSource UV predictor is not numeric")?;
                if !value.is_finite() {
                    bail!("GaiaSource UV predictor is not finite");
                }
                Ok((name.clone(), value))
            })
            .collect::<Result<BTreeMap<_, _>>>()
            .ok();
        let entry = GaiaSourceEntry {
            source_id,
            icrs,
            phot_g_mean_mag: optional_f64(&row, phot_g_index)?,
            phot_bp_mean_mag: optional_f64(&row, phot_bp_index)?,
            phot_rp_mean_mag: optional_f64(&row, phot_rp_index)?,
            bp_rp: optional_f64(&row, bp_rp_index)?,
            duplicated_source: optional_bool(&row, duplicated_index)?,
            in_qso_candidates: optional_bool(&row, qso_index)?,
            in_galaxy_candidates: optional_bool(&row, galaxy_index)?,
            predictors,
        };
        if source_ids.insert(source_id, entry).is_some() {
            bail!("GaiaSource partition contains duplicate source_id {source_id}");
        }
    }
    Ok(source_ids)
}

fn optional_column(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|header| header.trim() == name)
}

fn optional_f64(row: &csv::StringRecord, index: Option<usize>) -> Result<Option<f64>> {
    let Some(index) = index else {
        return Ok(None);
    };
    let raw = row
        .get(index)
        .context("GaiaSource row is missing an optional numeric field")?
        .trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("null") || raw == "nan" {
        return Ok(None);
    }
    let value = raw
        .parse::<f64>()
        .with_context(|| format!("GaiaSource numeric field is invalid: {raw}"))?;
    if !value.is_finite() {
        return Ok(None);
    }
    Ok(Some(value))
}

fn optional_bool(row: &csv::StringRecord, index: Option<usize>) -> Result<bool> {
    let Some(index) = index else {
        return Ok(false);
    };
    let raw = row
        .get(index)
        .context("GaiaSource row is missing an optional boolean field")?
        .trim();
    if raw.is_empty() {
        return Ok(false);
    }
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "t" | "yes" => Ok(true),
        "0" | "false" | "f" | "no" => Ok(false),
        _ => bail!("GaiaSource boolean field is invalid: {raw}"),
    }
}

fn workspace_roots(configured: &Path) -> (PathBuf, bool) {
    let is_worker = configured
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "workers");
    if is_worker {
        (
            configured
                .parent()
                .and_then(Path::parent)
                .unwrap_or(configured)
                .to_path_buf(),
            true,
        )
    } else {
        (configured.to_path_buf(), false)
    }
}

pub(crate) fn write_artifact_index(root: &Path, artifacts: &[Artifact]) -> Result<()> {
    artifact_store::atomic_write(
        &root.join("outputs/artifacts.json"),
        &serde_json::to_vec_pretty(artifacts)?,
    )
}

/// Hand-checkable analytical fixtures for the Starlight uncertainty
/// contract (issue #94). See `docs/nsb_components/starlight/uncertainty-contract.md`.
#[cfg(test)]
mod uncertainty_fixtures;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::checksum_io;
    use crate::starlight::config::OfficialChecksumAlgorithm;
    use crate::starlight::sources::acquisition::AcquisitionReceipt;
    use crate::starlight::sources::inventory::{SourceInventory, SourceInventoryEntry};
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use serde_json::Value;
    use std::fs;
    use std::io::Write;

    #[test]
    fn paired_partition_builds_a_strict_shard_and_maps() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let partition = "000000-003111";
        let oracle: Value = serde_json::from_slice(&fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/gaiaxpy_oracle/record-01.json"),
        )?)?;
        let source_id = oracle["source_id"].as_str().context("oracle source_id")?;
        let gaia_only_source_id = "999";

        let gaia_bytes = gzip_bytes(
            format!("source_id,ra,dec\n{source_id},45.0,20.0\n{gaia_only_source_id},50.0,10.0\n")
                .as_bytes(),
        )?;
        let correlations = vec![0.0; 55 * 54 / 2];
        let arrays = |name: &str| serde_json::to_string(&oracle[name]).unwrap();
        let mut xp_csv = csv::Writer::from_writer(Vec::new());
        xp_csv.write_record([
            "source_id",
            "bp_n_parameters",
            "bp_standard_deviation",
            "rp_n_parameters",
            "rp_standard_deviation",
            "bp_coefficients",
            "bp_coefficient_errors",
            "bp_coefficient_correlations",
            "rp_coefficients",
            "rp_coefficient_errors",
            "rp_coefficient_correlations",
            "bp_n_relevant_bases",
            "rp_n_relevant_bases",
        ])?;
        xp_csv.write_record([
            source_id,
            "55",
            oracle["bp_standard_deviation"]
                .as_f64()
                .context("bp standard deviation")?
                .to_string()
                .as_str(),
            "55",
            oracle["rp_standard_deviation"]
                .as_f64()
                .context("rp standard deviation")?
                .to_string()
                .as_str(),
            &arrays("bp_coefficients"),
            &arrays("bp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            &arrays("rp_coefficients"),
            &arrays("rp_coefficient_errors"),
            &serde_json::to_string(&correlations)?,
            "55",
            "55",
        ])?;
        let xp_bytes = gzip_bytes(&xp_csv.into_inner()?)?;

        let products = vec![
            product("gaia-source", "GaiaSource_"),
            product("xp-continuous", "XpContinuousMeanSpectrum_"),
        ];
        install_object_and_receipt(
            workspace,
            &products[0],
            partition,
            &gaia_bytes,
            "GaiaSource_000000-003111.csv.gz",
        )?;
        install_object_and_receipt(
            workspace,
            &products[1],
            partition,
            &xp_bytes,
            "XpContinuousMeanSpectrum_000000-003111.csv.gz",
        )?;

        let canonical_nside = 256;
        let artifacts = build_partitions(
            workspace,
            &products,
            &[partition.to_string()],
            1,
            canonical_nside,
            StarlightProductBand::Measured336To650,
            None,
            None,
            None,
        )?;
        assert_eq!(artifacts.len(), 1);
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&artifacts[0].path)?)?;
        assert_eq!(shard.nside, canonical_nside);
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.admitted_sources)
                .sum::<u64>(),
            1
        );
        assert_eq!(
            shard.exclusion_reasons.get("no_xp_spectrum").copied(),
            Some(1)
        );
        let reconciled = workspace
            .join("outputs/shards")
            .join(format!("{partition}.json"));
        artifact_store::atomic_write(&reconciled, &fs::read(&artifacts[0].path)?)?;
        let maps = crate::starlight::map::product::emit_maps(
            workspace,
            &[partition.to_string()],
            canonical_nside,
            StarlightProductBand::Measured336To650,
            None,
            None,
        )?;
        assert_eq!(maps.len(), 2);
        crate::starlight::map::product::validate_map(
            &workspace
                .join("outputs")
                .join(format!("starlight_nside{canonical_nside}.csv")),
            canonical_nside,
        )?;
        Ok(())
    }

    fn product(id: &str, prefix: &str) -> GaiaProductConfig {
        GaiaProductConfig {
            id: id.to_string(),
            base_url: format!("https://example.test/{id}/"),
            checksum_manifest_url: format!("https://example.test/{id}/checksums"),
            checksum_manifest_sha256: "a".repeat(64),
            checksum_algorithm: OfficialChecksumAlgorithm::Md5,
            expected_partitions: Some(1),
            filename_prefix: prefix.to_string(),
            filename_suffix: ".csv.gz".to_string(),
        }
    }

    fn install_object_and_receipt(
        workspace: &Path,
        product: &GaiaProductConfig,
        partition: &str,
        bytes: &[u8],
        filename: &str,
    ) -> Result<()> {
        let object_sha = checksum_io::sha256_bytes(bytes);
        let object_path = workspace.join("cache/objects/sha256").join(&object_sha);
        artifact_store::atomic_write(&object_path, bytes)?;
        let entry = SourceInventoryEntry {
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            url: format!("{}{filename}", product.base_url),
            official_checksum: "0".repeat(32),
        };
        let inventory = SourceInventory {
            schema_version: 1,
            product_id: product.id.clone(),
            base_url: product.base_url.clone(),
            checksum_manifest_url: product.checksum_manifest_url.clone(),
            checksum_manifest_sha256: product.checksum_manifest_sha256.clone(),
            official_checksum_algorithm: product.checksum_algorithm,
            entries: vec![entry.clone()],
        };
        artifact_store::atomic_write(
            &workspace
                .join("inventories")
                .join(format!("{}.inventory.json", product.id)),
            &serde_json::to_vec_pretty(&inventory)?,
        )?;
        let receipt = AcquisitionReceipt {
            schema_version: 1,
            product_id: product.id.clone(),
            partition_id: partition.to_string(),
            filename: filename.to_string(),
            source_url: entry.url,
            official_checksum_algorithm: product.checksum_algorithm,
            official_checksum: entry.official_checksum,
            sha256: object_sha,
            bytes: bytes.len() as u64,
            object_path,
        };
        artifact_store::atomic_write(
            &workspace
                .join("cache/receipts")
                .join(&product.id)
                .join(format!("{partition}.json")),
            &serde_json::to_vec_pretty(&receipt)?,
        )
    }

    fn gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes)?;
        Ok(encoder.finish()?)
    }
}
