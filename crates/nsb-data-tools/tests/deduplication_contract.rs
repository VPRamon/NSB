use nsb_data_tools::scientific_contract::{
    authoritative_gaia_xp_photon_contract, gaia_xp_photon_contract, gaia_xp_photon_contracts_match,
};
use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("read source directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                visit(&path, files);
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    visit(root, &mut files);
    files
}

#[test]
fn generated_scientific_contract_matches_rust_authority() {
    assert!(gaia_xp_photon_contracts_match(
        gaia_xp_photon_contract(),
        &authoritative_gaia_xp_photon_contract(),
    ));
}

#[test]
fn migration_python_does_not_reimplement_photon_integration() {
    let root = repository_root();
    for relative in [
        "tools/starlight-xp-continuous/reconstruct_and_integrate.py",
        "tools/starlight-xp-continuous/phase5b_gaiaxpy_flux_validate.py",
    ] {
        let text = fs::read_to_string(root.join(relative)).expect("migration script");
        assert!(text.contains("gaia_xp_photon_integration_v1.json"));
        for forbidden in [
            "6.62607015e-34",
            "299792458.0",
            "BAND_MIN_NM =",
            "BAND_MAX_NM =",
            "GRID_STEP_NM =",
            "np.trapezoid",
            "np.trapz",
            "photon_energy_j",
        ] {
            assert!(
                !text.contains(forbidden),
                "{relative} independently defines forbidden scientific logic {forbidden:?}"
            );
        }
    }
}

#[test]
fn canonical_xp_continuous_schema_has_no_legacy_parallel_model() {
    let root = repository_root();
    let text = fs::read_to_string(root.join("crates/nsb-data-tools/src/gaia_xp_continuous.rs"))
        .expect("XP continuous module");
    assert!(!text.contains("ContinuousCoefficients"));
    assert!(!text.contains("canonical_to_legacy"));
}

#[test]
fn production_rust_has_one_sha256_file_implementation() {
    let root = repository_root().join("crates/nsb-data-tools/src");
    let definitions = rust_files(&root)
        .into_iter()
        .filter_map(|path| {
            let text = fs::read_to_string(&path).ok()?;
            let count = text.matches("fn sha256_file(").count();
            (count > 0).then_some((path, count))
        })
        .collect::<Vec<_>>();
    assert_eq!(definitions.len(), 1, "SHA-256 helpers: {definitions:?}");
    assert!(definitions[0].0.ends_with("checksum_io.rs"));
    assert_eq!(definitions[0].1, 1);
}

#[test]
fn retained_data_tools_do_not_spawn_sibling_cargo_binaries() {
    let root = repository_root().join("crates/nsb-data-tools/src");
    for path in rust_files(&root) {
        let text = fs::read_to_string(&path).expect("Rust source");
        assert!(
            !text.contains("Command::new(\"cargo\")"),
            "{} launches a sibling workspace binary through cargo",
            path.display()
        );
    }
}
