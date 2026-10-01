//! CLI diagnose coverage tests (kept out of `mod.rs` so dataset_contract
//! substring bans on `cli/mod.rs` do not fire on fixture paths).

use super::*;
use crate::platform::{artifact_store, checksum_io};
use crate::starlight::config::{GaiaProductConfig, OfficialChecksumAlgorithm};
use crate::starlight::healpix::test_support::fixture_icrs_from_source_id;
use crate::starlight::map::accumulator::PartitionShard;
use crate::starlight::sources::acquisition::AcquisitionReceipt;
use crate::starlight::sources::inventory::{SourceInventory, SourceInventoryEntry};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn gzip_bytes(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
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
) -> anyhow::Result<()> {
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

#[test]
fn diagnose_export_measured_336_650_arm() -> anyhow::Result<()> {
    use crate::starlight::config::StarlightProductBand;
    use crate::starlight::map::accumulator::UvCorrectionShardMetadata;
    use crate::starlight::map::product::{emit_maps, MergeReport};
    use crate::starlight::uv::{
        ApplicabilityStatus, CalibrationStatus, CombinedBandFlux, EvaluationDecision,
        ModelResponse, SystematicCorrelation,
    };

    let temporary = TempDir::new()?;
    let workspace = temporary.path();
    const UV_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let metadata = UvCorrectionShardMetadata {
        model_id: "fixture-cli-export".to_string(),
        artifact_sha256: UV_SHA.to_string(),
        calibration_status: CalibrationStatus::Validated,
        response: ModelResponse::AbsoluteUvPhotonFlux,
        measured_conditional_residual_statistical_correlation_bits: 0.0_f64.to_bits(),
        systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
    };
    let mut shard = PartitionShard::new_with_policy(
        "cli-export",
        1,
        StarlightProductBand::Combined300To650,
        Some(metadata),
    )?;
    shard.admit_corrected(
        fixture_icrs_from_source_id(0),
        &CombinedBandFlux {
            flux_300_336_ph_m2_s: 1.0,
            flux_336_650_ph_m2_s: 10.0,
            flux_300_650_ph_m2_s: 11.0,
            statistical_uncertainty_300_336_ph_m2_s: 0.1,
            statistical_uncertainty_336_650_ph_m2_s: 0.2,
            statistical_uncertainty_300_650_ph_m2_s: 0.3,
            systematic_uncertainty_300_336_ph_m2_s: 0.0,
            systematic_uncertainty_300_650_ph_m2_s: 0.0,
            applicability_status: ApplicabilityStatus::InDomain,
            decision: EvaluationDecision::Applied,
            model_id: "fixture-cli-export".to_string(),
            artifact_sha256: UV_SHA.to_string(),
            systematic_correlation: SystematicCorrelation::IndependentBetweenSources,
        },
    )?;
    let shard_dir = workspace.join("outputs/shards");
    fs::create_dir_all(&shard_dir)?;
    shard.write(&shard_dir.join("cli-export.json"))?;
    emit_maps(
        workspace,
        &["cli-export".to_string()],
        1,
        StarlightProductBand::Combined300To650,
        Some(UV_SHA),
        None,
    )?;
    let merge: MergeReport =
        serde_json::from_slice(&fs::read(workspace.join("outputs/merge_report.json"))?)?;
    let output = workspace.join("measured.csv");
    let provenance = workspace.join("measured.json");
    execute_starlight_diagnose(StarlightDiagnoseArgs {
        command: StarlightDiagnoseCommand::ExportMeasured336650(
            StarlightDiagnoseExportMeasuredArgs {
                workspace: workspace.to_path_buf(),
                output: output.clone(),
                provenance: Some(provenance.clone()),
                parent_combined_sha256: Some(merge.canonical_map.sha256.clone()),
                source_commit: None,
            },
        ),
    })?;
    assert!(output.is_file());
    assert!(provenance.is_file());
    Ok(())
}

#[test]
fn diagnose_flux_attribution_arm_and_override_mismatch() -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let workspace = temporary.path();
    let partition = "000000-003111";
    let oracle: Value = serde_json::from_slice(&fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/gaiaxpy_oracle/record-01.json"),
    )?)?;
    let source_id = oracle["source_id"].as_str().unwrap();
    let gaia_bytes = gzip_bytes(format!("source_id,ra,dec\n{source_id},45.0,20.0\n").as_bytes())?;
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
        &oracle["bp_standard_deviation"]
            .as_f64()
            .unwrap()
            .to_string(),
        "55",
        &oracle["rp_standard_deviation"]
            .as_f64()
            .unwrap()
            .to_string(),
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
    let config_path = workspace.join("config.toml");
    let mut products_toml = String::new();
    for product in &products {
        products_toml.push_str(&format!(
            r#"
[[starlight.gaia_products]]
id = "{id}"
base_url = "{base_url}"
checksum_manifest_url = "{checksum_manifest_url}"
checksum_manifest_sha256 = "{checksum_manifest_sha256}"
checksum_algorithm = "md5"
expected_partitions = 1
filename_prefix = "{filename_prefix}"
filename_suffix = "{filename_suffix}"
"#,
            id = product.id,
            base_url = product.base_url,
            checksum_manifest_url = product.checksum_manifest_url,
            checksum_manifest_sha256 = product.checksum_manifest_sha256,
            filename_prefix = product.filename_prefix,
            filename_suffix = product.filename_suffix,
        ));
    }
    fs::write(
        &config_path,
        format!(
            r#"schema_version = 1
dataset = "starlight"
[workspace]
root = "{workspace}"
[starlight]
product_band = "combined-300-650"
[starlight.map]
canonical_nside = 128
{products_toml}
"#,
            workspace = workspace.display(),
        ),
    )?;
    let partitions_path = workspace.join("partitions.txt");
    fs::write(&partitions_path, format!("{partition}\n"))?;
    let output = workspace.join("flux.json");
    execute_starlight_diagnose(StarlightDiagnoseArgs {
        command: StarlightDiagnoseCommand::FluxAttribution(StarlightDiagnoseFluxAttributionArgs {
            config: config_path.clone(),
            workspace: workspace.to_path_buf(),
            repo_root: workspace.to_path_buf(),
            commit: "cli-flux".into(),
            output: output.clone(),
            partitions: Some(partitions_path),
            photometric_artifact_path: None,
            photometric_artifact_sha256: None,
        }),
    })?;
    assert!(output.is_file());

    let mismatch = execute_starlight_diagnose(StarlightDiagnoseArgs {
        command: StarlightDiagnoseCommand::FluxAttribution(StarlightDiagnoseFluxAttributionArgs {
            config: config_path,
            workspace: workspace.to_path_buf(),
            repo_root: workspace.to_path_buf(),
            commit: "cli-flux".into(),
            output: workspace.join("flux-bad.json"),
            partitions: None,
            photometric_artifact_path: Some(workspace.join("missing.json")),
            photometric_artifact_sha256: None,
        }),
    });
    assert!(mismatch.is_err());
    assert!(format!("{:#}", mismatch.unwrap_err())
        .contains("both --photometric-artifact-path and --photometric-artifact-sha256"));
    Ok(())
}
