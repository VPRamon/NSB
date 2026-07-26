use std::fs;
use std::path::{Path, PathBuf};

const SERVICE_COMMANDS: &[&str] = &[
    "audit_gaia_starlight_exclusions",
    "build_integrated_starlight_product",
    "build_starlight_map",
    "consolidate_gaia_starlight_samples",
    "generate_gaia_starlight_release_inputs",
    "generate_starlight_sample_queries",
    "index_gaia_xp_continuous_bulk",
    "normalize_xp_continuous_coefficients",
    "pack_starlight_asset",
    "prepare_gaia_starlight_catalogue",
    "prepare_tycho_starlight_catalogue",
    "query_gaia_tap",
    "sweep_starlight_nside",
    "train_starlight_photometry_models",
    "validate_starlight_map",
    "validate_xp_continuous_reconstruction",
    "verify_assets",
];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn every_registered_command_has_a_thin_adapter() {
    let manifest = read(crate_root().join("Cargo.toml"));
    for name in SERVICE_COMMANDS {
        assert!(
            manifest.contains(&format!("name = \"{name}\"")),
            "service command {name} is absent from Cargo.toml"
        );
        let path = crate_root().join(format!("src/bin/{name}.rs"));
        let source = read(&path);
        assert!(
            source.lines().count() < 30,
            "{} owns command behavior instead of adapting a service",
            path.display()
        );
        assert!(source.contains("tool_logging::init_from_env"));
        assert!(source.contains(&format!("tool_services::{name}::run_cli")));
        for forbidden in [
            "clap::Parser",
            "serde_json::Value",
            "Command::new",
            "struct Args",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} reintroduced executable-owned behavior {forbidden:?}",
                path.display()
            );
        }

        let service = read(crate_root().join(format!("src/tool_services/{name}.rs")));
        assert!(
            service.contains("pub fn run_cli") || service.contains("pub async fn run_cli"),
            "service {name} does not expose a documented CLI entrypoint"
        );
    }
}

#[test]
fn continuous_bulk_binary_remains_a_thin_adapter() {
    let source = read(crate_root().join("src/bin/download_gaia_xp_continuous_bulk.rs"));
    assert!(
        source.lines().count() < 100,
        "supported binary grew orchestration logic"
    );
    assert!(source.contains("run_continuous_bulk_download"));
    assert!(!source.contains("BulkDownloader"));
    assert!(!source.contains("serde_json::Value"));
    assert!(!source.contains("Command::new"));
}

#[test]
fn typed_pipeline_boundary_rejects_legacy_orchestration_patterns() {
    let pipeline_root = crate_root().join("src/pipeline");
    let forbidden = [
        "row_limit == 0",
        "unwrap_or(args.skip",
        "advance_after_mini_pilot",
        "serde_json::Value",
        "Command::new(\"cargo\")",
    ];
    for filename in ["admission.rs", "checkpoint.rs", "contracts.rs", "state.rs"] {
        let path = pipeline_root.join(filename);
        let source = read(&path);
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{} reintroduced forbidden pattern {pattern:?}",
                path.display()
            );
        }
    }
}

#[test]
fn temporary_operational_artifacts_are_not_committed() {
    for path in [
        ".github/diagnostics/thin-transform.log",
        ".github/workflows/issue-60-apply-thin.yml",
        ".github/workflows/issue-60-thin-binaries.yml",
    ] {
        assert!(
            !crate_root().join("../..").join(path).exists(),
            "temporary operational artifact {path} is committed"
        );
    }
}

#[test]
fn architecture_document_covers_release_and_resume_contracts() {
    let source = read(crate_root().join("../../docs/specifications/data-product-pipeline.md"));
    for required in [
        "RowSelection::FullPartition",
        "ProductionAdmission::evaluate",
        "PartitionState::resume_action",
        "Releasable",
        "schema_version = 1",
        "thin executable adapter",
    ] {
        assert!(
            source.contains(required),
            "architecture document is missing {required:?}"
        );
    }
}
