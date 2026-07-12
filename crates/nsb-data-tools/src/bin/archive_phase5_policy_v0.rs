//! Archive exploratory Phase 5 overlap validation (policy v0) without overwriting.

use anyhow::{Context, Result};
use clap::Parser;
use nsb_data_tools::checksum_io::sha256_file;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    phase5_root: PathBuf,
    #[arg(
        long,
        default_value = "phase5-policy-v0-exploratory-no-explicit-uncertainty-model"
    )]
    archive_dir_name: String,
}

const ARTIFACTS: &[&str] = &[
    "phase5_overlap_validation.json",
    "phase5_overlap_validation.md",
    "phase5_overlap_predictions.csv",
    "phase5_overlap_stratified_metrics.csv",
    "phase5_frozen_validation_policy.json",
];

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let phase5_root = expand(args.phase5_root);
    let archive_root = phase5_root.join(&args.archive_dir_name);
    if archive_root.exists() {
        anyhow::bail!(
            "archive already exists at {}; refusing to overwrite",
            archive_root.display()
        );
    }
    fs::create_dir_all(&archive_root)?;

    let mut checksum_lines = Vec::new();
    for name in ARTIFACTS {
        let src = phase5_root.join(name);
        if !src.is_file() {
            anyhow::bail!("missing required artifact {}", src.display());
        }
        let dst = archive_root.join(name);
        fs::copy(&src, &dst).with_context(|| format!("copy {name}"))?;
        checksum_lines.push(format!("{}\t{}", sha256_file(&dst)?, name));
    }

    let manifest = serde_json::json!({
        "schema_version": 1,
        "archive_id": args.archive_dir_name,
        "archived_at_utc": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "software_commit": git_commit(),
        "source_phase5_root": phase5_root.display().to_string(),
        "note": "Exploratory overlap validation using absolute reconstruction uncertainty; coverage gates failed at 1.0 due to wrong contract for correlated XP products.",
        "failed_gates": ["coverage_68", "coverage_95"],
        "uncertainty_model": null,
        "policy": null,
        "artifacts": ARTIFACTS,
    });
    fs::write(
        archive_root.join("archive_manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    checksum_lines.push(format!(
        "{}\tarchive_manifest.json",
        sha256_file(&archive_root.join("archive_manifest.json"))?
    ));
    checksum_lines.sort();
    fs::write(
        archive_root.join("phase5-policy-v0.sha256sum"),
        checksum_lines.join("\n") + "\n",
    )?;
    println!(
        "archived Phase 5 exploratory policy v0 -> {}",
        archive_root.display()
    );
    Ok(())
}
