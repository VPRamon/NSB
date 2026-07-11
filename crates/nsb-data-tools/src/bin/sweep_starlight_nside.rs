use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb::{StarlightMap, StarlightProvenance};
use serde::Serialize;
use serde_json::Value;
use siderust::checksum::{sha256, to_hex};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const NSIDES: [u32; 3] = [64, 128, 256];
const GAIA_XP_MODEL: &str = "gaia_dr3_xp_photon_radiance_336_650nm_v1";
const BAND_MIN_NM: f64 = 336.0;
const BAND_MAX_NM: f64 = 650.0;
const MAX_TOTAL_FLUX_RELATIVE_DELTA: f64 = 1.0e-9;

#[derive(Debug, Parser)]
#[command(about = "Run the Gaia starlight release nside sweep")]
struct Args {
    /// Canonical Gaia starlight source CSV.
    #[arg(long)]
    input: PathBuf,
    /// Sweep output directory.
    #[arg(long, default_value = "target/starlight-release/sweep")]
    output_dir: PathBuf,
    /// Independent validation reference JSON.
    #[arg(long)]
    reference: PathBuf,
    /// Source catalogue checksum for the canonical input.
    #[arg(long)]
    catalog_checksum: String,
    /// Reviewed Gaia derived-product license or redistribution policy.
    #[arg(long)]
    catalog_license: String,
    /// UTC generation timestamp.
    #[arg(long)]
    generation_date_utc: String,
    /// Maximum packed CSV size admitted by the automated recommendation.
    #[arg(long, default_value_t = 256.0)]
    max_release_mib: f64,
    /// Maximum measured runtime load time admitted by the automated recommendation.
    #[arg(long, default_value_t = 15.0)]
    max_runtime_load_seconds: f64,
    /// Maximum empty-pixel fraction admitted by the automated recommendation.
    #[arg(long, default_value_t = 0.85)]
    max_empty_pixel_fraction: f64,
    /// Minimum mean catalogue sources in each non-empty pixel.
    #[arg(long, default_value_t = 2.0)]
    min_sources_per_nonempty_pixel: f64,
    /// Maximum relative drift of independent bright-region means from nside=256.
    #[arg(long, default_value_t = 0.10)]
    max_bright_region_relative_delta: f64,
    /// Maximum high-latitude MAD-noise ratio relative to nside=64.
    #[arg(long, default_value_t = 2.0)]
    max_high_latitude_noise_factor: f64,
}

#[derive(Debug, Serialize)]
struct NsideSummary {
    nside: u32,
    pixels: u64,
    sources_used: Option<u64>,
    mean_sources_per_pixel: Option<f64>,
    mean_sources_per_nonempty_pixel: Option<f64>,
    map_csv_bytes: u64,
    release_csv_bytes: u64,
    generation_seconds: f64,
    validation_seconds: f64,
    pack_seconds: f64,
    runtime_load_seconds: f64,
    empty_pixels: u64,
    empty_pixel_fraction: f64,
    input_integrated_flux_sum_ph_cm2_ns: Option<f64>,
    output_integrated_flux_sum_ph_cm2_ns: Option<f64>,
    integrated_flux_relative_error: Option<f64>,
    total_flux_relative_delta_to_nside256: Option<f64>,
    finite_nonnegative_pass: bool,
    spectral_contract_pass: bool,
    plane_pole_pass: bool,
    plane_mean_ph_cm2_ns_sr: Option<f64>,
    pole_mean_ph_cm2_ns_sr: Option<f64>,
    plane_pole_ratio: Option<f64>,
    longitude_wrap_pass: bool,
    longitude_wrap_metric: Option<f64>,
    longitude_wrap_threshold: Option<f64>,
    independent_comparison_pass: bool,
    brightest_independent_region_mean_ph_cm2_ns_sr: Option<f64>,
    bright_region_relative_delta_to_nside256: Option<f64>,
    bright_pixel_p99_ph_cm2_ns_sr: Option<f64>,
    bright_top_one_percent_mean_ph_cm2_ns_sr: Option<f64>,
    high_latitude_noise_mad_ratio: Option<f64>,
    high_latitude_noise_factor_to_nside64: Option<f64>,
    production_ready: bool,
    within_size_budget: bool,
    within_runtime_budget: bool,
    within_empty_pixel_budget: bool,
    sufficient_sources_per_nonempty_pixel: bool,
    bright_region_stable: bool,
    high_latitude_noise_acceptable: bool,
    total_flux_stable: bool,
    smoothing_recommended: bool,
    eligible_for_recommendation: bool,
}

#[derive(Debug, Serialize)]
struct AggregateSummary {
    schema_version: u32,
    photometry_model: &'static str,
    band_nm: [f64; 2],
    recommendation_policy: &'static str,
    smoothing_policy: &'static str,
    selection_constraints: SelectionConstraints,
    recommended_nside: Option<u32>,
    review_required: bool,
    review_template: String,
    summaries: Vec<NsideSummary>,
}

#[derive(Debug, Serialize)]
struct SelectionConstraints {
    max_release_bytes: u64,
    max_runtime_load_seconds: f64,
    max_empty_pixel_fraction: f64,
    min_sources_per_nonempty_pixel: f64,
    max_bright_region_relative_delta: f64,
    max_high_latitude_noise_factor: f64,
    max_total_flux_relative_delta: f64,
}

#[derive(Debug, Serialize)]
struct ReviewTemplate {
    schema_version: u32,
    sweep_report_sha256: String,
    reviewed: bool,
    selected_nside: Option<u32>,
    reviewer: Option<String>,
    reviewed_at_utc: Option<String>,
    rationale: Option<String>,
}

fn main() -> Result<()> {
    run(Args::parse())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;
    build_release_tools()?;

    let mut summaries = Vec::new();
    for nside in NSIDES {
        summaries.push(run_one(&args, nside)?);
    }
    assess_candidates(&mut summaries, &args);
    let recommended_nside = recommend(&summaries);
    let review_template = args.output_dir.join("nside_review.template.json");
    let aggregate = AggregateSummary {
        schema_version: 1,
        photometry_model: GAIA_XP_MODEL,
        band_nm: [BAND_MIN_NM, BAND_MAX_NM],
        recommendation_policy: "Choose the highest angular resolution that passes every science gate, remains within explicit size/runtime/sparsity/noise limits, and preserves independent bright-region means relative to nside=256. No automated recommendation is sufficient for production: a maintainer must review and attest the checksummed report.",
        smoothing_policy: "No smoothing is applied by this sweep. A resolution whose sparsity or high-latitude noise indicates smoothing is marked ineligible; any future smoothing proposal must define its kernel and angular scale, conserve total flux, and repeat independent validation.",
        selection_constraints: SelectionConstraints {
            max_release_bytes: max_release_bytes(&args),
            max_runtime_load_seconds: args.max_runtime_load_seconds,
            max_empty_pixel_fraction: args.max_empty_pixel_fraction,
            min_sources_per_nonempty_pixel: args.min_sources_per_nonempty_pixel,
            max_bright_region_relative_delta: args.max_bright_region_relative_delta,
            max_high_latitude_noise_factor: args.max_high_latitude_noise_factor,
            max_total_flux_relative_delta: MAX_TOTAL_FLUX_RELATIVE_DELTA,
        },
        recommended_nside,
        review_required: true,
        review_template: review_template.display().to_string(),
        summaries,
    };
    let summary_path = args.output_dir.join("summary.json");
    write_json(&summary_path, &aggregate)?;
    let summary_raw = std::fs::read(&summary_path)
        .with_context(|| format!("failed to checksum {}", summary_path.display()))?;
    write_json(
        &review_template,
        &ReviewTemplate {
            schema_version: 1,
            sweep_report_sha256: format!("sha256:{}", to_hex(&sha256(&summary_raw))),
            reviewed: false,
            selected_nside: recommended_nside,
            reviewer: None,
            reviewed_at_utc: None,
            rationale: None,
        },
    )?;
    if recommended_nside.is_none() {
        bail!("no nside recommendation satisfies all science and operational constraints");
    }
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    for (name, value) in [
        ("--max-release-mib", args.max_release_mib),
        ("--max-runtime-load-seconds", args.max_runtime_load_seconds),
        (
            "--min-sources-per-nonempty-pixel",
            args.min_sources_per_nonempty_pixel,
        ),
        (
            "--max-high-latitude-noise-factor",
            args.max_high_latitude_noise_factor,
        ),
    ] {
        if !value.is_finite() || value <= 0.0 {
            bail!("{name} must be finite and greater than zero");
        }
    }
    for (name, value) in [
        ("--max-empty-pixel-fraction", args.max_empty_pixel_fraction),
        (
            "--max-bright-region-relative-delta",
            args.max_bright_region_relative_delta,
        ),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            bail!("{name} must be finite and in [0, 1]");
        }
    }
    Ok(())
}

fn run_one(args: &Args, nside: u32) -> Result<NsideSummary> {
    let dir = args.output_dir.join(format!("nside{nside}"));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let map = dir.join("starlight_map.csv");
    let diagnostics = dir.join("diagnostics.json");
    let validation = dir.join("validation.json");
    let release = dir.join("starlight_map.release.csv");
    let manifest = dir.join("starlight_map.release.toml");

    let started = Instant::now();
    cargo_tool(
        "build_starlight_map",
        vec![
            "--input".to_string(),
            path_str(&args.input)?.to_string(),
            "--output".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics-output".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--nside".to_string(),
            nside.to_string(),
            "--ordering".to_string(),
            "ring".to_string(),
            "--catalog-name".to_string(),
            "Gaia".to_string(),
            "--catalog-release".to_string(),
            "DR3".to_string(),
            "--catalog-license".to_string(),
            args.catalog_license.clone(),
            "--catalog-checksum".to_string(),
            args.catalog_checksum.clone(),
            "--photometry-model".to_string(),
            GAIA_XP_MODEL.to_string(),
            "--band-min-nm".to_string(),
            BAND_MIN_NM.to_string(),
            "--band-max-nm".to_string(),
            BAND_MAX_NM.to_string(),
            "--generation-date-utc".to_string(),
            args.generation_date_utc.clone(),
        ],
    )?;
    let generation_seconds = started.elapsed().as_secs_f64();

    let validation_started = Instant::now();
    cargo_tool(
        "validate_starlight_map",
        vec![
            "--input".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--reference".to_string(),
            path_str(&args.reference)?.to_string(),
            "--output".to_string(),
            path_str(&validation)?.to_string(),
        ],
    )?;
    let validation_seconds = validation_started.elapsed().as_secs_f64();

    let pack_started = Instant::now();
    cargo_tool(
        "pack_starlight_asset",
        vec![
            "--input".to_string(),
            path_str(&map)?.to_string(),
            "--diagnostics".to_string(),
            path_str(&diagnostics)?.to_string(),
            "--validation".to_string(),
            path_str(&validation)?.to_string(),
            "--output".to_string(),
            path_str(&release)?.to_string(),
            "--manifest".to_string(),
            path_str(&manifest)?.to_string(),
            "--candidate".to_string(),
        ],
    )?;
    let pack_seconds = pack_started.elapsed().as_secs_f64();

    let runtime_load_started = Instant::now();
    let release_raw = std::fs::read_to_string(&release)
        .with_context(|| format!("failed to runtime-load {}", release.display()))?;
    let runtime_map =
        StarlightMap::from_csv_str(&release_raw, StarlightProvenance::test_fixture())?;
    if runtime_map.pixels().len() != usize::try_from(12 * u64::from(nside) * u64::from(nside))? {
        bail!("runtime-loaded nside={nside} map has an unexpected pixel count");
    }
    let runtime_load_seconds = runtime_load_started.elapsed().as_secs_f64();

    let validation_json: Value = read_json(&validation)?;
    let pixels = 12 * u64::from(nside) * u64::from(nside);
    let empty_pixels = validation_json["empty_pixels"].as_u64().unwrap_or(pixels);
    let summary = NsideSummary {
        nside,
        pixels,
        sources_used: validation_json["sources_used"].as_u64(),
        mean_sources_per_pixel: validation_json["mean_sources_per_pixel"].as_f64(),
        mean_sources_per_nonempty_pixel: validation_json["mean_sources_per_nonempty_pixel"]
            .as_f64(),
        map_csv_bytes: file_len(&map)?,
        release_csv_bytes: file_len(&release)?,
        generation_seconds,
        validation_seconds,
        pack_seconds,
        runtime_load_seconds,
        empty_pixels,
        empty_pixel_fraction: validation_json["empty_pixel_fraction"]
            .as_f64()
            .unwrap_or(1.0),
        input_integrated_flux_sum_ph_cm2_ns: validation_json["input_integrated_flux_sum_ph_cm2_ns"]
            .as_f64(),
        output_integrated_flux_sum_ph_cm2_ns: validation_json
            ["output_integrated_flux_sum_ph_cm2_ns"]
            .as_f64(),
        integrated_flux_relative_error: validation_json["integrated_flux_relative_error"].as_f64(),
        total_flux_relative_delta_to_nside256: None,
        finite_nonnegative_pass: validation_json["finite_nonnegative_pass"]
            .as_bool()
            .unwrap_or(false),
        spectral_contract_pass: validation_json["spectral_contract_pass"]
            .as_bool()
            .unwrap_or(false),
        plane_pole_pass: validation_json["plane_pole_pass"]
            .as_bool()
            .unwrap_or(false),
        plane_mean_ph_cm2_ns_sr: validation_json["plane_mean_ph_cm2_ns_sr"].as_f64(),
        pole_mean_ph_cm2_ns_sr: validation_json["pole_mean_ph_cm2_ns_sr"].as_f64(),
        plane_pole_ratio: validation_json["plane_pole_ratio"].as_f64(),
        longitude_wrap_pass: validation_json["longitude_wrap_pass"]
            .as_bool()
            .unwrap_or(false),
        longitude_wrap_metric: validation_json["longitude_wrap_metric"].as_f64(),
        longitude_wrap_threshold: validation_json["longitude_wrap_threshold"].as_f64(),
        independent_comparison_pass: validation_json["independent_comparison_pass"]
            .as_bool()
            .unwrap_or(false),
        brightest_independent_region_mean_ph_cm2_ns_sr: brightest_region_mean(&validation_json),
        bright_region_relative_delta_to_nside256: None,
        bright_pixel_p99_ph_cm2_ns_sr: validation_json["bright_pixel_p99_ph_cm2_ns_sr"].as_f64(),
        bright_top_one_percent_mean_ph_cm2_ns_sr: validation_json
            ["bright_top_one_percent_mean_ph_cm2_ns_sr"]
            .as_f64(),
        high_latitude_noise_mad_ratio: validation_json["high_latitude_noise_mad_ratio"].as_f64(),
        high_latitude_noise_factor_to_nside64: None,
        production_ready: validation_json["production_ready"]
            .as_bool()
            .unwrap_or(false),
        within_size_budget: false,
        within_runtime_budget: false,
        within_empty_pixel_budget: false,
        sufficient_sources_per_nonempty_pixel: false,
        bright_region_stable: false,
        high_latitude_noise_acceptable: false,
        total_flux_stable: false,
        smoothing_recommended: false,
        eligible_for_recommendation: false,
    };
    write_json(&dir.join("summary.json"), &summary)?;
    Ok(summary)
}

fn recommend(summaries: &[NsideSummary]) -> Option<u32> {
    summaries
        .iter()
        .filter(|summary| summary.eligible_for_recommendation)
        .map(|summary| summary.nside)
        .max()
}

fn assess_candidates(summaries: &mut [NsideSummary], args: &Args) {
    let bright_reference = summaries
        .iter()
        .find(|summary| summary.nside == 256)
        .and_then(|summary| summary.brightest_independent_region_mean_ph_cm2_ns_sr);
    let noise_reference = summaries
        .iter()
        .find(|summary| summary.nside == 64)
        .and_then(|summary| summary.high_latitude_noise_mad_ratio);
    let total_flux_reference = summaries
        .iter()
        .find(|summary| summary.nside == 256)
        .and_then(|summary| summary.output_integrated_flux_sum_ph_cm2_ns);
    let max_release_bytes = max_release_bytes(args);

    for summary in summaries {
        summary.bright_region_relative_delta_to_nside256 = match (
            summary.brightest_independent_region_mean_ph_cm2_ns_sr,
            bright_reference,
        ) {
            (Some(value), Some(reference)) => Some(relative_delta(value, reference)),
            _ => None,
        };
        summary.high_latitude_noise_factor_to_nside64 =
            match (summary.high_latitude_noise_mad_ratio, noise_reference) {
                (Some(value), Some(reference)) => Some(ratio_with_zero_baseline(value, reference)),
                _ => None,
            };
        summary.total_flux_relative_delta_to_nside256 = match (
            summary.output_integrated_flux_sum_ph_cm2_ns,
            total_flux_reference,
        ) {
            (Some(value), Some(reference)) => Some(relative_delta(value, reference)),
            _ => None,
        };
        summary.within_size_budget = summary.release_csv_bytes <= max_release_bytes;
        summary.within_runtime_budget =
            summary.runtime_load_seconds <= args.max_runtime_load_seconds;
        summary.within_empty_pixel_budget =
            summary.empty_pixel_fraction <= args.max_empty_pixel_fraction;
        summary.sufficient_sources_per_nonempty_pixel = summary
            .mean_sources_per_nonempty_pixel
            .is_some_and(|value| value >= args.min_sources_per_nonempty_pixel);
        summary.bright_region_stable = summary
            .bright_region_relative_delta_to_nside256
            .is_some_and(|value| value <= args.max_bright_region_relative_delta);
        summary.high_latitude_noise_acceptable = summary
            .high_latitude_noise_factor_to_nside64
            .is_some_and(|value| value <= args.max_high_latitude_noise_factor);
        summary.total_flux_stable = summary
            .total_flux_relative_delta_to_nside256
            .is_some_and(|value| value <= MAX_TOTAL_FLUX_RELATIVE_DELTA);
        summary.smoothing_recommended = !summary.within_empty_pixel_budget
            || !summary.sufficient_sources_per_nonempty_pixel
            || !summary.high_latitude_noise_acceptable;
        summary.eligible_for_recommendation = summary.production_ready
            && summary.finite_nonnegative_pass
            && summary.spectral_contract_pass
            && summary.plane_pole_pass
            && summary.longitude_wrap_pass
            && summary.independent_comparison_pass
            && summary.within_size_budget
            && summary.within_runtime_budget
            && summary.within_empty_pixel_budget
            && summary.sufficient_sources_per_nonempty_pixel
            && summary.bright_region_stable
            && summary.high_latitude_noise_acceptable
            && summary.total_flux_stable;
    }
}

fn max_release_bytes(args: &Args) -> u64 {
    (args.max_release_mib * 1024.0 * 1024.0).round() as u64
}

fn relative_delta(value: f64, reference: f64) -> f64 {
    (value - reference).abs() / value.abs().max(reference.abs()).max(f64::EPSILON)
}

fn ratio_with_zero_baseline(value: f64, reference: f64) -> f64 {
    if reference.abs() <= f64::EPSILON {
        if value.abs() <= f64::EPSILON {
            1.0
        } else {
            f64::MAX
        }
    } else {
        value / reference
    }
}

fn brightest_region_mean(validation: &Value) -> Option<f64> {
    validation["independent_comparison"]["regions"]
        .as_array()?
        .iter()
        .filter(|region| region["pass"].as_bool().unwrap_or(false))
        .filter_map(|region| region["observed_mean"].as_f64())
        .max_by(f64::total_cmp)
}

fn cargo_tool(bin: &str, args: Vec<String>) -> Result<()> {
    let mut command = Command::new("cargo");
    command.args([
        "run",
        "--locked",
        "--release",
        "-p",
        "nsb-data-tools",
        "--bin",
        bin,
        "--",
    ]);
    command.args(args);
    let status = command.status().context("failed to spawn cargo")?;
    if !status.success() {
        bail!("sweep subcommand failed with status {status}");
    }
    Ok(())
}

fn build_release_tools() -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            "nsb-data-tools",
            "--bin",
            "build_starlight_map",
            "--bin",
            "validate_starlight_map",
            "--bin",
            "pack_starlight_asset",
        ])
        .status()
        .context("failed to build release-mode starlight tools before timing")?;
    if !status.success() {
        bail!("release-mode starlight tool build failed with status {status}");
    }
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not UTF-8: {}", path.display()))
}

fn file_len(path: &Path) -> Result<u64> {
    Ok(std::fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len())
}

fn read_json(path: &Path) -> Result<Value> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let raw = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{raw}\n"))
        .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommendation_maximizes_resolution_only_within_all_evidence_gates() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0),
            fixture_summary(128, 3.0),
            fixture_summary(256, 1.0),
        ];
        assess_candidates(&mut summaries, &args);
        assert!(summaries[0].eligible_for_recommendation);
        assert!(summaries[1].eligible_for_recommendation);
        assert!(!summaries[2].sufficient_sources_per_nonempty_pixel);
        assert!(summaries[2].smoothing_recommended);
        assert!(!summaries[2].eligible_for_recommendation);
        assert_eq!(recommend(&summaries), Some(128));
    }

    #[test]
    fn recommendation_is_absent_when_independent_validation_is_not_production_grade() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0),
            fixture_summary(128, 3.0),
            fixture_summary(256, 2.5),
        ];
        for summary in &mut summaries {
            summary.production_ready = false;
            summary.independent_comparison_pass = false;
        }
        assess_candidates(&mut summaries, &args);
        assert_eq!(recommend(&summaries), None);
    }

    fn fixture_args() -> Args {
        Args {
            input: PathBuf::from("catalogue.csv"),
            output_dir: PathBuf::from("sweep"),
            reference: PathBuf::from("reference.json"),
            catalog_checksum: format!("sha256:{}", "1".repeat(64)),
            catalog_license: "reviewed policy".to_string(),
            generation_date_utc: "2026-07-11T00:00:00Z".to_string(),
            max_release_mib: 256.0,
            max_runtime_load_seconds: 15.0,
            max_empty_pixel_fraction: 0.85,
            min_sources_per_nonempty_pixel: 2.0,
            max_bright_region_relative_delta: 0.10,
            max_high_latitude_noise_factor: 2.0,
        }
    }

    fn fixture_summary(nside: u32, sources_per_nonempty_pixel: f64) -> NsideSummary {
        let pixels = 12 * u64::from(nside) * u64::from(nside);
        NsideSummary {
            nside,
            pixels,
            sources_used: Some(1000),
            mean_sources_per_pixel: Some(1000.0 / pixels as f64),
            mean_sources_per_nonempty_pixel: Some(sources_per_nonempty_pixel),
            map_csv_bytes: 1024,
            release_csv_bytes: 1024,
            generation_seconds: 1.0,
            validation_seconds: 1.0,
            pack_seconds: 1.0,
            runtime_load_seconds: 0.1,
            empty_pixels: pixels / 10,
            empty_pixel_fraction: 0.1,
            input_integrated_flux_sum_ph_cm2_ns: Some(1.0),
            output_integrated_flux_sum_ph_cm2_ns: Some(1.0),
            integrated_flux_relative_error: Some(0.0),
            total_flux_relative_delta_to_nside256: None,
            finite_nonnegative_pass: true,
            spectral_contract_pass: true,
            plane_pole_pass: true,
            plane_mean_ph_cm2_ns_sr: Some(2.0),
            pole_mean_ph_cm2_ns_sr: Some(1.0),
            plane_pole_ratio: Some(2.0),
            longitude_wrap_pass: true,
            longitude_wrap_metric: Some(1.0),
            longitude_wrap_threshold: Some(10.0),
            independent_comparison_pass: true,
            brightest_independent_region_mean_ph_cm2_ns_sr: Some(10.0),
            bright_region_relative_delta_to_nside256: None,
            bright_pixel_p99_ph_cm2_ns_sr: Some(8.0),
            bright_top_one_percent_mean_ph_cm2_ns_sr: Some(10.0),
            high_latitude_noise_mad_ratio: Some(0.2),
            high_latitude_noise_factor_to_nside64: None,
            production_ready: true,
            within_size_budget: false,
            within_runtime_budget: false,
            within_empty_pixel_budget: false,
            sufficient_sources_per_nonempty_pixel: false,
            bright_region_stable: false,
            high_latitude_noise_acceptable: false,
            total_flux_stable: false,
            smoothing_recommended: false,
            eligible_for_recommendation: false,
        }
    }
}
