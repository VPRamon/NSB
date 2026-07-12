use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
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
fn architecture_document_covers_release_and_resume_contracts() {
    let source = read(crate_root().join("../../docs/DATA_PRODUCT_PIPELINE_ARCHITECTURE.md"));
    for required in [
        "RowSelection::FullPartition",
        "ProductionAdmission::evaluate",
        "PartitionState::resume_action",
        "Releasable",
        "schema_version = 1",
    ] {
        assert!(
            source.contains(required),
            "architecture document is missing {required:?}"
        );
    }
}
