//! End-to-end fixture exercising the integrated 300–650 nm Starlight pipeline:
//! normalized contributions → integrated product → validation → pack → runtime load.

use nsb::{StarlightMap, StarlightProvenance};
use nsb_data_tools::starlight_integrated::INTEGRATED_PHOTOMETRY_MODEL;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONTRIBUTION_HEADER: &str = "source_or_bin_id,healpix_index,multiplicity,measured_300_650,inferred_300_650,completeness_correction,statistical_uncertainty,systematic_uncertainty,flags_extrapolation,flags_crowding,branch\n";

fn cargo_bin(bin: &str, args: &[&str]) {
    let status = Command::new("cargo")
        .args([
            "run",
            "--locked",
            "-p",
            "nsb-data-tools",
            "--bin",
            bin,
            "--",
        ])
        .args(args)
        .status()
        .unwrap_or_else(|err| panic!("failed to spawn {bin}: {err}"));
    assert!(status.success(), "{bin} failed with {status}");
}

fn write_contributions(dir: &Path) -> PathBuf {
    let csv = dir.join("contributions.csv");
    fs::write(
        &csv,
        format!(
            "{CONTRIBUTION_HEADER}\
             s0,0,2,10,1,0.5,2,1,true,false,xp_sampled\n\
             s1,11,1,5,0,1,3,0.5,false,true,no_xp\n"
        ),
    )
    .expect("write contributions");
    let checksum = nsb_data_tools::checksum_io::sha256_file(&csv).expect("checksum");
    let manifest = dir.join("inputs.toml");
    fs::write(
        &manifest,
        format!(
            "schema_version = 1\nrelease_id = \"pipeline-fixture-v1\"\nmodel_checksum = \"sha256:{}\"\n\n[[inputs]]\npath = \"contributions.csv\"\nsha256 = \"{}\"\n",
            "a".repeat(64),
            checksum.trim_start_matches("sha256:")
        ),
    )
    .expect("write manifest");
    manifest
}

fn write_reference(path: &Path) {
    fs::write(
        path,
        r#"{
  "schema_version": 1,
  "production_use": true,
  "band_nm": [300.0, 650.0],
  "units": "ph cm-2 ns-1 sr-1",
  "regions": [
    {
      "name": "fixture",
      "frame": "galactic",
      "l_deg": 30.0,
      "b_deg": 0.0,
      "aperture_deg": 90.0,
      "expected_min": 1.0e-15,
      "expected_max": 1.0e-9,
      "source": "https://example.invalid/pipeline-fixture"
    }
  ]
}
"#,
    )
    .expect("write reference");
}

#[test]
fn integrated_pipeline_fixture_runs_end_to_end() {
    let dir = tempfile::tempdir().expect("tempdir");
    let build_dir = dir.path().join("build");
    let manifest = write_contributions(dir.path());
    let reference = dir.path().join("reference.json");
    write_reference(&reference);
    let model_checksum = format!("sha256:{}", "a".repeat(64));

    fs::create_dir_all(&build_dir).expect("build dir");
    cargo_bin(
        "build_integrated_starlight_product",
        &[
            "--inputs-manifest",
            manifest.to_str().unwrap(),
            "--nside",
            "1",
            "--release-id",
            "pipeline-fixture-v1",
            "--model-checksum",
            &model_checksum,
            "--output-dir",
            build_dir.to_str().unwrap(),
            "--candidate-only",
        ],
    );

    let mean_map = build_dir.join("starlight_mean.release.csv");
    let diagnostics = build_dir.join("starlight_source_contributions.diagnostics.json");
    let validation = build_dir.join("validation.json");
    let release = build_dir.join("starlight_map.release.csv");
    let runtime_manifest = build_dir.join("starlight_map.release.toml");

    cargo_bin(
        "validate_starlight_map",
        &[
            "--input",
            mean_map.to_str().unwrap(),
            "--diagnostics",
            diagnostics.to_str().unwrap(),
            "--reference",
            reference.to_str().unwrap(),
            "--output",
            validation.to_str().unwrap(),
        ],
    );

    cargo_bin(
        "pack_starlight_asset",
        &[
            "--input",
            mean_map.to_str().unwrap(),
            "--diagnostics",
            diagnostics.to_str().unwrap(),
            "--validation",
            validation.to_str().unwrap(),
            "--output",
            release.to_str().unwrap(),
            "--manifest",
            runtime_manifest.to_str().unwrap(),
            "--candidate",
        ],
    );

    let release_raw = fs::read_to_string(&release).expect("release csv");
    assert!(release_raw.contains("integrated_ph_cm2_ns_sr"));
    assert!(release_raw.contains("statistical_uncertainty_ph_cm2_ns_sr"));
    assert!(release_raw.contains(INTEGRATED_PHOTOMETRY_MODEL));

    let map = StarlightMap::from_csv_str(&release_raw, StarlightProvenance::test_fixture())
        .expect("runtime load");
    assert_eq!(map.pixels().len(), 12);
    assert!(map
        .pixels()
        .iter()
        .any(|pixel| pixel.statistical_uncertainty.is_some()));
}
