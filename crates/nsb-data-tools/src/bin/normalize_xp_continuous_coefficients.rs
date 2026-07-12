//! Normalize raw XP continuous coefficient downloads to canonical CSV.

use anyhow::{Context, Result};
use clap::Parser;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_xp_continuous::{parse_datalink_gaiaxpy_csv, write_gaiaxpy_datalink_csv};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    raw_dir: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long)]
    manifest_json: PathBuf,
    #[arg(long, default_value = "phase5-batch")]
    retrieval_batch: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoefficientManifestEntry {
    source_id: String,
    raw_sha256: String,
    canonical_sha256: String,
    bp_n_parameters: usize,
    rp_n_parameters: usize,
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoefficientManifest {
    schema_version: u32,
    retrieval_batch: String,
    entries: Vec<CoefficientManifestEntry>,
}

fn source_id_from_stem(stem: &str) -> String {
    stem.strip_prefix("xp_source_").unwrap_or(stem).to_string()
}

fn is_coefficient_csv(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("csv")
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".csv") && !n.contains(".part"))
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&args.raw_dir).context("read raw_dir")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_coefficient_csv(&path) {
            continue;
        }
        let source_id = source_id_from_stem(
            path.file_stem()
                .and_then(|s| s.to_str())
                .context("source_id stem")?,
        );
        let raw_bytes = fs::read(&path)?;
        let raw_sha256 = sha256_file(&path)?;
        let status = match parse_datalink_gaiaxpy_csv(&raw_bytes, &source_id) {
            Ok(mut record) => {
                record.source_checksum = Some(raw_sha256.clone());
                record
                    .quality_flags
                    .push(format!("retrieval_batch:{}", args.retrieval_batch));
                let out = args.output_dir.join(format!("{source_id}.csv"));
                write_gaiaxpy_datalink_csv(&out, &record)?;
                let canonical_sha256 = sha256_file(&out)?;
                entries.push(CoefficientManifestEntry {
                    source_id: source_id.clone(),
                    raw_sha256,
                    canonical_sha256,
                    bp_n_parameters: record.bp_n_parameters,
                    rp_n_parameters: record.rp_n_parameters,
                    status: "valid".to_string(),
                });
                "valid"
            }
            Err(err) => {
                entries.push(CoefficientManifestEntry {
                    source_id: source_id.clone(),
                    raw_sha256,
                    canonical_sha256: String::new(),
                    bp_n_parameters: 0,
                    rp_n_parameters: 0,
                    status: format!("invalid: {err:#}"),
                });
                "invalid"
            }
        };
        eprintln!("{source_id}: {status}");
    }
    entries.sort_by_key(|entry| entry.source_id.clone());
    let manifest = CoefficientManifest {
        schema_version: 1,
        retrieval_batch: args.retrieval_batch.clone(),
        entries,
    };
    if let Some(parent) = args.manifest_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.manifest_json,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "normalized {} coefficient files -> {}",
        manifest.entries.len(),
        args.output_dir.display()
    );
    Ok(())
}
