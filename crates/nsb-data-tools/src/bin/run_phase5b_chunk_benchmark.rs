//! Benchmark chunk sizes for Phase 5B bulk streaming mini-pilot.

use anyhow::Result;
use clap::Parser;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Parser)]
#[command(about = "Benchmark Gaia XP continuous bulk mini-pilot chunk sizes")]
struct Args {
    #[arg(long)]
    bulk_gz: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 500)]
    row_limit: usize,
    #[arg(long, default_value = "100,500,1000")]
    chunk_sizes: String,
    #[arg(long)]
    python: Option<PathBuf>,
    #[arg(long)]
    reconstruct_script: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ChunkBenchmarkRow {
    chunk_size: usize,
    sources_reconstructed: u64,
    sources_per_second: f64,
    wall_elapsed_seconds: f64,
    peak_rss_kib: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    let python = args
        .python
        .unwrap_or_else(|| PathBuf::from("tools/starlight-xp-continuous/.venv/bin/python"));
    let reconstruct_script = args.reconstruct_script.unwrap_or_else(|| {
        PathBuf::from("tools/starlight-xp-continuous/reconstruct_and_integrate.py")
    });
    let mut rows = Vec::new();
    for chunk in args
        .chunk_sizes
        .split(',')
        .filter_map(|value| value.trim().parse::<usize>().ok())
    {
        let out = args.output_dir.join(format!("chunk_{chunk}"));
        let started = Instant::now();
        let status = Command::new("cargo")
            .args([
                "run",
                "--locked",
                "-q",
                "-p",
                "nsb-data-tools",
                "--bin",
                "run_phase5b_mini_pilot",
                "--",
                "--bulk-gz",
            ])
            .arg(&args.bulk_gz)
            .arg("--output-dir")
            .arg(&out)
            .arg("--row-limit")
            .arg(args.row_limit.to_string())
            .arg("--batch-size")
            .arg(chunk.to_string())
            .arg("--python")
            .arg(&python)
            .arg("--reconstruct-script")
            .arg(&reconstruct_script)
            .arg("--skip-normalized-output")
            .status()?;
        if !status.success() {
            anyhow::bail!("chunk benchmark failed for chunk_size={chunk}");
        }
        let metrics: serde_json::Value = serde_json::from_str(&fs::read_to_string(
            out.join("phase5b_mini_pilot_metrics.json"),
        )?)?;
        rows.push(ChunkBenchmarkRow {
            chunk_size: chunk,
            sources_reconstructed: metrics["sources_reconstructed"].as_u64().unwrap_or(0),
            sources_per_second: metrics["sources_per_second"].as_f64().unwrap_or(0.0),
            wall_elapsed_seconds: started.elapsed().as_secs_f64(),
            peak_rss_kib: metrics["peak_rss_kib"].as_u64().unwrap_or(0),
        });
    }
    let selected = rows
        .iter()
        .max_by(|a, b| {
            a.sources_per_second
                .partial_cmp(&b.sources_per_second)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|row| row.chunk_size)
        .unwrap_or(100);
    let report = serde_json::json!({
        "schema_version": 1,
        "row_limit": args.row_limit,
        "rows": rows,
        "selected_chunk_size": selected,
        "note": "Prefer the smallest chunk size within 95% of peak throughput and lowest RSS."
    });
    fs::write(
        args.output_dir.join("phase5b_chunk_benchmark.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!("phase5b chunk benchmark selected chunk_size={selected}");
    Ok(())
}
