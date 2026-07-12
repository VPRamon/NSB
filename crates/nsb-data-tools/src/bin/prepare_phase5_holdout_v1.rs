//! Prepare independent Phase 5 holdout v1 with spatial cells disjoint from Phase 4.

use anyhow::Result;
use clap::{Parser, Subcommand};
use csv::{ReaderBuilder, WriterBuilder};
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::starlight_phase5::{
    load_split_map, write_targets_csv, Phase5TargetRow, PHASE4_SPLITS,
};
use nsb_data_tools::starlight_science::SpatialSplitSpec;
use serde::Serialize;
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const HOLDOUT_NSIDE: u32 = 64;
const HOLDOUT_ROWS_PER_STRATUM: usize = 2048;
const HOLDOUT_TARGET_PER_STRATUM: usize = 8;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "~/nsb-data/starlight-gaia-release/missing-flux")]
    missing_flux_root: PathBuf,
    #[arg(
        long,
        default_value = "~/nsb-data/starlight-gaia-release/missing-flux/phase5/holdout_v1"
    )]
    holdout_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write holdout ADQL queries (random_index DESC, larger TOP).
    GenerateQueries,
    /// Consolidate TAP CSV results into holdout source lists.
    Consolidate {
        #[arg(long, default_value = "results")]
        results_dir: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct Stratum {
    name: &'static str,
    predicate: &'static str,
}

fn expand(path: PathBuf) -> PathBuf {
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_prefix("~/")) {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path
}

fn overlap_strata() -> &'static [Stratum] {
    &[
        Stratum { name: "g_bright", predicate: "phot_g_mean_mag < 8" },
        Stratum { name: "g_intermediate", predicate: "phot_g_mean_mag >= 8 AND phot_g_mean_mag < 14" },
        Stratum { name: "g_faint", predicate: "phot_g_mean_mag >= 14 AND phot_g_mean_mag < 18" },
        Stratum { name: "g_very_faint", predicate: "phot_g_mean_mag >= 18" },
        Stratum { name: "colour_blue", predicate: "bp_rp < 0" },
        Stratum { name: "colour_solar", predicate: "bp_rp >= 0 AND bp_rp < 1.5" },
        Stratum { name: "colour_red", predicate: "bp_rp >= 1.5 AND bp_rp < 3" },
        Stratum { name: "colour_very_red", predicate: "bp_rp >= 3" },
        Stratum { name: "galactic_plane", predicate: "ABS(b) < 10" },
        Stratum { name: "galactic_centre", predicate: "ABS(b) < 10 AND (l < 20 OR l >= 340)" },
        Stratum { name: "north_pole", predicate: "b >= 60" },
        Stratum { name: "south_pole", predicate: "b <= -60" },
        Stratum { name: "longitude_seam", predicate: "l < 5 OR l >= 355" },
        Stratum { name: "crowded_blended", predicate: "ipd_frac_multi_peak > 10 OR phot_bp_n_blended_transits > 0 OR phot_rp_n_blended_transits > 0" },
        Stratum { name: "high_bp_rp_excess", predicate: "phot_bp_rp_excess_factor > 1.5" },
        Stratum { name: "low_g_snr", predicate: "phot_g_mean_flux_over_error > 0 AND phot_g_mean_flux_over_error < 20" },
        Stratum { name: "high_g_snr", predicate: "phot_g_mean_flux_over_error >= 100" },
        Stratum { name: "red_extinguished_plane", predicate: "ABS(b) < 10 AND bp_rp >= 3" },
        Stratum { name: "duplicated", predicate: "duplicated_source = 'True'" },
        Stratum { name: "variable", predicate: "phot_variable_flag = 'VARIABLE'" },
        Stratum { name: "extragalactic_candidates", predicate: "in_qso_candidates = 'True' OR in_galaxy_candidates = 'True'" },
    ]
}

fn render_holdout_query(stratum: Stratum) -> String {
    format!(
        "SELECT TOP {rows}\n    source_id,\n    random_index,\n    has_xp_continuous,\n    has_xp_sampled,\n    ra,\n    dec,\n    l,\n    b,\n    phot_g_mean_mag,\n    phot_bp_mean_mag,\n    phot_rp_mean_mag,\n    bp_rp,\n    phot_g_mean_flux,\n    phot_bp_mean_flux,\n    phot_rp_mean_flux,\n    phot_g_mean_flux_error,\n    phot_bp_mean_flux_error,\n    phot_rp_mean_flux_error,\n    phot_g_mean_flux_over_error,\n    phot_bp_mean_flux_over_error,\n    phot_rp_mean_flux_over_error,\n    phot_bp_rp_excess_factor,\n    phot_bp_n_blended_transits,\n    phot_rp_n_blended_transits,\n    ipd_frac_multi_peak,\n    ruwe,\n    duplicated_source,\n    phot_variable_flag,\n    non_single_star,\n    in_qso_candidates,\n    in_galaxy_candidates\nFROM gaiadr3.gaia_source\nWHERE has_xp_continuous = 'True' AND has_xp_sampled = 'True'\n  AND ({pred})\nORDER BY random_index DESC\n",
        rows = HOLDOUT_ROWS_PER_STRATUM,
        pred = stratum.predicate
    )
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

fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
}

fn load_forbidden_sets(
    missing_flux_root: &Path,
) -> Result<(HashSet<u64>, HashSet<u32>, HashSet<u64>)> {
    let splits = load_split_map(&missing_flux_root.join(PHASE4_SPLITS))?;
    let mut phase4_source_ids = HashSet::new();
    let mut phase4_cells = HashSet::new();
    for (source_id, (_, cell)) in &splits {
        phase4_source_ids.insert(*source_id);
        phase4_cells.insert(*cell);
    }
    let mut overlap_ids = HashSet::new();
    let mut reader =
        ReaderBuilder::new().from_path(missing_flux_root.join("phase4_sample_sources.csv"))?;
    let headers = reader.headers()?.clone();
    let sid_idx = headers.iter().position(|h| h == "source_id").unwrap();
    let hc = headers
        .iter()
        .position(|h| h == "has_xp_continuous")
        .unwrap();
    let hs = headers.iter().position(|h| h == "has_xp_sampled").unwrap();
    for row in reader.records() {
        let row = row?;
        if parse_bool(row.get(hc).unwrap()) && parse_bool(row.get(hs).unwrap()) {
            overlap_ids.insert(row.get(sid_idx).unwrap().parse()?);
        }
    }
    Ok((phase4_source_ids, phase4_cells, overlap_ids))
}

fn generate_queries(holdout_root: &Path) -> Result<()> {
    let jobs = holdout_root.join("jobs");
    fs::create_dir_all(&jobs)?;
    let mut entries = Vec::new();
    for stratum in overlap_strata() {
        let query = render_holdout_query(*stratum);
        let name = format!("holdout_v1_{}.adql", stratum.name);
        let path = jobs.join(&name);
        fs::write(&path, &query)?;
        entries.push(serde_json::json!({
            "stratum": stratum.name,
            "path": name,
            "sha256": sha256_file(&path)?,
            "max_rows": HOLDOUT_ROWS_PER_STRATUM,
            "ordering": "random_index DESC",
        }));
    }
    let manifest = serde_json::json!({
        "schema_version": 1,
        "holdout_id": "phase5_holdout_v1",
        "population": "xp_sampled_overlap",
        "spatial_disjoint_from": "phase4_split_assignments spatial_cell set",
        "ordering_policy": "random_index DESC to avoid Phase 4 ASC overlap sample",
        "queries": entries,
    });
    fs::write(
        holdout_root.join("phase5_holdout_v1_query_manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "wrote {} holdout queries -> {}",
        entries.len(),
        jobs.display()
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct HoldoutSplitManifest {
    schema_version: u32,
    holdout_id: String,
    spatial_nside: u32,
    forbidden_phase4_cells: u64,
    forbidden_phase4_source_ids: u64,
    selected_sources: u64,
    selected_cells: u64,
    strata_covered: u64,
    software_commit: String,
    generation_timestamp_utc: String,
}

fn consolidate(missing_flux_root: &Path, holdout_root: &Path, results_dir: &str) -> Result<()> {
    let (phase4_source_ids, phase4_cells, phase4_overlap_ids) =
        load_forbidden_sets(missing_flux_root)?;
    let grid = HealpixGrid::new(Nside::new(HOLDOUT_NSIDE)?, HealpixOrdering::Ring)?;
    let results_root = holdout_root.join(results_dir);
    let mut memberships = Vec::new();
    let mut selected: BTreeMap<u64, Phase5TargetRow> = BTreeMap::new();
    let mut per_stratum = BTreeMap::new();

    for stratum in overlap_strata() {
        let path = results_root.join(format!("holdout_v1_{}.csv", stratum.name));
        if !path.is_file() {
            continue;
        }
        let mut reader = ReaderBuilder::new().from_path(&path)?;
        let headers = reader.headers()?.clone();
        let idx = |name: &str| headers.iter().position(|h| h == name);
        let mut picked = 0_usize;
        for record in reader.records() {
            if picked >= HOLDOUT_TARGET_PER_STRATUM {
                break;
            }
            let record = record?;
            let source_id: u64 = record.get(idx("source_id").unwrap()).unwrap().parse()?;
            if phase4_source_ids.contains(&source_id) || phase4_overlap_ids.contains(&source_id) {
                continue;
            }
            let l: f64 = record.get(idx("l").unwrap()).unwrap().parse()?;
            let b: f64 = record.get(idx("b").unwrap()).unwrap().parse()?;
            let cell = spatial_cell(&grid, l, b)? as u32;
            if phase4_cells.contains(&cell) {
                continue;
            }
            if selected.contains_key(&source_id) {
                continue;
            }
            let row = Phase5TargetRow {
                source_id,
                population: "xp_sampled_overlap".to_string(),
                split: "holdout_v1".to_string(),
                spatial_cell: cell,
                strata: stratum.name.to_string(),
                phot_g_mean_mag: record
                    .get(idx("phot_g_mean_mag").unwrap())
                    .and_then(|v| v.parse().ok()),
                bp_rp: record
                    .get(idx("bp_rp").unwrap())
                    .and_then(|v| v.parse().ok()),
                phot_g_mean_flux_over_error: record
                    .get(idx("phot_g_mean_flux_over_error").unwrap())
                    .and_then(|v| v.parse().ok()),
                phot_bp_rp_excess_factor: record
                    .get(idx("phot_bp_rp_excess_factor").unwrap())
                    .and_then(|v| v.parse().ok()),
                phot_bp_n_blended_transits: record
                    .get(idx("phot_bp_n_blended_transits").unwrap())
                    .and_then(|v| v.parse().ok()),
                phot_rp_n_blended_transits: record
                    .get(idx("phot_rp_n_blended_transits").unwrap())
                    .and_then(|v| v.parse().ok()),
                l: Some(l),
                b: Some(b),
                duplicated_source: record
                    .get(idx("duplicated_source").unwrap())
                    .is_some_and(parse_bool),
                phot_variable_flag: record
                    .get(idx("phot_variable_flag").unwrap())
                    .unwrap_or("")
                    .to_string(),
                in_qso_candidates: record
                    .get(idx("in_qso_candidates").unwrap())
                    .is_some_and(parse_bool),
                in_galaxy_candidates: record
                    .get(idx("in_galaxy_candidates").unwrap())
                    .is_some_and(parse_bool),
            };
            memberships.push((
                source_id,
                "xp_sampled_overlap".to_string(),
                stratum.name.to_string(),
            ));
            selected.insert(source_id, row);
            picked += 1;
        }
        per_stratum.insert(stratum.name.to_string(), picked);
    }

    if selected.is_empty() {
        anyhow::bail!(
            "no holdout sources selected; run TAP queries into {}",
            results_root.display()
        );
    }

    let rows: Vec<_> = selected.into_values().collect();
    write_targets_csv(&holdout_root.join("phase5_holdout_v1_sources.csv"), &rows)?;

    let mut membership_writer =
        WriterBuilder::new().from_path(holdout_root.join("phase5_holdout_v1_memberships.csv"))?;
    membership_writer.write_record(["source_id", "population", "stratum"])?;
    for (source_id, population, stratum) in &memberships {
        membership_writer.write_record([
            source_id.to_string(),
            population.clone(),
            stratum.clone(),
        ])?;
    }
    membership_writer.flush()?;

    let selected_cells: HashSet<_> = rows.iter().map(|r| r.spatial_cell).collect();
    let split = SpatialSplitSpec {
        algorithm: "splitmix64_spatial_cell_v1".to_string(),
        seed: 0x0005_0001_0001,
        spatial_nside: HOLDOUT_NSIDE,
        train_buckets: vec![0, 1, 2],
        validation_buckets: vec![3, 4],
        test_buckets: vec![5, 6, 7],
        bucket_modulus: 8,
    };
    let manifest = HoldoutSplitManifest {
        schema_version: 1,
        holdout_id: "phase5_holdout_v1".to_string(),
        spatial_nside: HOLDOUT_NSIDE,
        forbidden_phase4_cells: phase4_cells.len() as u64,
        forbidden_phase4_source_ids: phase4_source_ids.len() as u64,
        selected_sources: rows.len() as u64,
        selected_cells: selected_cells.len() as u64,
        strata_covered: per_stratum.values().filter(|count| **count > 0).count() as u64,
        software_commit: std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        generation_timestamp_utc: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };
    fs::write(
        holdout_root.join("phase5_holdout_v1_split_manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "manifest": manifest,
            "stratum_selection_counts": per_stratum,
            "spatial_split_spec": split,
            "note": "Independent holdout; cells disjoint from Phase 4 train/validation/test.",
        }))? + "\n",
    )?;

    let checksum_path = holdout_root.join("phase5_holdout_v1.sha256sum");
    let files = [
        "phase5_holdout_v1_sources.csv",
        "phase5_holdout_v1_memberships.csv",
        "phase5_holdout_v1_split_manifest.json",
    ];
    let mut lines = Vec::new();
    for name in files {
        let path = holdout_root.join(name);
        lines.push(format!("{}\t{}", sha256_file(&path)?, name));
    }
    lines.sort();
    fs::write(checksum_path, lines.join("\n") + "\n")?;
    println!(
        "holdout v1 consolidated: {} sources across {} disjoint cells",
        rows.len(),
        selected_cells.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let missing_flux_root = expand(args.missing_flux_root);
    let holdout_root = expand(args.holdout_root);
    fs::create_dir_all(&holdout_root)?;
    match args.command {
        Command::GenerateQueries => generate_queries(&holdout_root),
        Command::Consolidate { results_dir } => {
            consolidate(&missing_flux_root, &holdout_root, &results_dir)
        }
    }
}
