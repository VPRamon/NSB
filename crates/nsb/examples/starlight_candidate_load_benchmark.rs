//! Crude, reproducible local timing for the Starlight release-candidate map.
//!
//! This is not a scientific artifact, a Criterion benchmark, or a CI gate. It
//! exists to produce honest wall-clock numbers for
//! `docs/nsb_components/starlight/production-runs/performance-v1.json`
//! (issue #90) using existing parsing building blocks. It does not invent or
//! extrapolate timings.
//!
//! Two measurements are attempted:
//!
//! 1. The public runtime API, `StarlightMap::from_csv_path`. As of this
//!    writing this is expected to fail: the runtime parser only understands
//!    the `healpix_index,...` schema used by validated external/manual-seed
//!    maps, not the `nsb-healpix-starlight-candidate-v5` schema
//!    (`pixel,flux_ph_m2_s,...`) written by the Gaia production pipeline.
//!    `StarlightModel::BundledProductionGaiaDr3` (production registry pair) is expected to
//!    add runtime support for that schema; this harness records whatever the
//!    current behaviour actually is rather than assuming success.
//! 2. A crude proxy: reading the file and iterating every CSV row with the
//!    same `csv` crate machinery `nsb::components::starlight::map` already
//!    uses for the `healpix_index` schema (comment='#', full trim), without
//!    semantic validation. This approximates raw I/O + row-parsing cost only;
//!    it is not the validated production load path (`dataset starlight
//!    validate`, which additionally checksums the file and cross-checks
//!    `merge_report.json`).
//!
//! See `docs/specifications/performance.md` for the project's Criterion
//! methodology, which this intentionally does not replace.

use csv::ReaderBuilder;
use nsb::{StarlightMap, StarlightProvenance};
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let path = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/starlight_nside128.csv")
    });

    let file_size_bytes = std::fs::metadata(&path)
        .unwrap_or_else(|error| panic!("failed to stat {}: {error}", path.display()))
        .len();
    println!("path={}", path.display());
    println!("file_size_bytes={file_size_bytes}");

    // Measurement 1: the public runtime API, as-is.
    let harness_fallback = StarlightProvenance::new(
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
    let start = Instant::now();
    match StarlightMap::from_csv_path(&path, harness_fallback) {
        Ok(map) => {
            let elapsed = start.elapsed();
            println!("runtime_api_load_result=ok");
            println!("runtime_api_pixel_count={}", map.pixels().len());
            println!("runtime_api_load_seconds={:.6}", elapsed.as_secs_f64());
        }
        Err(error) => {
            println!("runtime_api_load_result=unsupported_schema");
            println!("runtime_api_load_error={error}");
        }
    }

    // Measurement 2: crude proxy timing (raw read + full CSV row iteration).
    let start = Instant::now();
    let raw = std::fs::read_to_string(&path).expect("candidate map is valid UTF-8");
    let read_elapsed = start.elapsed();

    let start = Instant::now();
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(raw.as_bytes());
    let mut row_count: u64 = 0;
    for record in reader.records() {
        record.expect("candidate map row is well-formed CSV");
        row_count += 1;
    }
    let parse_elapsed = start.elapsed();

    println!("proxy_read_seconds={:.6}", read_elapsed.as_secs_f64());
    println!("proxy_parse_seconds={:.6}", parse_elapsed.as_secs_f64());
    println!(
        "proxy_total_seconds={:.6}",
        (read_elapsed + parse_elapsed).as_secs_f64()
    );
    println!("proxy_row_count={row_count}");
}
