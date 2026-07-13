use anyhow::{bail, Context, Result};
use clap::Parser;
use csv::{ReaderBuilder, StringRecord};
use flate2::read::GzDecoder;
use nsb_data_tools::checksum_io::sha256_file;
use nsb_data_tools::gaia_xp::{
    integrate_sampled_photon_flux, parse_gaia_sampled_array_into, PhotonFluxIntegral,
    XP_SAMPLED_GRID_LEN,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};

const GAIA_SOURCE_HEALPIX_SHIFT: u32 = 43;

#[derive(Debug, Parser)]
#[command(about = "Reparse and scientifically audit Gaia XP non-positive integrals")]
struct Args {
    /// Directory containing official XpSampledMeanSpectrum_*.csv.gz files.
    #[arg(long)]
    bulk_dir: PathBuf,
    /// Existing scientific-exclusions CSV emitted by the canonical preparation.
    #[arg(long)]
    exclusions: PathBuf,
    /// TAP metadata/xp_summary CSV for exactly the excluded source IDs.
    #[arg(long)]
    metadata: PathBuf,
    /// JSON audit output. A sibling .sha256 file is also emitted.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Clone)]
struct ExpectedExclusion {
    source_id: u64,
    total: f64,
    positive: f64,
    negative: f64,
    negative_samples: usize,
    band_samples: usize,
}

#[derive(Debug, Serialize)]
struct AuditReport {
    schema_version: u32,
    production_use: bool,
    complete_for_direct_xp_exclusion: bool,
    complete_for_final_300_650_product: bool,
    band_nm: [f64; 2],
    source_population: &'static str,
    bulk_input_files: Vec<FileEvidence>,
    exclusions_input: FileEvidence,
    metadata_input: FileEvidence,
    sources_expected: usize,
    sources_found_once: usize,
    parser_recomputations_matched: usize,
    alternative_photometric_estimates_applied: usize,
    aggregate_signed_integral_ph_m2_s: f64,
    aggregate_positive_contribution_ph_m2_s: f64,
    aggregate_negative_contribution_ph_m2_s: f64,
    aggregate_quadrature_statistical_uncertainty_ph_m2_s: f64,
    decision: &'static str,
    remaining_gate: &'static str,
    sources: Vec<SourceAudit>,
}

#[derive(Debug, Serialize)]
struct FileEvidence {
    path: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct SourceAudit {
    source_id: u64,
    origin_file: String,
    integral_total_ph_m2_s: f64,
    positive_contribution_ph_m2_s: f64,
    negative_contribution_ph_m2_s: f64,
    negative_samples: usize,
    band_samples: usize,
    negative_sample_fraction: f64,
    statistical_uncertainty_ph_m2_s: f64,
    signed_signal_to_noise: f64,
    parser_evidence: ParserEvidence,
    gaia_quality: BTreeMap<String, String>,
    direct_xp_decision: &'static str,
    final_product_decision: &'static str,
    rationale: String,
}

#[derive(Debug, Serialize)]
struct ParserEvidence {
    found_exactly_once: bool,
    flux_samples: usize,
    flux_error_samples: usize,
    all_flux_finite: bool,
    all_flux_errors_finite_nonnegative: bool,
    recomputed_matches_recorded: bool,
}

/// Run the `audit_gaia_starlight_exclusions` command using process arguments.
pub fn run_cli() -> Result<()> {
    run(&Args::parse())
}

fn run(args: &Args) -> Result<()> {
    let expected = read_exclusions(&args.exclusions)?;
    let metadata = read_metadata(&args.metadata, &expected)?;
    let selected_files = select_bulk_files(&args.bulk_dir, expected.keys().copied())?;
    let mut found = BTreeMap::<u64, (PathBuf, PhotonFluxIntegral)>::new();
    for path in &selected_files {
        scan_bulk_file(path, &expected, &mut found)?;
    }

    let mut sources = Vec::with_capacity(expected.len());
    let mut parser_matches = 0_usize;
    let mut signed_sum = 0.0;
    let mut positive_sum = 0.0;
    let mut negative_sum = 0.0;
    let mut variance_sum = 0.0;
    for (source_id, recorded) in &expected {
        let (origin, integral) = found.get(source_id).with_context(|| {
            format!("excluded source {source_id} absent from selected bulk files")
        })?;
        let matches = integral_matches(*integral, recorded);
        parser_matches += usize::from(matches);
        if !matches {
            bail!("recomputed integral for source {source_id} does not match exclusions sidecar");
        }
        let uncertainty = integral
            .uncertainty_ph_m2_s
            .context("official Gaia XP sampled row lacks flux uncertainty")?;
        let signal_to_noise = if uncertainty == 0.0 {
            if integral.total_ph_m2_s == 0.0 {
                0.0
            } else {
                integral.total_ph_m2_s.signum() * f64::INFINITY
            }
        } else {
            integral.total_ph_m2_s / uncertainty
        };
        signed_sum += integral.total_ph_m2_s;
        positive_sum += integral.positive_ph_m2_s;
        negative_sum += integral.negative_ph_m2_s;
        variance_sum += uncertainty * uncertainty;
        let rationale = if signal_to_noise.abs() <= 5.0 {
            format!(
                "The official 343-sample row reparses exactly and its signed 336-650 nm integral is non-positive at {signal_to_noise:.3} sigma. Retain the direct-XP exclusion and route the source through the calibrated photometric fallback with widened uncertainty."
            )
        } else {
            format!(
                "The official 343-sample row reparses exactly, but its signed 336-650 nm integral is non-positive at {signal_to_noise:.3} sigma. Treat it as an XP quality outlier and route it through the calibrated photometric fallback; do not clip spectral samples."
            )
        };
        sources.push(SourceAudit {
            source_id: recorded.source_id,
            origin_file: origin.display().to_string(),
            integral_total_ph_m2_s: integral.total_ph_m2_s,
            positive_contribution_ph_m2_s: integral.positive_ph_m2_s,
            negative_contribution_ph_m2_s: integral.negative_ph_m2_s,
            negative_samples: integral.negative_samples,
            band_samples: integral.band_samples,
            negative_sample_fraction: integral.negative_sample_fraction(),
            statistical_uncertainty_ph_m2_s: uncertainty,
            signed_signal_to_noise: signal_to_noise,
            parser_evidence: ParserEvidence {
                found_exactly_once: true,
                flux_samples: XP_SAMPLED_GRID_LEN,
                flux_error_samples: XP_SAMPLED_GRID_LEN,
                all_flux_finite: true,
                all_flux_errors_finite_nonnegative: true,
                recomputed_matches_recorded: matches,
            },
            gaia_quality: metadata[source_id].clone(),
            direct_xp_decision: "exclude_non_positive_signed_integral",
            final_product_decision: "require_calibrated_photometric_fallback",
            rationale,
        });
    }

    let report = AuditReport {
        schema_version: 1,
        production_use: false,
        complete_for_direct_xp_exclusion: parser_matches == expected.len(),
        complete_for_final_300_650_product: false,
        band_nm: [336.0, 650.0],
        source_population: "ten Gaia DR3 XP sampled rows with non-positive signed 336-650 nm photon integrals",
        bulk_input_files: selected_files
            .iter()
            .map(|path| file_evidence(path))
            .collect::<Result<_>>()?,
        exclusions_input: file_evidence(&args.exclusions)?,
        metadata_input: file_evidence(&args.metadata)?,
        sources_expected: expected.len(),
        sources_found_once: found.len(),
        parser_recomputations_matched: parser_matches,
        alternative_photometric_estimates_applied: 0,
        aggregate_signed_integral_ph_m2_s: signed_sum,
        aggregate_positive_contribution_ph_m2_s: positive_sum,
        aggregate_negative_contribution_ph_m2_s: negative_sum,
        aggregate_quadrature_statistical_uncertainty_ph_m2_s: variance_sum.sqrt(),
        decision: "direct XP exclusions verified; final-product substitution not yet complete",
        remaining_gate: "apply the independently validated 300-650 nm photometric fallback and include its systematic uncertainty",
        sources,
    };
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = format!("{}\n", serde_json::to_string_pretty(&report)?);
    write_atomic(&args.output, raw.as_bytes())?;
    let digest = sha256_file(&args.output)?;
    let checksum_path = args.output.with_extension("json.sha256");
    let checksum_line = format!(
        "{digest}  {}\n",
        args.output
            .file_name()
            .and_then(|value| value.to_str())
            .context("audit output filename is not UTF-8")?
    );
    write_atomic(&checksum_path, checksum_line.as_bytes())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "output": args.output,
            "sha256": digest,
            "sources": expected.len(),
            "production_use": false,
        }))?
    );
    Ok(())
}

fn read_exclusions(path: &Path) -> Result<BTreeMap<u64, ExpectedExclusion>> {
    let mut reader = ReaderBuilder::new().trim(csv::Trim::All).from_path(path)?;
    let headers = reader.headers()?.clone();
    let source = required_header(&headers, "source_id")?;
    let reason = required_header(&headers, "reason")?;
    let total = required_header(&headers, "integrated_photon_flux_ph_m2_s")?;
    let positive = required_header(&headers, "positive_contribution_ph_m2_s")?;
    let negative = required_header(&headers, "negative_contribution_ph_m2_s")?;
    let negative_samples = required_header(&headers, "negative_samples")?;
    let band_samples = required_header(&headers, "band_samples")?;
    let mut output = BTreeMap::new();
    for row in reader.records() {
        let row = row?;
        if row.get(reason) != Some("non-positive integrated photon flux") {
            bail!("unexpected scientific exclusion reason");
        }
        let source_id = parse::<u64>(&row, source, "source_id")?;
        let value = ExpectedExclusion {
            source_id,
            total: parse(&row, total, "integrated_photon_flux_ph_m2_s")?,
            positive: parse(&row, positive, "positive_contribution_ph_m2_s")?,
            negative: parse(&row, negative, "negative_contribution_ph_m2_s")?,
            negative_samples: parse(&row, negative_samples, "negative_samples")?,
            band_samples: parse(&row, band_samples, "band_samples")?,
        };
        if output.insert(source_id, value).is_some() {
            bail!("duplicate source_id {source_id} in exclusions input");
        }
    }
    if output.is_empty() {
        bail!("exclusions input is empty");
    }
    Ok(output)
}

fn read_metadata(
    path: &Path,
    expected: &BTreeMap<u64, ExpectedExclusion>,
) -> Result<BTreeMap<u64, BTreeMap<String, String>>> {
    let mut reader = ReaderBuilder::new().trim(csv::Trim::All).from_path(path)?;
    let headers = reader.headers()?.clone();
    let source = required_header(&headers, "source_id")?;
    let mut output = BTreeMap::new();
    for row in reader.records() {
        let row = row?;
        let source_id = parse::<u64>(&row, source, "source_id")?;
        if !expected.contains_key(&source_id) {
            bail!("metadata contains unexpected source_id {source_id}");
        }
        let values = headers
            .iter()
            .zip(row.iter())
            .filter(|(name, _)| *name != "source_id")
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        if output.insert(source_id, values).is_some() {
            bail!("metadata contains duplicate source_id {source_id}");
        }
    }
    if output.keys().copied().collect::<BTreeSet<_>>()
        != expected.keys().copied().collect::<BTreeSet<_>>()
    {
        bail!("metadata source population does not exactly match exclusions");
    }
    Ok(output)
}

fn select_bulk_files(dir: &Path, source_ids: impl Iterator<Item = u64>) -> Result<Vec<PathBuf>> {
    let indices: BTreeSet<u64> = source_ids
        .map(|source_id| source_id >> GAIA_SOURCE_HEALPIX_SHIFT)
        .collect();
    let mut selected = BTreeSet::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let Some((lower, upper)) = bulk_range(&path) else {
            continue;
        };
        if indices
            .iter()
            .any(|index| lower <= *index && *index <= upper)
        {
            selected.insert(path);
        }
    }
    for index in &indices {
        let matches = selected
            .iter()
            .filter(|path| {
                bulk_range(path).is_some_and(|(lower, upper)| lower <= *index && *index <= upper)
            })
            .count();
        if matches != 1 {
            bail!("expected one bulk file for Gaia HEALPix index {index}, found {matches}");
        }
    }
    Ok(selected.into_iter().collect())
}

fn bulk_range(path: &Path) -> Option<(u64, u64)> {
    let name = path.file_name()?.to_str()?;
    let range = name
        .strip_prefix("XpSampledMeanSpectrum_")?
        .strip_suffix(".csv.gz")?;
    let (lower, upper) = range.split_once('-')?;
    Some((lower.parse().ok()?, upper.parse().ok()?))
}

fn scan_bulk_file(
    path: &Path,
    expected: &BTreeMap<u64, ExpectedExclusion>,
    found: &mut BTreeMap<u64, (PathBuf, PhotonFluxIntegral)>,
) -> Result<()> {
    let file = File::open(path)?;
    let decoder = GzDecoder::new(file);
    let mut reader = ReaderBuilder::new()
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(decoder);
    let headers = reader.headers()?.clone();
    let source = required_header(&headers, "source_id")?;
    let flux = required_header(&headers, "flux")?;
    let flux_error = required_header(&headers, "flux_error")?;
    let mut flux_values = Vec::with_capacity(XP_SAMPLED_GRID_LEN);
    let mut error_values = Vec::with_capacity(XP_SAMPLED_GRID_LEN);
    for row in reader.records() {
        let row = row?;
        let source_id = parse::<u64>(&row, source, "source_id")?;
        if !expected.contains_key(&source_id) {
            continue;
        }
        parse_gaia_sampled_array_into(
            field(&row, flux, "flux")?,
            "flux",
            &mut flux_values,
            Some(source_id),
            Some(path),
        )?;
        parse_gaia_sampled_array_into(
            field(&row, flux_error, "flux_error")?,
            "flux_error",
            &mut error_values,
            Some(source_id),
            Some(path),
        )?;
        let integral = integrate_sampled_photon_flux(&flux_values, &error_values)?;
        if found
            .insert(source_id, (path.to_path_buf(), integral))
            .is_some()
        {
            bail!("excluded source_id {source_id} appears more than once in official bulk files");
        }
    }
    Ok(())
}

fn integral_matches(actual: PhotonFluxIntegral, expected: &ExpectedExclusion) -> bool {
    actual.negative_samples == expected.negative_samples
        && actual.band_samples == expected.band_samples
        && relative_close(actual.total_ph_m2_s, expected.total, 1.0e-12)
        && relative_close(actual.positive_ph_m2_s, expected.positive, 1.0e-12)
        && relative_close(actual.negative_ph_m2_s, expected.negative, 1.0e-12)
}

fn relative_close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance * actual.abs().max(expected.abs()).max(1.0)
}

fn file_evidence(path: &Path) -> Result<FileEvidence> {
    Ok(FileEvidence {
        path: path.display().to_string(),
        sha256: sha256_file(path)?,
        bytes: std::fs::metadata(path)?.len(),
    })
}

fn required_header(headers: &StringRecord, name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|value| value == name)
        .with_context(|| format!("missing required CSV header {name:?}"))
}

fn field<'a>(row: &'a StringRecord, index: usize, name: &str) -> Result<&'a str> {
    row.get(index)
        .with_context(|| format!("CSV row is missing field {name:?}"))
}

fn parse<T>(row: &StringRecord, index: usize, name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    field(row, index, name)?
        .parse()
        .with_context(|| format!("invalid {name}"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let part = path.with_extension("part");
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_filename_range_and_integral_comparison_are_strict() {
        assert_eq!(
            bulk_range(Path::new("XpSampledMeanSpectrum_234558-234761.csv.gz")),
            Some((234_558, 234_761))
        );
        assert_eq!(bulk_range(Path::new("other.csv.gz")), None);
        let integral = PhotonFluxIntegral {
            total_ph_m2_s: -1.0,
            positive_ph_m2_s: 2.0,
            negative_ph_m2_s: -3.0,
            negative_samples: 4,
            band_samples: 158,
            uncertainty_ph_m2_s: Some(5.0),
            negative_contribution_ratio: 1.5,
        };
        let expected = ExpectedExclusion {
            source_id: 1,
            total: -1.0,
            positive: 2.0,
            negative: -3.0,
            negative_samples: 4,
            band_samples: 158,
        };
        assert!(integral_matches(integral, &expected));
        assert!(!integral_matches(
            PhotonFluxIntegral {
                negative_samples: 3,
                ..integral
            },
            &expected
        ));
    }
}
