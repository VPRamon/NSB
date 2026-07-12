//! Identify the extra Phase 5 canonical coefficient file vs target list.

use anyhow::Result;
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::checksum_io::sha256_file;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    phase5_root: PathBuf,
    #[arg(long)]
    targets_csv: PathBuf,
    #[arg(long)]
    canonical_dir: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn main() -> Result<()> {
    let args = Args::parse();
    let phase5_root = expand(args.phase5_root);
    let targets_csv = expand(args.targets_csv);
    let canonical_dir = expand(args.canonical_dir);
    let output_json = expand(args.output_json);

    let mut targets = HashSet::new();
    let mut reader = ReaderBuilder::new().from_path(&targets_csv)?;
    let headers = reader.headers()?.clone();
    let idx = headers.iter().position(|h| h == "source_id").unwrap();
    for row in reader.records() {
        targets.insert(row?.get(idx).unwrap().to_string());
    }

    let mut canonical_files = Vec::new();
    for entry in fs::read_dir(&canonical_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            canonical_files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    canonical_files.sort();

    let mut extras = Vec::new();
    let mut matched = 0_u64;
    for name in &canonical_files {
        let stem = name.strip_suffix(".csv").unwrap_or(name);
        if targets.contains(stem) {
            matched += 1;
        } else {
            let path = canonical_dir.join(name);
            extras.push(serde_json::json!({
                "file": name,
                "source_id": stem,
                "sha256": sha256_file(&path).unwrap_or_default(),
            }));
        }
    }

    let report = serde_json::json!({
        "schema_version": 1,
        "targets": targets.len(),
        "canonical_files": canonical_files.len(),
        "matched": matched,
        "extra_files": extras,
        "missing_targets": targets.iter().filter(|id| !canonical_files.iter().any(|f| f.starts_with(id.as_str()))).count(),
        "reconciliation_note": "12.199 canonical vs 12.198 targets requires identifying the one extra coefficient file without deletion.",
        "phase5_root": phase5_root.display().to_string(),
    });
    if let Some(parent) = output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_json, serde_json::to_string_pretty(&report)? + "\n")?;
    println!(
        "coefficient audit: {} canonical, {} targets, {} extra",
        canonical_files.len(),
        targets.len(),
        extras.len()
    );
    if let Some(extra) = extras.first() {
        println!("extra source_id={}", extra["source_id"]);
    }
    Ok(())
}
