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
const SUMMARY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Parser)]
#[command(about = "Run the Gaia starlight release nside sweep")]
struct Args {
    /// Canonical Gaia starlight source CSV (required unless --assess-existing).
    #[arg(long)]
    input: Option<PathBuf>,
    /// Sweep output directory.
    #[arg(long, default_value = "target/starlight-release/sweep")]
    output_dir: PathBuf,
    /// Independent validation reference JSON (required unless --assess-existing).
    #[arg(long)]
    reference: Option<PathBuf>,
    /// Source catalogue checksum for the canonical input (optional with --assess-existing).
    #[arg(long)]
    catalog_checksum: Option<String>,
    /// Reviewed Gaia derived-product license or redistribution policy.
    #[arg(long)]
    catalog_license: Option<String>,
    /// UTC generation timestamp.
    #[arg(long)]
    generation_date_utc: Option<String>,
    /// Reassess existing per-nside artefacts without rebuilding maps.
    #[arg(long)]
    assess_existing: bool,
    /// Fail unless a production-ready candidate is selected.
    #[arg(long)]
    require_production_ready: bool,
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

#[derive(Debug, Clone, Serialize)]
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
    flux_conservation_pass: bool,
    plane_pole_pass: bool,
    plane_mean_ph_cm2_ns_sr: Option<f64>,
    pole_mean_ph_cm2_ns_sr: Option<f64>,
    plane_pole_ratio: Option<f64>,
    longitude_wrap_pass: bool,
    longitude_wrap_metric: Option<f64>,
    longitude_wrap_threshold: Option<f64>,
    independent_regions_pass: bool,
    independent_reference_production_use: bool,
    independent_comparison_pass: bool,
    brightest_independent_region_mean_ph_cm2_ns_sr: Option<f64>,
    bright_region_relative_delta_to_nside256: Option<f64>,
    bright_pixel_p99_ph_cm2_ns_sr: Option<f64>,
    bright_top_one_percent_mean_ph_cm2_ns_sr: Option<f64>,
    high_latitude_noise_mad_ratio: Option<f64>,
    high_latitude_noise_factor_to_nside64: Option<f64>,
    candidate_science_ready: bool,
    candidate_operational_ready: bool,
    production_ready: bool,
    within_size_budget: bool,
    within_runtime_budget: bool,
    within_empty_pixel_budget: bool,
    sufficient_sources_per_nonempty_pixel: bool,
    bright_region_stable: bool,
    high_latitude_noise_acceptable: bool,
    total_flux_stable: bool,
    smoothing_recommended: bool,
    eligible_for_candidate_recommendation: bool,
    eligible_for_production: bool,
}

#[derive(Debug, Serialize)]
struct AggregateSummary {
    schema_version: u32,
    photometry_model: &'static str,
    band_nm: [f64; 2],
    recommendation_policy: &'static str,
    smoothing_policy: &'static str,
    selection_constraints: SelectionConstraints,
    recommended_candidate_nside: Option<u32>,
    candidate_recommendation_passed: bool,
    production_ready: bool,
    production_blockers: Vec<&'static str>,
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

    let mut summaries = if args.assess_existing {
        assess_existing_artifacts(&args)?
    } else {
        build_release_tools()?;
        let input = args
            .input
            .as_ref()
            .context("missing --input for full sweep")?;
        let reference = args
            .reference
            .as_ref()
            .context("missing --reference for full sweep")?;
        let catalog_checksum = args
            .catalog_checksum
            .as_ref()
            .context("missing --catalog-checksum for full sweep")?;
        let catalog_license = args
            .catalog_license
            .as_ref()
            .context("missing --catalog-license for full sweep")?;
        let generation_date_utc = args
            .generation_date_utc
            .as_ref()
            .context("missing --generation-date-utc for full sweep")?;
        let mut built = Vec::new();
        for nside in NSIDES {
            built.push(run_one(
                input,
                reference,
                catalog_checksum,
                catalog_license,
                generation_date_utc,
                &args,
                nside,
            )?);
        }
        built
    };

    assess_candidates(&mut summaries, &args);
    finalize_eligibility(&mut summaries);
    let recommended_candidate_nside = recommend_candidate(&summaries);
    let candidate_recommendation_passed = recommended_candidate_nside.is_some();
    let production_blockers = compute_production_blockers(&summaries, recommended_candidate_nside);
    let production_blocker_summary = production_blockers.join(", ");
    let production_ready = candidate_recommendation_passed
        && production_blockers.is_empty()
        && summaries.iter().any(|summary| {
            summary.nside == recommended_candidate_nside.unwrap() && summary.eligible_for_production
        });

    let review_template = args.output_dir.join("nside_review.template.json");
    let aggregate = AggregateSummary {
        schema_version: SUMMARY_SCHEMA_VERSION,
        photometry_model: GAIA_XP_MODEL,
        band_nm: [BAND_MIN_NM, BAND_MAX_NM],
        recommendation_policy: "Choose the highest angular resolution that passes every internal science and operational gate. Candidate recommendation does not require production-grade independent reference evidence. Production promotion remains blocked until reviewed external reference, missing-flux assessment, and redistribution policy gates are satisfied.",
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
        recommended_candidate_nside,
        candidate_recommendation_passed,
        production_ready,
        production_blockers,
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
            schema_version: SUMMARY_SCHEMA_VERSION,
            sweep_report_sha256: format!("sha256:{}", to_hex(&sha256(&summary_raw))),
            reviewed: false,
            selected_nside: recommended_candidate_nside,
            reviewer: None,
            reviewed_at_utc: None,
            rationale: None,
        },
    )?;

    if !candidate_recommendation_passed {
        bail!("no nside candidate satisfies all internal science and operational constraints");
    }
    if args.require_production_ready && !production_ready {
        bail!(
            "candidate recommendation passed but production gates remain blocked: {production_blocker_summary}"
        );
    }
    Ok(())
}

fn validate_args(args: &Args) -> Result<()> {
    if !args.assess_existing {
        if args.input.is_none() {
            bail!("--input is required unless --assess-existing is set");
        }
        if args.reference.is_none() {
            bail!("--reference is required unless --assess-existing is set");
        }
        if args.catalog_checksum.is_none() {
            bail!("--catalog-checksum is required unless --assess-existing is set");
        }
        if args.catalog_license.is_none() {
            bail!("--catalog-license is required unless --assess-existing is set");
        }
        if args.generation_date_utc.is_none() {
            bail!("--generation-date-utc is required unless --assess-existing is set");
        }
    }
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

fn assess_existing_artifacts(args: &Args) -> Result<Vec<NsideSummary>> {
    let mut catalog_checksum = None;
    let mut summaries = Vec::new();
    for nside in NSIDES {
        let summary = load_existing_nside(&args.output_dir, nside, &mut catalog_checksum)?;
        summaries.push(summary);
    }
    Ok(summaries)
}

fn load_existing_nside(
    output_dir: &Path,
    nside: u32,
    catalog_checksum: &mut Option<String>,
) -> Result<NsideSummary> {
    let dir = output_dir.join(format!("nside{nside}"));
    let map = dir.join("starlight_map.csv");
    let diagnostics_path = dir.join("diagnostics.json");
    let validation_path = dir.join("validation.json");
    let release = dir.join("starlight_map.release.csv");
    let manifest_path = dir.join("starlight_map.release.toml");
    for path in [
        &map,
        &diagnostics_path,
        &validation_path,
        &release,
        &manifest_path,
    ] {
        if !path.is_file() {
            bail!("missing required sweep artefact: {}", path.display());
        }
    }

    let diagnostics: Value = read_json(&diagnostics_path)?;
    let validation: Value = read_json(&validation_path)?;
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let manifest_nside = manifest
        .get("map_resolution")
        .and_then(|value| value.as_str())
        .and_then(|text| text.split("nside=").nth(1))
        .and_then(|text| text.split_whitespace().next())
        .and_then(|text| text.parse::<u32>().ok())
        .ok_or_else(|| anyhow::anyhow!("manifest map_resolution missing nside"))?;
    if manifest_nside != nside {
        bail!("manifest nside {manifest_nside} does not match directory nside{nside}");
    }
    let manifest_checksum = manifest
        .get("source_catalogue_checksum")
        .and_then(|value| value.as_str())
        .context("manifest missing source_catalogue_checksum")?
        .to_string();
    match catalog_checksum {
        Some(expected) if expected != &manifest_checksum => {
            bail!(
                "inconsistent source_catalogue_checksum across sweep artefacts: expected {expected}, found {manifest_checksum}"
            );
        }
        None => *catalog_checksum = Some(manifest_checksum.clone()),
        _ => {}
    }
    if let Some(output_sha256) = diagnostics["output_sha256"].as_str() {
        nsb_data_tools::checksum_io::verify_sha256_file(&map, output_sha256, "diagnostics map")?;
    }
    let release_sha256 = manifest.get("map_sha256").and_then(|value| value.as_str());
    if let Some(expected) = release_sha256 {
        nsb_data_tools::checksum_io::verify_sha256_file(&release, expected, "release map")?;
    }
    let diagnostics_nside = diagnostics["nside"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("diagnostics missing nside"))?;
    if diagnostics_nside != u64::from(nside) {
        bail!("diagnostics nside {diagnostics_nside} does not match directory nside{nside}");
    }
    let expected_pixels = 12 * u64::from(nside) * u64::from(nside);
    let diagnostics_pixels = diagnostics["expected_pixels"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("diagnostics missing expected_pixels"))?;
    if diagnostics_pixels != expected_pixels {
        bail!(
            "diagnostics expected_pixels {diagnostics_pixels} does not match HEALPix nside={nside}"
        );
    }
    if diagnostics["photometry_model"].as_str() != Some(GAIA_XP_MODEL) {
        bail!("diagnostics photometry_model does not match the Gaia XP contract");
    }
    if validation["photometry_model"].as_str() != Some(GAIA_XP_MODEL) {
        bail!("validation photometry_model does not match the Gaia XP contract");
    }
    let validation_pixels = validation["pixel_count"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("validation missing pixel_count"))?;
    if validation_pixels != expected_pixels {
        bail!("validation pixel_count {validation_pixels} does not match HEALPix nside={nside}");
    }

    let runtime_load_started = Instant::now();
    let release_raw = std::fs::read_to_string(&release)
        .with_context(|| format!("failed to runtime-load {}", release.display()))?;
    let runtime_map =
        StarlightMap::from_csv_str(&release_raw, StarlightProvenance::test_fixture())?;
    if runtime_map.pixels().len() != usize::try_from(expected_pixels)? {
        bail!("runtime-loaded nside={nside} map has an unexpected pixel count");
    }
    let runtime_load_seconds = runtime_load_started.elapsed().as_secs_f64();

    let (
        independent_regions_pass,
        independent_reference_production_use,
        independent_comparison_pass,
    ) = independent_fields_from_validation(&validation);
    let prior_timing = dir.join("summary.json");
    let (generation_seconds, validation_seconds, pack_seconds) = if prior_timing.is_file() {
        let prior: Value = read_json(&prior_timing)?;
        (
            prior["generation_seconds"].as_f64().unwrap_or(0.0),
            prior["validation_seconds"].as_f64().unwrap_or(0.0),
            prior["pack_seconds"].as_f64().unwrap_or(0.0),
        )
    } else {
        (0.0, 0.0, 0.0)
    };

    let pixels = expected_pixels;
    let empty_pixels = validation["empty_pixels"].as_u64().unwrap_or(pixels);
    let flux_conservation_pass = validation["flux_conservation_pass"]
        .as_bool()
        .unwrap_or(false);
    let summary = NsideSummary {
        nside,
        pixels,
        sources_used: validation["sources_used"].as_u64(),
        mean_sources_per_pixel: validation["mean_sources_per_pixel"].as_f64(),
        mean_sources_per_nonempty_pixel: validation["mean_sources_per_nonempty_pixel"].as_f64(),
        map_csv_bytes: file_len(&map)?,
        release_csv_bytes: file_len(&release)?,
        generation_seconds,
        validation_seconds,
        pack_seconds,
        runtime_load_seconds,
        empty_pixels,
        empty_pixel_fraction: validation["empty_pixel_fraction"].as_f64().unwrap_or(1.0),
        input_integrated_flux_sum_ph_cm2_ns: validation["input_integrated_flux_sum_ph_cm2_ns"]
            .as_f64(),
        output_integrated_flux_sum_ph_cm2_ns: validation["output_integrated_flux_sum_ph_cm2_ns"]
            .as_f64(),
        integrated_flux_relative_error: validation["integrated_flux_relative_error"].as_f64(),
        total_flux_relative_delta_to_nside256: None,
        finite_nonnegative_pass: validation["finite_nonnegative_pass"]
            .as_bool()
            .unwrap_or(false),
        spectral_contract_pass: validation["spectral_contract_pass"]
            .as_bool()
            .unwrap_or(false),
        flux_conservation_pass,
        plane_pole_pass: validation["plane_pole_pass"].as_bool().unwrap_or(false),
        plane_mean_ph_cm2_ns_sr: validation["plane_mean_ph_cm2_ns_sr"].as_f64(),
        pole_mean_ph_cm2_ns_sr: validation["pole_mean_ph_cm2_ns_sr"].as_f64(),
        plane_pole_ratio: validation["plane_pole_ratio"].as_f64(),
        longitude_wrap_pass: validation["longitude_wrap_pass"].as_bool().unwrap_or(false),
        longitude_wrap_metric: validation["longitude_wrap_metric"].as_f64(),
        longitude_wrap_threshold: validation["longitude_wrap_threshold"].as_f64(),
        independent_regions_pass,
        independent_reference_production_use,
        independent_comparison_pass,
        brightest_independent_region_mean_ph_cm2_ns_sr: brightest_region_mean(&validation),
        bright_region_relative_delta_to_nside256: None,
        bright_pixel_p99_ph_cm2_ns_sr: validation["bright_pixel_p99_ph_cm2_ns_sr"].as_f64(),
        bright_top_one_percent_mean_ph_cm2_ns_sr: validation
            ["bright_top_one_percent_mean_ph_cm2_ns_sr"]
            .as_f64(),
        high_latitude_noise_mad_ratio: validation["high_latitude_noise_mad_ratio"].as_f64(),
        high_latitude_noise_factor_to_nside64: None,
        candidate_science_ready: false,
        candidate_operational_ready: false,
        production_ready: validation["production_ready"].as_bool().unwrap_or(false),
        within_size_budget: false,
        within_runtime_budget: false,
        within_empty_pixel_budget: false,
        sufficient_sources_per_nonempty_pixel: false,
        bright_region_stable: false,
        high_latitude_noise_acceptable: false,
        total_flux_stable: false,
        smoothing_recommended: false,
        eligible_for_candidate_recommendation: false,
        eligible_for_production: false,
    };
    write_json(&dir.join("summary.json"), &summary)?;
    Ok(summary)
}

fn run_one(
    input: &Path,
    reference: &Path,
    catalog_checksum: &str,
    catalog_license: &str,
    generation_date_utc: &str,
    args: &Args,
    nside: u32,
) -> Result<NsideSummary> {
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
            path_str(input)?.to_string(),
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
            catalog_license.to_string(),
            "--catalog-checksum".to_string(),
            catalog_checksum.to_string(),
            "--photometry-model".to_string(),
            GAIA_XP_MODEL.to_string(),
            "--band-min-nm".to_string(),
            BAND_MIN_NM.to_string(),
            "--band-max-nm".to_string(),
            BAND_MAX_NM.to_string(),
            "--generation-date-utc".to_string(),
            generation_date_utc.to_string(),
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
            path_str(reference)?.to_string(),
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
    let (
        independent_regions_pass,
        independent_reference_production_use,
        independent_comparison_pass,
    ) = independent_fields_from_validation(&validation_json);
    let flux_conservation_pass = validation_json["flux_conservation_pass"]
        .as_bool()
        .unwrap_or(false);
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
        flux_conservation_pass,
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
        independent_regions_pass,
        independent_reference_production_use,
        independent_comparison_pass,
        brightest_independent_region_mean_ph_cm2_ns_sr: brightest_region_mean(&validation_json),
        bright_region_relative_delta_to_nside256: None,
        bright_pixel_p99_ph_cm2_ns_sr: validation_json["bright_pixel_p99_ph_cm2_ns_sr"].as_f64(),
        bright_top_one_percent_mean_ph_cm2_ns_sr: validation_json
            ["bright_top_one_percent_mean_ph_cm2_ns_sr"]
            .as_f64(),
        high_latitude_noise_mad_ratio: validation_json["high_latitude_noise_mad_ratio"].as_f64(),
        high_latitude_noise_factor_to_nside64: None,
        candidate_science_ready: false,
        candidate_operational_ready: false,
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
        eligible_for_candidate_recommendation: false,
        eligible_for_production: false,
    };
    write_json(&dir.join("summary.json"), &summary)?;
    Ok(summary)
}

fn independent_fields_from_validation(validation: &Value) -> (bool, bool, bool) {
    let regions_pass = validation
        .get("independent_regions_pass")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            validation["independent_comparison"]["regions"]
                .as_array()
                .is_some_and(|regions| {
                    regions
                        .iter()
                        .all(|region| region["pass"].as_bool().unwrap_or(false))
                })
        });
    let production_use = validation
        .get("independent_reference_production_use")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            validation["independent_comparison"]["production_use"]
                .as_bool()
                .unwrap_or(false)
        });
    let comparison_pass = validation
        .get("independent_comparison_pass")
        .and_then(Value::as_bool)
        .unwrap_or(regions_pass && production_use);
    (regions_pass, production_use, comparison_pass)
}

fn candidate_science_ready(summary: &NsideSummary) -> bool {
    summary.finite_nonnegative_pass
        && summary.spectral_contract_pass
        && summary.flux_conservation_pass
        && summary.plane_pole_pass
        && summary.longitude_wrap_pass
}

fn candidate_operational_ready(summary: &NsideSummary) -> bool {
    summary.within_size_budget
        && summary.within_runtime_budget
        && summary.within_empty_pixel_budget
        && summary.sufficient_sources_per_nonempty_pixel
        && summary.bright_region_stable
        && summary.high_latitude_noise_acceptable
        && summary.total_flux_stable
}

fn finalize_eligibility(summaries: &mut [NsideSummary]) {
    for summary in summaries {
        summary.candidate_science_ready = candidate_science_ready(summary);
        summary.candidate_operational_ready = candidate_operational_ready(summary);
        summary.eligible_for_candidate_recommendation =
            summary.candidate_science_ready && summary.candidate_operational_ready;
        summary.eligible_for_production = summary.eligible_for_candidate_recommendation
            && summary.independent_comparison_pass
            && summary.independent_reference_production_use
            && missing_flux_report_approved()
            && redistribution_policy_approved();
    }
}

fn missing_flux_report_approved() -> bool {
    false
}

fn redistribution_policy_approved() -> bool {
    false
}

fn compute_production_blockers(
    summaries: &[NsideSummary],
    recommended: Option<u32>,
) -> Vec<&'static str> {
    let mut blockers = Vec::new();
    if let Some(nside) = recommended {
        if let Some(summary) = summaries.iter().find(|entry| entry.nside == nside) {
            if !summary.independent_reference_production_use {
                blockers.push("independent_reference_not_approved_for_production");
            }
        }
    }
    if !missing_flux_report_approved() {
        blockers.push("missing_flux_report_not_approved");
    }
    if !redistribution_policy_approved() {
        blockers.push("redistribution_policy_not_approved");
    }
    blockers
}

fn recommend_candidate(summaries: &[NsideSummary]) -> Option<u32> {
    summaries
        .iter()
        .filter(|summary| summary.eligible_for_candidate_recommendation)
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
    fn candidate_recommendation_ignores_provisional_reference() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, false),
            fixture_summary(128, 3.0, true, false),
            fixture_summary(256, 2.5, true, false),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert_eq!(recommend_candidate(&summaries), Some(256));
        let blockers = compute_production_blockers(&summaries, Some(256));
        assert!(blockers.contains(&"independent_reference_not_approved_for_production"));
        assert!(!summaries[2].eligible_for_production);
        assert!(summaries[2].eligible_for_candidate_recommendation);
    }

    #[test]
    fn recommendation_maximizes_resolution_only_within_internal_gates() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, false),
            fixture_summary(128, 3.0, true, false),
            fixture_summary(256, 1.0, true, false),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(summaries[0].eligible_for_candidate_recommendation);
        assert!(summaries[1].eligible_for_candidate_recommendation);
        assert!(!summaries[2].sufficient_sources_per_nonempty_pixel);
        assert!(summaries[2].smoothing_recommended);
        assert!(!summaries[2].eligible_for_candidate_recommendation);
        assert_eq!(recommend_candidate(&summaries), Some(128));
    }

    #[test]
    fn production_can_be_eligible_only_with_approved_external_gates() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true),
            fixture_summary(128, 3.0, true, true),
            fixture_summary(256, 2.5, true, true),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(summaries[2].eligible_for_candidate_recommendation);
        assert!(!summaries[2].eligible_for_production);
    }

    #[test]
    fn provisional_reference_never_enables_production_ready() {
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(256, 2.5, true, false)];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(!summaries[0].eligible_for_production);
        assert!(!summaries[0].production_ready);
    }

    #[test]
    fn recommendation_is_absent_when_no_internal_candidate_passes() {
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(256, 0.5, true, false)];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert_eq!(recommend_candidate(&summaries), None);
    }

    #[test]
    fn oversized_candidate_is_ineligible() {
        let mut args = fixture_args();
        args.max_release_mib = 0.0005;
        let mut summaries = vec![fixture_summary(256, 2.5, true, false)];
        summaries[0].release_csv_bytes = 1_000_000;
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(!summaries[0].within_size_budget);
        assert!(!summaries[0].eligible_for_candidate_recommendation);
    }

    #[test]
    fn independent_fields_derive_from_legacy_validation_schema() {
        let validation = serde_json::json!({
            "independent_comparison_pass": false,
            "independent_comparison": {
                "production_use": false,
                "regions": [
                    {"pass": true},
                    {"pass": true}
                ]
            }
        });
        let (regions, production_use, comparison) = independent_fields_from_validation(&validation);
        assert!(regions);
        assert!(!production_use);
        assert!(!comparison);
    }

    fn fixture_args() -> Args {
        Args {
            input: Some(PathBuf::from("catalogue.csv")),
            output_dir: PathBuf::from("sweep"),
            reference: Some(PathBuf::from("reference.json")),
            catalog_checksum: Some(format!("sha256:{}", "1".repeat(64))),
            catalog_license: Some("reviewed policy".to_string()),
            generation_date_utc: Some("2026-07-11T00:00:00Z".to_string()),
            assess_existing: false,
            require_production_ready: false,
            max_release_mib: 256.0,
            max_runtime_load_seconds: 15.0,
            max_empty_pixel_fraction: 0.85,
            min_sources_per_nonempty_pixel: 2.0,
            max_bright_region_relative_delta: 0.10,
            max_high_latitude_noise_factor: 2.0,
        }
    }

    fn fixture_summary(
        nside: u32,
        sources_per_nonempty_pixel: f64,
        flux_conservation_pass: bool,
        production_reference: bool,
    ) -> NsideSummary {
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
            flux_conservation_pass,
            plane_pole_pass: true,
            plane_mean_ph_cm2_ns_sr: Some(2.0),
            pole_mean_ph_cm2_ns_sr: Some(1.0),
            plane_pole_ratio: Some(2.0),
            longitude_wrap_pass: true,
            longitude_wrap_metric: Some(1.0),
            longitude_wrap_threshold: Some(10.0),
            independent_regions_pass: true,
            independent_reference_production_use: production_reference,
            independent_comparison_pass: production_reference,
            brightest_independent_region_mean_ph_cm2_ns_sr: Some(10.0),
            bright_region_relative_delta_to_nside256: None,
            bright_pixel_p99_ph_cm2_ns_sr: Some(8.0),
            bright_top_one_percent_mean_ph_cm2_ns_sr: Some(10.0),
            high_latitude_noise_mad_ratio: Some(0.2),
            high_latitude_noise_factor_to_nside64: None,
            candidate_science_ready: false,
            candidate_operational_ready: false,
            production_ready: production_reference,
            within_size_budget: false,
            within_runtime_budget: false,
            within_empty_pixel_budget: false,
            sufficient_sources_per_nonempty_pixel: false,
            bright_region_stable: false,
            high_latitude_noise_acceptable: false,
            total_flux_stable: false,
            smoothing_recommended: false,
            eligible_for_candidate_recommendation: false,
            eligible_for_production: false,
        }
    }
}
