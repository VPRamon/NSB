//! Consolidate stratified Gaia TAP sampling jobs into deduplicated training tables.

use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::WriterBuilder;
use nsb_data_tools::starlight_sampling::{
    default_spatial_split, inventory_jobs, photometry_branch, required_strata, write_sha256sum,
    JobClassification, PopulationInventory, SAMPLE_CSV_COLUMNS,
};
use nsb_data_tools::starlight_science::{DataPartition, SpatialSplitSpec};
use serde::Serialize;
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Parser)]
#[command(about = "Inventory, validate, deduplicate, and split Gaia starlight stratified samples")]
struct Args {
    #[arg(long, default_value = "~/nsb-data/starlight-gaia-release/missing-flux")]
    root: PathBuf,
    #[arg(long)]
    git_commit: Option<String>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    let root = expand_home(&args.root);
    let jobs_root = root.join("jobs");
    let results_root = root.join("results");
    let queries_root = root.join("queries/stratified");
    let output_dir = root.clone();
    let split = default_spatial_split();
    split.validate()?;

    let inventory = inventory_jobs(&jobs_root, &results_root)?;
    write_inventory(&output_dir, &inventory)?;

    ensure_required_strata(&inventory)?;
    ensure_no_unrecovered_completed(&inventory)?;

    let (sources, memberships, split_counts) =
        nsb_data_tools::starlight_sampling::consolidate_stratified_samples(
            &inventory,
            &results_root,
            &split,
        )?;

    let grid = HealpixGrid::new(
        Nside::new(split.spatial_nside).context("invalid split nside")?,
        HealpixOrdering::Ring,
    )?;
    let headers: Vec<String> = SAMPLE_CSV_COLUMNS
        .iter()
        .map(|col| (*col).to_string())
        .collect();
    let mut split_assignments = Vec::new();
    let mut coverage = CoverageBuilder::default();
    let mut source_partitions: HashMap<u64, DataPartition> = HashMap::new();

    write_sources_csv(&output_dir, &headers, &sources)?;
    write_memberships_csv(&output_dir, &memberships)?;

    for record in &sources {
        let idx = |name: &str| -> Result<usize> {
            headers
                .iter()
                .position(|field| field == name)
                .with_context(|| format!("missing column {name}"))
        };
        let source_id = record
            .get(idx("source_id")?)
            .context("source_id")?
            .parse::<u64>()?;
        let lon = record.get(idx("l")?).context("l")?.parse::<f64>()?;
        let lat = record.get(idx("b")?).context("b")?.parse::<f64>()?;
        let spatial_cell = spatial_cell(&grid, lon, lat)?;
        let partition = split.partition(spatial_cell)?;
        source_partitions.insert(source_id, partition);
        split_assignments.push((source_id, spatial_cell, partition_label(partition)));
        coverage.observe_source(record, &headers, partition, &memberships, source_id);
    }
    write_split_assignments(&output_dir, &split_assignments)?;

    let coverage_report = coverage.finish(&split, &split_counts, &memberships, &inventory);
    write_coverage(&output_dir, &coverage_report)?;

    let manifest = build_manifest(
        &root,
        &queries_root,
        &inventory,
        sources.len(),
        memberships.len(),
        &split,
        args.git_commit.unwrap_or_else(detect_git_commit),
    )?;
    let manifest_path = output_dir.join("phase4_inputs.manifest.json");
    fs::write(
        &manifest_path,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;

    write_sha256sum(
        &output_dir,
        &[
            output_dir.join("phase4_job_inventory.json"),
            output_dir.join("phase4_job_inventory.csv"),
            output_dir.join("phase4_sample_sources.csv"),
            output_dir.join("phase4_sample_memberships.csv"),
            output_dir.join("phase4_split_assignments.csv"),
            output_dir.join("phase4_sampling_coverage.json"),
            manifest_path,
        ],
    )?;

    println!(
        "phase4 complete: {} unique sources, {} memberships, splits {:?}",
        sources.len(),
        memberships.len(),
        split_counts
    );
    Ok(())
}

fn expand_home(path: &Path) -> PathBuf {
    if let Some(rest) = path.to_str().and_then(|value| value.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

fn write_inventory(
    dir: &Path,
    inventory: &[nsb_data_tools::starlight_sampling::JobInventoryEntry],
) -> Result<()> {
    fs::write(
        dir.join("phase4_job_inventory.json"),
        format!("{}\n", serde_json::to_string_pretty(inventory)?),
    )?;
    let mut writer = WriterBuilder::new().from_path(dir.join("phase4_job_inventory.csv"))?;
    writer.write_record([
        "job_id",
        "population",
        "stratum",
        "classification",
        "valid",
        "row_count",
        "result_sha256",
        "remote_job_url",
        "error_class",
        "action_required",
    ])?;
    for entry in inventory {
        writer.write_record([
            &entry.job_id,
            entry.population.as_deref().unwrap_or(""),
            entry.stratum.as_deref().unwrap_or(""),
            &format!("{:?}", entry.classification),
            &entry.valid.to_string(),
            &entry
                .row_count
                .map(|value| value.to_string())
                .unwrap_or_default(),
            entry.result_sha256.as_deref().unwrap_or(""),
            entry.remote_job_url.as_deref().unwrap_or(""),
            entry.error_class.as_deref().unwrap_or(""),
            entry.action_required.as_deref().unwrap_or(""),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn ensure_required_strata(
    inventory: &[nsb_data_tools::starlight_sampling::JobInventoryEntry],
) -> Result<()> {
    let mut missing = Vec::new();
    for (population, strata) in required_strata() {
        for stratum in strata {
            let job_id = format!("{population}_{stratum}");
            let entry = inventory.iter().find(|item| item.job_id == job_id);
            match entry {
                Some(item) if item.classification == JobClassification::CompletedValid => {}
                Some(item) => missing.push(format!(
                    "{job_id}: {:?} ({})",
                    item.classification,
                    item.action_required.as_deref().unwrap_or("no action")
                )),
                None => missing.push(format!("{job_id}: missing job directory")),
            }
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    bail!("required strata not satisfied:\n{}", missing.join("\n"));
}

fn ensure_no_unrecovered_completed(
    inventory: &[nsb_data_tools::starlight_sampling::JobInventoryEntry],
) -> Result<()> {
    let stale: Vec<_> = inventory
        .iter()
        .filter(|entry| {
            matches!(
                entry.classification,
                JobClassification::CompletedInvalid
                    | JobClassification::Pending
                    | JobClassification::Running
            ) && entry.population.is_some()
        })
        .map(|entry| entry.job_id.clone())
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    bail!("unrecovered stratified jobs remain: {}", stale.join(", "));
}

fn write_sources_csv(dir: &Path, headers: &[String], records: &[csv::StringRecord]) -> Result<()> {
    let path = dir.join("phase4_sample_sources.csv");
    let mut writer = WriterBuilder::new().from_path(&path)?;
    writer.write_record(headers)?;
    for record in records {
        writer.write_record(record)?;
    }
    writer.flush()?;
    Ok(())
}

fn write_memberships_csv(dir: &Path, memberships: &[(u64, String, String)]) -> Result<()> {
    let path = dir.join("phase4_sample_memberships.csv");
    let mut writer = WriterBuilder::new().from_path(&path)?;
    writer.write_record(["source_id", "population", "stratum"])?;
    for (source_id, population, stratum) in memberships {
        writer.write_record([source_id.to_string(), population.clone(), stratum.clone()])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_split_assignments(dir: &Path, rows: &[(u64, u64, String)]) -> Result<()> {
    let path = dir.join("phase4_split_assignments.csv");
    let mut writer = WriterBuilder::new().from_path(&path)?;
    writer.write_record(["source_id", "spatial_cell", "split"])?;
    for (source_id, cell, split) in rows {
        writer.write_record([source_id.to_string(), cell.to_string(), split.clone()])?;
    }
    writer.flush()?;
    Ok(())
}

fn spatial_cell(grid: &HealpixGrid, lon_deg: f64, lat_deg: f64) -> Result<u64> {
    let lon = lon_deg.to_radians();
    let lat = lat_deg.to_radians();
    let cos_lat = lat.cos();
    let direction = CartesianDirection::<Galactic>::from_array([
        cos_lat * lon.cos(),
        cos_lat * lon.sin(),
        lat.sin(),
    ]);
    Ok(grid.direction_to_pixel(direction)?.get())
}

fn partition_label(partition: DataPartition) -> String {
    match partition {
        DataPartition::Train => "train".to_string(),
        DataPartition::Validation => "validation".to_string(),
        DataPartition::Test => "test".to_string(),
    }
}

#[derive(Default)]
struct CoverageBuilder {
    by_split: BTreeMap<String, SplitCoverage>,
}

#[derive(Default, Serialize)]
struct SplitCoverage {
    unique_sources: usize,
    g_mag_bins: BTreeMap<String, u64>,
    colour_bins: BTreeMap<String, u64>,
    sky_regions: BTreeMap<String, u64>,
    quality_flags: BTreeMap<String, u64>,
    photometry_branches: BTreeMap<String, u64>,
    populations: BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct CoverageReport {
    schema_version: u32,
    unique_sources: usize,
    membership_rows: usize,
    split_counts: BTreeMap<String, u64>,
    split_policy: SpatialSplitSpec,
    by_split: BTreeMap<String, SplitCoverage>,
    population_inventory: PopulationInventory,
    sampled_fractions: BTreeMap<String, f64>,
    domain_support: BTreeMap<String, bool>,
    limitations: Vec<String>,
}

impl CoverageBuilder {
    fn observe_source(
        &mut self,
        record: &csv::StringRecord,
        headers: &[String],
        partition: DataPartition,
        memberships: &[(u64, String, String)],
        source_id: u64,
    ) {
        let split = partition_label(partition);
        let entry = self.by_split.entry(split).or_default();
        entry.unique_sources += 1;
        let idx = |name: &str| headers.iter().position(|field| field == name);
        let g = idx("phot_g_mean_mag")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse::<f64>().ok());
        let bp_rp = idx("bp_rp")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse::<f64>().ok());
        let lat = idx("b")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse::<f64>().ok());
        let lon = idx("l")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse::<f64>().ok());
        let snr = idx("phot_g_mean_flux_over_error")
            .and_then(|i| record.get(i))
            .and_then(|v| v.parse::<f64>().ok());
        *entry.g_mag_bins.entry(g_bin(g).to_string()).or_default() += 1;
        *entry
            .colour_bins
            .entry(colour_bin(bp_rp).to_string())
            .or_default() += 1;
        if let (Some(lat), Some(lon)) = (lat, lon) {
            *entry
                .sky_regions
                .entry(sky_region(lat, lon).to_string())
                .or_default() += 1;
        }
        if snr.is_some_and(|value| value > 0.0 && value < 20.0) {
            *entry
                .quality_flags
                .entry("low_g_snr".to_string())
                .or_default() += 1;
        }
        if snr.is_some_and(|value| value >= 100.0) {
            *entry
                .quality_flags
                .entry("high_g_snr".to_string())
                .or_default() += 1;
        }
        if let Ok(branch) = photometry_branch(record, &csv::StringRecord::from(headers)) {
            *entry.photometry_branches.entry(branch).or_default() += 1;
        }
        for (_, population, _) in memberships.iter().filter(|(id, _, _)| *id == source_id) {
            *entry.populations.entry(population.clone()).or_default() += 1;
        }
    }

    fn finish(
        self,
        split: &SpatialSplitSpec,
        split_counts: &BTreeMap<String, u64>,
        memberships: &[(u64, String, String)],
        inventory: &[nsb_data_tools::starlight_sampling::JobInventoryEntry],
    ) -> CoverageReport {
        let unique_sources = split_counts.values().sum::<u64>() as usize;
        let inventory_counts = PopulationInventory::default();
        let mut sampled_fractions = BTreeMap::new();
        for (label, total) in [
            ("xp_sampled_overlap", inventory_counts.xp_sampled),
            ("xp_continuous_only", inventory_counts.xp_continuous_only),
            ("no_xp", inventory_counts.no_xp),
        ] {
            let sampled = memberships
                .iter()
                .filter(|(_, population, _)| population == label)
                .map(|(id, _, _)| id)
                .collect::<HashSet<_>>()
                .len() as f64;
            sampled_fractions.insert(label.to_string(), sampled / total as f64);
        }
        let domain_support = build_domain_support(&self.by_split);
        let limitations = inventory
            .iter()
            .filter(|entry| entry.classification == JobClassification::ErrorNonretryable)
            .map(|entry| {
                format!(
                    "{} preserved as non-retryable ADQL failure for audit",
                    entry.job_id
                )
            })
            .collect();
        CoverageReport {
            schema_version: 1,
            unique_sources,
            membership_rows: memberships.len(),
            split_counts: split_counts.clone(),
            split_policy: split.clone(),
            by_split: self.by_split,
            population_inventory: inventory_counts,
            sampled_fractions,
            domain_support,
            limitations,
        }
    }
}

fn g_bin(g: Option<f64>) -> &'static str {
    match g {
        Some(value) if value < 8.0 => "g_bright",
        Some(value) if value < 14.0 => "g_intermediate",
        Some(value) if value < 18.0 => "g_faint",
        Some(_) => "g_very_faint",
        None => "g_missing",
    }
}

fn colour_bin(bp_rp: Option<f64>) -> &'static str {
    match bp_rp {
        Some(value) if value < 0.0 => "colour_blue",
        Some(value) if value < 1.5 => "colour_solar",
        Some(value) if value < 3.0 => "colour_red",
        Some(_) => "colour_very_red",
        None => "colour_missing",
    }
}

fn sky_region(lat: f64, lon: f64) -> &'static str {
    if lat >= 60.0 {
        "north_pole"
    } else if lat <= -60.0 {
        "south_pole"
    } else if lat.abs() < 10.0 && !(20.0..340.0).contains(&lon) {
        "galactic_centre"
    } else if lat.abs() < 10.0 {
        "galactic_plane"
    } else if !(5.0..355.0).contains(&lon) {
        "longitude_seam"
    } else {
        "general_sky"
    }
}

fn build_domain_support(by_split: &BTreeMap<String, SplitCoverage>) -> BTreeMap<String, bool> {
    let required = [
        "colour_blue",
        "colour_very_red",
        "g_bright",
        "g_very_faint",
        "galactic_plane",
        "galactic_centre",
        "north_pole",
        "south_pole",
        "longitude_seam",
        "low_g_snr",
        "branch_partial_colour",
        "branch_g_only",
        "branch_no_photometry",
    ];
    let mut supported: HashSet<&str> = HashSet::new();
    for (split_name, split) in by_split {
        if split_name != "validation" && split_name != "test" {
            continue;
        }
        for key in split.colour_bins.keys() {
            supported.insert(key.as_str());
        }
        for key in split.g_mag_bins.keys() {
            supported.insert(key.as_str());
        }
        for key in split.sky_regions.keys() {
            supported.insert(key.as_str());
        }
        for key in split.quality_flags.keys() {
            supported.insert(key.as_str());
        }
        for key in split.photometry_branches.keys() {
            supported.insert(key.as_str());
        }
    }
    required
        .into_iter()
        .map(|key| (key.to_string(), supported.contains(key)))
        .collect()
}

fn write_coverage(dir: &Path, report: &CoverageReport) -> Result<()> {
    fs::write(
        dir.join("phase4_sampling_coverage.json"),
        format!("{}\n", serde_json::to_string_pretty(report)?),
    )?;
    let md = render_coverage_md(report);
    fs::write(dir.join("phase4_sampling_coverage.md"), md)?;
    Ok(())
}

fn render_coverage_md(report: &CoverageReport) -> String {
    let mut out = String::from("# Phase 4 sampling coverage\n\n");
    out.push_str(&format!(
        "- Unique deduplicated sources: **{}**\n- Membership rows (stratum assignments): **{}**\n",
        report.unique_sources, report.membership_rows
    ));
    out.push_str("\n## Split counts\n\n");
    for (split, count) in &report.split_counts {
        out.push_str(&format!("- {split}: {count}\n"));
    }
    out.push_str("\n## Population reconciliation\n\n");
    for (population, fraction) in &report.sampled_fractions {
        out.push_str(&format!(
            "- {population}: sampled fraction {fraction:.3e}\n"
        ));
    }
    out.push_str("\n## Domain support (validation/test must include true)\n\n");
    for (domain, ok) in &report.domain_support {
        out.push_str(&format!(
            "- {domain}: {}\n",
            if *ok { "yes" } else { "MISSING" }
        ));
    }
    if !report.limitations.is_empty() {
        out.push_str("\n## Limitations\n\n");
        for item in &report.limitations {
            out.push_str(&format!("- {item}\n"));
        }
    }
    out
}

#[derive(Serialize)]
struct Phase4Manifest {
    schema_version: u32,
    gaia_release: String,
    tap_endpoint: String,
    software_commit: String,
    generation_timestamp_utc: String,
    query_files: BTreeMap<String, String>,
    jobs: Vec<BTreeMap<String, serde_json::Value>>,
    result_files: BTreeMap<String, String>,
    row_counts: BTreeMap<String, u64>,
    population_totals: PopulationInventory,
    deduplication_policy: String,
    split_policy: SpatialSplitSpec,
    limitations: Vec<String>,
}

fn build_manifest(
    _root: &Path,
    queries_root: &Path,
    inventory: &[nsb_data_tools::starlight_sampling::JobInventoryEntry],
    unique_sources: usize,
    membership_rows: usize,
    split: &SpatialSplitSpec,
    software_commit: String,
) -> Result<Phase4Manifest> {
    let mut query_files = BTreeMap::new();
    if queries_root.join("sample_queries.manifest.json").is_file() {
        let raw = fs::read_to_string(queries_root.join("sample_queries.manifest.json"))?;
        let manifest: serde_json::Value = serde_json::from_str(&raw)?;
        if let Some(queries) = manifest["queries"].as_array() {
            for entry in queries {
                if let (Some(path), Some(sha)) = (entry["path"].as_str(), entry["sha256"].as_str())
                {
                    query_files.insert(path.to_string(), sha.to_string());
                }
            }
        }
    }
    let mut result_files = BTreeMap::new();
    let mut row_counts = BTreeMap::new();
    let mut jobs = Vec::new();
    let mut endpoint = "https://gea.esac.esa.int/tap-server/tap".to_string();
    for entry in inventory {
        if entry.valid {
            if let (Some(path), Some(sha)) = (&entry.result_path, &entry.result_sha256) {
                result_files.insert(path.clone(), sha.clone());
            }
            if let Some(count) = entry.row_count {
                row_counts.insert(entry.job_id.clone(), count);
            }
        }
        let mut job = BTreeMap::new();
        job.insert("job_id".into(), entry.job_id.clone().into());
        job.insert(
            "classification".into(),
            format!("{:?}", entry.classification).into(),
        );
        if let Some(url) = &entry.remote_job_url {
            endpoint = url.split("/async/").next().unwrap_or(&endpoint).to_string();
            job.insert("remote_job_url".into(), url.clone().into());
        }
        jobs.push(job);
    }
    let limitations = vec![
        format!("stratified oversampling; {unique_sources} unique sources from {membership_rows} memberships"),
        "02_invalid_original preserved as documented ADQL boolean failure".to_string(),
    ];
    Ok(Phase4Manifest {
        schema_version: 1,
        gaia_release: "Gaia DR3".to_string(),
        tap_endpoint: endpoint,
        software_commit,
        generation_timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        query_files,
        jobs,
        result_files,
        row_counts,
        population_totals: PopulationInventory::default(),
        deduplication_policy: "one master row per source_id; all stratum memberships retained"
            .to_string(),
        split_policy: split.clone(),
        limitations,
    })
}

fn detect_git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}
