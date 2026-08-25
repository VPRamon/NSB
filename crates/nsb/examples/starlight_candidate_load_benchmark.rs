//! Timing harness for packed Starlight runtime maps.
//!
//! Architecture: candidate-v5 → `dataset starlight pack` → `.release.csv` →
//! `StarlightMap` runtime load. This example never loads the sparse
//! `nsb-healpix-starlight-candidate-v5` candidate CSV through the runtime API.
//!
//! Historical wall-clock numbers in
//! `docs/nsb_components/starlight/production-runs/performance-v1.json` remain
//! frozen evidence for issue #90 and are not rewritten by this harness.

use nsb::{StarlightMap, StarlightProvenance};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn packed_nside1_fixture() -> String {
    let mut raw = String::from(concat!(
        "# map_type=healpix\n",
        "# nside=1\n",
        "# ordering=ring\n",
        "# coordinate_frame=galactic\n",
        "# s10_diagnostics=not_provided\n",
        "healpix_index,integrated_ph_cm2_ns_sr,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    for index in 0..12 {
        let integrated = index as f64 + 1.0;
        raw.push_str(&format!(
            "{index},{integrated},{stat},{sys},{tot}\n",
            stat = integrated * 0.1,
            sys = integrated * 0.2,
            tot = integrated * 0.25
        ));
    }
    raw
}

fn main() {
    let provenance = StarlightProvenance::new(
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        "benchmark-harness-only",
        None::<String>,
    );

    // Always time a dense packed fixture so the harness works without a local
    // packed production artifact.
    let fixture = packed_nside1_fixture();
    let start = Instant::now();
    let map = StarlightMap::from_csv_str(&fixture, provenance.clone())
        .expect("packed nside=1 fixture loads");
    let fixture_elapsed = start.elapsed();
    println!("packed_fixture_load_result=ok");
    println!("packed_fixture_pixel_count={}", map.pixels().len());
    println!(
        "packed_fixture_load_seconds={:.6}",
        fixture_elapsed.as_secs_f64()
    );

    let path = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/starlight_nside128.release.csv")
    });
    println!("packed_runtime_path={}", path.display());
    if !path.is_file() {
        println!("packed_runtime_load_result=missing");
        println!(
            "packed_runtime_load_hint=run `nsb-data dataset starlight pack` first, then pass the .release.csv path"
        );
        return;
    }

    let file_size_bytes = std::fs::metadata(&path)
        .unwrap_or_else(|error| panic!("failed to stat {}: {error}", path.display()))
        .len();
    println!("packed_runtime_file_size_bytes={file_size_bytes}");

    let start = Instant::now();
    match StarlightMap::from_csv_path(&path, provenance) {
        Ok(map) => {
            let elapsed = start.elapsed();
            println!("packed_runtime_load_result=ok");
            println!("packed_runtime_pixel_count={}", map.pixels().len());
            println!("packed_runtime_load_seconds={:.6}", elapsed.as_secs_f64());
        }
        Err(error) => {
            println!("packed_runtime_load_result=error");
            println!("packed_runtime_load_error={error}");
        }
    }
}
