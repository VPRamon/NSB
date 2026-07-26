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

fn files_containing(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    collect_files_containing(root, root, pattern, &mut matches);
    matches.sort();
    matches
}

fn collect_files_containing(
    root: &Path,
    directory: &Path,
    pattern: &str,
    matches: &mut Vec<PathBuf>,
) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
    {
        let path = entry.expect("read source entry").path();
        let metadata = fs::symlink_metadata(&path)
            .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()));
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files_containing(root, &path, pattern, matches);
        } else if metadata.is_file() {
            let source = read(&path);
            if source.contains(pattern) {
                matches.push(
                    path.strip_prefix(root)
                        .expect("source path must be below root")
                        .to_path_buf(),
                );
            }
        }
    }
}

#[test]
fn hierarchical_cli_is_the_only_thin_adapter() {
    let manifest = read(crate_root().join("Cargo.toml"));
    assert_eq!(manifest.matches("[[bin]]").count(), 1);
    let path = crate_root().join("src/bin/nsb-data.rs");
    let source = read(&path);
    assert!(source.lines().count() < 10);
    assert!(source.contains("nsb_data_tools::cli::run"));
    let cli = read(crate_root().join("src/cli/mod.rs"));
    assert!(cli.contains("starlight map build"));
    assert!(cli.contains("RenderToolReference"));
}

#[test]
fn flat_or_legacy_module_surfaces_are_absent() {
    let source_root = crate_root().join("src");
    let library = read(source_root.join("lib.rs"));
    for forbidden in [
        "pub mod gaia_bulk",
        "pub mod gaia_usb_cache",
        "pub mod gaia_xp_continuous",
        "pub mod checksum_io",
        "pub mod artifact_io",
    ] {
        assert!(
            !library.contains(forbidden),
            "legacy public alias remains: {forbidden:?}"
        );
    }

    for file in [
        "gaia_xp.rs",
        "gaia_xp_continuous.rs",
        "gaia_tap.rs",
        "starlight_sampling.rs",
        "tool_services",
        "domains",
        "provenance.rs",
    ] {
        assert!(
            !source_root.join(file).exists(),
            "flat or obsolete source surface {file:?} remains"
        );
    }
}

#[test]
fn typed_pipeline_boundary_rejects_legacy_orchestration_patterns() {
    let pipeline_root = crate_root().join("src/platform/pipeline");
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
