//! Production processing for one immutable Gaia partition pair.

pub(crate) mod gaia_source;
pub(crate) mod processing;

use self::gaia_source::{load_gaia_sources, GaiaSourceEntry};
use self::processing::{
    measured_xp_flux_and_uncertainty, population_branch_reason, scientific_exclusion_reason,
};
use super::bright_stars::{load_bright_star_artifact, BrightStarArtifact};
use super::config::{
    ArtifactPinConfig, GaiaProductConfig, StarlightProductBand, UvCorrectionConfig,
};
use super::healpix::{self};
use super::map::accumulator::{PartitionShard, UvCorrectionShardMetadata};
use super::photometric::{PhotometricCorrection, PhotometricFeatures, RouteDecision};
use super::selection::SelectionCorrection;
use super::sources::acquisition;
use super::uv::{EvaluationDecision, MeasuredBandInput, UvCorrection, UvEvaluationInput};
use super::xp::GaiaXpContinuousCalibrator;
use crate::dataset::Artifact;
use crate::platform::artifact_store;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

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
    bright_star_config: Option<&ArtifactPinConfig>,
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
    let bright_star_artifact = bright_star_config
        .map(|config| load_bright_star_artifact(&config.artifact_path, &config.sha256))
        .transpose()?;
    if let Some(artifact) = &bright_star_artifact {
        super::pipeline::ensure_bright_star_product_compatible(product_band, artifact)?;
        if artifact.nside != canonical_nside {
            bail!("bright-star artifact nside does not match canonical Starlight nside");
        }
    }
    let suppressed_gaia_source_ids = bright_star_artifact
        .as_ref()
        .map(BrightStarArtifact::suppressed_gaia_source_ids)
        .transpose()?
        .unwrap_or_default();
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
            let suppressed_gaia_source_ids = &suppressed_gaia_source_ids;
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
                            suppressed_gaia_source_ids,
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
    suppressed_gaia_source_ids: &BTreeSet<u64>,
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
    let ultraviolet_metadata =
        ultraviolet_correction.map(UvCorrectionShardMetadata::from_correction);
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
        if suppressed_gaia_source_ids.contains(&source_id) {
            if !shard.bright_star_suppressed_gaia_ids.insert(source_id) {
                bail!("Gaia source_id {source_id} was suppressed more than once in one shard");
            }
            exclude_gaia_source(
                &mut shard,
                gaia_source,
                "bright_star_replaced_by_supplement",
            )?;
            continue;
        }
        if let Some(reason) = scientific_exclusion_reason(gaia_source) {
            exclude_gaia_source(&mut shard, gaia_source, reason)?;
            continue;
        }
        let product = match calibrator.calibrate(&record) {
            Ok(product) => product,
            Err(_) => {
                exclude_gaia_source(&mut shard, gaia_source, "calibration_failed")?;
                continue;
            }
        };
        let (flux, statistical_uncertainty) = match measured_xp_flux_and_uncertainty(&product) {
            Ok(values) => values,
            Err(reason) => {
                exclude_gaia_source(&mut shard, gaia_source, reason)?;
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
        } else if suppressed_gaia_source_ids.contains(&source_id) {
            shard
                .bright_star_base_admitted_replacement_gaia_ids
                .insert(source_id);
        }
    }

    let mut remaining: Vec<_> = gaia_sources
        .iter()
        .filter(|(source_id, _)| !processed.contains(source_id))
        .collect();
    remaining.sort_by_key(|(source_id, _)| *source_id);
    for (source_id, gaia_source) in remaining {
        if suppressed_gaia_source_ids.contains(source_id) {
            if !shard.bright_star_suppressed_gaia_ids.insert(*source_id) {
                bail!("Gaia source_id {source_id} was suppressed more than once in one shard");
            }
            exclude_gaia_source(
                &mut shard,
                gaia_source,
                "bright_star_replaced_by_supplement",
            )?;
            continue;
        }
        if let Some(reason) = scientific_exclusion_reason(gaia_source) {
            exclude_gaia_source(&mut shard, gaia_source, reason)?;
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
        } else if suppressed_gaia_source_ids.contains(source_id) {
            shard
                .bright_star_base_admitted_replacement_gaia_ids
                .insert(*source_id);
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

/// Construct the supplement contribution as one synthetic shard matching the
/// configured Starlight product band. Gaia replacements have already been
/// excluded by the worker path above.
pub(crate) fn bright_star_supplement_shard(
    artifact: &BrightStarArtifact,
    verified_artifact_sha256: &str,
    canonical_nside: u32,
    product_band: StarlightProductBand,
    ultraviolet_correction: Option<UvCorrectionShardMetadata>,
) -> Result<PartitionShard> {
    artifact.validate()?;
    if artifact.nside != canonical_nside {
        bail!("bright-star artifact nside does not match canonical Starlight nside");
    }
    let artifact_band = artifact.product_band_kind()?;
    match (product_band, artifact_band) {
        (
            StarlightProductBand::Measured336To650,
            crate::starlight::bright_stars::BrightStarArtifactProductBand::Measured336To650,
        ) => {
            if ultraviolet_correction.is_some() {
                bail!("measured bright-star shards forbid UV correction metadata");
            }
        }
        (
            StarlightProductBand::Combined300To650,
            crate::starlight::bright_stars::BrightStarArtifactProductBand::Combined300To650,
        ) => {
            if ultraviolet_correction.is_none() {
                bail!("combined bright-star shards require the production UV correction metadata for merge identity");
            }
        }
        (StarlightProductBand::Combined300To650, _) => {
            bail!("measured-only bright-star artifact cannot enter a combined 300-650 product");
        }
        (StarlightProductBand::Measured336To650, _) => {
            bail!("combined bright-star artifact cannot enter a measured-only product");
        }
    }
    let mut shard = PartitionShard::new_with_policy(
        "bright-star-supplement",
        canonical_nside,
        product_band,
        ultraviolet_correction,
    )?;
    shard.bright_star_supplement_provenance =
        Some(artifact.supplement_provenance(verified_artifact_sha256)?);
    shard.bright_star_replacement_gaia_ids = artifact.suppressed_gaia_source_ids()?;
    for source in &artifact.sources {
        if matches!(
            source.class,
            super::bright_stars::SupplementClass::SupplementOnly
                | super::bright_stars::SupplementClass::MatchedAndReplacesPrimary
        ) {
            shard.admit_bright_star_source(source, artifact_band)?;
        }
    }
    shard.validate()?;
    Ok(shard)
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
        // Combined 300–650 requires a scientifically estimated 300–336 nm
        // contribution. Missing UV predictors exclude the source from the
        // canonical combined map; Experiment C quantifies the measured
        // 336–650 nm flux that is thereby omitted (#182).
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
    let healpix = healpix::icrs_equatorial_nested_pixel(
        gaia_source.icrs.ra_deg,
        gaia_source.icrs.dec_deg,
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

    #[test]
    fn xp_path_applies_same_scientific_exclusions_as_non_xp_path() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let partition = "000000-003111";
        let oracle: Value = serde_json::from_slice(&fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/gaiaxpy_oracle/record-01.json"),
        )?)?;
        let source_id = oracle["source_id"].as_str().context("oracle source_id")?;
        let gaia_bytes = gzip_bytes(
            format!("source_id,ra,dec,in_galaxy_candidates\n{source_id},45.0,20.0,true\n")
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
        let artifacts = build_partitions(
            workspace,
            &products,
            &[partition.to_string()],
            1,
            256,
            StarlightProductBand::Measured336To650,
            None,
            None,
            None,
            None,
        )?;
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&artifacts[0].path)?)?;
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.admitted_sources)
                .sum::<u64>(),
            0,
            "galaxy candidates with XP must not be admitted"
        );
        assert_eq!(
            shard
                .exclusion_reasons
                .get("scientific_exclusion_nonstellar")
                .copied(),
            Some(1)
        );
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

    #[test]
    fn production_partition_accumulates_source_in_galactic_pixel_not_legacy_source_id_pixel(
    ) -> Result<()> {
        use crate::starlight::healpix::{
            galactic_nested_pixel_from_icrs_position,
            legacy_equatorial_bitshift_mislabelled_as_galactic_pixel,
            test_support::fixture_icrs_from_source_id,
        };

        const CANONICAL_NSIDE: u32 = 128;
        let source_id = 4_295_806_660_u64;
        let icrs = fixture_icrs_from_source_id(source_id);
        let expected_galactic =
            galactic_nested_pixel_from_icrs_position(icrs.ra_deg, icrs.dec_deg, CANONICAL_NSIDE)?;
        let legacy_pixel =
            legacy_equatorial_bitshift_mislabelled_as_galactic_pixel(source_id, CANONICAL_NSIDE)?;
        assert_ne!(
            expected_galactic, legacy_pixel,
            "fixture must separate production Galactic pixel from legacy source_id pixel"
        );

        let temporary = tempfile::tempdir()?;
        let workspace = temporary.path();
        let partition = "000000-003111";
        let oracle: Value = serde_json::from_slice(&fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/gaiaxpy_oracle/record-01.json"),
        )?)?;
        let gaia_bytes = gzip_bytes(
            format!(
                "source_id,ra,dec\n{source_id},{},{}\n",
                icrs.ra_deg, icrs.dec_deg
            )
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
            &source_id.to_string(),
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

        let artifacts = build_partitions(
            workspace,
            &products,
            &[partition.to_string()],
            1,
            CANONICAL_NSIDE,
            StarlightProductBand::Measured336To650,
            None,
            None,
            None,
            None,
        )?;
        assert_eq!(artifacts.len(), 1);
        let shard: PartitionShard = serde_json::from_slice(&fs::read(&artifacts[0].path)?)?;
        assert_eq!(shard.nside, CANONICAL_NSIDE);
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.admitted_sources)
                .sum::<u64>(),
            1,
            "exactly one source must be admitted through the production XP path"
        );
        assert_eq!(
            shard
                .pixels
                .get(&expected_galactic)
                .map(|pixel| pixel.admitted_sources),
            Some(1),
            "production accumulation must land in the ICRS→Galactic pixel {expected_galactic}"
        );
        assert_eq!(
            shard
                .pixels
                .get(&legacy_pixel)
                .map(|pixel| pixel.admitted_sources)
                .unwrap_or(0),
            0,
            "legacy source_id-derived pixel {legacy_pixel} must remain empty"
        );
        Ok(())
    }

    #[test]
    fn bright_star_supplement_shard_carries_verified_provenance() -> Result<()> {
        use crate::starlight::bright_stars::{
            BrightStarArtifact, BrightStarPopulationPolicy, BrightStarPrecedencePolicy,
            BrightStarSourceRecord, CorrelatedUncertainty, SupplementClass, BRIGHT_STAR_MODEL_ID,
            POPULATION_POLICY_ID_V1, PRECEDENCE_POLICY_ID_V1,
        };

        let commit = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let sources = vec![
            BrightStarSourceRecord {
                source_id: "hip-1".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::SupplementOnly,
                gaia_source_id: None,
                ra_deg_j2016: 10.0,
                dec_deg_j2016: 20.0,
                flux_336_650_ph_m2_s: 4.0,
                statistical_uncertainty_ph_m2_s: 0.1,
                systematic_independent_uncertainty_ph_m2_s: 0.2,
                systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                    correlation_group_id: "hip2-zero-point".into(),
                    uncertainty_ph_m2_s: 0.05,
                }],
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            },
            BrightStarSourceRecord {
                source_id: "hip-2".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::MatchedAndReplacesPrimary,
                gaia_source_id: Some(42),
                ra_deg_j2016: 11.0,
                dec_deg_j2016: 21.0,
                flux_336_650_ph_m2_s: 6.0,
                statistical_uncertainty_ph_m2_s: 0.1,
                systematic_independent_uncertainty_ph_m2_s: 0.2,
                systematic_catalogue_correlated: Vec::new(),
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            },
            BrightStarSourceRecord {
                source_id: "hip-3".into(),
                origin_catalogue: "hip2".into(),
                class: SupplementClass::MatchedAndRejectedAsDuplicate,
                gaia_source_id: Some(99),
                ra_deg_j2016: 12.0,
                dec_deg_j2016: 22.0,
                flux_336_650_ph_m2_s: 100.0,
                statistical_uncertainty_ph_m2_s: 1.0,
                systematic_independent_uncertainty_ph_m2_s: 1.0,
                systematic_catalogue_correlated: Vec::new(),
                band_components: None,
                spectral_route: "fixture".into(),
                classification_reason: "fixture".into(),
            },
        ];
        let artifact = BrightStarArtifact::from_sources(
            1,
            commit,
            Vec::new(),
            sources,
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
        )?;
        let sha = "e".repeat(64);
        let shard = bright_star_supplement_shard(
            &artifact,
            &sha,
            1,
            StarlightProductBand::Measured336To650,
            None,
        )?;
        let provenance = shard.bright_star_supplement_provenance.as_ref().unwrap();
        assert_eq!(provenance.artifact_sha256, sha);
        assert_eq!(provenance.model_id, BRIGHT_STAR_MODEL_ID);
        assert_eq!(provenance.population_policy_id, POPULATION_POLICY_ID_V1);
        assert_eq!(provenance.precedence_policy_id, PRECEDENCE_POLICY_ID_V1);
        assert_eq!(provenance.build_commit, commit);
        assert_eq!(shard.bright_star_replacement_gaia_ids, BTreeSet::from([42]));
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.admitted_sources)
                .sum::<u64>(),
            2
        );
        assert!(bright_star_supplement_shard(
            &artifact,
            &sha,
            2,
            StarlightProductBand::Measured336To650,
            None,
        )
        .is_err());
        assert!(bright_star_supplement_shard(
            &artifact,
            "short",
            1,
            StarlightProductBand::Measured336To650,
            None,
        )
        .is_err());
        // Measured artifact must fail closed against a combined product request.
        assert!(bright_star_supplement_shard(
            &artifact,
            &sha,
            1,
            StarlightProductBand::Combined300To650,
            None,
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn combined_bright_star_supplement_shard_admits_band_components() -> Result<()> {
        use crate::starlight::bright_stars::{
            BrightStarArtifact, BrightStarArtifactProductBand, BrightStarBandComponents,
            BrightStarPopulationPolicy, BrightStarPrecedencePolicy, BrightStarSourceRecord,
            CorrelatedUncertainty, SupplementClass, BRIGHT_STAR_MODEL_ID_COMBINED,
            BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID,
        };
        use crate::starlight::uv::{CalibrationStatus, ModelResponse, SystematicCorrelation};

        let commit = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let source = BrightStarSourceRecord {
            source_id: "hip-1".into(),
            origin_catalogue: "hip2".into(),
            class: SupplementClass::SupplementOnly,
            gaia_source_id: None,
            ra_deg_j2016: 10.0,
            dec_deg_j2016: 20.0,
            flux_336_650_ph_m2_s: 4.0,
            statistical_uncertainty_ph_m2_s: 0.1,
            systematic_independent_uncertainty_ph_m2_s: 0.2,
            systematic_catalogue_correlated: vec![CorrelatedUncertainty {
                correlation_group_id: "hip2-zero-point".into(),
                uncertainty_ph_m2_s: 0.05,
            }],
            band_components: Some(BrightStarBandComponents {
                flux_300_336_ph_m2_s: 1.0,
                flux_300_650_ph_m2_s: 5.0,
                statistical_uncertainty_300_336_ph_m2_s: 0.025,
                statistical_uncertainty_300_650_ph_m2_s: 0.125,
                systematic_independent_uncertainty_300_336_ph_m2_s: 0.05,
                systematic_independent_uncertainty_300_650_ph_m2_s: 0.25,
                systematic_catalogue_correlated_300_336: vec![CorrelatedUncertainty {
                    correlation_group_id: "hip2-zero-point".into(),
                    uncertainty_ph_m2_s: 0.0125,
                }],
                systematic_catalogue_correlated_300_650: vec![CorrelatedUncertainty {
                    correlation_group_id: "hip2-zero-point".into(),
                    uncertainty_ph_m2_s: 0.0625,
                }],
            }),
            spectral_route: "fixture".into(),
            classification_reason: "fixture".into(),
        };
        let artifact = BrightStarArtifact::from_sources_for_band(
            1,
            commit,
            Vec::new(),
            vec![source],
            BrightStarPopulationPolicy::v1(),
            BrightStarPrecedencePolicy::v1(),
            BrightStarArtifactProductBand::Combined300To650,
        )?;
        let sha = "e".repeat(64);
        let uv_metadata = UvCorrectionShardMetadata {
            model_id: "fixture-uv".into(),
            artifact_sha256: "f".repeat(64),
            calibration_status: CalibrationStatus::Validated,
            response: ModelResponse::AbsoluteUvPhotonFlux,
            measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
            systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
        };
        let shard = bright_star_supplement_shard(
            &artifact,
            &sha,
            1,
            StarlightProductBand::Combined300To650,
            Some(uv_metadata),
        )?;
        let provenance = shard.bright_star_supplement_provenance.as_ref().unwrap();
        assert_eq!(provenance.model_id, BRIGHT_STAR_MODEL_ID_COMBINED);
        assert_eq!(
            provenance.product_band,
            BRIGHT_STAR_PRODUCT_BAND_COMBINED_ID
        );
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.flux_ph_m2_s.value())
                .sum::<f64>(),
            5.0
        );
        assert_eq!(
            shard
                .pixels
                .values()
                .map(|pixel| pixel.flux_300_336_ph_m2_s.value())
                .sum::<f64>(),
            1.0
        );
        // Combined artifact must fail closed against measured product requests.
        assert!(bright_star_supplement_shard(
            &artifact,
            &sha,
            1,
            StarlightProductBand::Measured336To650,
            None,
        )
        .is_err());
        Ok(())
    }
}
