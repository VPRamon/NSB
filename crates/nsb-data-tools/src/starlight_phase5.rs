//! Phase 5 — Gaia DR3 XP continuous acquisition, reconstruction, and validation.

use crate::checksum_io::{sha256_file, verify_sha256_file};
use anyhow::{Context, Result};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const PHASE4_SHA256SUM: &str = "phase4.sha256sum";
pub const PHASE4_MANIFEST: &str = "phase4_inputs.manifest.json";
pub const PHASE4_SAMPLE_SOURCES: &str = "phase4_sample_sources.csv";
pub const PHASE4_MEMBERSHIPS: &str = "phase4_sample_memberships.csv";
pub const PHASE4_SPLITS: &str = "phase4_split_assignments.csv";

/// Frozen catastrophic-outlier threshold (absolute relative error).
pub const CATASTROPHIC_RELATIVE_ERROR: f64 = 0.50;

/// Production gates for XP continuous overlap validation (validation + test).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XpContinuousGates {
    pub max_flux_weighted_abs_bias: f64,
    pub max_median_abs_relative_bias: f64,
    pub max_p95_abs_relative_error: f64,
    pub coverage_68_min: f64,
    pub coverage_68_max: f64,
    pub coverage_95_min: f64,
    pub coverage_95_max: f64,
}

impl Default for XpContinuousGates {
    fn default() -> Self {
        Self {
            max_flux_weighted_abs_bias: 0.03,
            max_median_abs_relative_bias: 0.05,
            max_p95_abs_relative_error: 0.10,
            coverage_68_min: 0.63,
            coverage_68_max: 0.73,
            coverage_95_min: 0.90,
            coverage_95_max: 0.98,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase4InputSnapshot {
    pub schema_version: u32,
    pub gaia_release: String,
    pub software_commit: String,
    pub generation_timestamp_utc: String,
    pub phase4_manifest_sha256: String,
    pub sample_sources_sha256: String,
    pub memberships_sha256: String,
    pub split_assignments_sha256: String,
    pub phase4_sha256sum_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase5TargetRow {
    pub source_id: u64,
    pub population: String,
    pub split: String,
    pub spatial_cell: u32,
    pub strata: String,
    pub phot_g_mean_mag: Option<f64>,
    pub bp_rp: Option<f64>,
    pub phot_g_mean_flux_over_error: Option<f64>,
    pub phot_bp_rp_excess_factor: Option<f64>,
    pub phot_bp_n_blended_transits: Option<u32>,
    pub phot_rp_n_blended_transits: Option<u32>,
    pub l: Option<f64>,
    pub b: Option<f64>,
    pub duplicated_source: bool,
    pub phot_variable_flag: String,
    pub in_qso_candidates: bool,
    pub in_galaxy_candidates: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OverlapComparison {
    pub source_id: u64,
    pub split: String,
    pub sampled_flux_ph_m2_s: f64,
    pub reconstructed_flux_ph_m2_s: f64,
    pub statistical_uncertainty_ph_m2_s: f64,
    pub systematic_uncertainty_ph_m2_s: f64,
    pub total_uncertainty_ph_m2_s: f64,
    pub relative_error: f64,
    pub phot_g_mean_mag: Option<f64>,
    pub bp_rp: Option<f64>,
    pub phot_g_snr: Option<f64>,
    pub phot_bp_rp_excess_factor: Option<f64>,
    pub l: Option<f64>,
    pub b: Option<f64>,
    pub g_mag_bin: String,
    pub colour_bin: String,
    pub snr_bin: String,
    pub sky_region: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricBundle {
    pub sample_count: u64,
    pub mean_signed_relative_bias: f64,
    pub median_signed_relative_bias: f64,
    pub flux_weighted_integrated_bias: f64,
    pub mae_relative: f64,
    pub rmse_relative: f64,
    pub robust_relative_error: f64,
    pub p50_abs_relative_error: f64,
    pub p68_abs_relative_error: f64,
    pub p90_abs_relative_error: f64,
    pub p95_abs_relative_error: f64,
    pub p99_abs_relative_error: f64,
    pub coverage_68: f64,
    pub coverage_95: f64,
    pub catastrophic_outlier_fraction: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateEvaluation {
    pub gates: XpContinuousGates,
    pub passed: bool,
    pub failures: Vec<String>,
}

pub fn verify_phase4_inputs(missing_flux_root: &Path) -> Result<Phase4InputSnapshot> {
    let sha_path = missing_flux_root.join(PHASE4_SHA256SUM);
    let manifest_path = missing_flux_root.join(PHASE4_MANIFEST);
    let sources_path = missing_flux_root.join(PHASE4_SAMPLE_SOURCES);
    let memberships_path = missing_flux_root.join(PHASE4_MEMBERSHIPS);
    let splits_path = missing_flux_root.join(PHASE4_SPLITS);

    let expected: HashMap<String, String> = fs::read_to_string(&sha_path)?
        .lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once('\t')?;
            Some((name.to_string(), hash.to_string()))
        })
        .collect();

    for (name, hash) in &expected {
        let path = missing_flux_root.join(name);
        verify_sha256_file(&path, hash, "phase4")?;
    }

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path)?).context("phase4 manifest")?;

    Ok(Phase4InputSnapshot {
        schema_version: 1,
        gaia_release: manifest["gaia_release"]
            .as_str()
            .unwrap_or("Gaia DR3")
            .to_string(),
        software_commit: manifest["software_commit"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        generation_timestamp_utc: manifest["generation_timestamp_utc"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        phase4_manifest_sha256: sha256_file(&manifest_path)?,
        sample_sources_sha256: sha256_file(&sources_path)?,
        memberships_sha256: sha256_file(&memberships_path)?,
        split_assignments_sha256: sha256_file(&splits_path)?,
        phase4_sha256sum_sha256: sha256_file(&sha_path)?,
    })
}

pub fn write_phase4_snapshot(path: &Path, snapshot: &Phase4InputSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(snapshot)? + "\n")?;
    Ok(())
}

pub fn load_split_map(path: &Path) -> Result<HashMap<u64, (String, u32)>> {
    let mut map = HashMap::new();
    let mut reader = ReaderBuilder::new().from_path(path)?;
    for row in reader.records() {
        let row = row?;
        let source_id: u64 = row.get(0).context("source_id")?.parse()?;
        let spatial_cell: u32 = row.get(1).context("spatial_cell")?.parse()?;
        let split = row.get(2).context("split")?.to_string();
        map.insert(source_id, (split, spatial_cell));
    }
    Ok(map)
}

pub fn load_membership_strata(path: &Path) -> Result<HashMap<u64, Vec<String>>> {
    let mut map: HashMap<u64, Vec<String>> = HashMap::new();
    let mut reader = ReaderBuilder::new().from_path(path)?;
    for row in reader.records() {
        let row = row?;
        let source_id: u64 = row.get(0).context("source_id")?.parse()?;
        let stratum = row.get(2).context("stratum")?.to_string();
        map.entry(source_id).or_default().push(stratum);
    }
    Ok(map)
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

fn field_f64(record: &csv::StringRecord, headers: &csv::StringRecord, name: &str) -> Option<f64> {
    let idx = headers.iter().position(|h| h == name)?;
    record.get(idx)?.parse().ok()
}

fn field_u32(record: &csv::StringRecord, headers: &csv::StringRecord, name: &str) -> Option<u32> {
    let idx = headers.iter().position(|h| h == name)?;
    record.get(idx)?.parse().ok()
}

fn field_str(record: &csv::StringRecord, headers: &csv::StringRecord, name: &str) -> String {
    headers
        .iter()
        .position(|h| h == name)
        .and_then(|idx| record.get(idx))
        .unwrap_or("")
        .to_string()
}

pub fn extract_phase5_targets(
    missing_flux_root: &Path,
    population: &str,
) -> Result<Vec<Phase5TargetRow>> {
    let sources_path = missing_flux_root.join(PHASE4_SAMPLE_SOURCES);
    let splits = load_split_map(&missing_flux_root.join(PHASE4_SPLITS))?;
    let strata_map = load_membership_strata(&missing_flux_root.join(PHASE4_MEMBERSHIPS))?;

    let mut reader = ReaderBuilder::new().from_path(sources_path)?;
    let headers = reader.headers()?.clone();
    let mut out = Vec::new();

    for record in reader.records() {
        let record = record?;
        let has_continuous = field_str(&record, &headers, "has_xp_continuous");
        let has_sampled = field_str(&record, &headers, "has_xp_sampled");
        let is_overlap = parse_bool(&has_continuous) && parse_bool(&has_sampled);
        let is_continuous_only = parse_bool(&has_continuous) && !parse_bool(&has_sampled);
        let pop = match population {
            "xp_sampled_overlap" if is_overlap => "xp_sampled_overlap",
            "xp_continuous_only" if is_continuous_only => "xp_continuous_only",
            _ => continue,
        };

        let source_id: u64 = field_str(&record, &headers, "source_id").parse()?;
        let (split, spatial_cell) = splits
            .get(&source_id)
            .cloned()
            .with_context(|| format!("missing split for {source_id}"))?;
        let strata = strata_map
            .get(&source_id)
            .map(|entries| entries.join("|"))
            .unwrap_or_default();

        out.push(Phase5TargetRow {
            source_id,
            population: pop.to_string(),
            split,
            spatial_cell,
            strata,
            phot_g_mean_mag: field_f64(&record, &headers, "phot_g_mean_mag"),
            bp_rp: field_f64(&record, &headers, "bp_rp"),
            phot_g_mean_flux_over_error: field_f64(
                &record,
                &headers,
                "phot_g_mean_flux_over_error",
            ),
            phot_bp_rp_excess_factor: field_f64(&record, &headers, "phot_bp_rp_excess_factor"),
            phot_bp_n_blended_transits: field_u32(&record, &headers, "phot_bp_n_blended_transits"),
            phot_rp_n_blended_transits: field_u32(&record, &headers, "phot_rp_n_blended_transits"),
            l: field_f64(&record, &headers, "l"),
            b: field_f64(&record, &headers, "b"),
            duplicated_source: parse_bool(&field_str(&record, &headers, "duplicated_source")),
            phot_variable_flag: field_str(&record, &headers, "phot_variable_flag"),
            in_qso_candidates: parse_bool(&field_str(&record, &headers, "in_qso_candidates")),
            in_galaxy_candidates: parse_bool(&field_str(&record, &headers, "in_galaxy_candidates")),
        });
    }
    out.sort_by_key(|row| row.source_id);
    Ok(out)
}

pub fn write_targets_csv(path: &Path, rows: &[Phase5TargetRow]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut writer = WriterBuilder::new().from_path(path)?;
    writer.write_record([
        "source_id",
        "population",
        "split",
        "spatial_cell",
        "strata",
        "phot_g_mean_mag",
        "bp_rp",
        "phot_g_mean_flux_over_error",
        "phot_bp_rp_excess_factor",
        "phot_bp_n_blended_transits",
        "phot_rp_n_blended_transits",
        "l",
        "b",
        "duplicated_source",
        "phot_variable_flag",
        "in_qso_candidates",
        "in_galaxy_candidates",
    ])?;
    for row in rows {
        writer.write_record([
            row.source_id.to_string(),
            row.population.clone(),
            row.split.clone(),
            row.spatial_cell.to_string(),
            row.strata.clone(),
            opt_f64(row.phot_g_mean_mag),
            opt_f64(row.bp_rp),
            opt_f64(row.phot_g_mean_flux_over_error),
            opt_f64(row.phot_bp_rp_excess_factor),
            row.phot_bp_n_blended_transits
                .map(|v| v.to_string())
                .unwrap_or_default(),
            row.phot_rp_n_blended_transits
                .map(|v| v.to_string())
                .unwrap_or_default(),
            opt_f64(row.l),
            opt_f64(row.b),
            row.duplicated_source.to_string(),
            row.phot_variable_flag.clone(),
            row.in_qso_candidates.to_string(),
            row.in_galaxy_candidates.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn opt_f64(value: Option<f64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

pub struct CanonicalFluxLoad {
    pub flux_by_source: HashMap<u64, f64>,
    pub missing_source_ids: Vec<u64>,
}

pub fn load_canonical_sampled_flux(
    catalogue_path: &Path,
    source_ids: &HashSet<u64>,
) -> Result<CanonicalFluxLoad> {
    let mut reader = ReaderBuilder::new().from_path(catalogue_path)?;
    let headers = reader.headers()?.clone();
    let sid_idx = headers
        .iter()
        .position(|h| h == "source_id")
        .context("source_id")?;
    let flux_idx = headers
        .iter()
        .position(|h| h == "photon_flux_336_650_ph_m2_s")
        .context("photon_flux")?;
    let mut map = HashMap::new();
    for row in reader.records() {
        let row = row?;
        let source_id: u64 = row.get(sid_idx).context("sid")?.parse()?;
        if !source_ids.contains(&source_id) {
            continue;
        }
        let flux: f64 = row.get(flux_idx).context("flux")?.parse()?;
        map.insert(source_id, flux);
        if map.len() == source_ids.len() {
            break;
        }
    }
    let missing_source_ids: Vec<u64> = source_ids
        .iter()
        .filter(|id| !map.contains_key(id))
        .copied()
        .collect();
    Ok(CanonicalFluxLoad {
        flux_by_source: map,
        missing_source_ids,
    })
}

pub fn g_mag_bin(g: Option<f64>) -> &'static str {
    match g {
        Some(v) if v < 8.0 => "g_bright",
        Some(v) if v < 14.0 => "g_intermediate",
        Some(v) if v < 18.0 => "g_faint",
        Some(_) => "g_very_faint",
        None => "g_missing",
    }
}

pub fn colour_bin(bp_rp: Option<f64>) -> &'static str {
    match bp_rp {
        Some(v) if v < 0.5 => "colour_blue",
        Some(v) if v < 1.5 => "colour_solar",
        Some(v) if v < 2.5 => "colour_red",
        Some(_) => "colour_very_red",
        None => "colour_missing",
    }
}

pub fn snr_bin(snr: Option<f64>) -> &'static str {
    match snr {
        Some(v) if v < 10.0 => "low_g_snr",
        Some(v) if v < 50.0 => "g_snr_intermediate",
        Some(_) => "high_g_snr",
        None => "snr_missing",
    }
}

pub fn sky_region(l: Option<f64>, b: Option<f64>) -> &'static str {
    let (Some(lon), Some(lat)) = (l, b) else {
        return "sky_unknown";
    };
    if lat.abs() > 60.0 {
        return if lat > 0.0 {
            "north_pole"
        } else {
            "south_pole"
        };
    }
    if lat.abs() < 10.0 {
        if lon.abs() < 15.0 || lon > 345.0 {
            return "galactic_centre";
        }
        if !(20.0..340.0).contains(&lon) {
            return "longitude_seam";
        }
        return "galactic_plane";
    }
    "high_latitude"
}

pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn compute_metrics(rows: &[OverlapComparison], inflation: f64) -> MetricBundle {
    if rows.is_empty() {
        return MetricBundle::default();
    }
    let rel: Vec<f64> = rows.iter().map(|r| r.relative_error).collect();
    let abs_rel: Vec<f64> = rel.iter().map(|v| v.abs()).collect();
    let signed_mean = rel.iter().sum::<f64>() / rel.len() as f64;
    let flux_sum: f64 = rows.iter().map(|r| r.sampled_flux_ph_m2_s.abs()).sum();
    let flux_weighted_bias = if flux_sum > 0.0 {
        rows.iter()
            .map(|r| r.relative_error * r.sampled_flux_ph_m2_s.abs())
            .sum::<f64>()
            / flux_sum
    } else {
        0.0
    };
    let mae = abs_rel.iter().sum::<f64>() / abs_rel.len() as f64;
    let rmse = (rel.iter().map(|v| v * v).sum::<f64>() / rel.len() as f64).sqrt();
    let mut cover68 = 0_u64;
    let mut cover95 = 0_u64;
    let mut catastrophic = 0_u64;
    for row in rows {
        let sigma = row.total_uncertainty_ph_m2_s * inflation;
        let delta = (row.sampled_flux_ph_m2_s - row.reconstructed_flux_ph_m2_s).abs();
        if delta <= 1.0 * sigma {
            cover68 += 1;
        }
        if delta <= 1.96 * sigma {
            cover95 += 1;
        }
        if row.relative_error.abs() > CATASTROPHIC_RELATIVE_ERROR {
            catastrophic += 1;
        }
    }
    let n = rows.len() as f64;
    MetricBundle {
        sample_count: rows.len() as u64,
        mean_signed_relative_bias: signed_mean,
        median_signed_relative_bias: percentile(&rel, 0.5),
        flux_weighted_integrated_bias: flux_weighted_bias,
        mae_relative: mae,
        rmse_relative: rmse,
        robust_relative_error: percentile(&abs_rel, 0.5),
        p50_abs_relative_error: percentile(&abs_rel, 0.50),
        p68_abs_relative_error: percentile(&abs_rel, 0.68),
        p90_abs_relative_error: percentile(&abs_rel, 0.90),
        p95_abs_relative_error: percentile(&abs_rel, 0.95),
        p99_abs_relative_error: percentile(&abs_rel, 0.99),
        coverage_68: cover68 as f64 / n,
        coverage_95: cover95 as f64 / n,
        catastrophic_outlier_fraction: catastrophic as f64 / n,
    }
}

pub fn fit_uncertainty_inflation(train: &[OverlapComparison]) -> f64 {
    let target_68 = 0.68_f64;
    let mut best = 1.0_f64;
    let mut best_err = f64::MAX;
    let mut factor = 0.5_f64;
    while factor <= 8.0 {
        let metrics = compute_metrics(train, factor);
        let err = (metrics.coverage_68 - target_68).abs();
        if err < best_err {
            best_err = err;
            best = factor;
        }
        factor += 0.05;
    }
    best
}

pub fn evaluate_gates(metrics: &MetricBundle, gates: &XpContinuousGates) -> GateEvaluation {
    let mut failures = Vec::new();
    if metrics.flux_weighted_integrated_bias.abs() > gates.max_flux_weighted_abs_bias {
        failures.push(format!(
            "flux-weighted bias {:.4} exceeds {}",
            metrics.flux_weighted_integrated_bias, gates.max_flux_weighted_abs_bias
        ));
    }
    if metrics.median_signed_relative_bias.abs() > gates.max_median_abs_relative_bias {
        failures.push(format!(
            "median relative bias {:.4} exceeds {}",
            metrics.median_signed_relative_bias, gates.max_median_abs_relative_bias
        ));
    }
    if metrics.p95_abs_relative_error > gates.max_p95_abs_relative_error {
        failures.push(format!(
            "p95 abs relative error {:.4} exceeds {}",
            metrics.p95_abs_relative_error, gates.max_p95_abs_relative_error
        ));
    }
    if metrics.coverage_68 < gates.coverage_68_min || metrics.coverage_68 > gates.coverage_68_max {
        failures.push(format!(
            "68% coverage {:.3} outside [{}, {}]",
            metrics.coverage_68, gates.coverage_68_min, gates.coverage_68_max
        ));
    }
    if metrics.coverage_95 < gates.coverage_95_min || metrics.coverage_95 > gates.coverage_95_max {
        failures.push(format!(
            "95% coverage {:.3} outside [{}, {}]",
            metrics.coverage_95, gates.coverage_95_min, gates.coverage_95_max
        ));
    }
    GateEvaluation {
        gates: gates.clone(),
        passed: failures.is_empty(),
        failures,
    }
}

pub fn write_sha256sum(dir: &Path, files: &[PathBuf]) -> Result<()> {
    let mut lines = Vec::new();
    for path in files {
        if path.is_file() {
            lines.push(format!(
                "{}\t{}",
                sha256_file(path)?,
                path.file_name().and_then(|v| v.to_str()).unwrap_or("file")
            ));
        }
    }
    lines.sort();
    fs::write(dir.join("phase5.sha256sum"), lines.join("\n") + "\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_empty_is_zero() {
        let m = compute_metrics(&[], 1.0);
        assert_eq!(m.sample_count, 0);
    }

    #[test]
    fn gate_evaluation_catches_high_bias() {
        let metrics = MetricBundle {
            median_signed_relative_bias: 0.2,
            ..MetricBundle::default()
        };
        let eval = evaluate_gates(&metrics, &XpContinuousGates::default());
        assert!(!eval.passed);
    }
}
