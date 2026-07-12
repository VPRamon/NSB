//! Prepare holdout v1 through download reconciliation, normalize, reconstruct, preflight.

use anyhow::{Context, Result};
use clap::Parser;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::starlight_phase5::{
    audit_download_inventory, load_canonical_sampled_flux, load_phase5_targets,
    load_sampled_catalogue_exclusions, write_download_inventory_csv,
};
use nsb_data_tools::starlight_phase5_holdout::{
    build_preflight, count_reconstructed, create_execution_manifest, verify_holdout_independence,
    HoldoutExecutionManifest,
};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "~/nsb-data/starlight-gaia-release/missing-flux")]
    missing_flux_root: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5/holdout_v1"
    )]
    holdout_root: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5/phase5_frozen_validation_policy_v1.json"
    )]
    policy_json: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5/phase5_gaiaxpy_environment.json"
    )]
    gaiaxpy_env: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/gaia_dr3_starlight_sources.csv"
    )]
    canonical_catalogue: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/gaia_dr3_starlight_exclusions.csv"
    )]
    exclusions_csv: PathBuf,
    #[arg(long, default_value = "~/workspace/nsb")]
    repo_root: PathBuf,
    #[arg(long)]
    skip_reconstruct: bool,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Serialize)]
struct NormalizationReconciliation {
    schema_version: u32,
    downloaded_valid: u64,
    canonical_files: u64,
    matched_targets: u64,
    extra_files: Vec<String>,
    missing_targets: Vec<String>,
    reconciliation_ok: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let missing_flux_root = expand(args.missing_flux_root);
    let holdout_root = expand(args.holdout_root);
    let policy_json = expand(args.policy_json);
    let gaiaxpy_env = expand(args.gaiaxpy_env);
    let canonical_catalogue = expand(args.canonical_catalogue);
    let exclusions_csv = expand(args.exclusions_csv);
    let repo_root = expand(args.repo_root);

    let sources_path = holdout_root.join("phase5_holdout_v1_sources.csv");
    let split_manifest_path = holdout_root.join("phase5_holdout_v1_split_manifest.json");
    let execution_path = holdout_root.join("phase5_holdout_v1_execution_manifest.json");
    let raw_dir = holdout_root.join("coefficients/raw");
    let canonical_dir = holdout_root.join("coefficients/canonical");
    let reconstructed_dir = holdout_root.join("reconstruction/normalized");
    let checkpoint = holdout_root.join("coefficients/checkpoint.jsonl");

    let targets = load_phase5_targets(&sources_path)?;
    let target_ids: HashSet<_> = targets.iter().map(|t| t.source_id.to_string()).collect();

    let execution: HoldoutExecutionManifest = if execution_path.is_file() {
        serde_json::from_str(&fs::read_to_string(&execution_path)?)?
    } else {
        let manifest = create_execution_manifest(
            &policy_json,
            &sources_path,
            &split_manifest_path,
            &gaiaxpy_env,
            &git_commit(),
            "gaia_xp_continuous_canonical_v1",
        )?;
        fs::write(
            &execution_path,
            serde_json::to_string_pretty(&manifest)? + "\n",
        )?;
        manifest
    };

    // Download reconciliation
    let overlap_ids: HashSet<_> = targets.iter().map(|t| t.source_id).collect();
    let canonical = load_canonical_sampled_flux(&canonical_catalogue, &overlap_ids)?;
    let exclusions = load_sampled_catalogue_exclusions(&exclusions_csv)?;
    let (download_report, inventory_rows) = audit_download_inventory(
        &targets,
        &raw_dir,
        &checkpoint,
        "holdout-v1",
        &exclusions,
        &canonical.flux_by_source,
        false,
        0.0,
    )?;
    let inventory_csv = holdout_root.join("phase5_holdout_v1_download_inventory.csv");
    write_download_inventory_csv(&inventory_csv, &inventory_rows)?;
    let legacy_requests = holdout_root.join("phase5_holdout_requests.manifest.json");
    let requests_v1 = holdout_root.join("phase5_holdout_v1_requests.manifest.json");
    if legacy_requests.is_file() && !requests_v1.is_file() {
        fs::copy(&legacy_requests, &requests_v1)?;
    }
    fs::write(
        holdout_root.join("phase5_holdout_v1_download_status.json"),
        serde_json::to_string_pretty(&download_report)? + "\n",
    )?;
    let download_reconciliation = serde_json::json!({
        "schema_version": 1,
        "holdout_id": "phase5_holdout_v1",
        "requested": download_report.requested,
        "downloaded_valid": download_report.downloaded_valid,
        "pending": download_report.pending,
        "missing_from_canonical_reference": download_report.missing_from_canonical_reference,
        "excluded": download_report.excluded,
        "retryable_error": download_report.retryable_error,
        "nonretryable_error": download_report.nonretryable_error,
        "reconciliation_ok": download_report.pending == 0
            && download_report.downloaded_valid
                + download_report.missing_from_canonical_reference
                + download_report.excluded
                + download_report.retryable_error
                + download_report.nonretryable_error
                == download_report.requested,
    });
    fs::write(
        holdout_root.join("phase5_holdout_v1_download_reconciliation.json"),
        serde_json::to_string_pretty(&download_reconciliation)? + "\n",
    )?;
    if download_report.pending > 0 {
        anyhow::bail!(
            "holdout download incomplete: pending={} valid={}/{}",
            download_report.pending,
            download_report.downloaded_valid,
            download_report.requested
        );
    }

    // Independence
    let independence = verify_holdout_independence(&missing_flux_root, &targets)?;
    fs::write(
        holdout_root.join("phase5_holdout_v1_independence.json"),
        serde_json::to_string_pretty(&independence)? + "\n",
    )?;
    if !independence.passed {
        anyhow::bail!("holdout spatial independence failed");
    }

    // Normalize
    fs::create_dir_all(&canonical_dir)?;
    let norm_manifest = holdout_root.join("phase5_holdout_v1_normalization_manifest.json");
    let status = Command::new("cargo")
        .args([
            "run",
            "--locked",
            "-q",
            "-p",
            "nsb-data-tools",
            "--bin",
            "normalize_xp_continuous_coefficients",
            "--",
            "--raw-dir",
            &raw_dir.display().to_string(),
            "--output-dir",
            &canonical_dir.display().to_string(),
            "--manifest-json",
            &norm_manifest.display().to_string(),
        ])
        .current_dir(&repo_root)
        .status()
        .context("normalize holdout coefficients")?;
    if !status.success() {
        anyhow::bail!("normalize_xp_continuous_coefficients failed");
    }

    let mut canonical_files = Vec::new();
    for entry in fs::read_dir(&canonical_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            canonical_files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    let mut extra = Vec::new();
    let mut matched = 0_u64;
    for name in &canonical_files {
        let stem = name.strip_suffix(".csv").unwrap_or(name);
        if target_ids.contains(stem) {
            matched += 1;
        } else {
            extra.push(name.clone());
        }
    }
    let missing_targets: Vec<_> = target_ids
        .iter()
        .filter(|id| !canonical_files.iter().any(|f| f.starts_with(id.as_str())))
        .cloned()
        .collect();
    let norm_recon = NormalizationReconciliation {
        schema_version: 1,
        downloaded_valid: download_report.downloaded_valid,
        canonical_files: canonical_files.len() as u64,
        matched_targets: matched,
        extra_files: extra.clone(),
        missing_targets: missing_targets.clone(),
        reconciliation_ok: extra.is_empty()
            && missing_targets.is_empty()
            && matched == download_report.downloaded_valid,
    };
    fs::write(
        holdout_root.join("phase5_holdout_v1_normalization_reconciliation.json"),
        serde_json::to_string_pretty(&norm_recon)? + "\n",
    )?;
    if !norm_recon.reconciliation_ok {
        anyhow::bail!("holdout normalization reconciliation failed");
    }

    // Reconstruct
    if !args.skip_reconstruct {
        fs::create_dir_all(&reconstructed_dir)?;
        let recon_manifest = holdout_root.join("phase5_holdout_v1_reconstruction.manifest.json");
        let venv = repo_root.join("tools/starlight-xp-continuous/.venv/bin/python");
        let script = repo_root.join("tools/starlight-xp-continuous/reconstruct_and_integrate.py");
        let status = Command::new(&venv)
            .arg(&script)
            .arg("--coefficients-dir")
            .arg(&canonical_dir)
            .arg("--output-dir")
            .arg(&reconstructed_dir)
            .arg("--manifest")
            .arg(&recon_manifest)
            .status()
            .context("reconstruct holdout")?;
        if !status.success() {
            anyhow::bail!("GaiaXPy reconstruction failed");
        }
    }

    let target_u64: HashSet<u64> = targets.iter().map(|t| t.source_id).collect();
    let reconstructed_count = count_reconstructed(&reconstructed_dir, &target_u64);
    let recon_reconciliation = serde_json::json!({
        "schema_version": 1,
        "canonical_valid": matched,
        "reconstructed_valid": reconstructed_count,
        "reconciliation_ok": reconstructed_count == download_report.downloaded_valid,
    });
    fs::write(
        holdout_root.join("phase5_holdout_v1_reconstruction_reconciliation.json"),
        serde_json::to_string_pretty(&recon_reconciliation)? + "\n",
    )?;

    let preflight = build_preflight(
        &execution,
        &independence,
        &download_report,
        reconstructed_count,
        targets.len() as u64,
    );
    fs::write(
        holdout_root.join("phase5_holdout_v1_preflight.json"),
        serde_json::to_string_pretty(&preflight)? + "\n",
    )?;
    if !preflight.passed {
        anyhow::bail!("preflight failed: {:?}", preflight.failures);
    }

    println!(
        "holdout v1 pipeline ready: reconstructed={}/{} policy_checksum={}",
        reconstructed_count,
        targets.len(),
        &execution.policy_checksum[..16]
    );
    let _ = sha256_file(&execution_path)?;
    Ok(())
}
