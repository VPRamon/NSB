//! Inspect Phase 5 XP continuous download progress and write inventory.

use anyhow::Result;
use clap::Parser;
use nsb_data_tools::starlight_phase5::{
    audit_download_inventory, load_canonical_sampled_flux, load_phase5_targets,
    load_sampled_catalogue_exclusions, write_download_inventory_csv,
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

#[derive(Debug, Parser)]
struct Args {
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5"
    )]
    phase5_root: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5/phase5_all_xp_continuous_targets.csv"
    )]
    targets_csv: PathBuf,
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
    #[arg(long)]
    inventory_csv: Option<PathBuf>,
    #[arg(long)]
    status_json: Option<PathBuf>,
    #[arg(long)]
    reconciliation_json: Option<PathBuf>,
    #[arg(long, default_value = "phase5-batch")]
    batch_id: String,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn download_active() -> bool {
    Command::new("pgrep")
        .arg("-f")
        .arg("download_xp_continuous_phase5")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn estimate_throughput(raw_dir: &Path, window: Duration) -> f64 {
    let cutoff = SystemTime::now()
        .checked_sub(window)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut count = 0_u64;
    if let Ok(entries) = fs::read_dir(raw_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("csv") {
                continue;
            }
            if let Ok(meta) = fs::metadata(&path) {
                if meta.modified().ok().is_some_and(|mtime| mtime >= cutoff) {
                    count += 1;
                }
            }
        }
    }
    count as f64 / window.as_secs_f64().max(1.0)
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    args.phase5_root = expand(args.phase5_root);
    args.targets_csv = expand(args.targets_csv);
    args.canonical_catalogue = expand(args.canonical_catalogue);
    args.exclusions_csv = expand(args.exclusions_csv);

    let raw_dir = args.phase5_root.join("coefficients/raw");
    let checkpoint = args.phase5_root.join("coefficients/checkpoint.jsonl");
    let inventory_csv = args
        .inventory_csv
        .unwrap_or_else(|| args.phase5_root.join("phase5_download_inventory.csv"));
    let status_json = args
        .status_json
        .unwrap_or_else(|| args.phase5_root.join("phase5_download_status.json"));
    let reconciliation_json = args.reconciliation_json.unwrap_or_else(|| {
        args.phase5_root
            .join("phase5_population_reconciliation.json")
    });

    let targets = load_phase5_targets(&args.targets_csv)?;
    let overlap_ids: HashSet<_> = targets
        .iter()
        .filter(|t| t.population == "xp_sampled_overlap")
        .map(|t| t.source_id)
        .collect();
    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &overlap_ids)?;
    let exclusions = load_sampled_catalogue_exclusions(&args.exclusions_csv)?;
    let active = download_active();
    let throughput = estimate_throughput(&raw_dir, Duration::from_secs(600));

    let (report, rows) = audit_download_inventory(
        &targets,
        &raw_dir,
        &checkpoint,
        &args.batch_id,
        &exclusions,
        &canonical.flux_by_source,
        active,
        throughput,
    )?;
    write_download_inventory_csv(&inventory_csv, &rows)?;

    let reconciliation = serde_json::json!({
        "schema_version": 1,
        "generation_timestamp_utc": report.generation_timestamp_utc,
        "acquisition": report,
        "overlap_canonical_missing_count": canonical.missing_source_ids.len(),
        "overlap_canonical_missing_source_ids": canonical.missing_source_ids,
    });
    if let Some(parent) = reconciliation_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &reconciliation_json,
        serde_json::to_string_pretty(&reconciliation)? + "\n",
    )?;
    fs::write(&status_json, serde_json::to_string_pretty(&report)? + "\n")?;

    println!(
        "phase5 download inspect: requested={} valid={} pending={} stalled={} throughput={:.3}/s active={}",
        report.requested,
        report.downloaded_valid,
        report.pending,
        report.stalled,
        report.throughput_sources_per_second,
        report.download_active
    );
    Ok(())
}
