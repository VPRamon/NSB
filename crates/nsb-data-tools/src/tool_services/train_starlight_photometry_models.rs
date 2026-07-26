//! Train photometric starlight branch models on Phase 4 XP sampled targets.

use anyhow::{Context, Result};
use clap::Parser;
use csv::ReaderBuilder;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::starlight_phase5::load_canonical_sampled_flux;
use nsb_data_tools::starlight_sampling::{default_spatial_split, SAMPLE_CSV_COLUMNS};
use nsb_data_tools::starlight_science::{
    fit_branch_model, BranchModel, DataPartition, ModelFitSample, PhotometryBranch,
    PhotometryFeatures, ValidationMetrics,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(about = "Train photometric starlight models on Phase 4 XP sampled flux targets")]
struct Args {
    #[arg(long)]
    sample_sources: PathBuf,
    #[arg(long)]
    split_assignments: PathBuf,
    #[arg(long)]
    canonical_catalogue: PathBuf,
    #[arg(long)]
    output_json: PathBuf,
    #[arg(long)]
    report_json: PathBuf,
    #[arg(long, default_value = "phase6-photometry-v1")]
    model_id: String,
    #[arg(long, default_value = "starlight-gaia-dr3-candidate")]
    release_id: String,
    #[arg(long, default_value_t = 1.0e-6)]
    ridge: f64,
    #[arg(long, default_value_t = 0.02)]
    systematic_fractional_sigma: f64,
    #[arg(long, default_value_t = 3.0)]
    upper_bound_sigma_multiplier: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct PhotometryTrainingArtifact {
    schema_version: u32,
    model_id: String,
    release_id: String,
    band_nm: [f64; 2],
    training_data_sha256: String,
    canonical_catalogue_sha256: String,
    split_assignments_sha256: String,
    sample_sources_sha256: String,
    split: nsb_data_tools::starlight_science::SpatialSplitSpec,
    branches: Vec<BranchModel>,
    branch_counts: HashMap<String, BranchCounts>,
    uv_correction_status: &'static str,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct BranchCounts {
    train: u64,
    validation: u64,
    test: u64,
}

#[derive(Debug, Serialize)]
struct TrainingReport {
    schema_version: u32,
    model_id: String,
    release_id: String,
    branches_fitted: usize,
    total_xp_sampled_targets: u64,
    excluded_non_positive_flux: u64,
    excluded_missing_canonical: u64,
    branch_counts: HashMap<String, BranchCounts>,
    validation_metrics: HashMap<String, ValidationMetrics>,
}

fn parse_bool(raw: &str) -> bool {
    matches!(raw.trim().to_ascii_lowercase().as_str(), "true" | "t" | "1")
}

fn field<'a>(
    record: &'a csv::StringRecord,
    headers: &HashMap<String, usize>,
    name: &str,
) -> Result<&'a str> {
    let index = *headers
        .get(name)
        .with_context(|| format!("missing column {name}"))?;
    record
        .get(index)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("empty field {name}"))
}

fn optional_field<'a>(
    record: &'a csv::StringRecord,
    headers: &HashMap<String, usize>,
    name: &str,
) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|index| record.get(*index))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn load_headers(path: &Path) -> Result<HashMap<String, usize>> {
    let headers = ReaderBuilder::new()
        .from_path(path)?
        .headers()?
        .iter()
        .enumerate()
        .map(|(index, name)| (name.to_string(), index))
        .collect();
    Ok(headers)
}

fn load_splits(path: &Path) -> Result<HashMap<u64, (DataPartition, u64)>> {
    let mut reader = ReaderBuilder::new().from_path(path)?;
    let headers = reader.headers()?.clone();
    let sid_idx = headers
        .iter()
        .position(|h| h == "source_id")
        .context("source_id")?;
    let split_idx = headers.iter().position(|h| h == "split").context("split")?;
    let cell_idx = headers
        .iter()
        .position(|h| h == "spatial_cell")
        .context("spatial_cell")?;
    let mut map = HashMap::new();
    for row in reader.records() {
        let row = row?;
        let source_id: u64 = row.get(sid_idx).context("sid")?.parse()?;
        let spatial_cell: u64 = row.get(cell_idx).context("cell")?.parse()?;
        let split = match row.get(split_idx).context("split")? {
            "train" => DataPartition::Train,
            "validation" => DataPartition::Validation,
            "test" => DataPartition::Test,
            other => anyhow::bail!("unknown split {other}"),
        };
        map.insert(source_id, (split, spatial_cell));
    }
    Ok(map)
}

fn features_from_row(
    record: &csv::StringRecord,
    headers: &HashMap<String, usize>,
) -> Result<PhotometryFeatures> {
    Ok(PhotometryFeatures {
        g_flux_e_s: optional_field(record, headers, "phot_g_mean_flux")
            .and_then(|v: &str| v.parse().ok()),
        bp_flux_e_s: optional_field(record, headers, "phot_bp_mean_flux")
            .and_then(|v: &str| v.parse().ok()),
        rp_flux_e_s: optional_field(record, headers, "phot_rp_mean_flux")
            .and_then(|v: &str| v.parse().ok()),
        g_mag: optional_field(record, headers, "phot_g_mean_mag")
            .and_then(|v: &str| v.parse().ok()),
        bp_rp: optional_field(record, headers, "bp_rp").and_then(|v: &str| v.parse().ok()),
        bp_rp_excess: optional_field(record, headers, "phot_bp_rp_excess_factor")
            .and_then(|v: &str| v.parse().ok()),
        g_flux_over_error: optional_field(record, headers, "phot_g_mean_flux_over_error")
            .and_then(|v: &str| v.parse().ok()),
        bp_flux_over_error: optional_field(record, headers, "phot_bp_mean_flux_over_error")
            .and_then(|v: &str| v.parse().ok()),
        rp_flux_over_error: optional_field(record, headers, "phot_rp_mean_flux_over_error")
            .and_then(|v: &str| v.parse().ok()),
        galactic_lon_deg: field(record, headers, "l")?.parse()?,
        galactic_lat_deg: field(record, headers, "b")?.parse()?,
        extinction_proxy_mag: None,
        crowding_proxy: optional_field(record, headers, "ipd_frac_multi_peak")
            .and_then(|v: &str| v.parse().ok()),
    })
}

fn branch_feature_names(branch: PhotometryBranch) -> Vec<String> {
    match branch {
        PhotometryBranch::GBpRpColour => vec![
            "ln_g_flux".to_string(),
            "ln_bp_flux".to_string(),
            "ln_rp_flux".to_string(),
            "bp_rp".to_string(),
            "galactic_lon_sin".to_string(),
            "galactic_lon_cos".to_string(),
            "abs_galactic_lat".to_string(),
            "bp_rp_excess".to_string(),
            "ln_g_snr".to_string(),
        ],
        PhotometryBranch::PartialColour => vec![
            "ln_g_flux".to_string(),
            "ln_rp_flux".to_string(),
            "bp_rp".to_string(),
            "galactic_lon_sin".to_string(),
            "galactic_lon_cos".to_string(),
            "abs_galactic_lat".to_string(),
        ],
        PhotometryBranch::GOnly => vec![
            "ln_g_flux".to_string(),
            "galactic_lon_sin".to_string(),
            "galactic_lon_cos".to_string(),
            "abs_galactic_lat".to_string(),
            "ln_g_snr".to_string(),
        ],
        PhotometryBranch::NoUsablePhotometry => Vec::new(),
    }
}

/// Run the `train_starlight_photometry_models` command using process arguments.
pub fn run_cli() -> Result<()> {
    let args = Args::parse();
    for column in SAMPLE_CSV_COLUMNS {
        if !load_headers(&args.sample_sources)?.contains_key(column) {
            anyhow::bail!("sample_sources missing required column {column}");
        }
    }

    let sample_sha = sha256_file(&args.sample_sources)?;
    let split_sha = sha256_file(&args.split_assignments)?;
    let catalogue_sha = sha256_file(&args.canonical_catalogue)?;
    let splits = load_splits(&args.split_assignments)?;
    let headers = load_headers(&args.sample_sources)?;

    let mut sampled_ids = HashSet::new();
    let mut reader = ReaderBuilder::new().from_path(&args.sample_sources)?;
    for row in reader.records() {
        let row = row?;
        if parse_bool(field(&row, &headers, "has_xp_sampled")?) {
            sampled_ids.insert(field(&row, &headers, "source_id")?.parse()?);
        }
    }

    let canonical = load_canonical_sampled_flux(&args.canonical_catalogue, &sampled_ids)?;
    let mut train_by_branch: BTreeMap<PhotometryBranch, Vec<ModelFitSample>> = BTreeMap::new();
    let mut validation_by_branch: BTreeMap<PhotometryBranch, Vec<ModelFitSample>> = BTreeMap::new();
    let mut test_counts: BTreeMap<PhotometryBranch, u64> = BTreeMap::new();
    let mut excluded_missing = 0_u64;

    let mut reader = ReaderBuilder::new().from_path(&args.sample_sources)?;
    for row in reader.records() {
        let row = row?;
        if !parse_bool(field(&row, &headers, "has_xp_sampled")?) {
            continue;
        }
        let source_id: u64 = field(&row, &headers, "source_id")?.parse()?;
        let Some(flux) = canonical.flux_by_source.get(&source_id).copied() else {
            excluded_missing += 1;
            continue;
        };
        if !(flux.is_finite() && flux > 0.0) {
            continue;
        }
        let features = features_from_row(&row, &headers)?;
        let branch = features.branch();
        if branch == PhotometryBranch::NoUsablePhotometry {
            continue;
        }
        let (split, spatial_cell) = splits
            .get(&source_id)
            .copied()
            .unwrap_or((DataPartition::Train, source_id));
        let sample = ModelFitSample {
            source_id,
            spatial_cell,
            features,
            target: flux,
            target_one_sigma: (flux * 0.02).max(1.0),
        };
        match split {
            DataPartition::Train => train_by_branch.entry(branch).or_default().push(sample),
            DataPartition::Validation => {
                validation_by_branch.entry(branch).or_default().push(sample)
            }
            DataPartition::Test => *test_counts.entry(branch).or_default() += 1,
        }
    }

    let mut branches = Vec::new();
    let mut branch_counts = HashMap::new();
    let mut validation_metrics = HashMap::new();
    for branch in [
        PhotometryBranch::GBpRpColour,
        PhotometryBranch::PartialColour,
        PhotometryBranch::GOnly,
    ] {
        let training = train_by_branch.remove(&branch).unwrap_or_default();
        let validation = validation_by_branch.remove(&branch).unwrap_or_default();
        branch_counts.insert(
            format!("{branch:?}"),
            BranchCounts {
                train: training.len() as u64,
                validation: validation.len() as u64,
                test: test_counts.get(&branch).copied().unwrap_or(0),
            },
        );
        if training.len() < branch_feature_names(branch).len() + 2 || validation.is_empty() {
            log::info!("skip {branch:?}: insufficient train/validation rows");
            continue;
        }
        let fitted = fit_branch_model(
            &training,
            &validation,
            branch,
            &branch_feature_names(branch),
            args.ridge,
            args.systematic_fractional_sigma,
            args.upper_bound_sigma_multiplier,
        )?;
        validation_metrics.insert(format!("{branch:?}"), fitted.validation.clone());
        branches.push(fitted);
    }

    let artifact = PhotometryTrainingArtifact {
        schema_version: 1,
        model_id: args.model_id.clone(),
        release_id: args.release_id.clone(),
        band_nm: [336.0, 650.0],
        training_data_sha256: sample_sha.clone(),
        canonical_catalogue_sha256: catalogue_sha,
        split_assignments_sha256: split_sha,
        sample_sources_sha256: sample_sha,
        split: default_spatial_split(),
        branches,
        branch_counts: branch_counts.clone(),
        uv_correction_status: "pending_phase7_independent_calibration",
    };

    if let Some(parent) = args.output_json.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &args.output_json,
        serde_json::to_string_pretty(&artifact)? + "\n",
    )?;

    let report = TrainingReport {
        schema_version: 1,
        model_id: args.model_id,
        release_id: args.release_id,
        branches_fitted: artifact.branches.len(),
        total_xp_sampled_targets: sampled_ids.len() as u64,
        excluded_non_positive_flux: 0,
        excluded_missing_canonical: excluded_missing,
        branch_counts,
        validation_metrics,
    };
    fs::write(
        &args.report_json,
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    println!(
        "trained {} photometry branches -> {}",
        artifact.branches.len(),
        args.output_json.display()
    );
    Ok(())
}
