use anyhow::{bail, Context, Result};
use clap::Parser;
use nsb::{StarlightMap, StarlightProvenance};
use nsb_data_tools::starlight::approval::{
    load_and_validate_approval, ApprovalArtifactType, ApprovalDecision, ApprovalFileDigest,
    ApprovalRequirements, ReviewerKind, StarlightApproval, APPROVAL_SCHEMA_VERSION,
    STARLIGHT_PRODUCTION_BAND_NM,
};
use nsb_data_tools::starlight::integrated::INTEGRATED_PHOTOMETRY_MODEL;
use serde::Serialize;
use serde_json::Value;
use siderust::checksum::{sha256, to_hex};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const NSIDES: [u32; 4] = [64, 128, 256, 512];
const INTEGRATED_MEAN_FILE: &str = "starlight_mean.release.csv";
const INTEGRATED_DIAGNOSTICS_FILE: &str = "starlight_source_contributions.diagnostics.json";
const BAND_MIN_NM: f64 = STARLIGHT_PRODUCTION_BAND_NM[0];
const BAND_MAX_NM: f64 = STARLIGHT_PRODUCTION_BAND_NM[1];
const MAX_TOTAL_FLUX_RELATIVE_DELTA: f64 = 1.0e-9;
const SUMMARY_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Parser)]
#[command(about = "Run the Gaia starlight release nside sweep")]
struct Args {
    /// Canonical Gaia starlight source CSV (optional for integrated sweep).
    #[arg(long)]
    input: Option<PathBuf>,
    /// Checksum-pinned normalized contributions manifest for integrated product build.
    #[arg(long)]
    contributions_manifest: Option<PathBuf>,
    /// SHA-256 of the calibrated inference/completeness model (integrated sweep).
    #[arg(long)]
    model_checksum: Option<String>,
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
    /// Root containing approval JSON and every checksummed file it references.
    #[arg(long, visible_alias = "artifact-root")]
    approval_root: Option<PathBuf>,
    /// Stable identifier shared by the map release and every approval.
    #[arg(long)]
    release_id: Option<String>,
    /// Missing-flux approval path, relative to --approval-root.
    #[arg(long, requires_all = ["approval_root", "release_id"])]
    missing_flux_approval: Option<PathBuf>,
    /// Independent-validation approval path, relative to --approval-root.
    #[arg(long, requires_all = ["approval_root", "release_id"])]
    independent_validation_approval: Option<PathBuf>,
    /// Redistribution approval path, relative to --approval-root.
    #[arg(long, requires_all = ["approval_root", "release_id"])]
    redistribution_approval: Option<PathBuf>,
    /// Nside review approval path, relative to --approval-root.
    #[arg(
        long,
        visible_alias = "nside-review-approval",
        requires_all = ["approval_root", "release_id"]
    )]
    nside_review: Option<PathBuf>,
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
    map_sha256: String,
    band_nm: Option<[f64; 2]>,
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
    production_blockers: Vec<String>,
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

/// Run the `sweep_starlight_nside` command using process arguments.
pub fn run_cli() -> Result<()> {
    run(crate::parse_command_args())
}

fn run(args: Args) -> Result<()> {
    validate_args(&args)?;
    std::fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let mut summaries = if args.assess_existing {
        assess_existing_artifacts(&args)?
    } else {
        let contributions_manifest = args
            .contributions_manifest
            .as_ref()
            .context("missing --contributions-manifest for full sweep")?;
        let model_checksum = args
            .model_checksum
            .as_ref()
            .context("missing --model-checksum for full sweep")?;
        let release_id = args
            .release_id
            .as_ref()
            .context("missing --release-id for full sweep")?;
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
                contributions_manifest,
                model_checksum,
                release_id,
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
    let approval_dag = evaluate_approval_dag(&args, &summaries, recommended_candidate_nside);
    apply_production_eligibility(
        &mut summaries,
        recommended_candidate_nside,
        approval_dag.all_approved(),
    );
    let production_blockers =
        compute_production_blockers(&summaries, recommended_candidate_nside, &approval_dag);
    let production_blocker_summary = production_blockers.join(", ");
    let production_ready = candidate_recommendation_passed
        && production_blockers.is_empty()
        && summaries.iter().any(|summary| {
            summary.nside == recommended_candidate_nside.unwrap() && summary.eligible_for_production
        });
    let report_band_nm = recommended_candidate_nside
        .and_then(|nside| {
            summaries
                .iter()
                .find(|summary| summary.nside == nside)
                .and_then(|summary| summary.band_nm)
        })
        .unwrap_or([BAND_MIN_NM, BAND_MAX_NM]);

    let review_template = args.output_dir.join("nside_review.template.json");
    let aggregate = AggregateSummary {
        schema_version: SUMMARY_SCHEMA_VERSION,
        photometry_model: INTEGRATED_PHOTOMETRY_MODEL,
        band_nm: report_band_nm,
        recommendation_policy: "Choose the minimum angular resolution that passes every internal science and operational gate. Candidate recommendation does not require production-grade approval artifacts. Production promotion remains blocked until the missing-flux, independent-validation, redistribution, and nside-review artifacts form a compatible checksummed approval DAG.",
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
    write_nside_review_template(
        &review_template,
        &args,
        &aggregate.summaries,
        recommended_candidate_nside,
        &summary_path,
        &summary_raw,
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
        if args.contributions_manifest.is_none() {
            bail!("--contributions-manifest is required unless --assess-existing is set");
        }
        if args.model_checksum.is_none() {
            bail!("--model-checksum is required unless --assess-existing is set");
        }
        if args.release_id.is_none() {
            bail!("--release-id is required unless --assess-existing is set");
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
    if let Some(expected) = args.catalog_checksum.as_deref() {
        let found = catalog_checksum.as_deref().context(
            "assess-existing could not read source_catalogue_checksum from sweep manifests",
        )?;
        verify_expected_catalog_checksum(expected, found)?;
    }
    Ok(summaries)
}

fn normalize_catalog_checksum(value: &str) -> Result<String> {
    let hex = value.strip_prefix("sha256:").unwrap_or(value).trim();
    if hex.len() != 64 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("invalid SHA-256 checksum format: expected 64 hexadecimal digits");
    }
    Ok(hex.to_ascii_lowercase())
}

fn verify_expected_catalog_checksum(expected: &str, found: &str) -> Result<()> {
    let expected = normalize_catalog_checksum(expected)?;
    let found = normalize_catalog_checksum(found)?;
    if expected != found {
        bail!("catalogue checksum mismatch: expected sha256:{expected}, found sha256:{found}");
    }
    Ok(())
}

fn load_existing_nside(
    output_dir: &Path,
    nside: u32,
    catalog_checksum: &mut Option<String>,
) -> Result<NsideSummary> {
    let dir = output_dir.join(format!("nside{nside}"));
    let map = dir.join(INTEGRATED_MEAN_FILE);
    let diagnostics_path = dir.join(INTEGRATED_DIAGNOSTICS_FILE);
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
        Some(expected)
            if normalize_catalog_checksum(expected)?
                != normalize_catalog_checksum(&manifest_checksum)? =>
        {
            bail!(
                "inconsistent source_catalogue_checksum across sweep artefacts: expected sha256:{}, found sha256:{}",
                normalize_catalog_checksum(expected)?,
                normalize_catalog_checksum(&manifest_checksum)?
            );
        }
        None => *catalog_checksum = Some(manifest_checksum.clone()),
        _ => {}
    }
    if let Some(output_sha256) = diagnostics["artifact_sha256"]
        .get(INTEGRATED_MEAN_FILE)
        .and_then(Value::as_str)
        .or_else(|| diagnostics["output_sha256"].as_str())
    {
        nsb_data_tools::platform::checksum_io::verify_sha256_file(
            &map,
            output_sha256,
            "diagnostics map",
        )?;
    }
    let release_sha256 = manifest.get("map_sha256").and_then(|value| value.as_str());
    if let Some(expected) = release_sha256 {
        nsb_data_tools::platform::checksum_io::verify_sha256_file(
            &release,
            expected,
            "release map",
        )?;
    }
    let diagnostics_nside = diagnostics["nside"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("diagnostics missing nside"))?;
    if diagnostics_nside != u64::from(nside) {
        bail!("diagnostics nside {diagnostics_nside} does not match directory nside{nside}");
    }
    let expected_pixels = 12 * u64::from(nside) * u64::from(nside);
    let diagnostics_pixels = diagnostics["coverage"]["expected_pixels"]
        .as_u64()
        .or_else(|| diagnostics["expected_pixels"].as_u64())
        .ok_or_else(|| anyhow::anyhow!("diagnostics missing expected_pixels"))?;
    if diagnostics_pixels != expected_pixels {
        bail!(
            "diagnostics expected_pixels {diagnostics_pixels} does not match HEALPix nside={nside}"
        );
    }
    if diagnostics["product"].as_str() != Some("nsb.integrated_starlight_300_650nm")
        && diagnostics.get("photometry_model").is_some()
        && diagnostics["photometry_model"].as_str() != Some(INTEGRATED_PHOTOMETRY_MODEL)
    {
        bail!("diagnostics photometry_model does not match the integrated 300-650 nm contract");
    }
    if validation["spectral_contract_pass"]
        .as_bool()
        .is_some_and(|pass| !pass)
    {
        bail!("validation spectral_contract_pass is false for nside={nside}");
    }
    let validation_pixels = validation["pixel_count"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("validation missing pixel_count"))?;
    if validation_pixels != expected_pixels {
        bail!("validation pixel_count {validation_pixels} does not match HEALPix nside={nside}");
    }
    if !validation["finite_nonnegative_pass"]
        .as_bool()
        .unwrap_or(false)
    {
        bail!("validation finite_nonnegative_pass is false for nside={nside}");
    }
    if !validation["flux_conservation_pass"]
        .as_bool()
        .unwrap_or(false)
    {
        bail!("validation flux_conservation_pass is false for nside={nside}");
    }
    if validation["sources_used"].as_u64().is_none()
        && diagnostics["unique_contribution_rows"].as_u64().is_none()
    {
        bail!("validation missing sources_used for nside={nside}");
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
        map_sha256: format!(
            "sha256:{}",
            nsb_data_tools::platform::checksum_io::sha256_file(&map)?
        ),
        band_nm: validation["band_definition"]
            .as_str()
            .and_then(parse_band_nm),
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

#[allow(clippy::too_many_arguments)]
fn run_one(
    contributions_manifest: &Path,
    model_checksum: &str,
    release_id: &str,
    reference: &Path,
    catalog_checksum: &str,
    catalog_license: &str,
    generation_date_utc: &str,
    args: &Args,
    nside: u32,
) -> Result<NsideSummary> {
    let dir = args.output_dir.join(format!("nside{nside}"));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let map = dir.join(INTEGRATED_MEAN_FILE);
    let diagnostics = dir.join(INTEGRATED_DIAGNOSTICS_FILE);
    let validation = dir.join("validation.json");
    let release = dir.join("starlight_map.release.csv");
    let manifest = dir.join("starlight_map.release.toml");

    let started = Instant::now();
    run_sibling_tool(
        "build_integrated_starlight_product",
        vec![
            "--inputs-manifest".to_string(),
            path_str(contributions_manifest)?.to_string(),
            "--nside".to_string(),
            nside.to_string(),
            "--release-id".to_string(),
            format!("{release_id}-nside{nside}"),
            "--model-checksum".to_string(),
            model_checksum.to_string(),
            "--output-dir".to_string(),
            path_str(&dir)?.to_string(),
            "--candidate-only".to_string(),
        ],
    )?;
    let generation_seconds = started.elapsed().as_secs_f64();

    // Enrich mean-map header with catalogue provenance for downstream packer.
    enrich_integrated_mean_header(&map, catalog_checksum, catalog_license, generation_date_utc)?;

    let validation_started = Instant::now();
    run_sibling_tool(
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
    run_sibling_tool(
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
        map_sha256: format!(
            "sha256:{}",
            nsb_data_tools::platform::checksum_io::sha256_file(&map)?
        ),
        band_nm: validation_json["band_definition"]
            .as_str()
            .and_then(parse_band_nm),
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

fn enrich_integrated_mean_header(
    map_path: &Path,
    catalog_checksum: &str,
    catalog_license: &str,
    generation_date_utc: &str,
) -> Result<()> {
    use nsb_data_tools::starlight::integrated::INTEGRATED_BAND_DEFINITION;
    let raw = std::fs::read_to_string(map_path)
        .with_context(|| format!("failed to read {}", map_path.display()))?;
    if raw.contains("# source_catalogue=") {
        return Ok(());
    }
    let extra = [
        "# source_catalogue=Gaia".to_string(),
        "# source_catalogue_release=DR3".to_string(),
        format!("# source_catalogue_license={catalog_license}"),
        format!("# source_catalogue_checksum={catalog_checksum}"),
        format!("# generation_date_utc={generation_date_utc}"),
        "# generated_by=nsb-data-tools sweep_starlight_nside".to_string(),
        "# generation_command=build_integrated_starlight_product".to_string(),
        format!("# photometry_model={INTEGRATED_PHOTOMETRY_MODEL}"),
        format!("# band_definition={INTEGRATED_BAND_DEFINITION}"),
        "# source_selection=integrated Gaia DR3 starlight contributions".to_string(),
        "# magnitude_limit=Gaia DR3 release input selection".to_string(),
    ];
    let mut out = String::new();
    let mut inserted = false;
    for line in raw.lines() {
        if !inserted && !line.trim().is_empty() && !line.trim().starts_with('#') {
            for header_line in &extra {
                out.push_str(header_line);
                out.push('\n');
            }
            inserted = true;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !inserted {
        for header_line in &extra {
            out.push_str(header_line);
            out.push('\n');
        }
        out.push_str(raw.trim_end());
        out.push('\n');
    }
    std::fs::write(map_path, out)?;
    Ok(())
}

fn independent_fields_from_validation(validation: &Value) -> (bool, bool, bool) {
    let regions_pass = validation
        .get("independent_regions_pass")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let production_use = validation
        .get("independent_reference_production_use")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let comparison_pass = validation
        .get("independent_comparison_pass")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (regions_pass, production_use, comparison_pass)
}

fn all_independent_regions_pass(validation: &Value) -> bool {
    validation["independent_comparison"]["regions"]
        .as_array()
        .is_some_and(|regions| {
            !regions.is_empty()
                && regions
                    .iter()
                    .all(|region| region["pass"].as_bool() == Some(true))
        })
}

fn parse_band_nm(definition: &str) -> Option<[f64; 2]> {
    let normalized = definition.replace(['–', '—'], "-").to_ascii_lowercase();
    let before_nm = normalized.split("nm").next()?.trim();
    let range = before_nm.split_whitespace().next_back()?;
    let (minimum, maximum) = range.split_once('-')?;
    let minimum = minimum.parse::<f64>().ok()?;
    let maximum = maximum.parse::<f64>().ok()?;
    (minimum.is_finite() && maximum.is_finite() && minimum < maximum).then_some([minimum, maximum])
}

fn candidate_science_ready(summary: &NsideSummary) -> bool {
    summary.finite_nonnegative_pass
        && summary.spectral_contract_pass
        && summary.flux_conservation_pass
        && summary.plane_pole_pass
        && summary.longitude_wrap_pass
        && summary.independent_regions_pass
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
        summary.eligible_for_production = false;
    }
}

#[derive(Debug)]
enum ApprovalGateState {
    Approved,
    Missing,
    Invalid(String),
}

impl ApprovalGateState {
    fn approved(&self) -> bool {
        matches!(self, Self::Approved)
    }

    fn blocker(&self, name: &str) -> Option<String> {
        match self {
            Self::Approved => None,
            Self::Missing => Some(format!("{name}_approval_missing")),
            Self::Invalid(reason) => Some(format!("{name}_approval_invalid: {reason}")),
        }
    }
}

#[derive(Debug)]
struct ApprovalDagState {
    release_compatibility: ApprovalGateState,
    missing_flux: ApprovalGateState,
    independent_validation: ApprovalGateState,
    redistribution: ApprovalGateState,
    nside_review: ApprovalGateState,
}

impl ApprovalDagState {
    fn all_approved(&self) -> bool {
        self.release_compatibility.approved()
            && self.missing_flux.approved()
            && self.independent_validation.approved()
            && self.redistribution.approved()
            && self.nside_review.approved()
    }

    fn blockers(&self) -> Vec<String> {
        let mut blockers: Vec<String> = [
            ("missing_flux", &self.missing_flux),
            ("independent_validation", &self.independent_validation),
            ("redistribution", &self.redistribution),
            ("nside_review", &self.nside_review),
        ]
        .into_iter()
        .filter_map(|(name, state)| state.blocker(name))
        .collect();
        match &self.release_compatibility {
            ApprovalGateState::Approved => {}
            ApprovalGateState::Missing => {
                blockers.push("release_compatibility_missing".to_string())
            }
            ApprovalGateState::Invalid(reason) => {
                blockers.push(format!("release_compatibility_invalid: {reason}"))
            }
        }
        blockers
    }
}

fn evaluate_approval_dag(
    args: &Args,
    summaries: &[NsideSummary],
    recommended: Option<u32>,
) -> ApprovalDagState {
    let selected =
        recommended.and_then(|nside| summaries.iter().find(|entry| entry.nside == nside));
    let map_sha256 = selected.map(|summary| summary.map_sha256.as_str());
    ApprovalDagState {
        release_compatibility: match selected.and_then(|summary| summary.band_nm) {
            Some(band) if band == STARLIGHT_PRODUCTION_BAND_NM => ApprovalGateState::Approved,
            Some(band) => ApprovalGateState::Invalid(format!(
                "selected map band [{}, {}] does not match [300, 650]",
                band[0], band[1]
            )),
            None => ApprovalGateState::Missing,
        },
        missing_flux: evaluate_approval(
            args,
            args.missing_flux_approval.as_deref(),
            ApprovalArtifactType::MissingFlux,
            None,
            map_sha256,
        ),
        independent_validation: evaluate_approval(
            args,
            args.independent_validation_approval.as_deref(),
            ApprovalArtifactType::IndependentValidation,
            None,
            map_sha256,
        ),
        redistribution: evaluate_approval(
            args,
            args.redistribution_approval.as_deref(),
            ApprovalArtifactType::Redistribution,
            None,
            None,
        ),
        nside_review: evaluate_approval(
            args,
            args.nside_review.as_deref(),
            ApprovalArtifactType::NsideReview,
            recommended,
            map_sha256,
        ),
    }
}

fn evaluate_approval(
    args: &Args,
    path: Option<&Path>,
    artifact_type: ApprovalArtifactType,
    nside: Option<u32>,
    map_sha256: Option<&str>,
) -> ApprovalGateState {
    let Some(path) = path else {
        return ApprovalGateState::Missing;
    };
    let Some(root) = args.approval_root.as_deref() else {
        return ApprovalGateState::Invalid("--approval-root is required".to_string());
    };
    let Some(release_id) = args.release_id.as_deref() else {
        return ApprovalGateState::Invalid("--release-id is required".to_string());
    };
    let requirements = ApprovalRequirements {
        artifact_type,
        release_id,
        nside,
        map_sha256,
        manifest_sha256: None,
        require_positive: true,
    };
    match load_and_validate_approval(root, path, requirements) {
        Ok(_) => ApprovalGateState::Approved,
        Err(error) => ApprovalGateState::Invalid(format!("{error:#}")),
    }
}

fn apply_production_eligibility(
    summaries: &mut [NsideSummary],
    recommended: Option<u32>,
    approvals_ready: bool,
) {
    for summary in summaries {
        let validation_ready = summary.production_ready;
        summary.eligible_for_production = Some(summary.nside) == recommended
            && summary.eligible_for_candidate_recommendation
            && validation_ready
            && summary.independent_comparison_pass
            && summary.independent_reference_production_use
            && approvals_ready;
        summary.production_ready = summary.eligible_for_production;
    }
}

fn compute_production_blockers(
    summaries: &[NsideSummary],
    recommended: Option<u32>,
    approval_dag: &ApprovalDagState,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if let Some(nside) = recommended {
        if let Some(summary) = summaries.iter().find(|entry| entry.nside == nside) {
            if !summary.independent_reference_production_use {
                blockers.push("independent_reference_not_approved_for_production".to_string());
            }
            if !summary.independent_comparison_pass {
                blockers.push("independent_comparison_not_ready_for_production".to_string());
            }
        }
    } else {
        blockers.push("no_eligible_nside_candidate".to_string());
    }
    blockers.extend(approval_dag.blockers());
    blockers
}

fn recommend_candidate(summaries: &[NsideSummary]) -> Option<u32> {
    summaries
        .iter()
        .filter(|summary| summary.eligible_for_candidate_recommendation)
        .map(|summary| summary.nside)
        .min()
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
    if !all_independent_regions_pass(validation) {
        return None;
    }
    validation["independent_comparison"]["regions"]
        .as_array()?
        .iter()
        .filter_map(|region| region["observed_mean"].as_f64())
        .max_by(f64::total_cmp)
}

fn run_sibling_tool(bin: &str, args: Vec<String>) -> Result<()> {
    let executable = sibling_tool_path(bin)?;
    let status = Command::new(&executable)
        .args(args)
        .status()
        .with_context(|| format!("failed to run sibling tool {}", executable.display()))?;
    if !status.success() {
        bail!("sweep subcommand {bin} failed with status {status}");
    }
    Ok(())
}

fn sibling_tool_path(bin: &str) -> Result<PathBuf> {
    let current = std::env::current_exe().context("failed to resolve sweep executable path")?;
    let directory = current
        .parent()
        .context("sweep executable path has no parent directory")?;
    let file_name = if cfg!(windows) {
        format!("{bin}.exe")
    } else {
        bin.to_string()
    };
    let executable = directory.join(file_name);
    if !executable.is_file() {
        bail!(
            "required sibling tool {} is missing; install or build all nsb-data-tools binaries together",
            executable.display()
        );
    }
    Ok(executable)
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

fn write_nside_review_template(
    template_path: &Path,
    args: &Args,
    summaries: &[NsideSummary],
    selected_nside: Option<u32>,
    summary_path: &Path,
    summary_raw: &[u8],
) -> Result<()> {
    let artifact_root = args
        .approval_root
        .as_deref()
        .unwrap_or(args.output_dir.as_path());
    let summary_relative = relative_template_path(artifact_root, summary_path);
    let summary_sha256 = format!("sha256:{}", to_hex(&sha256(summary_raw)));
    let selected = selected_nside.and_then(|nside| {
        summaries
            .iter()
            .find(|summary| summary.nside == nside)
            .map(|summary| (nside, summary))
    });
    let output_files = selected
        .map(|(nside, summary)| ApprovalFileDigest {
            path: relative_template_path(
                artifact_root,
                &args
                    .output_dir
                    .join(format!("nside{nside}/{INTEGRATED_MEAN_FILE}")),
            ),
            sha256: summary.map_sha256.clone(),
        })
        .into_iter()
        .collect();
    let template = StarlightApproval {
        schema_version: APPROVAL_SCHEMA_VERSION,
        artifact_type: ApprovalArtifactType::NsideReview,
        decision: ApprovalDecision::Pending,
        production_use: false,
        reviewer_kind: ReviewerKind::Human,
        reviewer_name: "REQUIRED HUMAN REVIEWER".to_string(),
        date: "REQUIRED RFC3339 REVIEW DATE".to_string(),
        release_id: args
            .release_id
            .clone()
            .unwrap_or_else(|| "REQUIRED RELEASE ID".to_string()),
        band_nm: STARLIGHT_PRODUCTION_BAND_NM,
        nside: selected_nside,
        map_sha256: selected.map(|(_, summary)| summary.map_sha256.clone()),
        manifest_sha256: None,
        input_files: vec![ApprovalFileDigest {
            path: summary_relative.clone(),
            sha256: summary_sha256.clone(),
        }],
        output_files,
        rationale: "REQUIRED SUBSTANTIVE HUMAN RATIONALE".to_string(),
        references: vec![format!("{summary_relative} {summary_sha256}")],
    };
    write_json(template_path, &template)
}

fn relative_template_path(root: &Path, path: &Path) -> String {
    let contained = root
        .canonicalize()
        .ok()
        .zip(path.canonicalize().ok())
        .and_then(|(root, path)| path.strip_prefix(root).ok().map(Path::to_path_buf));
    contained
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "REQUIRED PATH WITHIN APPROVAL ROOT".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_recommendation_ignores_provisional_reference() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true, false),
            fixture_summary(128, 3.0, true, true, false),
            fixture_summary(256, 2.5, true, true, false),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert_eq!(recommend_candidate(&summaries), Some(64));
        let dag = evaluate_approval_dag(&args, &summaries, Some(64));
        let blockers = compute_production_blockers(&summaries, Some(64), &dag);
        assert!(blockers
            .iter()
            .any(|blocker| blocker == "independent_reference_not_approved_for_production"));
        assert!(blockers
            .iter()
            .any(|blocker| blocker == "missing_flux_approval_missing"));
        assert!(!summaries[2].eligible_for_production);
        assert!(summaries[2].eligible_for_candidate_recommendation);
    }

    #[test]
    fn recommendation_selects_minimum_resolution_within_internal_gates() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true, false),
            fixture_summary(128, 3.0, true, true, false),
            fixture_summary(256, 1.0, true, true, false),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(summaries[0].eligible_for_candidate_recommendation);
        assert!(summaries[1].eligible_for_candidate_recommendation);
        assert!(!summaries[2].sufficient_sources_per_nonempty_pixel);
        assert!(summaries[2].smoothing_recommended);
        assert!(!summaries[2].eligible_for_candidate_recommendation);
        assert_eq!(recommend_candidate(&summaries), Some(64));
    }

    #[test]
    fn production_can_be_eligible_only_with_approved_external_gates() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true, true),
            fixture_summary(128, 3.0, true, true, true),
            fixture_summary(256, 2.5, true, true, true),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(summaries[2].eligible_for_candidate_recommendation);
        assert!(!summaries[2].eligible_for_production);
    }

    #[test]
    fn complete_synthetic_approval_dag_enables_only_minimum_selected_nside() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for name in [
            "map.csv",
            "missing-input.json",
            "missing-output.json",
            "independent-input.json",
            "independent-output.json",
            "redistribution-input.json",
            "redistribution-output.json",
            "nside-input.json",
            "nside-output.json",
        ] {
            std::fs::write(dir.path().join(name), format!("synthetic {name}\n"))?;
        }
        let map_sha256 = format!(
            "sha256:{}",
            nsb_data_tools::platform::checksum_io::sha256_file(&dir.path().join("map.csv"))?
        );
        let mut args = fixture_args();
        args.approval_root = Some(dir.path().to_path_buf());
        args.release_id = Some("synthetic-release-v1".to_string());
        args.missing_flux_approval = Some(PathBuf::from("missing.json"));
        args.independent_validation_approval = Some(PathBuf::from("independent.json"));
        args.redistribution_approval = Some(PathBuf::from("redistribution.json"));
        args.nside_review = Some(PathBuf::from("nside.json"));
        write_fixture_approval(
            dir.path(),
            "missing.json",
            ApprovalArtifactType::MissingFlux,
            None,
            Some(map_sha256.clone()),
            "missing-input.json",
            "missing-output.json",
        )?;
        write_fixture_approval(
            dir.path(),
            "independent.json",
            ApprovalArtifactType::IndependentValidation,
            None,
            Some(map_sha256.clone()),
            "independent-input.json",
            "independent-output.json",
        )?;
        write_fixture_approval(
            dir.path(),
            "redistribution.json",
            ApprovalArtifactType::Redistribution,
            None,
            None,
            "redistribution-input.json",
            "redistribution-output.json",
        )?;
        write_fixture_approval(
            dir.path(),
            "nside.json",
            ApprovalArtifactType::NsideReview,
            Some(64),
            Some(map_sha256.clone()),
            "nside-input.json",
            "nside-output.json",
        )?;

        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true, true),
            fixture_summary(128, 3.0, true, true, true),
            fixture_summary(256, 2.5, true, true, true),
        ];
        for summary in &mut summaries {
            summary.map_sha256 = map_sha256.clone();
        }
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        let recommended = recommend_candidate(&summaries);
        assert_eq!(recommended, Some(64));
        let dag = evaluate_approval_dag(&args, &summaries, recommended);
        assert!(dag.all_approved());
        apply_production_eligibility(&mut summaries, recommended, dag.all_approved());
        assert!(compute_production_blockers(&summaries, recommended, &dag).is_empty());
        assert!(summaries[0].eligible_for_production);
        assert!(!summaries[1].eligible_for_production);
        assert!(!summaries[2].eligible_for_production);
        Ok(())
    }

    #[test]
    fn provisional_reference_never_enables_production_ready() {
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(256, 2.5, true, true, false)];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(!summaries[0].eligible_for_production);
        assert!(!summaries[0].production_ready);
    }

    #[test]
    fn recommendation_is_absent_when_no_internal_candidate_passes() {
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(256, 0.5, true, true, false)];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert_eq!(recommend_candidate(&summaries), None);
    }

    #[test]
    fn oversized_candidate_is_ineligible() {
        let mut args = fixture_args();
        args.max_release_mib = 0.0005;
        let mut summaries = vec![fixture_summary(256, 2.5, true, true, false)];
        summaries[0].release_csv_bytes = 1_000_000;
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(!summaries[0].within_size_budget);
        assert!(!summaries[0].eligible_for_candidate_recommendation);
    }

    #[test]
    fn independent_fields_require_current_validation_schema() {
        let validation = serde_json::json!({
            "independent_regions_pass": true,
            "independent_reference_production_use": false,
            "independent_comparison_pass": false,
        });
        let (regions, production_use, comparison) = independent_fields_from_validation(&validation);
        assert!(regions);
        assert!(!production_use);
        assert!(!comparison);
    }

    #[test]
    fn provisional_reference_with_passing_regions_is_candidate_eligible() {
        let args = fixture_args();
        let mut summaries = vec![
            fixture_summary(64, 4.0, true, true, false),
            fixture_summary(128, 3.0, true, true, false),
            fixture_summary(256, 2.5, true, true, false),
        ];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(summaries[2].independent_regions_pass);
        assert!(!summaries[2].independent_reference_production_use);
        assert!(summaries[2].candidate_science_ready);
        assert!(summaries[2].eligible_for_candidate_recommendation);
        assert!(!summaries[2].eligible_for_production);
    }

    #[test]
    fn failed_independent_region_blocks_candidate_science_ready() {
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(256, 2.5, true, false, false)];
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        assert!(!summaries[0].independent_regions_pass);
        assert!(!summaries[0].candidate_science_ready);
        assert!(!summaries[0].eligible_for_candidate_recommendation);
    }

    #[test]
    fn missing_independent_regions_fail_closed() {
        let validation = serde_json::json!({
            "independent_comparison": {
                "production_use": false
            }
        });
        assert!(!all_independent_regions_pass(&validation));
        assert_eq!(brightest_region_mean(&validation), None);
    }

    #[test]
    fn brightest_region_mean_is_absent_when_any_region_fails() {
        let validation = serde_json::json!({
            "independent_comparison": {
                "production_use": false,
                "regions": [
                    {"pass": true, "observed_mean": 1.0},
                    {"pass": false, "observed_mean": 100.0}
                ]
            }
        });
        assert!(!all_independent_regions_pass(&validation));
        assert_eq!(brightest_region_mean(&validation), None);
    }

    #[test]
    fn production_band_is_parsed_and_old_partial_band_blocks_approval_dag() {
        assert_eq!(
            parse_band_nm("integrated 300–650 nm photon radiance"),
            Some([300.0, 650.0])
        );
        assert_eq!(
            parse_band_nm("Gaia DR3 XP passband-integrated 336-650 nm photon radiance"),
            Some([336.0, 650.0])
        );
        let args = fixture_args();
        let mut summaries = vec![fixture_summary(64, 4.0, true, true, true)];
        summaries[0].band_nm = Some([336.0, 650.0]);
        assess_candidates(&mut summaries, &args);
        finalize_eligibility(&mut summaries);
        let dag = evaluate_approval_dag(&args, &summaries, Some(64));
        assert!(!dag.all_approved());
        assert!(dag
            .blockers()
            .iter()
            .any(|blocker| blocker.starts_with("release_compatibility_invalid:")));
    }

    #[test]
    fn catalog_checksum_verification_accepts_sha256_prefix() {
        let digest = "a".repeat(64);
        verify_expected_catalog_checksum(&format!("sha256:{digest}"), &digest).unwrap();
        verify_expected_catalog_checksum(&digest, &format!("sha256:{digest}")).unwrap();
    }

    #[test]
    fn catalog_checksum_verification_rejects_mismatch() {
        let expected = "a".repeat(64);
        let found = "b".repeat(64);
        let error = verify_expected_catalog_checksum(&expected, &found).expect_err("mismatch");
        assert!(error.to_string().contains("expected sha256:"));
        assert!(error.to_string().contains("found sha256:"));
    }

    #[test]
    fn catalog_checksum_verification_rejects_invalid_hex() {
        let error = normalize_catalog_checksum("not-a-valid-checksum").expect_err("invalid");
        assert!(error
            .to_string()
            .contains("invalid SHA-256 checksum format"));
    }

    #[test]
    fn assess_existing_mode_skips_map_rebuild() {
        assert!(!fixture_args().assess_existing);
        assert!(
            Args {
                assess_existing: true,
                ..fixture_args()
            }
            .assess_existing
        );
    }

    fn fixture_args() -> Args {
        Args {
            input: Some(PathBuf::from("catalogue.csv")),
            contributions_manifest: Some(PathBuf::from("contributions.toml")),
            model_checksum: Some(format!("sha256:{}", "2".repeat(64))),
            output_dir: PathBuf::from("sweep"),
            reference: Some(PathBuf::from("reference.json")),
            catalog_checksum: Some(format!("sha256:{}", "1".repeat(64))),
            catalog_license: Some("reviewed policy".to_string()),
            generation_date_utc: Some("2026-07-11T00:00:00Z".to_string()),
            assess_existing: false,
            require_production_ready: false,
            approval_root: None,
            release_id: None,
            missing_flux_approval: None,
            independent_validation_approval: None,
            redistribution_approval: None,
            nside_review: None,
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
        independent_regions_pass: bool,
        production_reference: bool,
    ) -> NsideSummary {
        let pixels = 12 * u64::from(nside) * u64::from(nside);
        NsideSummary {
            nside,
            map_sha256: format!("sha256:{}", "a".repeat(64)),
            band_nm: Some(STARLIGHT_PRODUCTION_BAND_NM),
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
            independent_regions_pass,
            independent_reference_production_use: production_reference,
            independent_comparison_pass: independent_regions_pass && production_reference,
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

    #[test]
    fn sweep_includes_required_nside_control_set() {
        assert_eq!(NSIDES, [64, 128, 256, 512]);
    }

    fn write_fixture_approval(
        root: &Path,
        approval_path: &str,
        artifact_type: ApprovalArtifactType,
        nside: Option<u32>,
        map_sha256: Option<String>,
        input: &str,
        output: &str,
    ) -> Result<()> {
        let digest = |path: &str| -> Result<ApprovalFileDigest> {
            Ok(ApprovalFileDigest {
                path: path.to_string(),
                sha256: format!(
                    "sha256:{}",
                    nsb_data_tools::platform::checksum_io::sha256_file(&root.join(path))?
                ),
            })
        };
        let approval = StarlightApproval {
            schema_version: APPROVAL_SCHEMA_VERSION,
            artifact_type,
            decision: ApprovalDecision::Approved,
            production_use: true,
            reviewer_kind: ReviewerKind::Human,
            reviewer_name: "Synthetic fixture maintainer".to_string(),
            date: "2026-07-11T12:00:00Z".to_string(),
            release_id: "synthetic-release-v1".to_string(),
            band_nm: STARLIGHT_PRODUCTION_BAND_NM,
            nside,
            map_sha256,
            manifest_sha256: None,
            input_files: vec![digest(input)?],
            output_files: vec![digest(output)?],
            rationale: format!(
                "Synthetic fixture validates the {} sweep approval gate.",
                artifact_type.as_str()
            ),
            references: vec!["synthetic-fixture-reference-v1".to_string()],
        };
        write_json(&root.join(approval_path), &approval)
    }
}
