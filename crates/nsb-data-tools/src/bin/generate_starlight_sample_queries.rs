use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};
use siderust::checksum::to_hex;
use std::path::{Path, PathBuf};

const DEFAULT_ROWS_PER_STRATUM: usize = 512;

#[derive(Debug, Parser)]
#[command(about = "Generate deterministic, scientifically stratified Gaia DR3 sample queries")]
struct Args {
    /// Destination for ADQL files and their checksum manifest.
    #[arg(long)]
    output_dir: PathBuf,
    /// Maximum rows returned independently for each population/stratum.
    #[arg(long, default_value_t = DEFAULT_ROWS_PER_STRATUM)]
    rows_per_stratum: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Population {
    XpSampledOverlap,
    XpContinuousOnly,
    NoXp,
}

impl Population {
    fn name(self) -> &'static str {
        match self {
            Self::XpSampledOverlap => "xp_sampled_overlap",
            Self::XpContinuousOnly => "xp_continuous_only",
            Self::NoXp => "no_xp",
        }
    }

    fn predicate(self) -> &'static str {
        match self {
            Self::XpSampledOverlap => "has_xp_continuous = 'True' AND has_xp_sampled = 'True'",
            Self::XpContinuousOnly => "has_xp_continuous = 'True' AND has_xp_sampled = 'False'",
            Self::NoXp => "has_xp_continuous = 'False' AND has_xp_sampled = 'False'",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Stratum {
    name: &'static str,
    predicate: &'static str,
    coverage: &'static [&'static str],
}

#[derive(Debug, Serialize)]
struct Manifest {
    schema_version: u32,
    catalogue: &'static str,
    release: &'static str,
    selection_policy: &'static str,
    rows_per_stratum: usize,
    queries: Vec<QueryEntry>,
}

#[derive(Debug, Serialize)]
struct QueryEntry {
    population: Population,
    stratum: &'static str,
    coverage: &'static [&'static str],
    path: String,
    sha256: String,
    max_rows: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    run(&args)
}

fn run(args: &Args) -> Result<()> {
    if args.rows_per_stratum == 0 || args.rows_per_stratum > 100_000 {
        bail!("--rows-per-stratum must be in 1..=100000");
    }
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let mut entries = Vec::new();
    for population in [
        Population::XpSampledOverlap,
        Population::XpContinuousOnly,
        Population::NoXp,
    ] {
        for stratum in common_strata()
            .iter()
            .chain(population_specific_strata(population).iter())
        {
            let query = render_query(population, *stratum, args.rows_per_stratum);
            let relative = format!("{}_{}.adql", population.name(), stratum.name);
            let path = args.output_dir.join(&relative);
            write_atomic(&path, query.as_bytes())?;
            entries.push(QueryEntry {
                population,
                stratum: stratum.name,
                coverage: stratum.coverage,
                path: relative,
                sha256: sha256_bytes(query.as_bytes()),
                max_rows: args.rows_per_stratum,
            });
        }
    }
    let manifest = Manifest {
        schema_version: 1,
        catalogue: "gaiadr3.gaia_source",
        release: "Gaia DR3",
        selection_policy: "Independent TOP sample within preregistered magnitude, colour, quality, and sky strata; deterministic random_index ordering is used only inside each scientific stratum. Spatial-cell train/validation/test splitting is applied after retrieval.",
        rows_per_stratum: args.rows_per_stratum,
        queries: entries,
    };
    let raw = format!("{}\n", serde_json::to_string_pretty(&manifest)?);
    write_atomic(
        &args.output_dir.join("sample_queries.manifest.json"),
        raw.as_bytes(),
    )?;
    Ok(())
}

fn common_strata() -> &'static [Stratum] {
    &[
        Stratum {
            name: "g_bright",
            predicate: "phot_g_mean_mag < 8",
            coverage: &["bright sources", "saturation"],
        },
        Stratum {
            name: "g_intermediate",
            predicate: "phot_g_mean_mag >= 8 AND phot_g_mean_mag < 14",
            coverage: &["intermediate magnitude"],
        },
        Stratum {
            name: "g_faint",
            predicate: "phot_g_mean_mag >= 14 AND phot_g_mean_mag < 18",
            coverage: &["faint sources"],
        },
        Stratum {
            name: "g_very_faint",
            predicate: "phot_g_mean_mag >= 18",
            coverage: &["effective faint limit"],
        },
        Stratum {
            name: "colour_blue",
            predicate: "bp_rp < 0",
            coverage: &["blue stars", "300-336 nm leverage"],
        },
        Stratum {
            name: "colour_solar",
            predicate: "bp_rp >= 0 AND bp_rp < 1.5",
            coverage: &["solar-like colour"],
        },
        Stratum {
            name: "colour_red",
            predicate: "bp_rp >= 1.5 AND bp_rp < 3",
            coverage: &["red stars"],
        },
        Stratum {
            name: "colour_very_red",
            predicate: "bp_rp >= 3",
            coverage: &["very red stars", "extinction proxy"],
        },
        Stratum {
            name: "galactic_plane",
            predicate: "ABS(b) < 10",
            coverage: &["Galactic plane", "crowding"],
        },
        Stratum {
            name: "galactic_centre",
            predicate: "ABS(b) < 10 AND (l < 20 OR l >= 340)",
            coverage: &["Galactic centre", "dense region", "seam"],
        },
        Stratum {
            name: "north_pole",
            predicate: "b >= 60",
            coverage: &["north Galactic pole", "low density"],
        },
        Stratum {
            name: "south_pole",
            predicate: "b <= -60",
            coverage: &["south Galactic pole", "low density"],
        },
        Stratum {
            name: "longitude_seam",
            predicate: "l < 5 OR l >= 355",
            coverage: &["0/360 degree seam"],
        },
        Stratum {
            name: "crowded_blended",
            predicate: "ipd_frac_multi_peak > 10 OR phot_bp_n_blended_transits > 0 OR phot_rp_n_blended_transits > 0",
            coverage: &["crowding", "blending"],
        },
        Stratum {
            name: "high_bp_rp_excess",
            predicate: "phot_bp_rp_excess_factor > 1.5",
            coverage: &["BP/RP excess", "quality tail"],
        },
        Stratum {
            name: "low_g_snr",
            predicate: "phot_g_mean_flux_over_error > 0 AND phot_g_mean_flux_over_error < 20",
            coverage: &["low signal-to-noise"],
        },
        Stratum {
            name: "high_g_snr",
            predicate: "phot_g_mean_flux_over_error >= 100",
            coverage: &["high signal-to-noise"],
        },
        Stratum {
            name: "red_extinguished_plane",
            predicate: "ABS(b) < 10 AND bp_rp >= 3",
            coverage: &["extinguished region proxy", "very red plane"],
        },
        Stratum {
            name: "duplicated",
            predicate: "duplicated_source = 'True'",
            coverage: &["duplicate-source quality"],
        },
        Stratum {
            name: "variable",
            predicate: "phot_variable_flag = 'VARIABLE'",
            coverage: &["variable stars"],
        },
        Stratum {
            name: "extragalactic_candidates",
            predicate: "in_qso_candidates = 'True' OR in_galaxy_candidates = 'True'",
            coverage: &["quasar contamination", "galaxy contamination"],
        },
    ]
}

fn population_specific_strata(population: Population) -> &'static [Stratum] {
    match population {
        Population::NoXp => &[
            Stratum {
                name: "branch_g_bp_rp_colour",
                predicate: "phot_g_mean_flux IS NOT NULL AND phot_bp_mean_flux IS NOT NULL AND phot_rp_mean_flux IS NOT NULL AND bp_rp IS NOT NULL",
                coverage: &["G+BP+RP+colour branch"],
            },
            Stratum {
                name: "branch_partial_colour",
                predicate: "phot_g_mean_flux IS NOT NULL AND ((phot_bp_mean_flux IS NULL AND phot_rp_mean_flux IS NOT NULL) OR (phot_bp_mean_flux IS NOT NULL AND phot_rp_mean_flux IS NULL) OR bp_rp IS NULL)",
                coverage: &["partial-colour branch"],
            },
            Stratum {
                name: "branch_g_only",
                predicate: "phot_g_mean_flux IS NOT NULL AND phot_bp_mean_flux IS NULL AND phot_rp_mean_flux IS NULL",
                coverage: &["G-only branch"],
            },
            Stratum {
                name: "branch_no_photometry",
                predicate: "phot_g_mean_flux IS NULL AND phot_bp_mean_flux IS NULL AND phot_rp_mean_flux IS NULL",
                coverage: &["no-usable-photometry branch", "upper bound"],
            },
        ],
        Population::XpSampledOverlap | Population::XpContinuousOnly => &[],
    }
}

fn render_query(population: Population, stratum: Stratum, rows: usize) -> String {
    format!(
        "SELECT TOP {rows}\n    source_id,\n    random_index,\n    has_xp_continuous,\n    has_xp_sampled,\n    ra,\n    dec,\n    l,\n    b,\n    phot_g_mean_mag,\n    phot_bp_mean_mag,\n    phot_rp_mean_mag,\n    bp_rp,\n    phot_g_mean_flux,\n    phot_bp_mean_flux,\n    phot_rp_mean_flux,\n    phot_g_mean_flux_error,\n    phot_bp_mean_flux_error,\n    phot_rp_mean_flux_error,\n    phot_g_mean_flux_over_error,\n    phot_bp_mean_flux_over_error,\n    phot_rp_mean_flux_over_error,\n    phot_bp_rp_excess_factor,\n    phot_bp_n_blended_transits,\n    phot_rp_n_blended_transits,\n    ipd_frac_multi_peak,\n    ruwe,\n    duplicated_source,\n    phot_variable_flag,\n    non_single_star,\n    in_qso_candidates,\n    in_galaxy_candidates\nFROM gaiadr3.gaia_source\nWHERE {}\n  AND ({})\nORDER BY random_index\n",
        population.predicate(),
        stratum.predicate
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let part = path.with_extension(format!(
        "{}.part",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("output")
    ));
    std::fs::write(&part, bytes).with_context(|| format!("failed to write {}", part.display()))?;
    std::fs::rename(&part, path).with_context(|| format!("failed to promote {}", path.display()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    to_hex(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_queries_are_stratified_deterministic_and_quote_booleans() {
        let dir = tempfile::tempdir().unwrap();
        let args = Args {
            output_dir: dir.path().to_path_buf(),
            rows_per_stratum: 7,
        };
        run(&args).unwrap();
        let first = std::fs::read(dir.path().join("sample_queries.manifest.json")).unwrap();
        run(&args).unwrap();
        let second = std::fs::read(dir.path().join("sample_queries.manifest.json")).unwrap();
        assert_eq!(first, second);
        let manifest: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert!(manifest["queries"].as_array().unwrap().len() > 60);
        let query =
            std::fs::read_to_string(dir.path().join("xp_continuous_only_galactic_centre.adql"))
                .unwrap();
        assert!(query.contains("has_xp_continuous = 'True'"));
        assert!(query.contains("has_xp_sampled = 'False'"));
        assert!(query.contains("ABS(b) < 10"));
        assert!(query.contains("ORDER BY random_index"));
        assert!(!query.contains("random_index <"));
    }

    #[test]
    fn no_xp_queries_cover_all_fallback_branches() {
        let strata = population_specific_strata(Population::NoXp);
        let names: Vec<_> = strata.iter().map(|entry| entry.name).collect();
        assert_eq!(
            names,
            vec![
                "branch_g_bp_rp_colour",
                "branch_partial_colour",
                "branch_g_only",
                "branch_no_photometry"
            ]
        );
    }
}
