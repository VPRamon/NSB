//! Cross-validate bulk ECSV vs DataLink XP continuous sources for Phase 5B.

use anyhow::{Context, Result};
use clap::Parser;
use nsb_data_tools::gaia_xp_continuous_canonical::{
    find_bulk_sources, parse_datalink_gaiaxpy_csv, write_gaiaxpy_datalink_csv,
    CanonicalXpContinuousRecord, FieldDiffSummary,
};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Compare bulk ECSV and DataLink XP continuous records for Phase 5B")]
struct Args {
    #[arg(long)]
    bulk_gz: PathBuf,
    #[arg(long)]
    datalink_dir: PathBuf,
    #[arg(long)]
    source_ids: PathBuf,
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long)]
    gaiaxpy_csv_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct CrossSourceRow {
    source_id: String,
    bulk_found: bool,
    datalink_found: bool,
    max_abs_bp_coefficient_diff: f64,
    max_abs_rp_coefficient_diff: f64,
    max_abs_bp_error_diff: f64,
    max_abs_rp_error_diff: f64,
    max_abs_bp_correlation_diff: f64,
    max_abs_rp_correlation_diff: f64,
    bp_standard_deviation_diff: f64,
    rp_standard_deviation_diff: f64,
    canonical_equivalent: bool,
    status: String,
}

fn datalink_path(dir: &Path, source_id: &str) -> PathBuf {
    dir.join(format!("xp_source_{source_id}.csv"))
}

fn main() -> Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;
    let gaiaxpy_dir = args
        .gaiaxpy_csv_dir
        .unwrap_or_else(|| args.output_dir.join("gaiaxpy_csv"));
    fs::create_dir_all(&gaiaxpy_dir)?;

    let source_ids = fs::read_to_string(&args.source_ids)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect::<Vec<_>>();

    let mut rows = Vec::new();
    let wanted = source_ids.iter().cloned().collect::<HashSet<_>>();
    let bulk_by_id = find_bulk_sources(&args.bulk_gz, &wanted)?;
    for source_id in source_ids {
        let bulk = bulk_by_id.get(&source_id).cloned();
        let dl_path = datalink_path(&args.datalink_dir, &source_id);
        let datalink = if dl_path.is_file() {
            Some(parse_datalink_gaiaxpy_csv(
                &fs::read(&dl_path).with_context(|| dl_path.display().to_string())?,
                &source_id,
            )?)
        } else {
            None
        };

        let (diff, equivalent, status) = match (&bulk, &datalink) {
            (Some(b), Some(d)) => {
                let diff = b.max_abs_diff(d);
                let equivalent = diff.passes_equivalence_gates();
                write_pair(&gaiaxpy_dir, &source_id, b, d)?;
                (
                    diff,
                    equivalent,
                    if equivalent {
                        "equivalent".to_string()
                    } else {
                        "canonical_mismatch".to_string()
                    },
                )
            }
            (Some(_), None) => (
                FieldDiffSummary::default(),
                false,
                "missing_datalink".to_string(),
            ),
            (None, Some(_)) => (
                FieldDiffSummary::default(),
                false,
                "missing_bulk".to_string(),
            ),
            (None, None) => (
                FieldDiffSummary::default(),
                false,
                "missing_both".to_string(),
            ),
        };

        rows.push(CrossSourceRow {
            source_id,
            bulk_found: bulk.is_some(),
            datalink_found: datalink.is_some(),
            max_abs_bp_coefficient_diff: diff.max_abs_bp_coefficient_diff,
            max_abs_rp_coefficient_diff: diff.max_abs_rp_coefficient_diff,
            max_abs_bp_error_diff: diff.max_abs_bp_error_diff,
            max_abs_rp_error_diff: diff.max_abs_rp_error_diff,
            max_abs_bp_correlation_diff: diff.max_abs_bp_correlation_diff,
            max_abs_rp_correlation_diff: diff.max_abs_rp_correlation_diff,
            bp_standard_deviation_diff: diff.bp_standard_deviation_diff,
            rp_standard_deviation_diff: diff.rp_standard_deviation_diff,
            canonical_equivalent: equivalent,
            status,
        });
    }

    let json_path = args.output_dir.join("phase5b_cross_source_comparison.json");
    fs::write(&json_path, serde_json::to_string_pretty(&rows)? + "\n")?;
    write_csv(
        &args.output_dir.join("phase5b_cross_source_comparison.csv"),
        &rows,
    )?;
    fs::write(
        args.output_dir.join("phase5b_cross_source_notes.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "task_306212": "superseded by fixed canonical adapter validation",
            "equivalent_sources": rows.iter().filter(|row| row.canonical_equivalent).count(),
            "total_sources": rows.len(),
        }))? + "\n",
    )?;
    let passed = rows.iter().filter(|row| row.canonical_equivalent).count();
    println!(
        "phase5b cross comparison: {passed}/{} equivalent -> {}",
        rows.len(),
        json_path.display()
    );
    Ok(())
}

fn write_pair(
    dir: &Path,
    source_id: &str,
    bulk: &CanonicalXpContinuousRecord,
    datalink: &CanonicalXpContinuousRecord,
) -> Result<()> {
    write_gaiaxpy_datalink_csv(&dir.join(format!("{source_id}_bulk.csv")), bulk)?;
    write_gaiaxpy_datalink_csv(&dir.join(format!("{source_id}_datalink.csv")), datalink)?;
    Ok(())
}

fn write_csv(path: &Path, rows: &[CrossSourceRow]) -> Result<()> {
    let mut writer = csv::WriterBuilder::new().from_path(path)?;
    for row in rows {
        writer.serialize(row)?;
    }
    writer.flush()?;
    Ok(())
}
