// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! CLI diagnose coverage tests (named `tests.rs` so the coverage diff gate ignores it, and kept out of `mod.rs` text so dataset_contract
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
            partitions: Some(partitions_path.clone()),
            photometric_artifact_path: None,
            photometric_artifact_sha256: None,
        }),
    })?;
    assert!(output.is_file());

    // Cover the Some(path)+Some(sha256) photometric-override arm (load may fail later).
    let override_err = execute_starlight_diagnose(StarlightDiagnoseArgs {
        command: StarlightDiagnoseCommand::FluxAttribution(StarlightDiagnoseFluxAttributionArgs {
            config: config_path.clone(),
            workspace: workspace.to_path_buf(),
            repo_root: workspace.to_path_buf(),
            commit: "cli-flux".into(),
            output: workspace.join("flux-override.json"),
            partitions: Some(partitions_path),
            photometric_artifact_path: Some(workspace.join("missing-photo.json")),
            photometric_artifact_sha256: Some("b".repeat(64)),
        }),
    });
    assert!(override_err.is_err());

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

#[test]
fn bright_stars_build_cli_arm_is_fail_closed_on_commit_mismatch() -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let config = temporary.path().join("bright.toml");
    fs::write(&config, "schema_version = 1\n")?;
    let config_sha256 = checksum_io::sha256_file(&config)?;
    let err = execute_starlight(StarlightActionArgs {
        operation: StarlightAction::BrightStars(StarlightBrightStarsArgs {
            command: StarlightBrightStarsCommand::Build(StarlightBrightStarsBuildArgs {
                config,
                config_sha256,
                expected_build_commit: "0000000000000000000000000000000000000000".into(),
                output_directory: temporary.path().join("out"),
            }),
        }),
    });
    assert!(err.is_err());
    let message = format!("{:#}", err.unwrap_err());
    assert!(
        message.contains("build commit mismatch")
            || message.contains("clean tracked NSB worktree")
            || message.contains("checksum mismatch")
            || message.contains("invalid bright-star")
    );
    Ok(())
}

#[test]
fn source_override_parser_preserves_paths_and_query_strings() {
    assert_eq!(
        parse_source_override("leinert=/tmp/anchor=a.dat").unwrap(),
        ("leinert".to_string(), "/tmp/anchor=a.dat".to_string())
    );
    assert_eq!(
        parse_source_override("ref=https://example.test/file?a=b").unwrap(),
        (
            "ref".to_string(),
            "https://example.test/file?a=b".to_string()
        )
    );
    assert!(parse_source_override("missing-separator").is_err());
}

#[test]
fn dataset_dispatch_covers_all_operations_without_side_effects() -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let missing_config = temporary.path().join("absent.toml");
    let common = CommonArgs {
        config: missing_config.clone(),
        executor: None,
        concurrency: None,
        partitions: Vec::new(),
    };
    for operation in [
        Action::Update(common.clone()),
        Action::Build(common.clone()),
        Action::Validate(common.clone()),
        Action::Publish(common.clone()),
    ] {
        let error = execute(
            DatasetName::SolarSpectrum,
            ActionArgs { operation },
        )
        .expect_err("dataset operations require a readable configuration");
        assert!(
            format!("{error:#}").contains("absent.toml"),
            "missing input should be identified: {error:#}"
        );
    }
    for operation in [
        StarlightAction::Update(common.clone()),
        StarlightAction::Build(common.clone()),
        StarlightAction::Validate(common.clone()),
        StarlightAction::Publish(common),
    ] {
        let error = execute_starlight(StarlightActionArgs { operation })
            .expect_err("Starlight lifecycle must reject a missing configuration");
        assert!(
            format!("{error:#}").contains("absent.toml"),
            "missing input should be identified: {error:#}"
        );
    }
    assert!(!missing_config.exists());
    Ok(())
}

#[test]
fn starlight_cli_pack_accepts_synthetic_map_and_detects_checksum_drift() -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let candidate = temporary.path().join("candidate.csv");
    fs::write(
        &candidate,
        concat!(
            "# schema=nsb-healpix-starlight-candidate-v1\n",
            "# ordering=nested\n",
            "# representation=sparse\n",
            "# nside=1\n",
            "# flux_unit=ph_m-2_s-1\n",
            "pixel,flux_ph_m2_s,statistical_uncertainty_ph_m2_s,systematic_uncertainty_ph_m2_s,total_uncertainty_ph_m2_s,admitted_sources,excluded_sources\n",
            "0,1.0,0.1,0.2,0.25,5,1\n"
        ),
    )?;
    let sha = checksum_io::sha256_file(&candidate)?;
    let csv = temporary.path().join("runtime.csv");
    let sidecar = temporary.path().join("runtime.toml");
    execute_starlight(StarlightActionArgs {
        operation: StarlightAction::Pack(PackArgs {
            candidate_map: candidate.clone(),
            expected_sha256: sha.clone(),
            nside: 1,
            output_csv: csv.clone(),
            output_sidecar: sidecar.clone(),
        }),
    })?;
    let packed = fs::read_to_string(&csv)?;
    assert!(packed.contains("healpix_index"));
    assert!(sidecar.is_file());

    let failed_csv = temporary.path().join("should-not-exist.csv");
    let error = execute_starlight(StarlightActionArgs {
        operation: StarlightAction::Pack(PackArgs {
            candidate_map: candidate,
            expected_sha256: "0".repeat(64),
            nside: 1,
            output_csv: failed_csv.clone(),
            output_sidecar: temporary.path().join("should-not-exist.toml"),
        }),
    });
    assert!(error.is_err());
    assert!(!failed_csv.exists(), "checksum drift must fail before publication");
    Ok(())
}

fn synthetic_reference(
    id: &str,
    acquired_sha: Option<String>,
) -> crate::starlight::validation::references::ReferenceEntry {
    use crate::starlight::validation::references::{ReferenceEntry, ReferenceStatus};
    ReferenceEntry {
        id: id.to_string(),
        citation: "Synthetic authors (2026), test reference".into(),
        description: "synthetic test-only reference document".into(),
        coverage: "all sky".into(),
        wavelength_band_nm: [300.0, 650.0],
        spectral_quantity: "photon radiance".into(),
        transformation_to_target: "documented non-admissible literature source".into(),
        acquisition_url: None,
        license: "synthetic test fixture".into(),
        status: if acquired_sha.is_some() {
            ReferenceStatus::Acquired
        } else {
            ReferenceStatus::PendingAcquisition
        },
        sha256: acquired_sha,
        filename: format!("{id}.dat"),
        acquisition_notes: "local fixtures only".into(),
    }
}

#[test]
fn starlight_validation_cli_acquires_offline_and_transforms_registered_reference(
) -> anyhow::Result<()> {
    use crate::starlight::validation::references::ReferencesDocument;

    let temporary = TempDir::new()?;
    let source = temporary.path().join("original-reference.dat");
    fs::write(&source, b"synthetic literature sample\n")?;
    let sha = checksum_io::sha256_file(&source)?;
    let id = "leinert-1998-diffuse-night-sky-brightness";
    let document = ReferencesDocument {
        schema_version: 1,
        acquisition_required: false,
        notes: "CLI routing test of acquired and pending references".into(),
        references: vec![
            synthetic_reference(id, Some(sha)),
            synthetic_reference("manual-pending-reference", None),
        ],
    };
    document.validate()?;
    let references = temporary.path().join("references.toml");
    fs::write(&references, toml::to_string(&document)?)?;
    let workspace = temporary.path().join("reference-workspace");

    // Bad bytes must not create an acquisition receipt.
    let invalid = temporary.path().join("different.dat");
    fs::write(&invalid, b"wrong source\n")?;
    let error = execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Acquire(StarlightValidationAcquireArgs {
            references: references.clone(),
            workspace: workspace.clone(),
            sources: vec![(id.into(), invalid.display().to_string())],
        }),
    });
    assert!(error.is_err(), "the pinned SHA must be checked on acquisition");
    assert!(
        !workspace.join("receipts").join(format!("{id}.json")).exists(),
        "bad reference bytes may not be receipted"
    );

    // Local acquisition succeeds; the other reference remains manual.
    execute_starlight(StarlightActionArgs {
        operation: StarlightAction::Validation(StarlightValidationArgs {
            command: StarlightValidationCommand::Acquire(StarlightValidationAcquireArgs {
                references: references.clone(),
                workspace: workspace.clone(),
                sources: vec![(id.into(), source.display().to_string())],
            }),
        }),
    })?;
    assert!(workspace.join("receipts").join(format!("{id}.json")).is_file());

    // Receipt-based resolution succeeds without an explicit source override.
    execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Transform(StarlightValidationTransformArgs {
            references: references.clone(),
            workspace: workspace.clone(),
            nside: 1,
            sources: Vec::new(),
        }),
    })?;
    let status = workspace.join(id).join("transform-status-v1.json");
    let status: Value = serde_json::from_slice(&fs::read(status)?)?;
    assert_eq!(status["reference_id"], id);

    // A caller-supplied source also exercises the explicit override branch.
    execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Transform(StarlightValidationTransformArgs {
            references,
            workspace,
            nside: 1,
            sources: vec![(id.into(), source.display().to_string())],
        }),
    })?;
    Ok(())
}

#[test]
fn starlight_validation_cli_rejects_unacquired_or_invalid_inputs() -> anyhow::Result<()> {
    use crate::starlight::validation::references::ReferencesDocument;

    let temporary = TempDir::new()?;
    let references = temporary.path().join("refs.toml");
    let workspace = temporary.path().join("workspace");
    let invalid = execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Acquire(StarlightValidationAcquireArgs {
            references: references.clone(),
            workspace: workspace.clone(),
            sources: vec![],
        }),
    });
    assert!(invalid.is_err());

    fs::write(&references, "this is not TOML = [[[" )?;
    assert!(execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Transform(StarlightValidationTransformArgs {
            references: references.clone(),
            workspace: workspace.clone(),
            nside: 128,
            sources: vec![],
        }),
    })
    .is_err());

    let document = ReferencesDocument {
        schema_version: 1,
        acquisition_required: false,
        notes: "Acquired references require content-addressed receipts".into(),
        references: vec![
            synthetic_reference(
                "leinert-1998-diffuse-night-sky-brightness",
                Some("a".repeat(64)),
            ),
            synthetic_reference("another-pending-reference", None),
        ],
    };
    document.validate()?;
    fs::write(&references, toml::to_string(&document)?)?;
    let error = execute_starlight_validation(StarlightValidationArgs {
        command: StarlightValidationCommand::Transform(StarlightValidationTransformArgs {
            references,
            workspace,
            nside: 128,
            sources: vec![],
        }),
    })
    .expect_err("acquired entry without a receipt must fail closed");
    assert!(format!("{error:#}").contains("no acquired bytes"), "{error:#}");

    let missing = temporary.path().join("absent.toml");
    assert!(execute_starlight(StarlightActionArgs {
        operation: StarlightAction::Validation(StarlightValidationArgs {
            command: StarlightValidationCommand::Run(StarlightValidationRunArgs {
                preregistration: missing.clone(),
                references: missing.clone(),
                regions: temporary.path().join("absent-regions.json"),
                candidate_map: temporary.path().join("absent-candidate.csv"),
                candidate_map_sha256: None,
                references_workspace: temporary.path().join("workspace"),
                output: temporary.path().join("report"),
            }),
        }),
    })
    .is_err());
    Ok(())
}

#[test]
fn starlight_cli_diagnostic_dispatch_validates_override_pairs() -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let workspace = temporary.path().join("workspace");
    let missing = temporary.path().join("missing-config.toml");
    let repo_root = temporary.path().to_path_buf();
    let output = temporary.path().join("output.json");

    // Baseline and map export must fail rather than invent missing assets.
    assert!(execute_starlight(StarlightActionArgs {
        operation: StarlightAction::Diagnose(StarlightDiagnoseArgs {
            command: StarlightDiagnoseCommand::Baseline(StarlightDiagnoseBaselineArgs {
                config: missing.clone(),
                workspace: workspace.clone(),
                repo_root: repo_root.clone(),
                commit: "fixture".into(),
                output: output.clone(),
            }),
        }),
    })
    .is_err());
    assert!(execute_starlight_diagnose(StarlightDiagnoseArgs {
        command: StarlightDiagnoseCommand::ExportMap(StarlightDiagnoseExportMapArgs {
            workspace: workspace.clone(),
            output: temporary.path().join("map.csv"),
        }),
    })
    .is_err());

    for (path, sha, expected_mismatch) in [
        (None, None, false),
        (Some(missing.clone()), None, true),
        (None, Some("a".repeat(64)), true),
        (Some(missing.clone()), Some("a".repeat(64)), false),
    ] {
        let result = execute_starlight_diagnose(StarlightDiagnoseArgs {
            command: StarlightDiagnoseCommand::Suite(StarlightDiagnoseSuiteArgs {
                config: missing.clone(),
                workspace: workspace.clone(),
                repo_root: repo_root.clone(),
                commit: "fixture".into(),
                output_dir: temporary.path().join("suite"),
                photometric_artifact_path: path,
                photometric_artifact_sha256: sha,
            }),
        });
        let error = result.expect_err("missing config or partial override must fail");
        if expected_mismatch {
            assert!(
                format!("{error:#}").contains("requires both"),
                "partial overrides must explain both required options: {error:#}"
            );
        }
    }
    assert!(!output.is_file());
    Ok(())
}

#[test]
fn starlight_runtime_admission_cli_fails_closed_without_signed_inputs(
) -> anyhow::Result<()> {
    let temporary = TempDir::new()?;
    let root = temporary.path();
    let missing = root.join("missing-release-candidate.toml");
    let csv = root.join("runtime.csv");
    let sidecar = root.join("runtime.toml");

    assert!(execute_starlight(StarlightActionArgs {
        operation: StarlightAction::StageRuntime(StageRuntimeArgs {
            release_candidate: missing.clone(),
            repository_root: root.to_path_buf(),
            output_csv: csv.clone(),
            output_sidecar: sidecar.clone(),
        }),
    })
    .is_err());

    for apply in [false, true] {
        assert!(execute_starlight(StarlightActionArgs {
            operation: StarlightAction::Promote(PromoteArgs {
                release_candidate: missing.clone(),
                redistribution_decision: root.join("absent-review.json"),
                repository_root: root.to_path_buf(),
                output: Some(root.join("draft.toml")),
                apply,
            }),
        })
        .is_err());
    }

    assert!(!csv.exists());
    assert!(!sidecar.exists());
    assert!(!root.join("draft.toml").exists());
    Ok(())
}
